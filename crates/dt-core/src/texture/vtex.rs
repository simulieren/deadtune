//! Compiled Source 2 textures (`.vtex_c`): parse the header, and drop the largest mip
//! levels without decoding pixels.
//!
//! Layout (reference: ValveResourceFormat `Resource/ResourceTypes/Texture.cs`, `Read`,
//! `SkipMipmaps`, `CalculateBufferSizeForMipLevel`):
//! - Source 2 resource header (see `hud::resource`), blocks RED2 + DATA.
//! - DATA holds the 40-byte VTEX header (version 1, flags, reflectivity, width, height,
//!   depth, format, mip count, picmip, extra-data offset + count) then extra-data entries
//!   (type, offset, size), each pointing at its payload inside the block.
//! - Pixel data follows the DATA block, outside the declared resource size, with the
//!   SMALLEST mip first and the largest last. A COMPRESSED_MIP_SIZE entry lists each
//!   mip's stored size (LZ4 when smaller than the raw size).
//!
//! Dropping the n largest mips therefore means: patch width, height, mip count, the
//! size table and the display rect, then cut the tail. The kept bytes never move.

use std::fmt;

#[derive(Debug, thiserror::Error)]
pub enum VtexError {
    #[error("malformed vtex: {0}")]
    Malformed(&'static str),
    #[error("{0}")]
    Resource(#[from] crate::hud::resource::ResourceError),
}

/// Why a texture is left at full size. Shared with the addon builder's stats.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum SkipReason {
    /// Its category is not selected.
    NotSelected,
    /// Lighting data (lightmaps, cubemaps, probes) with `exclude_lighting` on.
    Lighting,
    /// Only one mip level, so there is nothing smaller to keep.
    NoMips,
    /// Already at or under the size floor.
    TooSmall,
    /// Cubemap, volume, array, NO_LOD, or a raw JPEG/PNG/WebP payload.
    Unsupported,
    /// Not a vtex_c we can parse.
    Malformed,
}

impl fmt::Display for SkipReason {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            SkipReason::NotSelected => "category not selected",
            SkipReason::Lighting => "lighting data",
            SkipReason::NoMips => "no mip chain",
            SkipReason::TooSmall => "at size floor",
            SkipReason::Unsupported => "unsupported format or flags",
            SkipReason::Malformed => "malformed",
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Flags(pub u16);

impl Flags {
    pub const NO_LOD: Flags = Flags(1 << 3);
    pub const CUBE: Flags = Flags(1 << 4);
    pub const VOLUME: Flags = Flags(1 << 5);
    pub const ARRAY: Flags = Flags(1 << 6);

    pub fn contains(self, other: Flags) -> bool {
        self.0 & other.0 == other.0
    }
}

/// How pixel bytes are laid out for one format.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Layout {
    /// 4x4 blocks of `bytes` each (BCn, ETC, EAC).
    Blocks(usize),
    /// One texel of `bytes` each.
    Pixels(usize),
    /// A JPEG/PNG/WebP container: no mip chain to cut.
    Encoded,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Format(pub u8);

impl Format {
    pub fn layout(self) -> Option<Layout> {
        use Layout::*;
        Some(match self.0 {
            1 => Blocks(8),   // DXT1
            2 => Blocks(16),  // DXT5
            3 => Pixels(1),   // I8
            4 => Pixels(4),   // RGBA8888
            5 => Pixels(2),   // R16
            6 => Pixels(4),   // RG1616
            7 => Pixels(8),   // RGBA16161616
            8 => Pixels(2),   // R16F
            9 => Pixels(4),   // RG1616F
            10 => Pixels(8),  // RGBA16161616F
            11 => Pixels(4),  // R32F
            12 => Pixels(8),  // RG3232F
            13 => Pixels(12), // RGB323232F
            14 => Pixels(16), // RGBA32323232F
            15..=18 => Encoded,
            19 => Blocks(16), // BC6H
            20 => Blocks(16), // BC7
            21 => Blocks(16), // ATI2N
            22 => Pixels(2),  // IA88
            23 => Blocks(8),  // ETC2
            24 => Blocks(16), // ETC2_EAC
            25 => Blocks(8),  // R11_EAC
            26 => Blocks(16), // RG11_EAC
            27 => Blocks(8),  // ATI1N
            28 => Pixels(4),  // BGRA8888
            29 | 30 => Encoded,
            _ => return None,
        })
    }

    pub fn name(self) -> &'static str {
        const NAMES: [&str; 31] = [
            "UNKNOWN",
            "DXT1",
            "DXT5",
            "I8",
            "RGBA8888",
            "R16",
            "RG1616",
            "RGBA16161616",
            "R16F",
            "RG1616F",
            "RGBA16161616F",
            "R32F",
            "RG3232F",
            "RGB323232F",
            "RGBA32323232F",
            "JPEG_RGBA8888",
            "PNG_RGBA8888",
            "JPEG_DXT5",
            "PNG_DXT5",
            "BC6H",
            "BC7",
            "ATI2N",
            "IA88",
            "ETC2",
            "ETC2_EAC",
            "R11_EAC",
            "RG11_EAC",
            "ATI1N",
            "BGRA8888",
            "WEBP_RGBA8888",
            "WEBP_DXT5",
        ];
        NAMES.get(self.0 as usize).copied().unwrap_or("UNKNOWN")
    }
}

/// One mip level. Index 0 in `Vtex::mips` is the largest; it is stored last in the file.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Mip {
    pub width: u16,
    pub height: u16,
    /// Bytes in the file (LZ4-compressed size when the table says so, else raw size).
    pub stored_len: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct CompressedTable {
    /// Offset of the extra-data entry's `size` field.
    entry_size_at: usize,
    /// Offset of the `count` field inside the payload.
    count_at: usize,
    /// Offset of the first i32 of the per-mip array.
    array_at: usize,
    count: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Vtex {
    pub flags: Flags,
    pub width: u16,
    pub height: u16,
    pub depth: u16,
    pub format: Format,
    pub mips: Vec<Mip>,
    /// Sub-rect the engine displays, from the METADATA entry, when set.
    pub display_rect: Option<(u16, u16)>,
    /// Offset of the VTEX header (start of the DATA block).
    header_at: usize,
    /// Offset of the first pixel byte (end of the DATA block).
    pixel_start: usize,
    compressed: Option<CompressedTable>,
    /// Offset of the METADATA display rect (width u16, height u16).
    display_rect_at: Option<usize>,
}

const VTEX_HEADER: usize = 40;
const DIM_AT: usize = 20;
const EXTRA_METADATA: u32 = 3;
const EXTRA_COMPRESSED_MIP_SIZE: u32 = 4;

fn u16_at(b: &[u8], at: usize) -> Result<u16, VtexError> {
    let s = b.get(at..at + 2).ok_or(VtexError::Malformed("truncated"))?;
    Ok(u16::from_le_bytes([s[0], s[1]]))
}

fn u32_at(b: &[u8], at: usize) -> Result<u32, VtexError> {
    let s = b.get(at..at + 4).ok_or(VtexError::Malformed("truncated"))?;
    Ok(u32::from_le_bytes([s[0], s[1], s[2], s[3]]))
}

/// Dimension of mip `level`, never below 1.
pub fn mip_dim(dim: u16, level: u8) -> u16 {
    (dim >> level).max(1)
}

/// Raw (uncompressed) size of one mip, as the engine computes it.
pub fn raw_mip_len(width: u16, height: u16, depth: u16, layout: Layout) -> usize {
    let (w, h, d) = (width as usize, height as usize, depth as usize);
    match layout {
        Layout::Pixels(bytes) => w * h * d * bytes,
        Layout::Blocks(bytes) => {
            let w = w.div_ceil(4).max(1);
            let h = h.div_ceil(4).max(1);
            let d = if (2..4).contains(&d) { 4 } else { d };
            w * h * d * bytes
        }
        Layout::Encoded => 0,
    }
}

impl Vtex {
    pub fn parse(bytes: &[u8]) -> Result<Vtex, VtexError> {
        let (header_at, data_len) = data_block(bytes)?;
        let pixel_start = header_at + data_len;
        if data_len < VTEX_HEADER {
            return Err(VtexError::Malformed(
                "DATA block shorter than the VTEX header",
            ));
        }
        if u16_at(bytes, header_at)? != 1 {
            return Err(VtexError::Malformed("vtex version is not 1"));
        }
        let flags = Flags(u16_at(bytes, header_at + 2)?);
        let width = u16_at(bytes, header_at + DIM_AT)?;
        let height = u16_at(bytes, header_at + DIM_AT + 2)?;
        let depth = u16_at(bytes, header_at + DIM_AT + 4)?;
        let format = Format(bytes[header_at + DIM_AT + 6]);
        let mip_count = bytes[header_at + DIM_AT + 7] as usize;
        let extra_off = u32_at(bytes, header_at + 32)? as usize;
        let extra_count = u32_at(bytes, header_at + 36)? as usize;

        let mut compressed = None;
        let mut display_rect = None;
        let mut display_rect_at = None;
        let mut sizes: Option<Vec<usize>> = None;
        if extra_count > 0 {
            let mut entry = header_at + 32 + extra_off;
            for _ in 0..extra_count {
                let kind = u32_at(bytes, entry)?;
                // The offset is relative to its own field (VRF reads `offset - 8` past the
                // 12-byte entry): FALLBACK_BITS right after a one-entry table has offset 8.
                let payload = entry + 4 + u32_at(bytes, entry + 4)? as usize;
                let size = u32_at(bytes, entry + 8)? as usize;
                if payload + size > pixel_start {
                    return Err(VtexError::Malformed("extra data outside the DATA block"));
                }
                match kind {
                    EXTRA_METADATA => {
                        let w = u16_at(bytes, payload + 2)?;
                        let h = u16_at(bytes, payload + 4)?;
                        display_rect_at = Some(payload + 2);
                        if w > 0 && h > 0 {
                            display_rect = Some((w, h));
                        }
                    }
                    EXTRA_COMPRESSED_MIP_SIZE => {
                        let array_at = payload + 4 + u32_at(bytes, payload + 4)? as usize;
                        let count = u32_at(bytes, payload + 8)? as usize;
                        if count != mip_count {
                            return Err(VtexError::Malformed("size table length != mip count"));
                        }
                        let mut v = Vec::with_capacity(count);
                        for i in 0..count {
                            v.push(u32_at(bytes, array_at + 4 * i)? as usize);
                        }
                        sizes = Some(v);
                        compressed = Some(CompressedTable {
                            entry_size_at: entry + 8,
                            count_at: payload + 8,
                            array_at,
                            count,
                        });
                    }
                    _ => {}
                }
                entry += 12;
            }
        }

        let layout = format.layout();
        let mips = (0..mip_count)
            .map(|level| {
                let level = level as u8;
                let (w, h) = (mip_dim(width, level), mip_dim(height, level));
                let d = if flags.contains(Flags::VOLUME) {
                    mip_dim(depth, level)
                } else if flags.contains(Flags::CUBE) {
                    depth * 6
                } else {
                    depth
                };
                let raw = layout.map_or(0, |l| raw_mip_len(w, h, d, l));
                let stored_len = match &sizes {
                    Some(s) => raw.min(s[level as usize]),
                    None => raw,
                };
                Mip {
                    width: w,
                    height: h,
                    stored_len,
                }
            })
            .collect();

        Ok(Vtex {
            flags,
            width,
            height,
            depth,
            format,
            mips,
            display_rect,
            header_at,
            pixel_start,
            compressed,
            display_rect_at,
        })
    }

    /// Bytes of pixel data the header accounts for (the file should end exactly here).
    pub fn pixel_len(&self) -> usize {
        self.mips.iter().map(|m| m.stored_len).sum()
    }

    pub fn pixel_start(&self) -> usize {
        self.pixel_start
    }

    /// How many of the largest mips can go while the largest kept side stays at or
    /// above `min_side`, capped at `want`. `Err` says why none can.
    pub fn reducible_levels(&self, want: u8, min_side: u16) -> Result<u8, SkipReason> {
        let layout = self.format.layout().ok_or(SkipReason::Unsupported)?;
        if layout == Layout::Encoded
            || self.depth != 1
            || self.flags.contains(Flags::CUBE)
            || self.flags.contains(Flags::VOLUME)
            || self.flags.contains(Flags::ARRAY)
            || self.flags.contains(Flags::NO_LOD)
        {
            return Err(SkipReason::Unsupported);
        }
        if self.mips.len() < 2 {
            return Err(SkipReason::NoMips);
        }
        let fits = |level: usize| {
            let m = self.mips[level];
            m.width.max(m.height) >= min_side
        };
        let max = (1..self.mips.len()).take_while(|&l| fits(l)).count() as u8;
        let levels = want.min(max);
        if levels == 0 {
            return Err(SkipReason::TooSmall);
        }
        Ok(levels)
    }

    /// `bytes` with the `levels` largest mips removed. Caller checks
    /// `reducible_levels` first; `levels` must be below the mip count.
    pub fn strip(&self, bytes: &[u8], levels: u8) -> Vec<u8> {
        let n = levels as usize;
        assert!(n < self.mips.len(), "strip: not enough mips");
        let cut: usize = self.mips[..n].iter().map(|m| m.stored_len).sum();
        let mut out = bytes[..bytes.len() - cut].to_vec();
        let at = self.header_at + DIM_AT;
        out[at..at + 2].copy_from_slice(&mip_dim(self.width, levels).to_le_bytes());
        out[at + 2..at + 4].copy_from_slice(&mip_dim(self.height, levels).to_le_bytes());
        out[at + 7] = (self.mips.len() - n) as u8;
        if let (Some((w, h)), Some(at)) = (self.display_rect, self.display_rect_at) {
            out[at..at + 2].copy_from_slice(&mip_dim(w, levels).to_le_bytes());
            out[at + 2..at + 4].copy_from_slice(&mip_dim(h, levels).to_le_bytes());
        }
        if let Some(t) = self.compressed {
            let keep = t.count - n;
            out.copy_within(t.array_at + 4 * n..t.array_at + 4 * t.count, t.array_at);
            out[t.array_at + 4 * keep..t.array_at + 4 * t.count].fill(0);
            out[t.count_at..t.count_at + 4].copy_from_slice(&(keep as u32).to_le_bytes());
            let size = u32::from_le_bytes(
                out[t.entry_size_at..t.entry_size_at + 4]
                    .try_into()
                    .unwrap(),
            );
            if size as usize == 12 + 4 * t.count {
                out[t.entry_size_at..t.entry_size_at + 4]
                    .copy_from_slice(&(size - 4 * n as u32).to_le_bytes());
            }
        }
        out
    }
}

/// (offset, length) of the DATA block, from the resource block table.
fn data_block(bytes: &[u8]) -> Result<(usize, usize), VtexError> {
    let table_off = u32_at(bytes, 8)? as usize;
    let count = u32_at(bytes, 12)? as usize;
    let table = 8 + table_off;
    for i in 0..count {
        let entry = table + 12 * i;
        if bytes.get(entry..entry + 4) == Some(b"DATA") {
            let start = entry + 4 + u32_at(bytes, entry + 4)? as usize;
            let len = u32_at(bytes, entry + 8)? as usize;
            if start + len > bytes.len() {
                return Err(VtexError::Malformed("DATA block past end of file"));
            }
            return Ok((start, len));
        }
    }
    Err(VtexError::Malformed("no DATA block"))
}

/// `bytes` with the `levels` largest mips removed, or `None` when the texture has no
/// mip chain, is already at the floor, or has a layout we do not touch (cubemaps,
/// volumes, arrays, NO_LOD, raw image payloads). Never grows the file.
pub fn downscale(bytes: &[u8], levels: u8) -> Result<Option<Vec<u8>>, VtexError> {
    let v = Vtex::parse(bytes)?;
    if v.pixel_start + v.pixel_len() != bytes.len() {
        return Err(VtexError::Malformed(
            "mip sizes do not match the file length",
        ));
    }
    match v.reducible_levels(levels, 1) {
        Ok(got) if got == levels => Ok(Some(v.strip(bytes, levels))),
        _ => Ok(None),
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    pub const MASK: &[u8] =
        include_bytes!("../../tests/fixtures/texture/mask_512_dxt1_8mips.vtex_c");
    pub const COLOR: &[u8] =
        include_bytes!("../../tests/fixtures/texture/color_512_bc7_8mips.vtex_c");
    pub const TINY: &[u8] =
        include_bytes!("../../tests/fixtures/texture/selfillum_4x4_ati1n_1mip.vtex_c");
    pub const NOLOD: &[u8] =
        include_bytes!("../../tests/fixtures/texture/emissive_1024_ati1n_nolod_1mip.vtex_c");

    #[test]
    fn parses_fixtures() {
        let cases = [
            (MASK, 512, "DXT1", 8, Flags(0), None),
            (COLOR, 512, "BC7", 8, Flags(0), None),
            (TINY, 4, "ATI1N", 1, Flags(0), Some((1, 1))),
            (NOLOD, 1024, "ATI1N", 1, Flags::NO_LOD, None),
        ];
        for (bytes, side, fmt, mips, flags, rect) in cases {
            let v = Vtex::parse(bytes).unwrap();
            assert_eq!((v.width, v.height, v.depth), (side, side, 1), "{fmt}");
            assert_eq!(v.format.name(), fmt);
            assert_eq!(v.mips.len(), mips);
            assert_eq!(v.flags, flags);
            assert_eq!(v.display_rect, rect, "{fmt}: a 1x1 source padded to 4x4");
            assert_eq!(v.pixel_start + v.pixel_len(), bytes.len(), "{fmt} sizes");
            assert_eq!(v.mips[0].width, side);
            assert_eq!(v.mips.last().unwrap().width, mip_dim(side, mips as u8 - 1));
        }
        let v = Vtex::parse(TINY).unwrap();
        assert_eq!(v.pixel_len(), 8);
        assert_eq!(
            v.display_rect_at,
            Some(v.header_at + 40 + 2 * 12 + 1024 + 2),
            "FILL_TO_POW2 payload sits after both entries and the 1024-byte fallback bits"
        );
    }

    /// The research scope texture: one FALLBACK_BITS entry whose offset field is 8, so its
    /// 1024-byte payload starts 12 bytes after the entry and ends exactly at the pixels.
    #[test]
    fn extra_data_payload_offset_is_relative_to_its_field() {
        use crate::addons::sources::tests::research;
        use crate::hud::vpk::VpkDir;
        let up = VpkDir::open(&research("Vindicta Scope Downscale", "pak89_dir.vpk"))
            .unwrap()
            .read(crate::addons::native_scope::TEXTURE)
            .unwrap();
        let v = Vtex::parse(&up).unwrap();
        let entry = v.header_at + 32 + u32_at(&up, v.header_at + 32).unwrap() as usize;
        assert_eq!(u32_at(&up, entry).unwrap(), 1, "FALLBACK_BITS");
        assert_eq!(u32_at(&up, entry + 4).unwrap(), 8);
        assert_eq!(u32_at(&up, entry + 8).unwrap(), 1024);
        assert_eq!(entry + 4 + 8 + 1024, v.pixel_start());
        let mut moved = up.clone();
        moved[entry..entry + 4].copy_from_slice(&EXTRA_METADATA.to_le_bytes());
        let at = entry + 12 + 2;
        moved[at..at + 4].copy_from_slice(&[7, 0, 9, 0]);
        assert_eq!(Vtex::parse(&moved).unwrap().display_rect, Some((7, 9)));
    }

    #[test]
    fn mip_len_rules() {
        assert_eq!(raw_mip_len(512, 512, 1, Layout::Blocks(8)), 131072);
        assert_eq!(raw_mip_len(1, 1, 1, Layout::Blocks(16)), 16);
        assert_eq!(raw_mip_len(6, 6, 1, Layout::Blocks(8)), 32);
        assert_eq!(raw_mip_len(1080, 1080, 1, Layout::Pixels(4)), 4665600);
        assert_eq!(raw_mip_len(8, 8, 1, Layout::Encoded), 0);
        assert_eq!(mip_dim(512, 10), 1);
    }

    #[test]
    fn reducible_levels_rules() {
        let v = Vtex::parse(COLOR).unwrap();
        assert_eq!(v.reducible_levels(1, 1), Ok(1));
        assert_eq!(v.reducible_levels(2, 128), Ok(2));
        assert_eq!(v.reducible_levels(2, 256), Ok(1));
        assert_eq!(v.reducible_levels(2, 512), Err(SkipReason::TooSmall));
        assert_eq!(v.reducible_levels(9, 1), Ok(7));
        assert_eq!(
            Vtex::parse(TINY).unwrap().reducible_levels(1, 1),
            Err(SkipReason::NoMips)
        );
        assert_eq!(
            Vtex::parse(NOLOD).unwrap().reducible_levels(1, 1),
            Err(SkipReason::Unsupported)
        );
        let mut cube = v.clone();
        cube.flags = Flags::CUBE;
        assert_eq!(cube.reducible_levels(1, 1), Err(SkipReason::Unsupported));
        let mut png = v.clone();
        png.format = Format(16);
        assert_eq!(png.reducible_levels(1, 1), Err(SkipReason::Unsupported));
    }

    #[test]
    fn downscale_keeps_small_mips_byte_identical() {
        for (bytes, fmt) in [(MASK, "DXT1"), (COLOR, "BC7")] {
            let orig = Vtex::parse(bytes).unwrap();
            for levels in 1..=2u8 {
                let out = downscale(bytes, levels).unwrap().expect(fmt);
                assert!(out.len() < bytes.len());
                let v = Vtex::parse(&out).unwrap();
                assert_eq!(v.width, orig.width >> levels);
                assert_eq!(v.height, orig.height >> levels);
                assert_eq!(v.mips.len(), orig.mips.len() - levels as usize);
                assert_eq!(v.mips[..], orig.mips[levels as usize..]);
                assert_eq!(v.pixel_start() + v.pixel_len(), out.len());
                assert_eq!(
                    &out[v.pixel_start()..],
                    &bytes[orig.pixel_start()..orig.pixel_start() + v.pixel_len()]
                );
                assert_eq!(
                    &out[..orig.header_at + DIM_AT],
                    &bytes[..orig.header_at + DIM_AT]
                );
                let res = crate::hud::resource::Resource::parse(&out).unwrap();
                assert_eq!(
                    res.block(b"RED2"),
                    crate::hud::resource::Resource::parse(bytes)
                        .unwrap()
                        .block(b"RED2")
                );
            }
        }
    }

    #[test]
    fn downscale_declines() {
        assert_eq!(downscale(TINY, 1).unwrap(), None);
        assert_eq!(downscale(NOLOD, 1).unwrap(), None);
        assert_eq!(downscale(MASK, 8).unwrap(), None);
        assert!(downscale(&MASK[..100], 1).is_err());
        let mut short = MASK.to_vec();
        short.pop();
        assert!(downscale(&short, 1).is_err());
    }

    /// A vtex_c built from scratch: `width` square, DXT1, `mips` levels, a
    /// COMPRESSED_MIP_SIZE table (`stored` per level, largest first) and a METADATA
    /// display rect. Pixel bytes are the level number repeated.
    pub fn synthetic(width: u16, mips: u8, stored: &[u32], rect: (u16, u16)) -> Vec<u8> {
        use crate::hud::resource::{Block, Resource};
        let mut d = Vec::new();
        d.extend_from_slice(&1u16.to_le_bytes());
        d.extend_from_slice(&0u16.to_le_bytes());
        d.extend_from_slice(&[0u8; 16]);
        d.extend_from_slice(&width.to_le_bytes());
        d.extend_from_slice(&width.to_le_bytes());
        d.extend_from_slice(&1u16.to_le_bytes());
        d.push(1);
        d.push(mips);
        d.extend_from_slice(&0u32.to_le_bytes());
        d.extend_from_slice(&8u32.to_le_bytes());
        d.extend_from_slice(&2u32.to_le_bytes());
        let entries = d.len();
        let meta_payload = entries + 24;
        let comp_payload = meta_payload + 128;
        for (kind, payload, size) in [
            (EXTRA_METADATA, meta_payload, 128u32),
            (
                EXTRA_COMPRESSED_MIP_SIZE,
                comp_payload,
                12 + 4 * mips as u32,
            ),
        ] {
            let at = d.len();
            d.extend_from_slice(&kind.to_le_bytes());
            d.extend_from_slice(&((payload - at - 4) as u32).to_le_bytes());
            d.extend_from_slice(&size.to_le_bytes());
        }
        d.extend_from_slice(&0u16.to_le_bytes());
        d.extend_from_slice(&rect.0.to_le_bytes());
        d.extend_from_slice(&rect.1.to_le_bytes());
        d.resize(comp_payload, 0);
        d.extend_from_slice(&1u32.to_le_bytes());
        d.extend_from_slice(&8u32.to_le_bytes());
        d.extend_from_slice(&(mips as u32).to_le_bytes());
        for s in stored {
            d.extend_from_slice(&s.to_le_bytes());
        }
        let res = Resource {
            header_version: 12,
            type_version: 0,
            blocks: vec![
                Block {
                    name: *b"RED2",
                    data: vec![0; 8],
                },
                Block {
                    name: *b"DATA",
                    data: d,
                },
            ],
        };
        let mut out = res.to_bytes();
        for level in (0..mips).rev() {
            let raw = raw_mip_len(
                mip_dim(width, level),
                mip_dim(width, level),
                1,
                Layout::Blocks(8),
            );
            let n = raw.min(stored[level as usize] as usize);
            out.extend(std::iter::repeat_n(level, n));
        }
        out
    }

    #[test]
    fn compressed_table_and_display_rect() {
        let stored = [1000, 300, 128, 32, 8];
        let bytes = synthetic(64, 5, &stored, (60, 50));
        let v = Vtex::parse(&bytes).unwrap();
        assert_eq!(v.display_rect, Some((60, 50)));
        assert_eq!(
            v.mips.iter().map(|m| m.stored_len).collect::<Vec<_>>(),
            [1000, 300, 128, 32, 8]
        );
        assert_eq!(v.pixel_start() + v.pixel_len(), bytes.len());

        let out = downscale(&bytes, 2).unwrap().unwrap();
        let w = Vtex::parse(&out).unwrap();
        assert_eq!((w.width, w.height, w.mips.len()), (16, 16, 3));
        assert_eq!(w.display_rect, Some((15, 12)));
        assert_eq!(
            w.mips.iter().map(|m| m.stored_len).collect::<Vec<_>>(),
            [128, 32, 8]
        );
        assert_eq!(w.pixel_start() + w.pixel_len(), out.len());
        assert_eq!(
            &out[w.pixel_start()..],
            &bytes[v.pixel_start()..v.pixel_start() + 168]
        );
        let t = v.compressed.unwrap();
        let size = u32_at(&out, t.entry_size_at).unwrap();
        assert_eq!(size, 12 + 4 * 3);
        assert_eq!(u32_at(&out, t.count_at).unwrap(), 3);
        assert_eq!(&out[t.array_at + 12..t.array_at + 20], &[0; 8]);
        assert_eq!(out.len(), bytes.len() - 1300);
    }

    #[test]
    fn length_mismatch_is_malformed() {
        let mut bytes = synthetic(64, 5, &[1000, 300, 128, 32, 8], (0, 0));
        bytes.push(0);
        assert!(downscale(&bytes, 1).is_err());
        let mut count_off = synthetic(64, 5, &[1000, 300, 128, 32, 8], (0, 0));
        let v = Vtex::parse(&count_off).unwrap();
        let t = v.compressed.unwrap();
        count_off[t.count_at] = 4;
        assert!(matches!(
            Vtex::parse(&count_off),
            Err(VtexError::Malformed(_))
        ));
    }

    /// Re-derives the committed 512px fixtures from the full-size research textures.
    /// Needs the research VPKs, so it runs only with `--ignored`.
    #[test]
    #[ignore]
    fn research_originals() {
        use crate::hud::vpk::VpkDir;
        let base = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../research/configs/OptimizationLock/Various Addons Relating to Performance");
        let vpk = VpkDir::open(&base.join("Optimized Soul Container/pak01_dir.vpk")).unwrap();
        let dir = "models/props_gameplay/soul_container/materials";
        for (name, fixture) in [
            ("soul_container_mask_png_df842e61_png_21812aec", MASK),
            ("soul_container_color_png_75e69035", COLOR),
        ] {
            let full = vpk.read(&format!("{dir}/{name}.vtex_c")).unwrap();
            let v = Vtex::parse(&full).unwrap();
            assert_eq!((v.width, v.mips.len()), (2048, 10), "{name}");
            assert_eq!(v.pixel_start() + v.pixel_len(), full.len());
            assert_eq!(downscale(&full, 2).unwrap().unwrap(), fixture, "{name}");
        }
        for path in vpk.entries.keys().filter(|p| p.ends_with(".vtex_c")) {
            let full = vpk.read(path).unwrap();
            let v = Vtex::parse(&full).unwrap();
            assert_eq!(v.pixel_start() + v.pixel_len(), full.len(), "{path}");
        }
    }
}
