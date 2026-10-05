//! The Vindicta scope downscale, rebuilt from the player's game files. Tamara
//! Mochaccinae's `pak89_dir.vpk` replaces `panorama/images/hud/crosshair/scope_common_psd.vtex_c`,
//! the scope overlay the game ships as a 4096x4096 BGRA8888 texture with a single mip
//! (Panorama images are compiled NO_LOD and never mipmapped), with the same image at
//! 1080x1080: 4.4 MiB of VRAM instead of 64 MiB, and a full-screen sample from a texture
//! four times smaller while scoped. We read the player's own file from pak01, check it is
//! still a plain 8-bit single-mip 2D texture, area-average it so its longer side is
//! [`ScopeOptions::side`] (aspect kept), and write the pixels back into the original's own
//! resource container with only width and height changed, so RED2, flags, reflectivity and
//! the FALLBACK_BITS thumbnail stay the game's. Upstream also ships
//! `panorama/image_compiler.vdata_c`, the author's compile manifest; the texture does not
//! reference it and the game has no such file, so we leave it out.

use std::collections::BTreeMap;

use super::AddonError;
use crate::hud::resource::{Resource, ResourceError};
use crate::hud::vpk::VpkDir;
use crate::texture::resample::Image;
use crate::texture::vtex::{Flags, Format, Layout, Vtex, VtexError, raw_mip_len};

pub const TEXTURE: &str = "panorama/images/hud/crosshair/scope_common_psd.vtex_c";
/// Upstream's size, and the scope's height on a 1080p screen.
pub const DEFAULT_SIDE: u16 = 1080;
pub const MIN_SIDE: u16 = 256;

/// Offsets inside the VTEX header (the start of the DATA block).
const WIDTH_AT: usize = 20;
const EXTRA_OFFSET_AT: usize = 32;
const EXTRA_ENTRY: usize = 12;
const EXTRA_COMPRESSED_MIP_SIZE: u32 = 4;

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ScopeOptions {
    /// Longer side of the rebuilt texture, clamped to `MIN_SIDE..=` the original's.
    #[serde(default = "default_side")]
    pub side: u16,
}

fn default_side() -> u16 {
    DEFAULT_SIDE
}

impl Default for ScopeOptions {
    fn default() -> ScopeOptions {
        ScopeOptions { side: DEFAULT_SIDE }
    }
}

impl ScopeOptions {
    pub fn is_default(&self) -> bool {
        *self == ScopeOptions::default()
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ScopeError {
    #[error(transparent)]
    Vtex(#[from] VtexError),
    #[error(transparent)]
    Resource(#[from] ResourceError),
    /// The game's file is no longer a plain 8-bit single-mip 2D texture; says what it is.
    #[error("scope texture cannot be rebuilt: {0}")]
    Unsupported(String),
}

/// Bytes per pixel of the formats whose channels are single bytes, so a per-channel
/// mean is the right resample whatever the channel order.
fn channels(format: Format) -> Option<usize> {
    match format.name() {
        "I8" => Some(1),
        "IA88" => Some(2),
        "RGBA8888" | "BGRA8888" => Some(4),
        _ => None,
    }
}

/// Kinds of the extra-data entries of a VTEX DATA block.
fn extra_kinds(data: &[u8]) -> impl Iterator<Item = u32> + '_ {
    let u32_at = move |at: usize| {
        data.get(at..at + 4)
            .map(|s| u32::from_le_bytes([s[0], s[1], s[2], s[3]]))
    };
    let table = EXTRA_OFFSET_AT + u32_at(EXTRA_OFFSET_AT).unwrap_or(0) as usize;
    let count = u32_at(EXTRA_OFFSET_AT + 4).unwrap_or(0) as usize;
    (0..count)
        .map(move |i| table + EXTRA_ENTRY * i)
        .take_while(move |&at| at + EXTRA_ENTRY <= data.len())
        .filter_map(u32_at)
}

/// Size of a `width` x `height` texture scaled so its longer side is `side` (clamped to
/// `MIN_SIDE`), aspect kept; the input size when `side` is not smaller.
pub fn target_dims(width: u16, height: u16, side: u16) -> (u16, u16) {
    let long = width.max(height);
    let side = side.max(MIN_SIDE);
    if side >= long {
        return (width, height);
    }
    let scale = f64::from(side) / f64::from(long);
    let shrink = |d: u16| ((f64::from(d) * scale).round() as u16).max(1);
    (shrink(width), shrink(height))
}

/// `original` (a whole `.vtex_c`) with its single mip area-averaged so the longer side is
/// `side`, clamped to `MIN_SIDE`; the input unchanged when `side` is not smaller.
pub fn resize(original: &[u8], side: u16) -> Result<Vec<u8>, ScopeError> {
    let v = Vtex::parse(original)?;
    let refuse = |what: String| Err(ScopeError::Unsupported(what));
    let Some(c) = channels(v.format) else {
        return refuse(format!(
            "format {} is not 8 bits per channel",
            v.format.name()
        ));
    };
    if v.depth != 1
        || v.flags.contains(Flags::CUBE)
        || v.flags.contains(Flags::VOLUME)
        || v.flags.contains(Flags::ARRAY)
    {
        return refuse(format!(
            "a cube, volume or array texture (flags {:#06x}, depth {})",
            v.flags.0, v.depth
        ));
    }
    if v.mips.len() != 1 {
        return refuse(format!("{} mip levels; expected one", v.mips.len()));
    }
    if v.display_rect.is_some() {
        return refuse("a METADATA display rect".into());
    }
    let res = Resource::parse(original)?;
    let data = &res
        .block(b"DATA")
        .ok_or(ResourceError::MissingBlock("DATA"))?
        .data;
    if extra_kinds(data).any(|k| k == EXTRA_COMPRESSED_MIP_SIZE) {
        return refuse("a COMPRESSED_MIP_SIZE table (LZ4 mips)".into());
    }
    if v.width == 0 || v.height == 0 {
        return refuse(format!("zero size {}x{}", v.width, v.height));
    }
    let raw = raw_mip_len(v.width, v.height, 1, Layout::Pixels(c));
    let have = original.len().saturating_sub(v.pixel_start());
    if v.mips[0].stored_len != raw || have != raw {
        return refuse(format!(
            "pixel data is {have} bytes, {}x{} {} needs {raw}",
            v.width,
            v.height,
            v.format.name()
        ));
    }

    let (width, height) = target_dims(v.width, v.height, side);
    if (width, height) == (v.width, v.height) {
        return Ok(original.to_vec());
    }
    let image = Image::new(
        u32::from(v.width),
        u32::from(v.height),
        c,
        original[v.pixel_start()..].to_vec(),
    )
    .expect("pixel length checked against the header");
    let small = image.resample(u32::from(width), u32::from(height));

    let mut res = res;
    let data = &mut res
        .blocks
        .iter_mut()
        .find(|b| &b.name == b"DATA")
        .expect("DATA block found above")
        .data;
    data[WIDTH_AT..WIDTH_AT + 2].copy_from_slice(&width.to_le_bytes());
    data[WIDTH_AT + 2..WIDTH_AT + 4].copy_from_slice(&height.to_le_bytes());
    let mut out = res.to_bytes();
    out.extend_from_slice(&small.data);

    let back = Vtex::parse(&out)?;
    let consistent = (back.width, back.height, back.mips.len(), back.format)
        == (width, height, 1, v.format)
        && back.pixel_start() + back.pixel_len() == out.len();
    if !consistent {
        return refuse("the rebuilt header does not describe its pixel data".into());
    }
    Ok(out)
}

/// Our pak's one file: the player's scope texture at `opts.side`.
pub fn build(game: &VpkDir, opts: &ScopeOptions) -> Result<BTreeMap<String, Vec<u8>>, AddonError> {
    let original = game.read(TEXTURE)?;
    let rebuilt = resize(&original, opts.side)?;
    Ok(BTreeMap::from([(TEXTURE.to_string(), rebuilt)]))
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::addons::sources;
    use crate::addons::verify::{Expect, verify};
    use crate::hud::resource::Block;
    use crate::hud::vpk;
    use crate::texture::resample::tests::psnr;
    use crate::texture::vtex::tests::{COLOR, MASK, synthetic};

    const UPSTREAM_HEADER: usize = 2132;

    fn upstream() -> Vec<u8> {
        VpkDir::open(&sources::tests::research(
            "Vindicta Scope Downscale",
            "pak89_dir.vpk",
        ))
        .unwrap()
        .read(TEXTURE)
        .unwrap()
    }

    /// A minimal vtex_c: 8-byte RED2, a DATA block with the given header fields and
    /// extra-data entries, then `pixels` as they are.
    fn plain_vtex(
        width: u16,
        height: u16,
        format: u8,
        mips: u8,
        flags: u16,
        extras: &[(u32, Vec<u8>)],
        pixels: &[u8],
    ) -> Vec<u8> {
        let mut d = Vec::new();
        d.extend_from_slice(&1u16.to_le_bytes());
        d.extend_from_slice(&flags.to_le_bytes());
        d.extend_from_slice(&[0u8; 16]);
        d.extend_from_slice(&width.to_le_bytes());
        d.extend_from_slice(&height.to_le_bytes());
        d.extend_from_slice(&1u16.to_le_bytes());
        d.push(format);
        d.push(mips);
        d.extend_from_slice(&0u32.to_le_bytes());
        let table_offset = if extras.is_empty() { 0u32 } else { 8 };
        d.extend_from_slice(&table_offset.to_le_bytes());
        d.extend_from_slice(&(extras.len() as u32).to_le_bytes());
        let mut payload_at = d.len() + EXTRA_ENTRY * extras.len();
        for (kind, payload) in extras {
            let at = d.len();
            d.extend_from_slice(&kind.to_le_bytes());
            d.extend_from_slice(&((payload_at - at - 4) as u32).to_le_bytes());
            d.extend_from_slice(&(payload.len() as u32).to_le_bytes());
            payload_at += payload.len();
        }
        for (_, payload) in extras {
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
        out.extend_from_slice(pixels);
        out
    }

    /// Channel k of pixel (x, y): a linear ramp along x for even k, along y for odd k.
    fn ramp(x: f64, y: f64, w: u16, h: u16, k: usize) -> f64 {
        if k.is_multiple_of(2) {
            x * 255.0 / f64::from(w - 1)
        } else {
            y * 255.0 / f64::from(h - 1)
        }
    }

    fn gradient(w: u16, h: u16, c: usize) -> Vec<u8> {
        let mut px = Vec::new();
        for y in 0..h {
            for x in 0..w {
                for k in 0..c {
                    px.push(ramp(f64::from(x), f64::from(y), w, h, k).round() as u8);
                }
            }
        }
        px
    }

    fn pak_with(files: &BTreeMap<String, Vec<u8>>) -> VpkDir {
        VpkDir::in_memory(vpk::write(files)).unwrap()
    }

    /// Upstream's header with the dims patched, over pixels nearest-neighbour upscaled
    /// from upstream's own: the game's original, as near as we can make it on this Mac.
    pub fn synthetic_original(side: usize) -> Vec<u8> {
        let up = upstream();
        let v = Vtex::parse(&up).unwrap();
        let (w, start) = (v.width as usize, v.pixel_start());
        let mut out = up[..start].to_vec();
        let at = 1056 + WIDTH_AT;
        out[at..at + 2].copy_from_slice(&(side as u16).to_le_bytes());
        out[at + 2..at + 4].copy_from_slice(&(side as u16).to_le_bytes());
        out.reserve(side * side * 4);
        for y in 0..side {
            let sy = y * w / side;
            for x in 0..side {
                let sx = x * w / side;
                let i = start + (sy * w + sx) * 4;
                out.extend_from_slice(&up[i..i + 4]);
            }
        }
        out
    }

    #[test]
    fn upstream_header_facts_and_round_trip() {
        let up = upstream();
        assert_eq!(up.len(), 4_667_732);
        let v = Vtex::parse(&up).unwrap();
        assert_eq!((v.width, v.height, v.depth), (1080, 1080, 1));
        assert_eq!(v.format.name(), "BGRA8888");
        assert_eq!(v.flags, Flags::NO_LOD);
        assert_eq!(v.mips.len(), 1);
        assert_eq!(v.display_rect, None);
        assert_eq!(v.pixel_start(), UPSTREAM_HEADER);
        assert_eq!(v.pixel_len(), 1080 * 1080 * 4);
        let res = Resource::parse(&up).unwrap();
        assert_eq!(res.to_bytes(), &up[..UPSTREAM_HEADER]);
        let blocks: Vec<(&[u8], usize)> = res
            .blocks
            .iter()
            .map(|b| (&b.name[..], b.data.len()))
            .collect();
        assert_eq!(blocks, [(&b"RED2"[..], 1001), (&b"DATA"[..], 1076)]);
        let data = &res.block(b"DATA").unwrap().data;
        assert_eq!(extra_kinds(data).collect::<Vec<_>>(), [1]);
        assert_eq!(
            resize(&up, DEFAULT_SIDE).unwrap(),
            up,
            "already at the target: untouched"
        );
    }

    fn rebuild_matches_upstream(side: usize) -> f64 {
        let up = upstream();
        let files = BTreeMap::from([(TEXTURE.to_string(), synthetic_original(side))]);
        let built = build(&pak_with(&files), &ScopeOptions::default()).unwrap();
        assert_eq!(built.keys().collect::<Vec<_>>(), [TEXTURE]);
        let out = &built[TEXTURE];
        assert_eq!(out.len(), up.len());
        assert_eq!(&out[..UPSTREAM_HEADER], &up[..UPSTREAM_HEADER]);
        let (a, b) = (Vtex::parse(out).unwrap(), Vtex::parse(&up).unwrap());
        assert_eq!(
            (a.width, a.height, a.depth, a.format, a.flags, a.mips),
            (b.width, b.height, b.depth, b.format, b.flags, b.mips)
        );
        let got = verify(&pak_with(&built), &Expect::default());
        assert!(got.is_ok(), "{got}");
        assert_eq!(got.entries, 1);
        let db = psnr(&out[UPSTREAM_HEADER..], &up[UPSTREAM_HEADER..]);
        eprintln!("{side} -> 1080 vs upstream: {db:.1} dB");
        db
    }

    #[test]
    fn rebuilds_a_full_size_original_into_upstreams_header() {
        let db = rebuild_matches_upstream(4096);
        assert!(db > 45.0, "psnr vs upstream {db}");
    }

    #[test]
    fn rebuilds_a_half_size_original_too() {
        let db = rebuild_matches_upstream(2048);
        assert!(db > 40.0, "psnr vs upstream {db}");
    }

    #[test]
    fn every_supported_format_keeps_aspect_and_values() {
        for (format, c) in [(3u8, 1usize), (4, 4), (22, 2), (28, 4)] {
            let (w, h) = (1024u16, 768u16);
            let src = plain_vtex(w, h, format, 1, 0, &[], &gradient(w, h, c));
            let out = resize(&src, 256).unwrap();
            let v = Vtex::parse(&out).unwrap();
            assert_eq!(
                (v.width, v.height, v.mips.len()),
                (256, 192, 1),
                "format {format}"
            );
            assert_eq!(v.format, Format(format));
            assert_eq!(v.pixel_start() + v.pixel_len(), out.len());
            let px = &out[v.pixel_start()..];
            assert_eq!(px.len(), 256 * 192 * c);
            let ratio = 4.0;
            for oy in 0..192 {
                for ox in 0..256 {
                    for k in 0..c {
                        let cx = (f64::from(ox) + 0.5) * ratio - 0.5;
                        let cy = (f64::from(oy) + 0.5) * ratio - 0.5;
                        let want = ramp(cx, cy, w, h, k);
                        let got = f64::from(px[(oy as usize * 256 + ox as usize) * c + k]);
                        assert!(
                            (got - want).abs() <= 1.0,
                            "format {format} ({ox},{oy})[{k}]: {got} vs {want}"
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn returns_the_original_when_side_is_not_smaller_and_clamps_small_sides() {
        let src = plain_vtex(256, 192, 28, 1, 0, &[], &gradient(256, 192, 4));
        assert_eq!(resize(&src, 256).unwrap(), src);
        assert_eq!(resize(&src, 60000).unwrap(), src);
        let big = plain_vtex(1024, 512, 28, 1, 8, &[], &vec![0x80; 1024 * 512 * 4]);
        let out = resize(&big, 1).unwrap();
        let v = Vtex::parse(&out).unwrap();
        assert_eq!((v.width, v.height), (MIN_SIDE, MIN_SIDE / 2));
        assert_eq!(v.flags, Flags::NO_LOD);
        assert!(out[v.pixel_start()..].iter().all(|&p| p == 0x80));
    }

    #[test]
    fn refuses_what_it_cannot_rebuild() {
        let reason = |bytes: &[u8]| match resize(bytes, 64) {
            Err(ScopeError::Unsupported(what)) => what,
            other => panic!("expected a refusal, got {other:?}"),
        };
        assert!(reason(COLOR).contains("BC7"));
        assert!(reason(MASK).contains("DXT1"));
        assert!(reason(&synthetic(64, 5, &[2048, 512, 128, 32, 8], (0, 0))).contains("DXT1"));
        let px = vec![0u8; 256 * 192 * 4];
        assert!(reason(&plain_vtex(256, 192, 28, 2, 0, &[], &px)).contains("mip"));
        assert!(reason(&plain_vtex(256, 192, 28, 1, Flags::CUBE.0, &[], &px)).contains("cube"));
        let mut table = Vec::new();
        for n in [1u32, 8, 1, 256 * 192 * 4] {
            table.extend_from_slice(&n.to_le_bytes());
        }
        assert!(
            reason(&plain_vtex(256, 192, 28, 1, 0, &[(4, table)], &px))
                .contains("COMPRESSED_MIP_SIZE")
        );
        assert!(reason(&plain_vtex(256, 192, 28, 1, 0, &[], &px[..1000])).contains("pixel"));
        let long = [&px[..], &[0]].concat();
        assert!(reason(&plain_vtex(256, 192, 28, 1, 0, &[], &long)).contains("pixel"));
        assert!(reason(&plain_vtex(0, 192, 28, 1, 0, &[], &[])).contains("zero"));
        assert!(matches!(resize(&MASK[..100], 64), Err(ScopeError::Vtex(_))));

        let empty = pak_with(&BTreeMap::new());
        assert!(matches!(
            build(&empty, &ScopeOptions::default()),
            Err(AddonError::Vpk(_))
        ));
        let files = BTreeMap::from([(TEXTURE.to_string(), COLOR.to_vec())]);
        assert!(matches!(
            build(&pak_with(&files), &ScopeOptions::default()),
            Err(AddonError::Scope(ScopeError::Unsupported(_)))
        ));
    }

    #[test]
    fn options_default_and_serde() {
        let opts = ScopeOptions::default();
        assert_eq!(opts.side, 1080);
        assert!(opts.is_default());
        let text = toml::to_string(&ScopeOptions { side: 720 }).unwrap();
        assert_eq!(text.trim(), "side = 720");
        let back: ScopeOptions = toml::from_str(&text).unwrap();
        assert_eq!(back.side, 720);
        assert!(!back.is_default());
        assert_eq!(toml::from_str::<ScopeOptions>("").unwrap(), opts);
    }
}
