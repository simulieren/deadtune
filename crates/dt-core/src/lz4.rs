//! Raw LZ4 blocks (no frame header), as Source 2 stores KV3 buffers and texture mips.

#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
#[error("LZ4 block is malformed")]
pub struct Lz4Error;

/// Decodes one block into exactly `out_len` bytes.
pub fn decode_block(src: &[u8], out_len: usize) -> Result<Vec<u8>, Lz4Error> {
    let mut out = Vec::with_capacity(out_len.min(src.len().saturating_mul(255)));
    let mut i = 0;
    let byte = |i: &mut usize| -> Result<usize, Lz4Error> {
        let b = *src.get(*i).ok_or(Lz4Error)?;
        *i += 1;
        Ok(b as usize)
    };
    let extended = |i: &mut usize, mut n: usize| -> Result<usize, Lz4Error> {
        if n == 15 {
            loop {
                let b = byte(i)?;
                n += b;
                if b != 255 {
                    break;
                }
            }
        }
        Ok(n)
    };
    while i < src.len() {
        let token = byte(&mut i)?;
        let literals = extended(&mut i, token >> 4)?;
        let end = i
            .checked_add(literals)
            .filter(|&e| e <= src.len())
            .ok_or(Lz4Error)?;
        out.extend_from_slice(&src[i..end]);
        i = end;
        if i >= src.len() {
            break;
        }
        let offset = byte(&mut i)? | (byte(&mut i)? << 8);
        let length = extended(&mut i, token & 0xF)? + 4;
        if offset == 0 || offset > out.len() || out.len() + length > out_len {
            return Err(Lz4Error);
        }
        let start = out.len() - offset;
        for k in 0..length {
            out.push(out[start + k]);
        }
    }
    if out.len() != out_len {
        return Err(Lz4Error);
    }
    Ok(out)
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// A greedy block encoder for fixtures: longest match in the last 64 KB at each step,
    /// literals otherwise. Valid LZ4, not fast.
    pub fn encode_block(src: &[u8]) -> Vec<u8> {
        fn length(out: &mut Vec<u8>, mut n: usize) {
            while n >= 255 {
                out.push(255);
                n -= 255;
            }
            out.push(n as u8);
        }
        fn sequence(out: &mut Vec<u8>, literals: &[u8], m: Option<(usize, usize)>) {
            let lit = literals.len();
            let ml = m.map_or(0, |(_, l)| l - 4);
            out.push(((lit.min(15) << 4) | ml.min(15)) as u8);
            if lit >= 15 {
                length(out, lit - 15);
            }
            out.extend_from_slice(literals);
            if let Some((offset, _)) = m {
                out.extend_from_slice(&(offset as u16).to_le_bytes());
                if ml >= 15 {
                    length(out, ml - 15);
                }
            }
        }
        let mut out = Vec::new();
        let mut lit_start = 0;
        let mut i = 0;
        // The last five bytes are always literals, and a match cannot start within 12
        // bytes of the end, as the reference encoder requires.
        while i + 12 <= src.len() {
            let window = i.saturating_sub(65535);
            let mut best = None;
            for cand in window..i {
                let max = src.len() - 5 - i;
                let mut l = 0;
                while l < max && src[cand + l] == src[i + l] {
                    l += 1;
                }
                if l >= 4 && best.is_none_or(|(_, bl)| l > bl) {
                    best = Some((i - cand, l));
                }
            }
            match best {
                Some((offset, l)) => {
                    sequence(&mut out, &src[lit_start..i], Some((offset, l)));
                    i += l;
                    lit_start = i;
                }
                None => i += 1,
            }
        }
        sequence(&mut out, &src[lit_start..], None);
        out
    }

    #[test]
    fn literals_and_matches() {
        assert_eq!(decode_block(&[0x30, b'a', b'b', b'c'], 3).unwrap(), b"abc");
        let repeated = [0x35, b'a', b'b', b'c', 3, 0];
        assert_eq!(decode_block(&repeated, 12).unwrap(), b"abcabcabcabc");
        let long = [0x3F, b'x', b'y', b'z', 1, 0, 0x00, 0x10, b'!'];
        let out = decode_block(&long, 3 + 19 + 1).unwrap();
        assert_eq!(&out[..3], b"xyz");
        assert!(out[3..22].iter().all(|&b| b == b'z'));
        assert_eq!(out[22], b'!');
        assert_eq!(decode_block(&[0x30, b'a'], 3), Err(Lz4Error));
        assert_eq!(decode_block(&[0x05, 9, 0], 4), Err(Lz4Error));
        assert_eq!(decode_block(&[0x30, b'a', b'b', b'c'], 4), Err(Lz4Error));
        assert_eq!(decode_block(&[], 0).unwrap(), b"");
    }

    #[test]
    fn long_runs_and_overlapping_matches() {
        let src: Vec<u8> = (0..20).chain(b"ab".repeat(150)).chain(40..60).collect();
        let packed = hex(
            "ff07000102030405060708090a0b0c0d0e0f1011121361620200ff18f00528292a2b2c2d2e2f303132333435363738393a3b",
        );
        assert_eq!(decode_block(&packed, src.len()).unwrap(), src);
        assert!(decode_block(&packed, src.len() + 1).is_err());
        assert!(decode_block(&packed[..packed.len() - 1], src.len()).is_err());
        assert!(
            decode_block(&[0x10, b'a', 0x05, 0x00], 10).is_err(),
            "offset past start"
        );
        assert!(
            decode_block(&[0x10, b'a', 0x01, 0x00], 3).is_err(),
            "match runs past the declared length"
        );
    }

    #[test]
    fn encoder_round_trips_and_shrinks_repetition() {
        let cases: Vec<Vec<u8>> = vec![
            Vec::new(),
            b"abc".to_vec(),
            b"abcabcabcabcabcabcabcabc".to_vec(),
            (0..20).chain(b"ab".repeat(150)).chain(40..60).collect(),
            (0..=255u8).cycle().take(70_000).collect(),
            vec![7; 5000],
            (0..3000).map(|i| (i * 7919 % 251) as u8).collect(),
        ];
        for src in cases {
            let packed = encode_block(&src);
            assert_eq!(decode_block(&packed, src.len()).unwrap(), src);
        }
        assert!(encode_block(&[7; 5000]).len() < 40);
    }

    fn hex(s: &str) -> Vec<u8> {
        (0..s.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap())
            .collect()
    }
}
