//! Compiled textures to RGBA8 pixels: the top mip, any mip, or a thumbnail.
//!
//! Block formats follow the D3D11 BCn specs (ValveResourceFormat's `TextureDecoders`
//! are the cross-check); the PNG formats carry a whole PNG file as their pixel payload.

use std::borrow::Cow;

use crate::texture::png;
use crate::texture::resample::Image;
use crate::texture::vtex::{Flags, Format, Layout, Mip, Vtex, VtexError, mip_dim, raw_mip_len};

#[derive(Debug, thiserror::Error)]
pub enum DecodeError {
    #[error(transparent)]
    Vtex(#[from] VtexError),
    #[error("{format} textures are not supported ({reason})")]
    Unsupported {
        format: &'static str,
        reason: &'static str,
    },
    #[error("mip {level} does not exist ({mips} mips)")]
    NoMip { level: u8, mips: usize },
    #[error("pixel data is shorter than the header says")]
    Truncated,
    #[error(transparent)]
    Lz4(#[from] crate::lz4::Lz4Error),
    #[error("png: {0}")]
    Png(String),
}

/// RGBA8, rows top-down, no padding.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RgbaImage {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u8>,
}

impl RgbaImage {
    /// Transparent black.
    pub fn new(width: u32, height: u32) -> RgbaImage {
        RgbaImage {
            width,
            height,
            pixels: vec![0; width as usize * height as usize * 4],
        }
    }

    pub fn pixel(&self, x: u32, y: u32) -> [u8; 4] {
        let i = (y as usize * self.width as usize + x as usize) * 4;
        self.pixels[i..i + 4].try_into().expect("in bounds")
    }

    /// The image area-resampled so its longer side is `max_side`, aspect kept; itself
    /// when already within.
    pub fn fit(&self, max_side: u32) -> RgbaImage {
        let long = self.width.max(self.height);
        if long <= max_side || max_side == 0 {
            return self.clone();
        }
        let scale = |d: u32| ((u64::from(d) * u64::from(max_side)) / u64::from(long)).max(1) as u32;
        let out = Image::new(self.width, self.height, 4, self.pixels.clone())
            .expect("pixel count matches")
            .resample(scale(self.width), scale(self.height));
        RgbaImage {
            width: out.width,
            height: out.height,
            pixels: out.data,
        }
    }
}

/// The largest mip, cropped to the display rect when the source was padded to a power
/// of two.
pub fn decode(bytes: &[u8]) -> Result<RgbaImage, DecodeError> {
    decode_mip(bytes, 0)
}

/// Mip `level` (0 is the largest), cropped like `decode`.
pub fn decode_mip(bytes: &[u8], level: u8) -> Result<RgbaImage, DecodeError> {
    let v = Vtex::parse(bytes)?;
    refuse_shape(&v)?;
    let codec = codec(v.format)?;
    let mips = v.mips.len();
    let mip = *v
        .mips
        .get(level as usize)
        .ok_or(DecodeError::NoMip { level, mips })?;
    if bytes.len() < v.pixel_start() + v.pixel_len() {
        return Err(DecodeError::Truncated);
    }
    let image = match codec {
        Codec::Png => {
            if level != 0 {
                return Err(DecodeError::NoMip { level, mips });
            }
            png::read(&bytes[v.pixel_start()..]).map_err(|e| DecodeError::Png(e.0))?
        }
        Codec::Pixels { bytes: n, to_rgba } => {
            let raw = mip_bytes(bytes, &v, level, Layout::Pixels(n))?;
            RgbaImage {
                width: u32::from(mip.width),
                height: u32::from(mip.height),
                pixels: raw.chunks_exact(n).flat_map(to_rgba).collect(),
            }
        }
        Codec::Blocks { bytes: n, decode } => {
            let raw = mip_bytes(bytes, &v, level, Layout::Blocks(n))?;
            from_blocks(mip, n, decode, &raw)
        }
    };
    Ok(crop(image, v.display_rect, level))
}

/// The smallest mip whose longer side is at least `max_side` (mip 0 when none is),
/// area-resampled down to `max_side` when still larger.
pub fn thumbnail(bytes: &[u8], max_side: u32) -> Result<RgbaImage, DecodeError> {
    let v = Vtex::parse(bytes)?;
    let covers = |m: &Mip| u32::from(m.width.max(m.height)) >= max_side;
    let level = v.mips.iter().rposition(covers).unwrap_or(0);
    Ok(decode_mip(bytes, level as u8)?.fit(max_side))
}

enum Codec {
    Pixels {
        bytes: usize,
        to_rgba: fn(&[u8]) -> [u8; 4],
    },
    /// `decode` fills one 4x4 block, row-major.
    Blocks {
        bytes: usize,
        decode: fn(&[u8], &mut [[u8; 4]; 16]),
    },
    Png,
}

fn codec(format: Format) -> Result<Codec, DecodeError> {
    let reason = match format.0 {
        1 => {
            return Ok(Codec::Blocks {
                bytes: 8,
                decode: bc1,
            });
        }
        2 => {
            return Ok(Codec::Blocks {
                bytes: 16,
                decode: bc3,
            });
        }
        3 => {
            return Ok(Codec::Pixels {
                bytes: 1,
                to_rgba: |p| [p[0], p[0], p[0], 255],
            });
        }
        4 => {
            return Ok(Codec::Pixels {
                bytes: 4,
                to_rgba: |p| [p[0], p[1], p[2], p[3]],
            });
        }
        16 | 18 => return Ok(Codec::Png),
        20 => {
            return Ok(Codec::Blocks {
                bytes: 16,
                decode: bc7,
            });
        }
        21 => {
            return Ok(Codec::Blocks {
                bytes: 16,
                decode: bc5,
            });
        }
        22 => {
            return Ok(Codec::Pixels {
                bytes: 2,
                to_rgba: |p| [p[0], p[0], p[0], p[1]],
            });
        }
        27 => {
            return Ok(Codec::Blocks {
                bytes: 8,
                decode: bc4,
            });
        }
        28 => {
            return Ok(Codec::Pixels {
                bytes: 4,
                to_rgba: |p| [p[2], p[1], p[0], p[3]],
            });
        }
        5..=14 => "16-bit or float channels",
        15 | 17 => "JPEG payload",
        19 => "HDR block format",
        23..=26 => "mobile ETC format",
        29 | 30 => "WebP payload",
        _ => "unknown format",
    };
    Err(DecodeError::Unsupported {
        format: format.name(),
        reason,
    })
}

fn refuse_shape(v: &Vtex) -> Result<(), DecodeError> {
    let reason = if v.flags.contains(Flags::CUBE) {
        "cubemap"
    } else if v.flags.contains(Flags::VOLUME) || v.depth != 1 {
        "volume"
    } else if v.flags.contains(Flags::ARRAY) {
        "array"
    } else {
        return Ok(());
    };
    Err(DecodeError::Unsupported {
        format: v.format.name(),
        reason,
    })
}

/// The raw bytes of one mip: mips sit smallest first after `pixel_start`, each LZ4-packed
/// exactly when it is stored shorter than its raw size.
fn mip_bytes<'a>(
    bytes: &'a [u8],
    v: &Vtex,
    level: u8,
    layout: Layout,
) -> Result<Cow<'a, [u8]>, DecodeError> {
    let mip = v.mips[level as usize];
    let raw = raw_mip_len(mip.width, mip.height, 1, layout);
    let smaller: usize = v.mips[level as usize + 1..]
        .iter()
        .map(|m| m.stored_len)
        .sum();
    let start = v.pixel_start() + smaller;
    let stored = &bytes[start..start + mip.stored_len];
    if mip.stored_len < raw {
        Ok(Cow::Owned(crate::lz4::decode_block(stored, raw)?))
    } else {
        Ok(Cow::Borrowed(stored))
    }
}

/// Decodes the ceil(w/4) x ceil(h/4) block grid and clips it to the mip's size.
fn from_blocks(
    mip: Mip,
    block_bytes: usize,
    decode: fn(&[u8], &mut [[u8; 4]; 16]),
    raw: &[u8],
) -> RgbaImage {
    let (w, h) = (u32::from(mip.width), u32::from(mip.height));
    let cols = w.div_ceil(4).max(1) as usize;
    let mut out = RgbaImage::new(w, h);
    let mut block = [[0u8; 4]; 16];
    for (i, src) in raw.chunks_exact(block_bytes).enumerate() {
        let (bx, by) = ((i % cols) as u32 * 4, (i / cols) as u32 * 4);
        decode(src, &mut block);
        for (t, px) in block.iter().enumerate() {
            let (x, y) = (bx + t as u32 % 4, by + t as u32 / 4);
            if x < w && y < h {
                let at = (y as usize * w as usize + x as usize) * 4;
                out.pixels[at..at + 4].copy_from_slice(px);
            }
        }
    }
    out
}

fn crop(img: RgbaImage, rect: Option<(u16, u16)>, level: u8) -> RgbaImage {
    let Some((rw, rh)) = rect else { return img };
    let w = u32::from(mip_dim(rw, level)).min(img.width);
    let h = u32::from(mip_dim(rh, level)).min(img.height);
    if (w, h) == (img.width, img.height) {
        return img;
    }
    let (src_stride, dst_stride) = (img.width as usize * 4, w as usize * 4);
    let mut out = RgbaImage::new(w, h);
    for y in 0..h as usize {
        out.pixels[y * dst_stride..(y + 1) * dst_stride]
            .copy_from_slice(&img.pixels[y * src_stride..y * src_stride + dst_stride]);
    }
    out
}

fn rgb565(c: u16) -> [u8; 3] {
    let (r, g, b) = ((c >> 11) as u8 & 31, (c >> 5) as u8 & 63, c as u8 & 31);
    [
        (r << 3) | (r >> 2),
        (g << 2) | (g >> 4),
        (b << 3) | (b >> 2),
    ]
}

/// One BC1 colour block. With `opaque` (inside BC3) the four-colour palette is used
/// whatever the endpoint order; otherwise `c0 <= c1` selects three colours plus
/// transparent black.
fn bc1_colours(block: &[u8], opaque: bool) -> [[u8; 4]; 16] {
    let c0 = u16::from_le_bytes([block[0], block[1]]);
    let c1 = u16::from_le_bytes([block[2], block[3]]);
    let (a, b) = (rgb565(c0), rgb565(c1));
    let mix = |wa: u16, wb: u16, div: u16| -> [u8; 4] {
        let ch = |i: usize| ((wa * u16::from(a[i]) + wb * u16::from(b[i])) / div) as u8;
        [ch(0), ch(1), ch(2), 255]
    };
    let palette = if opaque || c0 > c1 {
        [mix(1, 0, 1), mix(0, 1, 1), mix(2, 1, 3), mix(1, 2, 3)]
    } else {
        [mix(1, 0, 1), mix(0, 1, 1), mix(1, 1, 2), [0, 0, 0, 0]]
    };
    let idx = u32::from_le_bytes([block[4], block[5], block[6], block[7]]);
    std::array::from_fn(|i| palette[(idx >> (2 * i)) as usize & 3])
}

/// One BC4-style 8-byte block: two endpoints and 3-bit indices into an 8- or 6-step ramp.
fn alpha_block(block: &[u8]) -> [u8; 16] {
    let (a0, a1) = (u16::from(block[0]), u16::from(block[1]));
    let mut palette = [a0, a1, 0, 0, 0, 0, 0, 255];
    if a0 > a1 {
        for i in 1..7 {
            palette[i + 1] = ((7 - i as u16) * a0 + i as u16 * a1) / 7;
        }
    } else {
        for i in 1..5 {
            palette[i + 1] = ((5 - i as u16) * a0 + i as u16 * a1) / 5;
        }
    }
    let mut idx = 0u64;
    for (i, &b) in block[2..8].iter().enumerate() {
        idx |= u64::from(b) << (8 * i);
    }
    std::array::from_fn(|i| palette[(idx >> (3 * i)) as usize & 7] as u8)
}

fn bc1(block: &[u8], out: &mut [[u8; 4]; 16]) {
    *out = bc1_colours(block, false);
}

fn bc3(block: &[u8], out: &mut [[u8; 4]; 16]) {
    let alpha = alpha_block(&block[..8]);
    *out = bc1_colours(&block[8..], true);
    for (px, a) in out.iter_mut().zip(alpha) {
        px[3] = a;
    }
}

fn bc4(block: &[u8], out: &mut [[u8; 4]; 16]) {
    for (px, v) in out.iter_mut().zip(alpha_block(block)) {
        *px = [v, v, v, 255];
    }
}

fn bc5(block: &[u8], out: &mut [[u8; 4]; 16]) {
    let (r, g) = (alpha_block(&block[..8]), alpha_block(&block[8..]));
    for (i, px) in out.iter_mut().enumerate() {
        *px = [r[i], g[i], 0, 255];
    }
}

#[derive(Clone, Copy)]
enum PBits {
    None,
    /// One bit per endpoint.
    PerEndpoint,
    /// One bit shared by both endpoints of a subset.
    PerSubset,
}

#[derive(Clone, Copy)]
struct Bc7Mode {
    subsets: usize,
    partition_bits: u8,
    rotation_bits: u8,
    selection_bit: u8,
    colour_bits: u8,
    alpha_bits: u8,
    pbits: PBits,
    index_bits: u8,
    index2_bits: u8,
}

impl Bc7Mode {
    /// Columns: subsets, partition bits, rotation bits, index selection bit, colour bits,
    /// alpha bits, p-bits, index bits, secondary index bits.
    const fn row(r: (usize, u8, u8, u8, u8, u8, PBits, u8, u8)) -> Bc7Mode {
        Bc7Mode {
            subsets: r.0,
            partition_bits: r.1,
            rotation_bits: r.2,
            selection_bit: r.3,
            colour_bits: r.4,
            alpha_bits: r.5,
            pbits: r.6,
            index_bits: r.7,
            index2_bits: r.8,
        }
    }
}

const BC7_MODES: [Bc7Mode; 8] = [
    Bc7Mode::row((3, 4, 0, 0, 4, 0, PBits::PerEndpoint, 3, 0)),
    Bc7Mode::row((2, 6, 0, 0, 6, 0, PBits::PerSubset, 3, 0)),
    Bc7Mode::row((3, 6, 0, 0, 5, 0, PBits::None, 2, 0)),
    Bc7Mode::row((2, 6, 0, 0, 7, 0, PBits::PerEndpoint, 2, 0)),
    Bc7Mode::row((1, 0, 2, 1, 5, 6, PBits::None, 2, 3)),
    Bc7Mode::row((1, 0, 2, 0, 7, 8, PBits::None, 2, 2)),
    Bc7Mode::row((1, 0, 0, 0, 7, 7, PBits::PerEndpoint, 4, 0)),
    Bc7Mode::row((2, 6, 0, 0, 5, 5, PBits::PerEndpoint, 2, 0)),
];

const WEIGHTS2: [u32; 4] = [0, 21, 43, 64];
const WEIGHTS3: [u32; 8] = [0, 9, 18, 27, 37, 46, 55, 64];
const WEIGHTS4: [u32; 16] = [0, 4, 9, 13, 17, 21, 26, 30, 34, 38, 43, 47, 51, 55, 60, 64];

fn bc7_interpolate(e0: u32, e1: u32, index: u32, bits: u8) -> u8 {
    let w = match bits {
        2 => WEIGHTS2[index as usize],
        3 => WEIGHTS3[index as usize],
        _ => WEIGHTS4[index as usize],
    };
    (((64 - w) * e0 + w * e1 + 32) >> 6) as u8
}

/// Widens a `bits`-wide endpoint (p-bit already appended) to 8 bits by replication.
fn bc7_expand(v: u32, bits: u8) -> u32 {
    (v << (8 - bits)) | (v >> (2 * bits - 8))
}

/// Anchor texel of the second subset, per partition.
const BC7_ANCHOR_2: [u8; 64] = [
    15, 15, 15, 15, 15, 15, 15, 15, 15, 15, 15, 15, 15, 15, 15, 15, 15, 2, 8, 2, 2, 8, 8, 15, 2, 8,
    2, 2, 8, 8, 2, 2, 15, 15, 6, 8, 2, 8, 15, 15, 2, 8, 2, 2, 2, 15, 15, 6, 6, 2, 6, 8, 15, 15, 2,
    2, 15, 15, 15, 15, 15, 2, 2, 15,
];
/// Anchor texel of the second subset of a three-subset partition.
const BC7_ANCHOR_3_SECOND: [u8; 64] = [
    3, 3, 15, 15, 8, 3, 15, 15, 8, 8, 6, 6, 6, 5, 3, 3, 3, 3, 8, 15, 3, 3, 6, 10, 5, 8, 8, 6, 8, 5,
    15, 15, 8, 15, 3, 5, 6, 10, 8, 15, 15, 3, 15, 5, 15, 15, 15, 15, 3, 15, 5, 5, 5, 8, 5, 10, 5,
    10, 8, 13, 15, 12, 3, 3,
];
/// Anchor texel of the third subset.
const BC7_ANCHOR_3_THIRD: [u8; 64] = [
    15, 8, 8, 3, 15, 15, 3, 8, 15, 15, 15, 15, 15, 15, 15, 8, 15, 8, 15, 3, 15, 8, 15, 8, 3, 15, 6,
    10, 15, 15, 10, 8, 15, 3, 15, 10, 10, 8, 9, 10, 6, 15, 8, 15, 3, 6, 6, 8, 15, 3, 15, 15, 15,
    15, 15, 15, 15, 15, 15, 15, 3, 15, 15, 8,
];

#[rustfmt::skip]
const BC7_PARTITIONS_2: [[u8; 16]; 64] = [
    [0,0,1,1, 0,0,1,1, 0,0,1,1, 0,0,1,1], [0,0,0,1, 0,0,0,1, 0,0,0,1, 0,0,0,1],
    [0,1,1,1, 0,1,1,1, 0,1,1,1, 0,1,1,1], [0,0,0,1, 0,0,1,1, 0,0,1,1, 0,1,1,1],
    [0,0,0,0, 0,0,0,1, 0,0,0,1, 0,0,1,1], [0,0,1,1, 0,1,1,1, 0,1,1,1, 1,1,1,1],
    [0,0,0,1, 0,0,1,1, 0,1,1,1, 1,1,1,1], [0,0,0,0, 0,0,0,1, 0,0,1,1, 0,1,1,1],
    [0,0,0,0, 0,0,0,0, 0,0,0,1, 0,0,1,1], [0,0,1,1, 0,1,1,1, 1,1,1,1, 1,1,1,1],
    [0,0,0,0, 0,0,0,1, 0,1,1,1, 1,1,1,1], [0,0,0,0, 0,0,0,0, 0,0,0,1, 0,1,1,1],
    [0,0,0,1, 0,1,1,1, 1,1,1,1, 1,1,1,1], [0,0,0,0, 0,0,0,0, 1,1,1,1, 1,1,1,1],
    [0,0,0,0, 1,1,1,1, 1,1,1,1, 1,1,1,1], [0,0,0,0, 0,0,0,0, 0,0,0,0, 1,1,1,1],
    [0,0,0,0, 1,0,0,0, 1,1,1,0, 1,1,1,1], [0,1,1,1, 0,0,0,1, 0,0,0,0, 0,0,0,0],
    [0,0,0,0, 0,0,0,0, 1,0,0,0, 1,1,1,0], [0,1,1,1, 0,0,1,1, 0,0,0,1, 0,0,0,0],
    [0,0,1,1, 0,0,0,1, 0,0,0,0, 0,0,0,0], [0,0,0,0, 1,0,0,0, 1,1,0,0, 1,1,1,0],
    [0,0,0,0, 0,0,0,0, 1,0,0,0, 1,1,0,0], [0,1,1,1, 0,0,1,1, 0,0,1,1, 0,0,0,1],
    [0,0,1,1, 0,0,0,1, 0,0,0,1, 0,0,0,0], [0,0,0,0, 1,0,0,0, 1,0,0,0, 1,1,0,0],
    [0,1,1,0, 0,1,1,0, 0,1,1,0, 0,1,1,0], [0,0,1,1, 0,1,1,0, 0,1,1,0, 1,1,0,0],
    [0,0,0,1, 0,1,1,1, 1,1,1,0, 1,0,0,0], [0,0,0,0, 1,1,1,1, 1,1,1,1, 0,0,0,0],
    [0,1,1,1, 0,0,0,1, 1,0,0,0, 1,1,1,0], [0,0,1,1, 1,0,0,1, 1,0,0,1, 1,1,0,0],
    [0,1,0,1, 0,1,0,1, 0,1,0,1, 0,1,0,1], [0,0,0,0, 1,1,1,1, 0,0,0,0, 1,1,1,1],
    [0,1,0,1, 1,0,1,0, 0,1,0,1, 1,0,1,0], [0,0,1,1, 0,0,1,1, 1,1,0,0, 1,1,0,0],
    [0,0,1,1, 1,1,0,0, 0,0,1,1, 1,1,0,0], [0,1,0,1, 0,1,0,1, 1,0,1,0, 1,0,1,0],
    [0,1,1,0, 1,0,0,1, 0,1,1,0, 1,0,0,1], [0,1,0,1, 1,0,1,0, 1,0,1,0, 0,1,0,1],
    [0,1,1,1, 0,0,1,1, 1,1,0,0, 1,1,1,0], [0,0,0,1, 0,0,1,1, 1,1,0,0, 1,0,0,0],
    [0,0,1,1, 0,0,1,0, 0,1,0,0, 1,1,0,0], [0,0,1,1, 1,0,1,1, 1,1,0,1, 1,1,0,0],
    [0,1,1,0, 1,0,0,1, 1,0,0,1, 0,1,1,0], [0,0,1,1, 1,1,0,0, 1,1,0,0, 0,0,1,1],
    [0,1,1,0, 0,1,1,0, 1,0,0,1, 1,0,0,1], [0,0,0,0, 0,1,1,0, 0,1,1,0, 0,0,0,0],
    [0,1,0,0, 1,1,1,0, 0,1,0,0, 0,0,0,0], [0,0,1,0, 0,1,1,1, 0,0,1,0, 0,0,0,0],
    [0,0,0,0, 0,0,1,0, 0,1,1,1, 0,0,1,0], [0,0,0,0, 0,1,0,0, 1,1,1,0, 0,1,0,0],
    [0,1,1,0, 1,1,0,0, 1,0,0,1, 0,0,1,1], [0,0,1,1, 0,1,1,0, 1,1,0,0, 1,0,0,1],
    [0,1,1,0, 0,0,1,1, 1,0,0,1, 1,1,0,0], [0,0,1,1, 1,0,0,1, 1,1,0,0, 0,1,1,0],
    [0,1,1,0, 1,1,0,0, 1,1,0,0, 1,0,0,1], [0,1,1,0, 0,0,1,1, 0,0,1,1, 1,0,0,1],
    [0,1,1,1, 1,1,1,0, 1,0,0,0, 0,0,0,1], [0,0,0,1, 1,0,0,0, 1,1,1,0, 0,1,1,1],
    [0,0,0,0, 1,1,1,1, 0,0,1,1, 0,0,1,1], [0,0,1,1, 0,0,1,1, 1,1,1,1, 0,0,0,0],
    [0,0,1,0, 0,0,1,0, 1,1,1,0, 1,1,1,0], [0,1,0,0, 0,1,0,0, 0,1,1,1, 0,1,1,1],
];

#[rustfmt::skip]
const BC7_PARTITIONS_3: [[u8; 16]; 64] = [
    [0,0,1,1, 0,0,1,1, 0,2,2,1, 2,2,2,2], [0,0,0,1, 0,0,1,1, 2,2,1,1, 2,2,2,1],
    [0,0,0,0, 2,0,0,1, 2,2,1,1, 2,2,1,1], [0,2,2,2, 0,0,2,2, 0,0,1,1, 0,1,1,1],
    [0,0,0,0, 0,0,0,0, 1,1,2,2, 1,1,2,2], [0,0,1,1, 0,0,1,1, 0,0,2,2, 0,0,2,2],
    [0,0,2,2, 0,0,2,2, 1,1,1,1, 1,1,1,1], [0,0,1,1, 0,0,1,1, 2,2,1,1, 2,2,1,1],
    [0,0,0,0, 0,0,0,0, 1,1,1,1, 2,2,2,2], [0,0,0,0, 1,1,1,1, 1,1,1,1, 2,2,2,2],
    [0,0,0,0, 1,1,1,1, 2,2,2,2, 2,2,2,2], [0,0,1,2, 0,0,1,2, 0,0,1,2, 0,0,1,2],
    [0,1,1,2, 0,1,1,2, 0,1,1,2, 0,1,1,2], [0,1,2,2, 0,1,2,2, 0,1,2,2, 0,1,2,2],
    [0,0,1,1, 0,1,1,2, 1,1,2,2, 1,2,2,2], [0,0,1,1, 2,0,0,1, 2,2,0,0, 2,2,2,0],
    [0,0,0,1, 0,0,1,1, 0,1,1,2, 1,1,2,2], [0,1,1,1, 0,0,1,1, 2,0,0,1, 2,2,0,0],
    [0,0,0,0, 1,1,2,2, 1,1,2,2, 1,1,2,2], [0,0,2,2, 0,0,2,2, 0,0,2,2, 1,1,1,1],
    [0,1,1,1, 0,1,1,1, 0,2,2,2, 0,2,2,2], [0,0,0,1, 0,0,0,1, 2,2,2,1, 2,2,2,1],
    [0,0,0,0, 0,0,1,1, 0,1,2,2, 0,1,2,2], [0,0,0,0, 1,1,0,0, 2,2,1,0, 2,2,1,0],
    [0,1,2,2, 0,1,2,2, 0,0,1,1, 0,0,0,0], [0,0,1,2, 0,0,1,2, 1,1,2,2, 2,2,2,2],
    [0,1,1,0, 1,2,2,1, 1,2,2,1, 0,1,1,0], [0,0,0,0, 0,1,1,0, 1,2,2,1, 1,2,2,1],
    [0,0,2,2, 1,1,0,2, 1,1,0,2, 0,0,2,2], [0,1,1,0, 0,1,1,0, 2,0,0,2, 2,2,2,2],
    [0,0,1,1, 0,1,2,2, 0,1,2,2, 0,0,1,1], [0,0,0,0, 2,0,0,0, 2,2,1,1, 2,2,2,1],
    [0,0,0,0, 0,0,0,2, 1,1,2,2, 1,2,2,2], [0,2,2,2, 0,0,2,2, 0,0,1,2, 0,0,1,1],
    [0,0,1,1, 0,0,1,2, 0,0,2,2, 0,2,2,2], [0,1,2,0, 0,1,2,0, 0,1,2,0, 0,1,2,0],
    [0,0,0,0, 1,1,1,1, 2,2,2,2, 0,0,0,0], [0,1,2,0, 1,2,0,1, 2,0,1,2, 0,1,2,0],
    [0,1,2,0, 2,0,1,2, 1,2,0,1, 0,1,2,0], [0,0,1,1, 2,2,0,0, 1,1,2,2, 0,0,1,1],
    [0,0,1,1, 1,1,2,2, 2,2,0,0, 0,0,1,1], [0,1,0,1, 0,1,0,1, 2,2,2,2, 2,2,2,2],
    [0,0,0,0, 0,0,0,0, 2,1,2,1, 2,1,2,1], [0,0,2,2, 1,1,2,2, 0,0,2,2, 1,1,2,2],
    [0,0,2,2, 0,0,1,1, 0,0,2,2, 0,0,1,1], [0,2,2,0, 1,2,2,1, 0,2,2,0, 1,2,2,1],
    [0,1,0,1, 2,2,2,2, 2,2,2,2, 0,1,0,1], [0,0,0,0, 2,1,2,1, 2,1,2,1, 2,1,2,1],
    [0,1,0,1, 0,1,0,1, 0,1,0,1, 2,2,2,2], [0,2,2,2, 0,1,1,1, 0,2,2,2, 0,1,1,1],
    [0,0,0,2, 1,1,1,2, 0,0,0,2, 1,1,1,2], [0,0,0,0, 2,1,1,2, 2,1,1,2, 2,1,1,2],
    [0,2,2,2, 0,1,1,1, 0,1,1,1, 0,2,2,2], [0,0,0,2, 1,1,1,2, 1,1,1,2, 0,0,0,2],
    [0,1,1,0, 0,1,1,0, 0,1,1,0, 2,2,2,2], [0,0,0,0, 0,0,0,0, 2,1,1,2, 2,1,1,2],
    [0,1,1,0, 0,1,1,0, 2,2,2,2, 2,2,2,2], [0,0,2,2, 0,0,1,1, 0,0,1,1, 0,0,2,2],
    [0,0,2,2, 1,1,2,2, 1,1,2,2, 0,0,2,2], [0,0,0,0, 0,0,0,0, 0,0,0,0, 2,1,1,2],
    [0,0,0,2, 0,0,0,1, 0,0,0,2, 0,0,0,1], [0,2,2,2, 1,2,2,2, 0,2,2,2, 1,2,2,2],
    [0,1,0,1, 2,2,2,2, 2,2,2,2, 2,2,2,2], [0,1,1,1, 2,0,1,1, 2,2,0,1, 2,2,2,0],
];

fn bc7(block: &[u8], out: &mut [[u8; 4]; 16]) {
    let bits = u128::from_le_bytes(block.try_into().expect("16-byte block"));
    let Some(mode) = (0..8).find(|&m| (bits >> m) & 1 == 1) else {
        *out = [[0; 4]; 16];
        return;
    };
    let m = BC7_MODES[mode];
    let mut at = mode as u32 + 1;
    let mut read = |n: u8| -> u32 {
        let v = (bits >> at) & ((1u128 << n) - 1);
        at += u32::from(n);
        v as u32
    };
    let partition = read(m.partition_bits) as usize;
    let rotation = read(m.rotation_bits);
    let selection = read(m.selection_bit);

    let mut ends = [[[0u32; 4]; 2]; 3];
    for ch in 0..3 {
        for e in ends.iter_mut().take(m.subsets) {
            e[0][ch] = read(m.colour_bits);
            e[1][ch] = read(m.colour_bits);
        }
    }
    if m.alpha_bits > 0 {
        for e in ends.iter_mut().take(m.subsets) {
            e[0][3] = read(m.alpha_bits);
            e[1][3] = read(m.alpha_bits);
        }
    }
    let p_added = !matches!(m.pbits, PBits::None) as u8;
    for e in ends.iter_mut().take(m.subsets) {
        match m.pbits {
            PBits::None => {}
            PBits::PerEndpoint => {
                for end in e.iter_mut() {
                    let p = read(1);
                    for v in end.iter_mut() {
                        *v = (*v << 1) | p;
                    }
                }
            }
            PBits::PerSubset => {
                let p = read(1);
                for v in e.iter_mut().flatten() {
                    *v = (*v << 1) | p;
                }
            }
        }
        for end in e.iter_mut() {
            for v in end.iter_mut().take(3) {
                *v = bc7_expand(*v, m.colour_bits + p_added);
            }
            end[3] = if m.alpha_bits > 0 {
                bc7_expand(end[3], m.alpha_bits + p_added)
            } else {
                255
            };
        }
    }

    let subset_of = |t: usize| -> usize {
        match m.subsets {
            1 => 0,
            2 => BC7_PARTITIONS_2[partition][t] as usize,
            _ => BC7_PARTITIONS_3[partition][t] as usize,
        }
    };
    let anchor_of = |s: usize| -> usize {
        match (s, m.subsets) {
            (0, _) => 0,
            (1, 2) => BC7_ANCHOR_2[partition] as usize,
            (1, _) => BC7_ANCHOR_3_SECOND[partition] as usize,
            _ => BC7_ANCHOR_3_THIRD[partition] as usize,
        }
    };
    let mut idx = [0u32; 16];
    let mut idx2 = [0u32; 16];
    for (t, v) in idx.iter_mut().enumerate() {
        *v = read(m.index_bits - (anchor_of(subset_of(t)) == t) as u8);
    }
    if m.index2_bits > 0 {
        for (t, v) in idx2.iter_mut().enumerate() {
            *v = read(m.index2_bits - (t == 0) as u8);
        }
    }

    for (t, px) in out.iter_mut().enumerate() {
        let [e0, e1] = ends[subset_of(t)];
        let (ci, cbits, ai, abits) = match (m.index2_bits, selection) {
            (0, _) => (idx[t], m.index_bits, idx[t], m.index_bits),
            (_, 0) => (idx[t], m.index_bits, idx2[t], m.index2_bits),
            _ => (idx2[t], m.index2_bits, idx[t], m.index_bits),
        };
        for ch in 0..3 {
            px[ch] = bc7_interpolate(e0[ch], e1[ch], ci, cbits);
        }
        px[3] = if m.alpha_bits > 0 {
            bc7_interpolate(e0[3], e1[3], ai, abits)
        } else {
            255
        };
        if rotation > 0 {
            px.swap(rotation as usize - 1, 3);
        }
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::texture::png;
    use crate::texture::vtex::tests::{COLOR, MASK, NOLOD, TINY};

    /// A vtex_c with `mips` raw pixel buffers, largest first. With `pack`, every mip the
    /// test LZ4 encoder can shrink is stored compressed and a COMPRESSED_MIP_SIZE table
    /// lists the stored sizes. `rect` adds a FILL_TO_POW2 display rect.
    pub(crate) fn build_vtex(
        format: u8,
        width: u16,
        height: u16,
        flags: u16,
        mips: &[Vec<u8>],
        pack: bool,
        rect: Option<(u16, u16)>,
    ) -> Vec<u8> {
        use crate::hud::resource::{Block, Resource};
        let stored: Vec<Vec<u8>> = mips
            .iter()
            .map(|raw| {
                let enc = crate::lz4::tests::encode_block(raw);
                if pack && enc.len() < raw.len() {
                    enc
                } else {
                    raw.clone()
                }
            })
            .collect();
        let mut extras: Vec<(u32, Vec<u8>)> = Vec::new();
        if let Some((w, h)) = rect {
            let mut p = vec![0, 0];
            p.extend_from_slice(&w.to_le_bytes());
            p.extend_from_slice(&h.to_le_bytes());
            extras.push((3, p));
        }
        if pack {
            let mut p = Vec::new();
            p.extend_from_slice(&1u32.to_le_bytes());
            p.extend_from_slice(&8u32.to_le_bytes());
            p.extend_from_slice(&(mips.len() as u32).to_le_bytes());
            for s in &stored {
                p.extend_from_slice(&(s.len() as u32).to_le_bytes());
            }
            extras.push((4, p));
        }
        let mut d = Vec::new();
        d.extend_from_slice(&1u16.to_le_bytes());
        d.extend_from_slice(&flags.to_le_bytes());
        d.extend_from_slice(&[0u8; 16]);
        d.extend_from_slice(&width.to_le_bytes());
        d.extend_from_slice(&height.to_le_bytes());
        d.extend_from_slice(&1u16.to_le_bytes());
        d.push(format);
        d.push(mips.len() as u8);
        d.extend_from_slice(&0u32.to_le_bytes());
        d.extend_from_slice(&if extras.is_empty() { 0u32 } else { 8 }.to_le_bytes());
        d.extend_from_slice(&(extras.len() as u32).to_le_bytes());
        let mut payload_at = d.len() + 12 * extras.len();
        for (kind, payload) in &extras {
            let at = d.len();
            d.extend_from_slice(&kind.to_le_bytes());
            d.extend_from_slice(&((payload_at - at - 4) as u32).to_le_bytes());
            d.extend_from_slice(&(payload.len() as u32).to_le_bytes());
            payload_at += payload.len();
        }
        for (_, payload) in &extras {
            d.extend_from_slice(payload);
        }
        let res = Resource {
            header_version: 12,
            type_version: 1,
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
        for s in stored.iter().rev() {
            out.extend_from_slice(s);
        }
        out
    }

    fn decode_one(format: u8, width: u16, height: u16, pixels: &[u8]) -> RgbaImage {
        decode(&build_vtex(
            format,
            width,
            height,
            0,
            &[pixels.to_vec()],
            false,
            None,
        ))
        .unwrap()
    }

    fn px(img: &RgbaImage) -> Vec<[u8; 4]> {
        img.pixels.as_chunks::<4>().0.to_vec()
    }

    /// Packs `(value, bit count)` fields LSB first into one 128-bit block.
    fn block128(fields: &[(u32, u8)]) -> [u8; 16] {
        let mut bits = 0u128;
        let mut at = 0;
        for &(v, n) in fields {
            assert!(n == 32 || v < 1 << n, "{v} does not fit {n} bits");
            bits |= u128::from(v) << at;
            at += u32::from(n);
        }
        assert_eq!(at, 128, "a block is 128 bits");
        bits.to_le_bytes()
    }

    /// 3-bit indices for 16 texels as the 6 index bytes of a BC4-style block.
    fn pack3(indices: [u8; 16]) -> [u8; 6] {
        let mut bits = 0u64;
        for (i, &v) in indices.iter().enumerate() {
            bits |= u64::from(v) << (3 * i);
        }
        bits.to_le_bytes()[..6].try_into().unwrap()
    }

    const RED565: [u8; 2] = [0x00, 0xF8];
    const BLACK565: [u8; 2] = [0x00, 0x00];
    /// r5 = 16, which replicates to 132.
    const HALF_RED565: [u8; 2] = [0x00, 0x80];
    /// Every row: indices 0, 1, 2, 3.
    const RAMP_IDX: [u8; 4] = [0xE4; 4];

    fn bc1_block(c0: [u8; 2], c1: [u8; 2]) -> Vec<u8> {
        [&c0[..], &c1[..], &RAMP_IDX[..]].concat()
    }

    #[test]
    fn pixel_codecs() {
        let bgra = decode_one(
            28,
            2,
            2,
            &[1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16],
        );
        assert_eq!((bgra.width, bgra.height), (2, 2));
        assert_eq!(
            px(&bgra),
            [
                [3, 2, 1, 4],
                [7, 6, 5, 8],
                [11, 10, 9, 12],
                [15, 14, 13, 16]
            ]
        );
        let rgba = decode_one(4, 3, 2, &(1..=24).collect::<Vec<u8>>());
        assert_eq!((rgba.width, rgba.height), (3, 2));
        assert_eq!(rgba.pixels, (1..=24).collect::<Vec<u8>>());
        let i8 = decode_one(3, 3, 2, &[10, 20, 30, 40, 50, 60]);
        assert_eq!(
            px(&i8),
            [
                [10, 10, 10, 255],
                [20, 20, 20, 255],
                [30, 30, 30, 255],
                [40, 40, 40, 255],
                [50, 50, 50, 255],
                [60, 60, 60, 255]
            ]
        );
        let ia88 = decode_one(22, 2, 2, &[10, 1, 20, 2, 30, 3, 40, 4]);
        assert_eq!(
            px(&ia88),
            [
                [10, 10, 10, 1],
                [20, 20, 20, 2],
                [30, 30, 30, 3],
                [40, 40, 40, 4]
            ]
        );
    }

    #[test]
    fn bc1_four_and_three_colour_modes() {
        let four = decode_one(1, 4, 4, &bc1_block(RED565, BLACK565));
        let row = [
            [255, 0, 0, 255],
            [0, 0, 0, 255],
            [170, 0, 0, 255],
            [85, 0, 0, 255],
        ];
        assert_eq!(px(&four), [row, row, row, row].concat());
        let three = decode_one(1, 4, 4, &bc1_block(BLACK565, HALF_RED565));
        let row = [
            [0, 0, 0, 255],
            [132, 0, 0, 255],
            [66, 0, 0, 255],
            [0, 0, 0, 0],
        ];
        assert_eq!(px(&three), [row, row, row, row].concat());
    }

    #[test]
    fn bc1_clips_a_partial_block_grid() {
        let blocks = [
            bc1_block(RED565, BLACK565),
            bc1_block(BLACK565, HALF_RED565),
            bc1_block(RED565, BLACK565),
            bc1_block(RED565, BLACK565),
        ]
        .concat();
        let img = decode_one(1, 6, 5, &blocks);
        assert_eq!((img.width, img.height), (6, 5));
        assert_eq!(img.pixel(0, 0), [255, 0, 0, 255]);
        assert_eq!(img.pixel(3, 3), [85, 0, 0, 255]);
        assert_eq!(img.pixel(4, 0), [0, 0, 0, 255]);
        assert_eq!(img.pixel(5, 0), [132, 0, 0, 255]);
        assert_eq!(img.pixel(5, 4), [0, 0, 0, 255]);
        assert_eq!(img.pixel(2, 4), [170, 0, 0, 255]);
    }

    const IDX_MOD8: [u8; 16] = [0, 1, 2, 3, 4, 5, 6, 7, 0, 1, 2, 3, 4, 5, 6, 7];
    const EIGHT_STEP: [u8; 8] = [70, 0, 60, 50, 40, 30, 20, 10];
    const SIX_STEP: [u8; 8] = [0, 50, 10, 20, 30, 40, 0, 255];

    fn alpha_block(a0: u8, a1: u8) -> Vec<u8> {
        [&[a0, a1][..], &pack3(IDX_MOD8)[..]].concat()
    }

    #[test]
    fn bc3_alpha_modes_and_opaque_colour_block() {
        let colour = [&BLACK565[..], &RED565[..], &[0xFF; 4][..]].concat();
        let eight = decode_one(2, 4, 4, &[alpha_block(70, 0), colour.clone()].concat());
        let six = decode_one(2, 4, 4, &[alpha_block(0, 50), colour].concat());
        for i in 0..16 {
            assert_eq!(px(&eight)[i], [170, 0, 0, EIGHT_STEP[i % 8]], "{i}");
            assert_eq!(px(&six)[i], [170, 0, 0, SIX_STEP[i % 8]], "{i}");
        }
    }

    #[test]
    fn bc4_and_bc5() {
        let bc4 = decode_one(27, 4, 4, &alpha_block(70, 0));
        let bc5 = decode_one(21, 4, 4, &[alpha_block(70, 0), alpha_block(0, 50)].concat());
        for i in 0..16 {
            let (r, g) = (EIGHT_STEP[i % 8], SIX_STEP[i % 8]);
            assert_eq!(px(&bc4)[i], [r, r, r, 255], "{i}");
            assert_eq!(px(&bc5)[i], [r, g, 0, 255], "{i}");
        }
    }

    fn bc7_one(block: [u8; 16]) -> Vec<[u8; 4]> {
        px(&decode_one(20, 4, 4, &block))
    }

    fn repeat(v: (u32, u8), n: usize) -> Vec<(u32, u8)> {
        vec![v; n]
    }

    #[test]
    fn bc7_mode6_single_subset_with_p_bits() {
        let mut f = vec![
            (1 << 6, 7),
            (0, 7),
            (127, 7),
            (0, 7),
            (0, 7),
            (0, 7),
            (0, 7),
            (127, 7),
            (127, 7),
            (1, 1),
            (1, 1),
            (0, 3),
            (15, 4),
            (8, 4),
        ];
        f.extend(repeat((0, 4), 13));
        let got = bc7_one(block128(&f));
        assert_eq!(got[0], [1, 1, 1, 255]);
        assert_eq!(got[1], [255, 1, 1, 255]);
        assert_eq!(got[2], [136, 1, 1, 255], "weight 34 of 64 towards e1");
        assert!(got[3..].iter().all(|&p| p == [1, 1, 1, 255]));
    }

    #[test]
    fn bc7_mode5_rotation_swaps_red_and_alpha() {
        let mut f = vec![
            (1 << 5, 6),
            (1, 2),
            (0, 7),
            (127, 7),
            (0, 7),
            (0, 7),
            (0, 7),
            (0, 7),
            (0, 8),
            (200, 8),
            (0, 1),
            (3, 2),
            (1, 2),
        ];
        f.extend(repeat((0, 2), 13));
        f.extend([(1, 1), (0, 2), (3, 2)]);
        f.extend(repeat((0, 2), 13));
        let got = bc7_one(block128(&f));
        assert_eq!(got[0], [66, 0, 0, 0]);
        assert_eq!(got[1], [0, 0, 0, 255]);
        assert_eq!(got[2], [200, 0, 0, 84]);
        assert!(got[3..].iter().all(|&p| p == [0, 0, 0, 0]));
    }

    #[test]
    fn bc7_mode4_index_selection_bit() {
        let fields = |selection: u32| {
            let mut f = vec![
                (1 << 4, 5),
                (0, 2),
                (selection, 1),
                (0, 5),
                (31, 5),
                (0, 5),
                (0, 5),
                (0, 5),
                (0, 5),
                (0, 6),
                (63, 6),
                (1, 1),
                (2, 2),
            ];
            f.extend(repeat((0, 2), 14));
            f.extend([(2, 2), (7, 3)]);
            f.extend(repeat((0, 3), 14));
            f
        };
        let three_bit_colour = bc7_one(block128(&fields(1)));
        assert_eq!(three_bit_colour[0], [72, 0, 0, 84]);
        assert_eq!(three_bit_colour[1], [255, 0, 0, 171]);
        assert!(three_bit_colour[2..].iter().all(|&p| p == [0, 0, 0, 0]));
        let two_bit_colour = bc7_one(block128(&fields(0)));
        assert_eq!(two_bit_colour[0], [84, 0, 0, 72]);
        assert_eq!(two_bit_colour[1], [171, 0, 0, 255]);
    }

    #[test]
    fn bc7_mode1_two_subsets_partition_zero() {
        let mut f = vec![(1 << 1, 2), (0, 6)];
        f.extend([(0, 6), (0, 6), (0, 6), (63, 6)]);
        f.extend([(63, 6), (63, 6), (0, 6), (0, 6)]);
        f.extend(repeat((0, 6), 4));
        f.extend([(1, 1), (0, 1)]);
        f.extend([(0, 2), (0, 3), (7, 3), (4, 3)]);
        f.extend(repeat((0, 3), 11));
        f.push((3, 2));
        let got = bc7_one(block128(&f));
        for i in [0, 1, 4, 5, 8, 9, 12, 13] {
            assert_eq!(got[i], [2, 255, 2, 255], "subset 0 texel {i}");
        }
        assert_eq!(got[2], [253, 0, 0, 255]);
        assert_eq!(got[3], [146, 0, 0, 255], "weight 37 of 64");
        assert_eq!(
            got[15],
            [107, 0, 0, 255],
            "anchor texel 15 has a 2-bit index"
        );
        for i in [6, 7, 10, 11, 14] {
            assert_eq!(got[i], [0, 0, 0, 255], "subset 1 texel {i}");
        }
    }

    #[test]
    fn bc7_invalid_mode_is_transparent_black() {
        assert!(bc7_one([0; 16]).iter().all(|&p| p == [0, 0, 0, 0]));
    }

    /// BGRA bytes of a `side` square mip: blue ramps along x, green steps every four rows,
    /// red marks the level. Rows repeat inside each step so LZ4 can shrink the big mips.
    fn mip_bgra(level: u8, side: u16) -> Vec<u8> {
        let mut out = Vec::new();
        for y in 0..side {
            for x in 0..side {
                out.extend_from_slice(&[(x * 16) as u8, ((y / 4) * 64) as u8, level * 40, 255]);
            }
        }
        out
    }

    fn mip_rgba(level: u8, side: u16) -> Vec<u8> {
        mip_bgra(level, side)
            .as_chunks::<4>()
            .0
            .iter()
            .flat_map(|p| [p[2], p[1], p[0], p[3]])
            .collect()
    }

    fn synthetic_16(rect: Option<(u16, u16)>) -> Vec<u8> {
        let mips: Vec<Vec<u8>> = (0..5).map(|l| mip_bgra(l, 16 >> l)).collect();
        build_vtex(28, 16, 16, 0, &mips, true, rect)
    }

    #[test]
    fn lz4_mips_decode_to_their_pixels() {
        let bytes = synthetic_16(None);
        let v = Vtex::parse(&bytes).unwrap();
        for level in 0..3 {
            let m = v.mips[level];
            assert!(
                m.stored_len < raw_mip_len(m.width, m.height, 1, Layout::Pixels(4)),
                "mip {level} is packed"
            );
        }
        assert_eq!(v.mips[4].stored_len, 4, "a 1x1 mip stays raw");
        for level in 0..5u8 {
            let img = decode_mip(&bytes, level).unwrap();
            let side = 16u32 >> level;
            assert_eq!((img.width, img.height), (side, side), "mip {level}");
            assert_eq!(img.pixels, mip_rgba(level, 16 >> level), "mip {level}");
        }
        assert_eq!(decode(&bytes).unwrap(), decode_mip(&bytes, 0).unwrap());
        assert!(matches!(
            decode_mip(&bytes, 5),
            Err(DecodeError::NoMip { level: 5, mips: 5 })
        ));
    }

    #[test]
    fn display_rect_crops_every_mip() {
        let bytes = synthetic_16(Some((10, 6)));
        let full = decode(&synthetic_16(None)).unwrap();
        let img = decode(&bytes).unwrap();
        assert_eq!((img.width, img.height), (10, 6));
        for y in 0..6 {
            for x in 0..10 {
                assert_eq!(img.pixel(x, y), full.pixel(x, y), "({x},{y})");
            }
        }
        let mip1 = decode_mip(&bytes, 1).unwrap();
        assert_eq!((mip1.width, mip1.height), (5, 3));
        assert_eq!(mip1.pixel(4, 2), [40, 0, 64, 255]);
        let mip4 = decode_mip(&bytes, 4).unwrap();
        assert_eq!((mip4.width, mip4.height), (1, 1));
    }

    #[test]
    fn thumbnail_picks_the_smallest_mip_that_still_covers() {
        let bytes = synthetic_16(None);
        assert_eq!(
            thumbnail(&bytes, 8).unwrap(),
            decode_mip(&bytes, 1).unwrap()
        );
        assert_eq!(
            thumbnail(&bytes, 1).unwrap(),
            decode_mip(&bytes, 4).unwrap()
        );
        assert_eq!(thumbnail(&bytes, 32).unwrap(), decode(&bytes).unwrap());
        let six = thumbnail(&bytes, 6).unwrap();
        assert_eq!((six.width, six.height), (6, 6));
        assert_eq!(six, decode_mip(&bytes, 1).unwrap().fit(6));
    }

    #[test]
    fn refusals() {
        let mut short = synthetic_16(None);
        short.pop();
        assert!(matches!(decode(&short), Err(DecodeError::Truncated)));
        let cube = build_vtex(28, 2, 2, 1 << 4, &[vec![0; 16]], false, None);
        assert!(matches!(
            decode(&cube),
            Err(DecodeError::Unsupported {
                format: "BGRA8888",
                reason: "cubemap"
            })
        ));
        let jpeg = build_vtex(15, 4, 4, 0, &[vec![]], false, None);
        assert!(matches!(
            decode(&jpeg),
            Err(DecodeError::Unsupported {
                format: "JPEG_RGBA8888",
                reason: "JPEG payload"
            })
        ));
        let hdr = build_vtex(19, 4, 4, 0, &[vec![0; 16]], false, None);
        assert!(matches!(
            decode(&hdr),
            Err(DecodeError::Unsupported {
                format: "BC6H",
                reason: "HDR block format"
            })
        ));
    }

    #[test]
    fn png_payload_formats() {
        let img = RgbaImage {
            width: 3,
            height: 2,
            pixels: (0..24).collect(),
        };
        for format in [16, 18] {
            let bytes = build_vtex(format, 3, 2, 0, &[png::write(&img)], false, None);
            assert_eq!(decode(&bytes).unwrap(), img, "format {format}");
            assert!(matches!(
                decode_mip(&bytes, 1),
                Err(DecodeError::NoMip { level: 1, .. })
            ));
        }
        let bad = build_vtex(16, 3, 2, 0, &[b"not png".to_vec()], false, None);
        assert!(matches!(decode(&bad), Err(DecodeError::Png(_))));
    }

    #[test]
    fn committed_fixtures() {
        for (bytes, side, name, flat) in [
            (MASK, 512, "DXT1", true),
            (COLOR, 512, "BC7", false),
            (NOLOD, 1024, "ATI1N", false),
        ] {
            let img = decode(bytes).unwrap();
            assert_eq!((img.width, img.height), (side, side), "{name}");
            let first = img.pixel(0, 0);
            assert_eq!(
                img.pixels.as_chunks::<4>().0.iter().all(|p| *p == first),
                flat,
                "{name}: the mask is solid black in every mip, the others have content"
            );
        }
        let tiny = decode(TINY).unwrap();
        assert_eq!((tiny.width, tiny.height), (1, 1), "FILL_TO_POW2 rect");
        let colour = decode(COLOR).unwrap();
        assert!(
            colour.pixels.as_chunks::<4>().0.iter().all(|p| p[3] >= 250),
            "opaque up to BC7 endpoint quantisation"
        );
        let thumb = thumbnail(COLOR, 48).unwrap();
        assert_eq!((thumb.width, thumb.height), (48, 48));
    }

    /// Mip 1 as the compiler built it against mip 0 box-filtered down: a wrong partition
    /// table or index order would garble whole blocks and sink the ratio.
    #[test]
    fn mip_chain_is_self_consistent() {
        for (bytes, name) in [(MASK, "DXT1"), (COLOR, "BC7")] {
            let shrunk = decode(bytes).unwrap().fit(256);
            let mip1 = decode_mip(bytes, 1).unwrap();
            let db = crate::texture::resample::tests::psnr(&shrunk.pixels, &mip1.pixels);
            assert!(db > 30.0, "{name}: {db} dB");
        }
    }

    fn research_vpks() -> Vec<std::path::PathBuf> {
        use crate::addons::sources::tests::research;
        let optilock = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../research/configs/OptiLock/Essential Fixes");
        vec![
            research("Optimized Soul Container", "pak01_dir.vpk"),
            research("Vindicta Scope Downscale", "pak89_dir.vpk"),
            research("Sinner Light Fix Mod", "pak26_dir.vpk"),
            research("Screenspace Particle Disabler", "pak02_dir.vpk"),
            research("Blur Disabler", "pak97_dir.vpk"),
            optilock.join("OptimizedMcGinnisWall.vpk"),
            optilock.join("SinnersLightFix.vpk"),
        ]
    }

    #[test]
    fn research_textures_all_decode() {
        use crate::hud::vpk::VpkDir;
        let mut seen = 0;
        let mut flat = Vec::new();
        for path in research_vpks() {
            let vpk = VpkDir::open(&path).unwrap();
            for entry in vpk.entries.keys().filter(|p| p.ends_with(".vtex_c")) {
                let bytes = vpk.read(entry).unwrap();
                let v = Vtex::parse(&bytes).unwrap();
                let img = decode(&bytes).unwrap_or_else(|e| panic!("{entry}: {e}"));
                let (w, h) = v.display_rect.unwrap_or((v.width, v.height));
                assert_eq!(
                    (img.width, img.height),
                    (u32::from(w.min(v.width)), u32::from(h.min(v.height))),
                    "{entry}"
                );
                let first = img.pixel(0, 0);
                let padded_constant = img.pixels.len() == 4;
                if !padded_constant
                    && !entry.contains("soul_container_mask")
                    && img.pixels.as_chunks::<4>().0.iter().all(|p| *p == first)
                {
                    flat.push(format!("{entry} {}x{} {first:?}", img.width, img.height));
                }
                if entry.contains("soul_container_color") {
                    let min = img.pixels.as_chunks::<4>().0.iter().map(|p| p[3]).min();
                    assert!(min >= Some(250), "{entry} alpha floor {min:?}");
                }
                seen += 1;
            }
        }
        assert!(seen >= 7, "{seen} textures");
        assert!(flat.is_empty(), "flat textures: {flat:#?}");
    }

    #[test]
    fn vindicta_scope_full_and_thumbnail() {
        use crate::addons::sources::tests::research;
        use crate::hud::vpk::VpkDir;
        let bytes = VpkDir::open(&research("Vindicta Scope Downscale", "pak89_dir.vpk"))
            .unwrap()
            .read(crate::addons::native_scope::TEXTURE)
            .unwrap();
        let full = decode(&bytes).unwrap();
        assert_eq!((full.width, full.height), (1080, 1080));
        let thumb = thumbnail(&bytes, 64).unwrap();
        assert_eq!((thumb.width, thumb.height), (64, 64));
    }

    fn walk(dir: &std::path::Path, out: &mut Vec<std::path::PathBuf>) {
        for entry in std::fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                walk(&path, out);
            } else if path.extension().is_some_and(|e| e == "vtex_c") {
                out.push(path);
            }
        }
    }

    /// Every panorama texture of a game snapshot decodes. Needs `DEADTUNE_GAME_SAMPLES`.
    #[test]
    fn game_samples_decode() {
        let Ok(dir) = std::env::var("DEADTUNE_GAME_SAMPLES") else {
            eprintln!("DEADTUNE_GAME_SAMPLES unset; skipping");
            return;
        };
        let mut paths = Vec::new();
        walk(
            &std::path::Path::new(&dir).join("raw/panorama/images"),
            &mut paths,
        );
        let mut ok = std::collections::BTreeMap::<String, usize>::new();
        let mut failed = std::collections::BTreeMap::<String, usize>::new();
        let mut failures = Vec::new();
        for path in &paths {
            let bytes = std::fs::read(path).unwrap();
            let format = Vtex::parse(&bytes)
                .map(|v| v.format.name().to_string())
                .unwrap_or_else(|_| "unparsed".into());
            match decode(&bytes).and_then(|_| thumbnail(&bytes, 48)) {
                Ok(_) => *ok.entry(format).or_default() += 1,
                Err(e) => {
                    *failed.entry(e.to_string()).or_default() += 1;
                    failures.push(format!("{}: {e}", path.display()));
                }
            }
        }
        eprintln!("decoded by format: {ok:?}");
        eprintln!("failed by error: {failed:?}");
        for f in &failures {
            eprintln!("{f}");
        }
        assert!(
            failures.is_empty(),
            "{} of {} failed",
            failures.len(),
            paths.len()
        );
    }
}
