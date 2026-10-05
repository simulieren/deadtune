//! Just enough of the zip format to pull one file out of a mod archive: stored or deflated
//! entries, no zip64, no encryption.

use std::io::Read;

#[derive(Debug, PartialEq, Eq, thiserror::Error)]
pub enum ZipError {
    #[error("not a zip archive")]
    NotZip,
    #[error("the zip archive is damaged")]
    Damaged,
    #[error("no {0} in the zip archive")]
    Missing(String),
    #[error("more than one {0} in the zip archive")]
    Ambiguous(String),
    #[error("{0} uses an unsupported zip feature")]
    Unsupported(String),
}

const EOCD: u32 = 0x0605_4b50;
const CENTRAL: u32 = 0x0201_4b50;
const LOCAL: u32 = 0x0403_4b50;

fn u16_at(b: &[u8], at: usize) -> Result<u16, ZipError> {
    b.get(at..at + 2)
        .map(|s| u16::from_le_bytes([s[0], s[1]]))
        .ok_or(ZipError::Damaged)
}

fn u32_at(b: &[u8], at: usize) -> Result<u32, ZipError> {
    b.get(at..at + 4)
        .map(|s| u32::from_le_bytes([s[0], s[1], s[2], s[3]]))
        .ok_or(ZipError::Damaged)
}

struct Entry<'a> {
    name: &'a str,
    flags: u16,
    method: u16,
    crc: u32,
    compressed: usize,
    size: usize,
    local_offset: usize,
}

fn entries(zip: &[u8]) -> Result<Vec<Entry<'_>>, ZipError> {
    let search_from = zip.len().saturating_sub(22 + u16::MAX as usize);
    let eocd = (search_from..=zip.len().saturating_sub(22))
        .rev()
        .find(|&at| u32_at(zip, at) == Ok(EOCD))
        .ok_or(ZipError::NotZip)?;
    let count = u16_at(zip, eocd + 10)? as usize;
    let mut at = u32_at(zip, eocd + 16)? as usize;
    let mut out = Vec::with_capacity(count);
    for _ in 0..count {
        if u32_at(zip, at)? != CENTRAL {
            return Err(ZipError::Damaged);
        }
        let name_len = u16_at(zip, at + 28)? as usize;
        let skip = name_len + u16_at(zip, at + 30)? as usize + u16_at(zip, at + 32)? as usize;
        let name = zip
            .get(at + 46..at + 46 + name_len)
            .ok_or(ZipError::Damaged)?;
        out.push(Entry {
            name: std::str::from_utf8(name).map_err(|_| ZipError::Damaged)?,
            flags: u16_at(zip, at + 8)?,
            method: u16_at(zip, at + 10)?,
            crc: u32_at(zip, at + 16)?,
            compressed: u32_at(zip, at + 20)? as usize,
            size: u32_at(zip, at + 24)? as usize,
            local_offset: u32_at(zip, at + 42)? as usize,
        });
        at += 46 + skip;
    }
    Ok(out)
}

/// The one entry whose file name (ignoring folders and case) is `file_name`.
pub fn extract(zip: &[u8], file_name: &str) -> Result<Vec<u8>, ZipError> {
    let all = entries(zip)?;
    let mut matches = all.iter().filter(|e| {
        e.name
            .rsplit('/')
            .next()
            .is_some_and(|base| base.eq_ignore_ascii_case(file_name))
    });
    let entry = matches
        .next()
        .ok_or_else(|| ZipError::Missing(file_name.to_string()))?;
    if matches.next().is_some() {
        return Err(ZipError::Ambiguous(file_name.to_string()));
    }
    let unsupported = || ZipError::Unsupported(entry.name.to_string());
    if entry.flags & 1 != 0 || entry.size == u32::MAX as usize {
        return Err(unsupported());
    }
    let local = entry.local_offset;
    if u32_at(zip, local)? != LOCAL {
        return Err(ZipError::Damaged);
    }
    let start = local + 30 + u16_at(zip, local + 26)? as usize + u16_at(zip, local + 28)? as usize;
    let data = zip
        .get(start..start + entry.compressed)
        .ok_or(ZipError::Damaged)?;
    let bytes = match entry.method {
        0 => data.to_vec(),
        8 => {
            let mut out = Vec::with_capacity(entry.size);
            flate2::read::DeflateDecoder::new(data)
                .take(entry.size as u64 + 1)
                .read_to_end(&mut out)
                .map_err(|_| ZipError::Damaged)?;
            out
        }
        _ => return Err(unsupported()),
    };
    let mut crc = flate2::Crc::new();
    crc.update(&bytes);
    if bytes.len() != entry.size || crc.sum() != entry.crc {
        return Err(ZipError::Damaged);
    }
    Ok(bytes)
}

pub fn is_zip(bytes: &[u8]) -> bool {
    bytes.starts_with(&LOCAL.to_le_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;

    const GI: &[u8] = include_bytes!("../tests/fixtures/sidelock_standin/gameinfo.gi");
    const DEFLATED: &[u8] = include_bytes!("../tests/fixtures/sidelock_standin/cfg.zip");
    const NESTED_STORED: &[u8] =
        include_bytes!("../tests/fixtures/sidelock_standin/nested_stored.zip");

    #[test]
    fn extracts_a_deflated_entry() {
        assert_eq!(extract(DEFLATED, "gameinfo.gi").unwrap(), GI);
    }

    #[test]
    fn extracts_a_stored_entry_inside_a_folder_ignoring_case() {
        assert_eq!(extract(NESTED_STORED, "GameInfo.gi").unwrap(), GI);
        assert_eq!(extract(NESTED_STORED, "readme.txt").unwrap(), b"readme\n");
    }

    #[test]
    fn missing_entry_and_non_zip_input_are_errors() {
        assert_eq!(
            extract(DEFLATED, "video.txt"),
            Err(ZipError::Missing("video.txt".into()))
        );
        assert_eq!(extract(GI, "gameinfo.gi"), Err(ZipError::NotZip));
        assert_eq!(extract(b"", "gameinfo.gi"), Err(ZipError::NotZip));
        assert!(is_zip(DEFLATED) && !is_zip(GI));
    }

    #[test]
    fn corrupted_data_fails_the_crc_check() {
        let mut bad = NESTED_STORED.to_vec();
        let at = bad.windows(8).position(|w| w == b"\"GameInf").unwrap();
        bad[at + 1] = b'X';
        assert_eq!(extract(&bad, "gameinfo.gi"), Err(ZipError::Damaged));
    }

    #[test]
    fn truncated_archive_is_not_a_panic() {
        for cut in [10, DEFLATED.len() / 2, DEFLATED.len() - 1] {
            assert!(extract(&DEFLATED[..cut], "gameinfo.gi").is_err(), "{cut}");
        }
    }
}
