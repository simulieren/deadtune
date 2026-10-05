//! Compiled vector icons (`.vsvg_c`): the SVG text in and out of the container, and a
//! rasteriser for previews (feature `svg`).

use crate::hud::resource::{self, Resource, ResourceError};
#[cfg(feature = "svg")]
use crate::texture::RgbaImage;

/// The SVG source inside a compiled `.vsvg_c`.
pub fn decode_svg(bytes: &[u8]) -> Result<String, ResourceError> {
    Ok(resource::style_text(&Resource::parse(bytes)?)?.to_string())
}

/// `original` with its SVG text replaced by `svg`; RED2 and the image table stay as they were.
pub fn with_svg_text(original: &[u8], svg: &str) -> Result<Vec<u8>, ResourceError> {
    Ok(resource::with_style_text(&Resource::parse(original)?, svg)?.to_bytes())
}

#[derive(Debug, thiserror::Error)]
pub enum SvgError {
    #[error("svg: {0}")]
    Parse(String),
    #[error("svg has no size")]
    Size,
}

/// Rasterises `svg` so its longer side is exactly `max_side` pixels (up or down; icons are
/// tiny), straight (not premultiplied) RGBA.
#[cfg(feature = "svg")]
pub fn rasterize(svg: &str, max_side: u32) -> Result<RgbaImage, SvgError> {
    use resvg::{tiny_skia, usvg};

    let tree = usvg::Tree::from_str(svg, &usvg::Options::default())
        .map_err(|e| SvgError::Parse(e.to_string()))?;
    let (w, h) = (tree.size().width(), tree.size().height());
    let scale = max_side as f32 / w.max(h);
    let shorter = ((w.min(h) * scale).ceil() as u32).max(1);
    let (width, height) = if w >= h {
        (max_side, shorter)
    } else {
        (shorter, max_side)
    };
    let mut pixmap = tiny_skia::Pixmap::new(width, height).ok_or(SvgError::Size)?;
    resvg::render(
        &tree,
        tiny_skia::Transform::from_scale(scale, scale),
        &mut pixmap.as_mut(),
    );
    let pixels = pixmap
        .pixels()
        .iter()
        .flat_map(|p| {
            let c = p.demultiply();
            [c.red(), c.green(), c.blue(), c.alpha()]
        })
        .collect();
    Ok(RgbaImage {
        width,
        height,
        pixels,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hud::inject::style_resource;

    const RECT: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" width="8" height="8"><rect width="4" height="8" fill="#f00"/></svg>"##;

    fn with_image_table(bytes: &[u8]) -> Vec<u8> {
        let mut res = Resource::parse(bytes).unwrap();
        let version = res.type_version;
        let data = &mut res
            .blocks
            .iter_mut()
            .find(|b| &b.name == b"DATA")
            .unwrap()
            .data;
        let mut table = 1u16.to_le_bytes().to_vec();
        table.extend_from_slice(b"file://{images}/icons/a.png\0");
        table.extend_from_slice(&[0x10, 0x00, 0x20, 0x00]);
        if version >= 3 {
            table.extend_from_slice(&[0xde, 0xad, 0xbe, 0xef]);
        }
        data.splice(4..6, table);
        res.to_bytes()
    }

    #[test]
    fn decodes_text() {
        assert_eq!(decode_svg(&style_resource(RECT)).unwrap(), RECT);
        let tabled = with_image_table(&style_resource(RECT));
        let res = Resource::parse(&tabled).unwrap();
        assert_eq!(&resource::image_table(&res).unwrap()[..2], &[1, 0]);
        assert_eq!(decode_svg(&tabled).unwrap(), RECT);
        assert!(decode_svg(b"<svg/>").is_err());
        assert!(decode_svg(&[]).is_err());
    }

    #[test]
    fn replaces_text_keeping_red2_and_images() {
        let original = with_image_table(&style_resource(RECT));
        let new = r#"<svg xmlns="http://www.w3.org/2000/svg" width="2" height="2"/>"#;
        let out = with_svg_text(&original, new).unwrap();
        assert_eq!(decode_svg(&out).unwrap(), new);

        let (before, after) = (
            Resource::parse(&original).unwrap(),
            Resource::parse(&out).unwrap(),
        );
        assert_eq!(after.block(b"RED2"), before.block(b"RED2"));
        assert_eq!(
            resource::image_table(&after).unwrap(),
            resource::image_table(&before).unwrap()
        );

        let same = with_svg_text(&original, &decode_svg(&original).unwrap()).unwrap();
        assert_eq!(same, original);
        assert!(with_svg_text(b"nope", new).is_err());
    }

    #[cfg(feature = "svg")]
    fn pixel(img: &RgbaImage, x: u32, y: u32) -> [u8; 4] {
        let i = ((y * img.width + x) * 4) as usize;
        img.pixels[i..i + 4].try_into().unwrap()
    }

    #[cfg(feature = "svg")]
    #[test]
    fn rasterizes_at_native_size() {
        let img = rasterize(RECT, 8).unwrap();
        assert_eq!((img.width, img.height, img.pixels.len()), (8, 8, 256));
        assert_eq!(pixel(&img, 1, 4), [255, 0, 0, 255]);
        assert_eq!(pixel(&img, 6, 4)[3], 0);
    }

    #[cfg(feature = "svg")]
    #[test]
    fn scales_longer_side_to_max() {
        let icon = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24"><circle cx="12" cy="12" r="8" fill="#fff"/></svg>"##;
        let img = rasterize(icon, 64).unwrap();
        assert_eq!((img.width, img.height), (64, 64));
        assert!(img.pixels.chunks(4).any(|p| p[3] == 255));

        let wide = r#"<svg xmlns="http://www.w3.org/2000/svg" width="40" height="10"><rect width="40" height="10"/></svg>"#;
        let img = rasterize(wide, 20).unwrap();
        assert_eq!((img.width, img.height), (20, 5));
    }

    #[cfg(feature = "svg")]
    #[test]
    fn renders_gradients_transforms_strokes_and_clips() {
        let svg = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 32 32">
            <defs>
                <linearGradient id="g" x1="0" y1="0" x2="1" y2="0">
                    <stop offset="0" stop-color="#00f"/><stop offset="1" stop-color="#ff0"/>
                </linearGradient>
                <clipPath id="c"><circle cx="16" cy="16" r="12"/></clipPath>
            </defs>
            <g transform="rotate(30 16 16)" clip-path="url(#c)">
                <rect x="2" y="2" width="28" height="28" fill="url(#g)" stroke="#0f0" stroke-width="2"/>
            </g>
        </svg>"##;
        let img = rasterize(svg, 32).unwrap();
        let distinct: std::collections::HashSet<&[u8]> = img.pixels.chunks(4).collect();
        assert!(distinct.len() > 2, "{}", distinct.len());
    }

    #[cfg(feature = "svg")]
    #[test]
    fn returns_straight_alpha() {
        let svg = r##"<svg xmlns="http://www.w3.org/2000/svg" width="4" height="4"><rect width="4" height="4" fill="#f00" fill-opacity="0.5"/></svg>"##;
        let [r, g, b, a] = pixel(&rasterize(svg, 4).unwrap(), 2, 2);
        assert!(a.abs_diff(128) <= 2, "alpha {a}");
        assert!(r >= 250, "red {r}");
        assert_eq!((g, b), (0, 0));
    }

    #[cfg(feature = "svg")]
    #[test]
    fn malformed_is_a_parse_error() {
        assert!(matches!(rasterize("<svg", 8), Err(SvgError::Parse(_))));
        assert!(matches!(rasterize("not xml", 8), Err(SvgError::Parse(_))));
    }

    fn vsvg_files(dir: &std::path::Path, out: &mut Vec<std::path::PathBuf>) {
        for entry in std::fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                vsvg_files(&path, out);
            } else if path.extension().is_some_and(|e| e == "vsvg_c") {
                out.push(path);
            }
        }
    }

    #[test]
    fn game_samples() {
        let Some(root) = std::env::var_os("DEADTUNE_GAME_SAMPLES") else {
            return;
        };
        let images = std::path::Path::new(&root).join("raw/panorama/images");
        let mut files = Vec::new();
        vsvg_files(&images, &mut files);
        files.sort();

        let features = [
            "<linearGradient",
            "<radialGradient",
            "<mask",
            "<clipPath",
            "<text",
            "<filter",
            "transform=",
            "stroke",
            "<use",
            "<style",
            "<image",
        ];
        let mut counts = [0usize; 11];
        let mut failures = Vec::new();
        for path in &files {
            let text = match decode_svg(&std::fs::read(path).unwrap()) {
                Ok(text) => text,
                Err(e) => {
                    failures.push(format!("{}: {e}", path.display()));
                    continue;
                }
            };
            let head = text.trim_start();
            if !(head.starts_with("<svg") || head.starts_with("<?xml")) {
                failures.push(format!(
                    "{}: starts with {:?}",
                    path.display(),
                    &head[..head.len().min(20)]
                ));
                continue;
            }
            for (count, needle) in counts.iter_mut().zip(features) {
                *count += usize::from(text.contains(needle));
            }
            #[cfg(feature = "svg")]
            if let Err(e) = rasterize(&text, 64) {
                failures.push(format!("{}: {e}", path.display()));
            }
        }

        eprintln!("{} .vsvg_c files under {}", files.len(), images.display());
        for (needle, count) in features.iter().zip(counts) {
            eprintln!("  {needle:<16} {count}");
        }
        for failure in &failures {
            eprintln!("FAIL {failure}");
        }
        assert!(!files.is_empty(), "no .vsvg_c under {}", images.display());
        assert!(failures.is_empty(), "{} failures", failures.len());
    }
}
