//! Sinner's Sacrifice light fix (HoppCX, original from piggy's Discord), rebuilt from the
//! installed game. The vault's lights are drawn from a fine black-and-white sequencer mask
//! that the shader scrolls; a lower mip of that mask is a grey blur, so a raised
//! `r_texture_stream_mip_bias` or a far LOD makes the pattern unreadable. Upstream `pak26`
//! changes two files and nothing else. The mask texture keeps only its full-size 1024x1024
//! ATI1N level, with the other ten levels dropped and the `NO_LOD` flag set, so there is no
//! smaller copy for the engine to choose. The vault model is the game's own (Valve's RED2,
//! every other block LZ4 as compiled) with two values changed in its DATA block:
//! `m_refLODGroupMasks` is `[7, 0, 0]` (the full-detail mesh in every LOD group) where the
//! three LOD meshes would carry `[1, 2, 4]`, and `m_lodGroupSwitchDistances` is
//! `[0, 1e6, 1e6]` where RED2 records switch points 12 and 25, so the full-detail mesh is
//! always drawn. That block alone is stored uncompressed, which accounts for the file being
//! 13,327 bytes larger than the game's: LZ4 of the same block lands within 100 bytes.
//! We do the same to the player's own copies: cut the texture's mip chain and set the flag,
//! decode the model's KV3 DATA (LZ4 or uncompressed), patch the values in place, and write
//! it back uncompressed. Evidence and the byte-level comparison: NOTES.md for this addon.

use std::collections::BTreeMap;

use super::AddonError;
use crate::hud::resource::{Resource, ResourceError};
use crate::hud::vpk::VpkDir;
use crate::texture::vtex::{Flags, Vtex};

pub const MODEL: &str = "models/props_gameplay/sinners_sacrifice_vault/sinners_sacrifice.vmdl_c";
pub const MASK: &str = "models/props_gameplay/sinners_sacrifice_vault/materials/lights_multimode_sequencer_emmisive_mask_psd_721acad7.vtex_c";

const LOD_MASKS: &str = "m_refLODGroupMasks";
const LOD_DISTANCES: &str = "m_lodGroupSwitchDistances";
/// Upstream's value; past any distance the camera reaches.
pub const FAR: f64 = 1_000_000.0;

/// Pak path -> bytes: the game's mask texture and vault model, changed as upstream does.
pub fn build(game: &VpkDir) -> Result<BTreeMap<String, Vec<u8>>, AddonError> {
    let mask = pin_top_mip(&game.read(MASK)?)?;
    let model = pin_lod0(&game.read(MODEL)?)?;
    Ok(BTreeMap::from([
        (MASK.to_string(), mask),
        (MODEL.to_string(), model),
    ]))
}

fn malformed(what: impl Into<String>) -> AddonError {
    AddonError::Resource(ResourceError::Malformed(what.into()))
}

fn u32_at(b: &[u8], at: usize) -> Result<u32, AddonError> {
    b.get(at..at + 4)
        .map(|s| u32::from_le_bytes([s[0], s[1], s[2], s[3]]))
        .ok_or_else(|| malformed("truncated"))
}

fn put_u32(b: &mut [u8], at: usize, v: u32) {
    b[at..at + 4].copy_from_slice(&v.to_le_bytes());
}

const VTEX_FLAGS: usize = 2;
const VTEX_MIP_COUNT: usize = 27;
const VTEX_EXTRA: usize = 32;
const EXTRA_COMPRESSED_MIP_SIZE: u32 = 4;

/// `bytes` with only the largest mip kept and `NO_LOD` set. The largest mip is stored
/// last, so the kept pixels are the file's tail.
pub fn pin_top_mip(bytes: &[u8]) -> Result<Vec<u8>, AddonError> {
    let bad = |e: crate::texture::vtex::VtexError| malformed(e.to_string());
    let v = Vtex::parse(bytes).map_err(bad)?;
    if v.pixel_start() + v.pixel_len() != bytes.len() {
        return Err(malformed(
            "mask texture: mip sizes do not match the file length",
        ));
    }
    if v.depth != 1
        || [Flags::CUBE, Flags::VOLUME, Flags::ARRAY]
            .iter()
            .any(|f| v.flags.contains(*f))
    {
        return Err(malformed("mask texture is not a plain 2D texture"));
    }
    let top = v.mips[0].stored_len;
    let header = v.pixel_start() - data_block_len(bytes)?;
    let mut out = bytes[..v.pixel_start()].to_vec();
    out.extend_from_slice(&bytes[bytes.len() - top..]);

    let flags = Flags(v.flags.0 | Flags::NO_LOD.0);
    out[header + VTEX_FLAGS..header + VTEX_FLAGS + 2].copy_from_slice(&flags.0.to_le_bytes());
    out[header + VTEX_MIP_COUNT] = 1;
    shrink_size_table(&mut out, header, v.mips.len())?;

    let back = Vtex::parse(&out).map_err(bad)?;
    if back.mips.len() != 1
        || !back.flags.contains(Flags::NO_LOD)
        || back.pixel_start() + back.pixel_len() != out.len()
    {
        return Err(AddonError::Invalid(
            "mask texture did not read back as one NO_LOD mip".into(),
        ));
    }
    Ok(out)
}

fn data_block_len(bytes: &[u8]) -> Result<usize, AddonError> {
    let table = 8 + u32_at(bytes, 8)? as usize;
    for i in 0..u32_at(bytes, 12)? as usize {
        let entry = table + 12 * i;
        if bytes.get(entry..entry + 4) == Some(b"DATA") {
            return Ok(u32_at(bytes, entry + 8)? as usize);
        }
    }
    Err(AddonError::Resource(ResourceError::MissingBlock("DATA")))
}

/// A COMPRESSED_MIP_SIZE table lists stored sizes largest mip first; keep the first.
fn shrink_size_table(out: &mut [u8], header: usize, count: usize) -> Result<(), AddonError> {
    let table = header + VTEX_EXTRA + u32_at(out, header + VTEX_EXTRA)? as usize;
    let entries =
        crate::texture::vtex::extras(out, header).map_err(|e| malformed(e.to_string()))?;
    for (i, extra) in entries.iter().enumerate() {
        if extra.kind == EXTRA_COMPRESSED_MIP_SIZE {
            let entry = table + 12 * i;
            let payload = extra.payload.start;
            let array = payload + 4 + u32_at(out, payload + 4)? as usize;
            put_u32(out, payload + 8, 1);
            out[array + 4..array + 4 * count].fill(0);
            if extra.payload.len() == 12 + 4 * count {
                put_u32(out, entry + 8, 16);
            }
        }
    }
    Ok(())
}

/// `bytes` (a compiled model) with the full-detail mesh drawn at every distance.
pub fn pin_lod0(bytes: &[u8]) -> Result<Vec<u8>, AddonError> {
    let mut res = Resource::parse(bytes)?;
    let block = res
        .blocks
        .iter_mut()
        .find(|b| &b.name == b"DATA")
        .ok_or(ResourceError::MissingBlock("DATA"))?;
    let mut kv = Kv3::decode(&block.data)?;
    kv.pin_lod0()?;
    block.data = kv.encode();

    let out = res.to_bytes();
    let (masks, distances) = lod_fields(&out)?;
    let all = masks.iter().fold(0, |a, m| a | m);
    if masks.first() != Some(&all)
        || masks.iter().skip(1).any(|&m| m != 0)
        || distances.iter().skip(1).any(|&d| d != FAR)
    {
        return Err(AddonError::Invalid(
            "model LOD fields did not read back pinned".into(),
        ));
    }
    Ok(out)
}

/// `(m_refLODGroupMasks, m_lodGroupSwitchDistances)` of a compiled model.
pub fn lod_fields(model: &[u8]) -> Result<(Vec<u64>, Vec<f64>), AddonError> {
    let res = Resource::parse(model)?;
    let data = &res
        .block(b"DATA")
        .ok_or(ResourceError::MissingBlock("DATA"))?
        .data;
    let kv = Kv3::decode(data)?;
    let lod = kv.lod_slots()?;
    let masks = lod
        .masks
        .iter()
        .map(|s| kv.int(s))
        .collect::<Result<_, _>>()?;
    let distances = lod
        .distances
        .iter()
        .map(|s| kv.float(s))
        .collect::<Result<_, _>>()?;
    Ok((masks, distances))
}

/// Binary KV3 version 5 (reference: ValveResourceFormat `BinaryKV3.cs`). Values live in
/// typed lanes (1, 2, 4 and 8 byte) of two buffers, read in tree order as a separate type
/// stream is walked. Patching a value in place is safe because no length changes.
struct Kv3 {
    header: Vec<u8>,
    buf1: Vec<u8>,
    buf2: Vec<u8>,
}

const KV3_MAGIC: &[u8; 4] = b"\x053VK";
const KV3_HEADER: usize = 120;
const H_COMPRESSION: usize = 20;
const H_FRAME: usize = 24;
const H_BYTES1: usize = 28;
const H_BYTES4: usize = 32;
const H_BYTES8: usize = 36;
const H_TYPES: usize = 40;
const H_UNCOMPRESSED: usize = 48;
const H_COMPRESSED: usize = 52;
const H_BLOBS: usize = 56;
const H_BYTES2: usize = 64;
const H_BLOB_SIZES: usize = 68;
const H_BUF1: usize = 72;
const H_BUF1_PACKED: usize = 76;
const H_BUF2: usize = 80;
const H_BUF2_PACKED: usize = 84;
const H_BUF2_LANES: usize = 88;
const H_OBJECTS2: usize = 108;

const UNCOMPRESSED: u32 = 0;
const LZ4: u32 = 1;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Buf {
    One,
    Two,
}

/// Where one primitive value sits: node type, buffer and byte offset.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Slot {
    ty: u8,
    buf: Buf,
    at: usize,
}

struct LodSlots {
    masks: Vec<Slot>,
    distances: Vec<Slot>,
}

impl Kv3 {
    fn decode(data: &[u8]) -> Result<Kv3, AddonError> {
        if data.get(..4) != Some(KV3_MAGIC) || data.len() < KV3_HEADER {
            return Err(malformed("model DATA is not binary KV3 version 5"));
        }
        let field = |at| u32_at(data, at).map(|v| v as usize);
        if field(H_BLOBS)? != 0 {
            return Err(malformed("model DATA carries binary blobs"));
        }
        let (len1, len2) = (field(H_BUF1)?, field(H_BUF2)?);
        let (buf1, buf2) = match u32_at(data, H_COMPRESSION)? {
            UNCOMPRESSED => {
                let body = &data[KV3_HEADER..];
                let buf1 = body.get(..len1).ok_or_else(|| malformed("KV3 truncated"))?;
                let buf2 = body
                    .get(len1..len1 + len2)
                    .ok_or_else(|| malformed("KV3 truncated"))?;
                (buf1.to_vec(), buf2.to_vec())
            }
            LZ4 => {
                let (packed1, packed2) = (field(H_BUF1_PACKED)?, field(H_BUF2_PACKED)?);
                let body = &data[KV3_HEADER..];
                let src1 = body
                    .get(..packed1)
                    .ok_or_else(|| malformed("KV3 truncated"))?;
                let src2 = body
                    .get(packed1..packed1 + packed2)
                    .ok_or_else(|| malformed("KV3 truncated"))?;
                (lz4_block(src1, len1)?, lz4_block(src2, len2)?)
            }
            other => {
                return Err(malformed(format!(
                    "model DATA uses KV3 compression {other} (2 is zstd); only LZ4 and none are supported"
                )));
            }
        };
        Ok(Kv3 {
            header: data[..KV3_HEADER].to_vec(),
            buf1,
            buf2,
        })
    }

    /// Uncompressed version 5, as upstream stores it.
    fn encode(&self) -> Vec<u8> {
        let mut h = self.header.clone();
        let total = u32_at(&h, H_UNCOMPRESSED).expect("header length checked in decode");
        put_u32(&mut h, H_COMPRESSION, UNCOMPRESSED);
        put_u32(&mut h, H_FRAME, 0);
        put_u32(&mut h, H_COMPRESSED, total);
        put_u32(&mut h, H_BLOB_SIZES, 0);
        put_u32(&mut h, H_BUF1_PACKED, 0);
        put_u32(&mut h, H_BUF2_PACKED, 0);
        let mut out = h;
        out.extend_from_slice(&self.buf1);
        out.extend_from_slice(&self.buf2);
        out
    }

    fn lod_slots(&self) -> Result<LodSlots, AddonError> {
        let mut w = Walker::new(self)?;
        let mut found = BTreeMap::new();
        w.root(&mut found)?;
        w.finish()?;
        let mut take = |key: &str| {
            found
                .remove(key)
                .ok_or_else(|| malformed(format!("model DATA has no {key}")))
        };
        Ok(LodSlots {
            masks: take(LOD_MASKS)?,
            distances: take(LOD_DISTANCES)?,
        })
    }

    fn pin_lod0(&mut self) -> Result<(), AddonError> {
        let lod = self.lod_slots()?;
        let all = lod
            .masks
            .iter()
            .map(|s| self.int(s))
            .try_fold(0u64, |a, m| m.map(|m| a | m))?;
        for (i, slot) in lod.masks.iter().enumerate() {
            self.set_int(slot, if i == 0 { all } else { 0 })?;
        }
        for slot in lod.distances.iter().skip(1) {
            self.set_float(slot, FAR)?;
        }
        Ok(())
    }

    fn bytes(&self, s: &Slot, len: usize) -> &[u8] {
        let b = match s.buf {
            Buf::One => &self.buf1,
            Buf::Two => &self.buf2,
        };
        &b[s.at..s.at + len]
    }

    fn bytes_mut(&mut self, s: &Slot, len: usize) -> &mut [u8] {
        let b = match s.buf {
            Buf::One => &mut self.buf1,
            Buf::Two => &mut self.buf2,
        };
        &mut b[s.at..s.at + len]
    }

    fn int(&self, s: &Slot) -> Result<u64, AddonError> {
        Ok(match s.ty {
            T_INT64_ZERO => 0,
            T_INT64_ONE => 1,
            T_INT32 | T_UINT32 => u32::from_le_bytes(self.bytes(s, 4).try_into().unwrap()) as u64,
            T_INT64 | T_UINT64 => u64::from_le_bytes(self.bytes(s, 8).try_into().unwrap()),
            t => return Err(malformed(format!("LOD mask has KV3 type {t}"))),
        })
    }

    fn set_int(&mut self, s: &Slot, v: u64) -> Result<(), AddonError> {
        match s.ty {
            T_INT32 | T_UINT32 if v <= u32::MAX as u64 => self
                .bytes_mut(s, 4)
                .copy_from_slice(&(v as u32).to_le_bytes()),
            T_INT64 | T_UINT64 => self.bytes_mut(s, 8).copy_from_slice(&v.to_le_bytes()),
            t => {
                return Err(malformed(format!(
                    "LOD mask with KV3 type {t} cannot be rewritten in place"
                )));
            }
        }
        Ok(())
    }

    fn float(&self, s: &Slot) -> Result<f64, AddonError> {
        Ok(match s.ty {
            T_DOUBLE_ZERO => 0.0,
            T_DOUBLE_ONE => 1.0,
            T_FLOAT => f32::from_le_bytes(self.bytes(s, 4).try_into().unwrap()) as f64,
            T_DOUBLE => f64::from_le_bytes(self.bytes(s, 8).try_into().unwrap()),
            t => return Err(malformed(format!("LOD distance has KV3 type {t}"))),
        })
    }

    fn set_float(&mut self, s: &Slot, v: f64) -> Result<(), AddonError> {
        match s.ty {
            T_FLOAT => self
                .bytes_mut(s, 4)
                .copy_from_slice(&(v as f32).to_le_bytes()),
            T_DOUBLE => self.bytes_mut(s, 8).copy_from_slice(&v.to_le_bytes()),
            t => {
                return Err(malformed(format!(
                    "LOD distance with KV3 type {t} cannot be rewritten in place"
                )));
            }
        }
        Ok(())
    }
}

const T_NULL: u8 = 1;
const T_BOOLEAN: u8 = 2;
const T_INT64: u8 = 3;
const T_UINT64: u8 = 4;
const T_DOUBLE: u8 = 5;
const T_STRING: u8 = 6;
const T_ARRAY: u8 = 8;
const T_OBJECT: u8 = 9;
const T_ARRAY_TYPED: u8 = 10;
const T_INT32: u8 = 11;
const T_UINT32: u8 = 12;
const T_TRUE: u8 = 13;
const T_FALSE: u8 = 14;
const T_INT64_ZERO: u8 = 15;
const T_INT64_ONE: u8 = 16;
const T_DOUBLE_ZERO: u8 = 17;
const T_DOUBLE_ONE: u8 = 18;
const T_FLOAT: u8 = 19;
const T_INT16: u8 = 20;
const T_UINT16: u8 = 21;
const T_INT8: u8 = 22;
const T_UINT8: u8 = 23;
const T_ARRAY_BYTE_LENGTH: u8 = 24;
const T_ARRAY_AUXILIARY: u8 = 25;

fn is_array(ty: u8) -> bool {
    matches!(
        ty,
        T_ARRAY | T_ARRAY_TYPED | T_ARRAY_BYTE_LENGTH | T_ARRAY_AUXILIARY
    )
}

/// Read cursors for the 1, 2, 4 and 8 byte lanes of one buffer: (next, end).
#[derive(Clone, Copy, Debug, Default)]
struct Lanes([(usize, usize); 4]);

impl Lanes {
    /// Lanes laid out back to back from `at`, each aligned to its width when present.
    fn layout(mut at: usize, counts: [usize; 4]) -> (Lanes, usize) {
        let mut lanes = Lanes::default();
        for (i, &n) in counts.iter().enumerate() {
            let width = 1 << i;
            if n > 0 {
                at = at.div_ceil(width) * width;
            }
            lanes.0[i] = (at, at + n * width);
            at += n * width;
        }
        (lanes, at)
    }

    fn take(&mut self, width: usize) -> Result<usize, AddonError> {
        let lane = &mut self.0[width.trailing_zeros() as usize];
        let at = lane.0;
        if at + width > lane.1 {
            return Err(malformed("KV3 lane overrun"));
        }
        lane.0 += width;
        Ok(at)
    }

    fn drained(&self) -> bool {
        self.0.iter().all(|(next, end)| next == end)
    }
}

struct Walker<'a> {
    kv: &'a Kv3,
    strings: Vec<&'a str>,
    aux: Lanes,
    main: Lanes,
    objects: (usize, usize),
    types: (usize, usize),
}

impl<'a> Walker<'a> {
    fn new(kv: &'a Kv3) -> Result<Walker<'a>, AddonError> {
        let h = |at| u32_at(&kv.header, at).map(|v| v as usize);
        let (mut aux, end1) =
            Lanes::layout(0, [h(H_BYTES1)?, h(H_BYTES2)?, h(H_BYTES4)?, h(H_BYTES8)?]);
        if end1 > kv.buf1.len() {
            return Err(malformed("KV3 buffer 1 shorter than its lanes"));
        }
        let count = u32_at(&kv.buf1, aux.take(4)?)? as usize;
        let mut strings = Vec::new();
        for _ in 0..count {
            let (start, end) = aux.0[0];
            let len = kv.buf1[start..end]
                .iter()
                .position(|&b| b == 0)
                .ok_or_else(|| malformed("KV3 string not terminated"))?;
            strings.push(
                std::str::from_utf8(&kv.buf1[start..start + len])
                    .map_err(|_| malformed("KV3 string is not UTF-8"))?,
            );
            aux.0[0].0 += len + 1;
        }
        let objects = h(H_OBJECTS2)? * 4;
        let lanes2 = [
            h(H_BUF2_LANES)?,
            h(H_BUF2_LANES + 4)?,
            h(H_BUF2_LANES + 8)?,
            h(H_BUF2_LANES + 12)?,
        ];
        let (main, types_at) = Lanes::layout(objects, lanes2);
        let types = (types_at, types_at + h(H_TYPES)?);
        if types.1 > kv.buf2.len() {
            return Err(malformed("KV3 buffer 2 shorter than its lanes"));
        }
        Ok(Walker {
            kv,
            strings,
            aux,
            main,
            objects: (0, objects),
            types,
        })
    }

    fn ty(&mut self) -> Result<u8, AddonError> {
        let mut next = || {
            let at = self.types.0;
            if at >= self.types.1 {
                return Err(malformed("KV3 type stream overrun"));
            }
            self.types.0 += 1;
            Ok(self.kv.buf2[at])
        };
        let b = next()?;
        if b & 0x80 != 0 {
            next()?;
        }
        if b & 0x40 != 0 {
            next()?;
        }
        Ok(b & 0x3f)
    }

    fn main_u32(&mut self) -> Result<usize, AddonError> {
        let at = self.main.take(4)?;
        Ok(u32_at(&self.kv.buf2, at)? as usize)
    }

    fn root(&mut self, found: &mut BTreeMap<String, Vec<Slot>>) -> Result<(), AddonError> {
        if self.ty()? != T_OBJECT {
            return Err(malformed("KV3 root is not an object"));
        }
        for _ in 0..self.object_len()? {
            let ty = self.ty()?;
            let key = self.key()?;
            if (key == LOD_MASKS || key == LOD_DISTANCES) && is_array(ty) {
                let elements = self.elements(ty)?;
                found.insert(key.to_string(), elements);
            } else {
                self.value(ty, Buf::Two)?;
            }
        }
        Ok(())
    }

    fn object_len(&mut self) -> Result<usize, AddonError> {
        let at = self.objects.0;
        if at + 4 > self.objects.1 {
            return Err(malformed("KV3 object lengths overrun"));
        }
        self.objects.0 += 4;
        Ok(u32_at(&self.kv.buf2, at)? as usize)
    }

    fn key(&mut self) -> Result<&'a str, AddonError> {
        let id = self.main_u32()?;
        Ok(self.strings.get(id).copied().unwrap_or(""))
    }

    /// Walks one value of node type `ty`, reading primitives from `lane`'s buffer. The
    /// slot's offset only means something for a primitive.
    fn value(&mut self, ty: u8, lane: Buf) -> Result<Slot, AddonError> {
        let width = match ty {
            T_BOOLEAN | T_INT8 | T_UINT8 => 1,
            T_INT16 | T_UINT16 => 2,
            T_INT32 | T_UINT32 | T_FLOAT => 4,
            T_INT64 | T_UINT64 | T_DOUBLE => 8,
            T_NULL | T_TRUE | T_FALSE | T_INT64_ZERO | T_INT64_ONE | T_DOUBLE_ZERO
            | T_DOUBLE_ONE => 0,
            T_STRING => {
                self.main_u32()?;
                0
            }
            T_OBJECT => {
                for _ in 0..self.object_len()? {
                    let ty = self.ty()?;
                    self.key()?;
                    self.value(ty, Buf::Two)?;
                }
                0
            }
            t if is_array(t) => {
                self.elements(t)?;
                0
            }
            other => return Err(malformed(format!("KV3 node type {other} not supported"))),
        };
        let at = match (width, lane) {
            (0, _) => 0,
            (w, Buf::One) => self.aux.take(w)?,
            (w, Buf::Two) => self.main.take(w)?,
        };
        Ok(Slot { ty, buf: lane, at })
    }

    /// Walks an array of node type `ty` and returns its elements' slots.
    fn elements(&mut self, ty: u8) -> Result<Vec<Slot>, AddonError> {
        if ty == T_ARRAY {
            let len = self.main_u32()?;
            return (0..len)
                .map(|_| {
                    let ty = self.ty()?;
                    self.value(ty, Buf::Two)
                })
                .collect();
        }
        let len = if ty == T_ARRAY_TYPED {
            self.main_u32()?
        } else {
            self.kv.buf2[self.main.take(1)?] as usize
        };
        let sub = self.ty()?;
        let lane = if ty == T_ARRAY_AUXILIARY {
            Buf::One
        } else {
            Buf::Two
        };
        (0..len).map(|_| self.value(sub, lane)).collect()
    }

    fn finish(&self) -> Result<(), AddonError> {
        if self.types.0 != self.types.1
            || self.objects.0 != self.objects.1
            || !self.main.drained()
            || !self.aux.drained()
        {
            return Err(malformed("KV3 walk did not consume every lane"));
        }
        Ok(())
    }
}

fn lz4_block(src: &[u8], len: usize) -> Result<Vec<u8>, AddonError> {
    crate::lz4::decode_block(src, len).map_err(|e| malformed(e.to_string()))
}

#[cfg(test)]
pub(crate) mod tests {
    use std::path::Path;

    use super::*;
    use crate::hud::vpk;

    const STOCK_DATA: &[u8] = include_bytes!("../../tests/fixtures/sinner/stock_model_data.kv3");

    fn upstream() -> VpkDir {
        let p = Path::new(env!("CARGO_MANIFEST_DIR")).join(
            "../../research/configs/OptimizationLock/Various Addons Relating to Performance/Sinner Light Fix Mod/pak26_dir.vpk",
        );
        VpkDir::open(&p).unwrap()
    }

    fn with_data(model: &[u8], data: &[u8]) -> Vec<u8> {
        let mut res = Resource::parse(model).unwrap();
        res.blocks
            .iter_mut()
            .find(|b| &b.name == b"DATA")
            .unwrap()
            .data = data.to_vec();
        res.to_bytes()
    }

    /// The upstream model with the DATA block as the stock game file most likely stores
    /// it (fixture made by `tests/fixtures/sinner/make_stock_data.py`).
    pub fn stock_model() -> Vec<u8> {
        with_data(&upstream().read(MODEL).unwrap(), STOCK_DATA)
    }

    /// The upstream mask with the ten smaller ATI1N levels put back in front of its
    /// 1024x1024 level and `NO_LOD` cleared: the stock texture's 701,196-byte layout
    /// (2,132 header bytes in the game's file, 16 fewer than upstream's) with the same
    /// level 0. The small levels' pixels are filler.
    pub fn stock_mask() -> Vec<u8> {
        let up = upstream().read(MASK).unwrap();
        let v = Vtex::parse(&up).unwrap();
        let header = v.pixel_start() - data_block_len(&up).unwrap();
        let mut out = up[..v.pixel_start()].to_vec();
        out[header + VTEX_FLAGS] &= !(Flags::NO_LOD.0 as u8);
        out[header + VTEX_MIP_COUNT] = 11;
        for level in (1..11u8).rev() {
            let side = (1024usize >> level).max(1);
            let blocks = side.div_ceil(4);
            out.extend(std::iter::repeat_n(level, blocks * blocks * 8));
        }
        out.extend_from_slice(&up[v.pixel_start()..]);
        out
    }

    /// The stock model with its DATA block marked zstd, which the builder refuses.
    pub fn zstd_model() -> Vec<u8> {
        let stock = stock_model();
        let mut data = Resource::parse(&stock)
            .unwrap()
            .block(b"DATA")
            .unwrap()
            .data
            .clone();
        put_u32(&mut data, H_COMPRESSION, 2);
        with_data(&stock, &data)
    }

    #[test]
    fn upstream_files_are_what_the_doc_says() {
        let up = upstream();
        assert_eq!(up.entries.len(), 2);
        let mask = Vtex::parse(&up.read(MASK).unwrap()).unwrap();
        assert_eq!(
            (mask.width, mask.height, mask.format.name()),
            (1024, 1024, "ATI1N")
        );
        assert_eq!(mask.mips.len(), 1);
        assert_eq!(mask.flags, Flags::NO_LOD);
        assert_eq!(mask.pixel_len(), 524_288);

        let model = up.read(MODEL).unwrap();
        assert_eq!(model.len(), 649_213);
        assert_eq!(Resource::parse(&model).unwrap().to_bytes(), model);
        assert_eq!(
            lod_fields(&model).unwrap(),
            (vec![7, 0, 0], vec![0.0, FAR, FAR])
        );
        let res = Resource::parse(&model).unwrap();
        let data = &res.block(b"DATA").unwrap().data;
        assert_eq!(u32_at(data, H_COMPRESSION).unwrap(), UNCOMPRESSED);
    }

    #[test]
    fn stock_fixture_decodes_to_the_red2_lod_values() {
        let stock = stock_model();
        let res = Resource::parse(&stock).unwrap();
        let data = &res.block(b"DATA").unwrap().data;
        assert_eq!(u32_at(data, H_COMPRESSION).unwrap(), LZ4);
        assert_eq!(
            lod_fields(&stock).unwrap(),
            (vec![1, 2, 4], vec![0.0, 12.0, 25.0])
        );
    }

    #[test]
    fn model_from_stock_matches_upstream_byte_for_byte() {
        let out = pin_lod0(&stock_model()).unwrap();
        assert_eq!(out, upstream().read(MODEL).unwrap());
    }

    #[test]
    fn mask_from_stock_matches_upstream_byte_for_byte() {
        let stock = stock_mask();
        assert_eq!(stock.len(), 701_196 + 16);
        let v = Vtex::parse(&stock).unwrap();
        assert_eq!((v.mips.len(), v.flags), (11, Flags(0)));
        assert_eq!(v.pixel_start() + v.pixel_len(), stock.len());
        assert_eq!(pin_top_mip(&stock).unwrap(), upstream().read(MASK).unwrap());
    }

    type Rebuild = fn(&[u8]) -> Result<Vec<u8>, AddonError>;

    #[test]
    fn rebuilding_is_idempotent() {
        let up = upstream();
        let rebuilds: [(&str, Rebuild); 2] = [(MASK, pin_top_mip), (MODEL, pin_lod0)];
        for (path, f) in rebuilds {
            let bytes = up.read(path).unwrap();
            assert_eq!(f(&bytes).unwrap(), bytes, "{path}");
        }
    }

    #[test]
    fn build_from_a_game_pak_passes_verify() {
        let game = VpkDir::in_memory(vpk::write(&BTreeMap::from([
            (MASK.to_string(), stock_mask()),
            (MODEL.to_string(), stock_model()),
            ("models/other.vmdl_c".to_string(), vec![1, 2, 3]),
        ])))
        .unwrap();
        let files = build(&game).unwrap();
        let up = upstream();
        assert_eq!(
            files.keys().collect::<Vec<_>>(),
            [MASK, MODEL].iter().collect::<Vec<_>>()
        );
        for (path, bytes) in &files {
            assert_eq!(bytes, &up.read(path).unwrap(), "{path}");
        }
        let pak = VpkDir::in_memory(vpk::write(&files)).unwrap();
        let verified = super::super::verify::verify(&pak, &Default::default());
        assert!(verified.is_ok(), "{verified}");
    }

    #[test]
    fn missing_game_file_is_an_error() {
        let game = VpkDir::in_memory(vpk::write(&BTreeMap::from([(
            MASK.to_string(),
            stock_mask(),
        )])))
        .unwrap();
        assert!(matches!(build(&game), Err(AddonError::Vpk(_))));
    }

    #[test]
    fn compressed_size_table_keeps_the_largest_entry() {
        let stored = [1000, 300, 128, 32, 8];
        let bytes = crate::texture::vtex::tests::synthetic(64, 5, &stored, (60, 50));
        let out = pin_top_mip(&bytes).unwrap();
        let v = Vtex::parse(&out).unwrap();
        assert_eq!(v.mips.len(), 1);
        assert_eq!(v.mips[0].stored_len, 1000);
        assert!(v.flags.contains(Flags::NO_LOD));
        assert_eq!(&out[v.pixel_start()..], &bytes[bytes.len() - 1000..]);
        assert!(
            out[v.pixel_start()..].iter().all(|&b| b == 0),
            "level 0 bytes kept"
        );
    }

    #[test]
    fn unsupported_inputs_are_errors_not_panics() {
        let stock = stock_model();
        let data = Resource::parse(&stock)
            .unwrap()
            .block(b"DATA")
            .unwrap()
            .data
            .clone();

        let mut zstd = data.clone();
        put_u32(&mut zstd, H_COMPRESSION, 2);
        let err = pin_lod0(&with_data(&stock, &zstd)).unwrap_err().to_string();
        assert!(err.contains("zstd"), "{err}");

        let mut cut = data.clone();
        cut.truncate(cut.len() - 10);
        assert!(pin_lod0(&with_data(&stock, &cut)).is_err());

        assert!(pin_lod0(&with_data(&stock, b"not kv3")).is_err());
        for at in 4..KV3_HEADER {
            let mut bad = STOCK_DATA.to_vec();
            bad[at] ^= 0xff;
            let _ = Kv3::decode(&bad).and_then(|kv| kv.lod_slots());
        }
        assert!(pin_top_mip(&[0; 64]).is_err());
        let mask = stock_mask();
        assert!(pin_top_mip(&mask[..mask.len() - 1]).is_err());
    }

    #[test]
    fn special_constant_lod_values_are_refused() {
        let mut kv = Kv3::decode(STOCK_DATA).unwrap();
        let constant = |ty| Slot {
            ty,
            buf: Buf::Two,
            at: 0,
        };
        assert_eq!(kv.int(&constant(T_INT64_ONE)).unwrap(), 1);
        assert_eq!(kv.float(&constant(T_DOUBLE_ZERO)).unwrap(), 0.0);
        assert!(kv.set_int(&constant(T_INT64_ONE), 7).is_err());
        assert!(kv.set_float(&constant(T_DOUBLE_ONE), FAR).is_err());
        assert!(kv.int(&constant(T_STRING)).is_err());
        let lod = kv.lod_slots().unwrap();
        assert!(kv.set_int(&lod.masks[0], u64::from(u32::MAX) + 1).is_err());
    }
}
