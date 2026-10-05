//! Replaces a Panorama image's pixels with the player's own. The output is the game's file
//! with only the texture header rewritten: uncompressed BGRA8888, one mip, `NO_LOD`, the
//! new size, and the extra data minus the entries that describe the old pixels (the
//! COMPRESSED_MIP_SIZE table and the METADATA display rect). RED2, the reflectivity and
//! the FALLBACK_BITS thumbnail stay the game's, which is the shape the Vindicta scope
//! rebuild proved the engine loads.
//!
//! Scope: single-mip `NO_LOD` 2D textures, which is how every `panorama/images/**`
//! `.vtex_c` is compiled. Mipmapped world textures would need a generated mip chain and
//! are refused, as are cubemaps, volumes, arrays and sprite sheets.

use super::frame::{self, Crop};
use super::png::RgbaImage;
use super::resample::Image;
use super::vtex::{self, Flags, Vtex, VtexError};
use crate::hud::resource::{Resource, ResourceError};

/// How the player's image is sized into the game's slot.
#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize,
)]
#[serde(rename_all = "snake_case")]
pub enum Fit {
    /// The game image's size, the player's image scaled to fit inside it with its aspect
    /// kept and the rest transparent.
    #[default]
    Original,
    /// The player's image at its own size, scaled down only past [`MAX_SIDE`].
    Own,
    /// The game image's size, covered by the player's image with its aspect kept and the
    /// overflow cropped evenly from both sides.
    Fill,
    /// The game image's size, the player's image squeezed to it.
    Stretch,
}

impl Fit {
    pub const ALL: [Fit; 4] = [Fit::Original, Fit::Fill, Fit::Stretch, Fit::Own];

    pub fn key(self) -> &'static str {
        match self {
            Fit::Original => "original",
            Fit::Own => "own",
            Fit::Fill => "fill",
            Fit::Stretch => "stretch",
        }
    }

    pub fn parse(text: &str) -> Option<Fit> {
        Fit::ALL.into_iter().find(|f| f.key() == text)
    }

    /// Whether the result always has the game image's size.
    pub fn keeps_slot(self) -> bool {
        self != Fit::Own
    }
}

/// Longest side [`Fit::Own`] keeps; the largest Panorama images the game ships are 4096.
pub const MAX_SIDE: u32 = 4096;

pub const BGRA8888: u8 = 28;
const EXTRA_SHEET: u32 = 2;
const EXTRA_METADATA: u32 = 3;
const EXTRA_COMPRESSED_MIP_SIZE: u32 = 4;
const VTEX_HEADER: usize = 40;
const FLAGS_AT: usize = 2;
const DIM_AT: usize = 20;
const EXTRA_OFFSET_AT: usize = 32;
const EXTRA_ENTRY: usize = 12;

#[derive(Debug, thiserror::Error)]
pub enum EncodeError {
    #[error(transparent)]
    Vtex(#[from] VtexError),
    #[error(transparent)]
    Resource(#[from] ResourceError),
    #[error("a cube, volume or array texture (flags {flags:#06x}, depth {depth})")]
    Layered { flags: u16, depth: u16 },
    #[error(
        "not a Panorama image: {mips} mip levels, NO_LOD {no_lod}; only single-mip NO_LOD images can be replaced"
    )]
    Mipmapped { mips: usize, no_lod: bool },
    #[error("a sprite sheet; replacing it would break its animation frames")]
    Sheet,
    #[error("the image is empty")]
    EmptyImage,
    #[error("the rebuilt texture does not read back as written: {0}")]
    Rebuilt(String),
}

/// `original` (a whole `.vtex_c`) with its pixels replaced by `image`, sized per `fit`.
pub fn replace(original: &[u8], image: &RgbaImage, fit: Fit) -> Result<Vec<u8>, EncodeError> {
    let v = Vtex::parse(original)?;
    if v.depth != 1
        || [Flags::CUBE, Flags::VOLUME, Flags::ARRAY]
            .iter()
            .any(|f| v.flags.contains(*f))
    {
        return Err(EncodeError::Layered {
            flags: v.flags.0,
            depth: v.depth,
        });
    }
    let no_lod = v.flags.contains(Flags::NO_LOD);
    if v.mips.len() != 1 || !no_lod {
        return Err(EncodeError::Mipmapped {
            mips: v.mips.len(),
            no_lod,
        });
    }
    let extras = vtex::extras(original, v.header_at())?;
    if extras.iter().any(|e| e.kind == EXTRA_SHEET) {
        return Err(EncodeError::Sheet);
    }
    if image.width == 0 || image.height == 0 {
        return Err(EncodeError::EmptyImage);
    }

    let (sw, sh) = v.display_rect.unwrap_or((v.width, v.height));
    let image = sized(image, fit, (sw.into(), sh.into()));
    let (width, height) = (image.width as u16, image.height as u16);
    let pixels = to_bgra(&image);

    let kept: Vec<&[u8]> = extras
        .iter()
        .filter(|e| ![EXTRA_METADATA, EXTRA_COMPRESSED_MIP_SIZE].contains(&e.kind))
        .map(|e| &original[e.payload.clone()])
        .collect();
    let kinds: Vec<u32> = extras
        .iter()
        .map(|e| e.kind)
        .filter(|k| ![EXTRA_METADATA, EXTRA_COMPRESSED_MIP_SIZE].contains(k))
        .collect();
    let header = &original[v.header_at()..v.header_at() + VTEX_HEADER];
    let flags = Flags(v.flags.0 | Flags::NO_LOD.0);
    let data = vtex_data(header, flags, width, height, &kinds, &kept);

    let mut res = Resource::parse(original)?;
    let block = res
        .blocks
        .iter_mut()
        .find(|b| &b.name == b"DATA")
        .ok_or(ResourceError::MissingBlock("DATA"))?;
    block.data = data;
    let mut out = res.to_bytes();
    out.extend_from_slice(&pixels);
    check(&out, width, height, &kinds, &kept)?;
    Ok(out)
}

/// `image` sized for a game image of `slot` pixels the way `fit` says.
pub fn sized(image: &RgbaImage, fit: Fit, slot: (u32, u32)) -> RgbaImage {
    let (width, height) = slot;
    match fit {
        Fit::Original => fitted(image, width, height),
        Fit::Own => {
            let (w, h) = own_size(image.width, image.height);
            fitted(image, w.into(), h.into())
        }
        Fit::Fill => {
            let size = (image.width, image.height);
            let scale = frame::fill_scale(size, slot);
            let centre = (f64::from(image.width) / 2.0, f64::from(image.height) / 2.0);
            let window = Crop::window(size, slot, scale, centre).apply(image);
            stretched(&window, width, height)
        }
        Fit::Stretch => stretched(image, width, height),
    }
}

fn stretched(image: &RgbaImage, width: u32, height: u32) -> RgbaImage {
    if (image.width, image.height) == (width, height) {
        return image.clone();
    }
    resample_premultiplied(image, width.max(1), height.max(1))
}

fn own_size(width: u32, height: u32) -> (u16, u16) {
    let long = width.max(height);
    if long <= MAX_SIDE {
        return (width as u16, height as u16);
    }
    let scale = f64::from(MAX_SIDE) / f64::from(long);
    let shrink = |d: u32| ((f64::from(d) * scale).round() as u16).max(1);
    (shrink(width), shrink(height))
}

/// `image` scaled to fit inside `width` x `height` with its aspect kept, centred on a
/// transparent canvas. Letterboxing rather than cropping or stretching: the player sees all
/// of their art undistorted, and transparent padding draws as nothing in Panorama.
fn fitted(image: &RgbaImage, width: u32, height: u32) -> RgbaImage {
    if (image.width, image.height) == (width, height) {
        return image.clone();
    }
    let scale = (f64::from(width) / f64::from(image.width))
        .min(f64::from(height) / f64::from(image.height));
    let fit = |d: u32, max: u32| ((f64::from(d) * scale).round() as u32).clamp(1, max);
    let (w, h) = (fit(image.width, width), fit(image.height, height));
    let scaled = resample_premultiplied(image, w, h);
    if (w, h) == (width, height) {
        return scaled;
    }
    let (left, top) = ((width - w) / 2, (height - h) / 2);
    let mut pixels = vec![0u8; width as usize * height as usize * 4];
    for (y, row) in scaled.pixels.chunks_exact(w as usize * 4).enumerate() {
        let at = ((top as usize + y) * width as usize + left as usize) * 4;
        pixels[at..at + row.len()].copy_from_slice(row);
    }
    RgbaImage {
        width,
        height,
        pixels,
    }
}

/// Area-averaged in premultiplied alpha, so transparent pixels' colour never bleeds into
/// the edges of what is drawn.
fn resample_premultiplied(image: &RgbaImage, width: u32, height: u32) -> RgbaImage {
    let pre: Vec<u8> = image
        .pixels
        .as_chunks::<4>()
        .0
        .iter()
        .flat_map(|&[r, g, b, alpha]| {
            let a = u16::from(alpha);
            let m = |c: u8| ((u16::from(c) * a + 127) / 255) as u8;
            [m(r), m(g), m(b), alpha]
        })
        .collect();
    let src =
        Image::new(image.width, image.height, 4, pre).expect("RgbaImage holds 4 bytes a pixel");
    let pixels = src
        .resample(width, height)
        .data
        .as_chunks::<4>()
        .0
        .iter()
        .flat_map(|&[r, g, b, alpha]| {
            let a = u16::from(alpha);
            let u = |c: u8| {
                (u16::from(c) * 255 + a / 2)
                    .checked_div(a)
                    .map_or(0, |v| v.min(255) as u8)
            };
            [u(r), u(g), u(b), alpha]
        })
        .collect();
    RgbaImage {
        width,
        height,
        pixels,
    }
}

fn to_bgra(image: &RgbaImage) -> Vec<u8> {
    image
        .pixels
        .as_chunks::<4>()
        .0
        .iter()
        .flat_map(|&[r, g, b, a]| [b, g, r, a])
        .collect()
}

/// A VTEX DATA block: the game's 40-byte header with flags, size, depth, format and mip
/// count set, then an extra-data table of `kinds` over `payloads`, each entry's offset
/// counted from its own offset field.
fn vtex_data(
    header: &[u8],
    flags: Flags,
    width: u16,
    height: u16,
    kinds: &[u32],
    payloads: &[&[u8]],
) -> Vec<u8> {
    let mut d = header.to_vec();
    d[FLAGS_AT..FLAGS_AT + 2].copy_from_slice(&flags.0.to_le_bytes());
    d[DIM_AT..DIM_AT + 2].copy_from_slice(&width.to_le_bytes());
    d[DIM_AT + 2..DIM_AT + 4].copy_from_slice(&height.to_le_bytes());
    d[DIM_AT + 4..DIM_AT + 6].copy_from_slice(&1u16.to_le_bytes());
    d[DIM_AT + 6] = BGRA8888;
    d[DIM_AT + 7] = 1;
    let table_offset = if kinds.is_empty() {
        0
    } else {
        (VTEX_HEADER - EXTRA_OFFSET_AT) as u32
    };
    d[EXTRA_OFFSET_AT..EXTRA_OFFSET_AT + 4].copy_from_slice(&table_offset.to_le_bytes());
    d[EXTRA_OFFSET_AT + 4..EXTRA_OFFSET_AT + 8]
        .copy_from_slice(&(kinds.len() as u32).to_le_bytes());
    let mut payload_at = VTEX_HEADER + EXTRA_ENTRY * kinds.len();
    for (kind, payload) in kinds.iter().zip(payloads) {
        let field = d.len() + 4;
        d.extend_from_slice(&kind.to_le_bytes());
        d.extend_from_slice(&((payload_at - field) as u32).to_le_bytes());
        d.extend_from_slice(&(payload.len() as u32).to_le_bytes());
        payload_at += payload.len();
    }
    for payload in payloads {
        d.extend_from_slice(payload);
    }
    d
}

fn check(
    out: &[u8],
    width: u16,
    height: u16,
    kinds: &[u32],
    payloads: &[&[u8]],
) -> Result<(), EncodeError> {
    let fail = |what: &str| Err(EncodeError::Rebuilt(what.to_string()));
    let back = Vtex::parse(out)?;
    if (
        back.width,
        back.height,
        back.depth,
        back.format.0,
        back.mips.len(),
    ) != (width, height, 1, BGRA8888, 1)
    {
        return fail("size, format or mip count");
    }
    if !back.flags.contains(Flags::NO_LOD) || back.display_rect.is_some() {
        return fail("flags or display rect");
    }
    if back.pixel_start() + back.pixel_len() != out.len() {
        return fail("pixel data length");
    }
    let extras = vtex::extras(out, back.header_at())?;
    let same = extras.len() == kinds.len()
        && extras
            .iter()
            .zip(kinds.iter().zip(payloads))
            .all(|(e, (k, p))| e.kind == *k && out.get(e.payload.clone()) == Some(*p));
    if !same {
        return fail("extra data");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::addons::native_scope::tests::plain_vtex;
    use crate::addons::sources;
    use crate::hud::vpk::VpkDir;
    use crate::texture::png::tests::pattern;
    use crate::texture::vtex::tests::{COLOR, NOLOD, TINY, synthetic};

    fn vindicta() -> Vec<u8> {
        VpkDir::open(&sources::tests::research(
            "Vindicta Scope Downscale",
            "pak89_dir.vpk",
        ))
        .unwrap()
        .read(crate::addons::native_scope::TEXTURE)
        .unwrap()
    }

    fn pixels(out: &[u8]) -> &[u8] {
        &out[Vtex::parse(out).unwrap().pixel_start()..]
    }

    fn bgra_at(out: &[u8], width: usize, x: usize, y: usize) -> [u8; 4] {
        let p = pixels(out);
        let i = (y * width + x) * 4;
        [p[i], p[i + 1], p[i + 2], p[i + 3]]
    }

    fn block(bytes: &[u8], name: &[u8; 4]) -> Vec<u8> {
        Resource::parse(bytes)
            .unwrap()
            .block(name)
            .unwrap()
            .data
            .clone()
    }

    #[test]
    fn own_size_keeps_every_pixel_and_the_games_extra_data() {
        let up = vindicta();
        let img = pattern(300, 200);
        let out = replace(&up, &img, Fit::Own).unwrap();
        let v = Vtex::parse(&out).unwrap();
        assert_eq!(
            (v.width, v.height, v.depth, v.format.0),
            (300, 200, 1, BGRA8888)
        );
        assert_eq!(v.flags, Flags::NO_LOD);
        assert_eq!(v.mips.len(), 1);
        assert_eq!(v.pixel_len(), 300 * 200 * 4);
        assert_eq!(v.pixel_start() + v.pixel_len(), out.len());
        assert_eq!(
            pixels(&out),
            to_bgra(&img),
            "pixels are the image, swizzled"
        );
        assert_eq!(block(&out, b"RED2"), block(&up, b"RED2"));
        let theirs = Vtex::parse(&up).unwrap();
        let (a, b) = (
            vtex::extras(&out, v.header_at()).unwrap(),
            vtex::extras(&up, theirs.header_at()).unwrap(),
        );
        assert_eq!(a.len(), 1);
        assert_eq!(
            out[a[0].payload.clone()],
            up[b[0].payload.clone()],
            "FALLBACK_BITS kept"
        );
        let (ours, game) = (block(&out, b"DATA"), block(&up, b"DATA"));
        assert_eq!(ours.len(), game.len());
        assert_eq!(
            ours[..DIM_AT],
            game[..DIM_AT],
            "version, flags, reflectivity"
        );
    }

    #[test]
    fn original_size_letterboxes_with_transparency() {
        let up = vindicta();
        let img = RgbaImage::new(300, 200, [10, 20, 30, 255].repeat(300 * 200)).unwrap();
        let out = replace(&up, &img, Fit::Original).unwrap();
        let v = Vtex::parse(&out).unwrap();
        assert_eq!((v.width, v.height, v.mips.len()), (1080, 1080, 1));
        assert_eq!(v.pixel_start() + 1080 * 1080 * 4, out.len());
        assert_eq!(
            bgra_at(&out, 1080, 540, 0),
            [0, 0, 0, 0],
            "top band transparent"
        );
        assert_eq!(bgra_at(&out, 1080, 540, 179), [0, 0, 0, 0]);
        assert_eq!(
            bgra_at(&out, 1080, 0, 180),
            [30, 20, 10, 255],
            "image starts at row 180"
        );
        assert_eq!(bgra_at(&out, 1080, 1079, 899), [30, 20, 10, 255]);
        assert_eq!(
            bgra_at(&out, 1080, 540, 900),
            [0, 0, 0, 0],
            "bottom band transparent"
        );
    }

    #[test]
    fn fill_crops_evenly_and_stretch_squeezes() {
        let mut pixels = Vec::new();
        for _ in 0..2 {
            for x in 0..4 {
                pixels.extend_from_slice(&[x * 60, 0, 0, 255]);
            }
        }
        let img = RgbaImage::new(4, 2, pixels).unwrap();
        let fill = sized(&img, Fit::Fill, (2, 2));
        assert_eq!((fill.width, fill.height), (2, 2));
        assert_eq!(fill.pixels[0], 60, "the left column is cropped off");
        assert_eq!(fill.pixels[4], 120, "and the right one");
        let stretch = sized(&img, Fit::Stretch, (2, 4));
        assert_eq!((stretch.width, stretch.height), (2, 4));
        assert!(stretch.pixels.chunks(4).all(|p| p[3] == 255), "no bars");
        let own = sized(&img, Fit::Own, (64, 64));
        assert_eq!((own.width, own.height), (4, 2));
        let up = vindicta();
        let out = replace(&up, &pattern(300, 200), Fit::Fill).unwrap();
        let v = Vtex::parse(&out).unwrap();
        assert_eq!((v.width, v.height), (1080, 1080));
        assert_ne!(bgra_at(&out, 1080, 540, 0)[3], 0, "no transparent band");
    }

    #[test]
    fn same_size_is_exact_and_downscale_keeps_edges_clean() {
        let src = plain_vtex(37, 21, 28, 1, Flags::NO_LOD.0, &[], &vec![0; 37 * 21 * 4]);
        let img = pattern(37, 21);
        assert_eq!(
            pixels(&replace(&src, &img, Fit::Original).unwrap()),
            to_bgra(&img)
        );

        let mut row = vec![0u8; 4 * 4];
        row[4..8].copy_from_slice(&[255, 0, 0, 255]);
        row[12..16].copy_from_slice(&[255, 0, 0, 255]);
        let red_and_clear = RgbaImage::new(4, 1, row).unwrap();
        let dst = plain_vtex(2, 1, 28, 1, Flags::NO_LOD.0, &[], &[0; 8]);
        let out = replace(&dst, &red_and_clear, Fit::Original).unwrap();
        assert_eq!(
            pixels(&out)[..4],
            [0, 0, 255, 128],
            "pure red at half alpha, no dark fringe"
        );
    }

    #[test]
    fn own_size_is_clamped_to_max_side() {
        let src = plain_vtex(8, 8, 28, 1, Flags::NO_LOD.0, &[], &[0; 256]);
        let img = RgbaImage::new(5000, 100, vec![200; 5000 * 100 * 4]).unwrap();
        let out = replace(&src, &img, Fit::Own).unwrap();
        let v = Vtex::parse(&out).unwrap();
        assert_eq!((v.width, v.height), (4096, 82));
        assert!(pixels(&out).iter().all(|&p| p == 200));
    }

    #[test]
    fn a_compressed_source_with_display_rect_and_size_table_becomes_plain_bgra() {
        let mut meta = vec![0u8; 128];
        meta[2..4].copy_from_slice(&100u16.to_le_bytes());
        meta[4..6].copy_from_slice(&60u16.to_le_bytes());
        let mut sizes = Vec::new();
        for n in [1u32, 8, 1, 500] {
            sizes.extend_from_slice(&n.to_le_bytes());
        }
        let fallback = vec![0xab; 64];
        let src = plain_vtex(
            128,
            64,
            20,
            1,
            Flags::NO_LOD.0,
            &[
                (1, fallback.clone()),
                (EXTRA_METADATA, meta),
                (EXTRA_COMPRESSED_MIP_SIZE, sizes),
            ],
            &[7; 500],
        );
        assert_eq!(Vtex::parse(&src).unwrap().display_rect, Some((100, 60)));
        let out = replace(&src, &pattern(50, 30), Fit::Original).unwrap();
        let v = Vtex::parse(&out).unwrap();
        assert_eq!((v.width, v.height, v.format.0), (100, 60, BGRA8888));
        assert_eq!(v.display_rect, None);
        let extras = vtex::extras(&out, v.header_at()).unwrap();
        assert_eq!(extras.iter().map(|e| e.kind).collect::<Vec<_>>(), [1]);
        assert_eq!(out[extras[0].payload.clone()], fallback);

        let out = replace(NOLOD, &pattern(16, 16), Fit::Original).unwrap();
        let v = Vtex::parse(&out).unwrap();
        assert_eq!((v.width, v.height, v.format.0), (1024, 1024, BGRA8888));
        assert_eq!(v.pixel_start() + v.pixel_len(), out.len());
    }

    #[test]
    fn refuses_what_is_not_a_panorama_image() {
        let img = pattern(4, 4);
        assert!(matches!(
            replace(COLOR, &img, Fit::Original),
            Err(EncodeError::Mipmapped {
                mips: 8,
                no_lod: false
            })
        ));
        assert!(matches!(
            replace(TINY, &img, Fit::Original),
            Err(EncodeError::Mipmapped {
                mips: 1,
                no_lod: false
            })
        ));
        assert!(matches!(
            replace(
                &synthetic(64, 5, &[2048, 512, 128, 32, 8], (0, 0)),
                &img,
                Fit::Own
            ),
            Err(EncodeError::Mipmapped { .. })
        ));
        let px = vec![0u8; 8 * 8 * 4];
        for flag in [Flags::CUBE, Flags::VOLUME, Flags::ARRAY] {
            let cube = plain_vtex(8, 8, 28, 1, flag.0 | Flags::NO_LOD.0, &[], &px);
            assert!(matches!(
                replace(&cube, &img, Fit::Own),
                Err(EncodeError::Layered { .. })
            ));
        }
        let sheet = plain_vtex(
            8,
            8,
            28,
            1,
            Flags::NO_LOD.0,
            &[(EXTRA_SHEET, vec![0; 16])],
            &px,
        );
        assert!(matches!(
            replace(&sheet, &img, Fit::Own),
            Err(EncodeError::Sheet)
        ));
        let plain = plain_vtex(8, 8, 28, 1, Flags::NO_LOD.0, &[], &px);
        let empty = RgbaImage::new(0, 0, Vec::new()).unwrap();
        assert!(matches!(
            replace(&plain, &empty, Fit::Own),
            Err(EncodeError::EmptyImage)
        ));
        assert!(matches!(
            replace(&plain[..30], &img, Fit::Own),
            Err(EncodeError::Vtex(_))
        ));
    }

    #[test]
    fn fit_keys_round_trip() {
        for fit in Fit::ALL {
            assert_eq!(Fit::parse(fit.key()), Some(fit));
            let text = toml::to_string(&std::collections::BTreeMap::from([("fit", fit)])).unwrap();
            assert_eq!(text.trim(), format!("fit = \"{}\"", fit.key()));
        }
        assert_eq!(Fit::parse("squash"), None);
        assert_eq!(Fit::default(), Fit::Original);
    }
}
