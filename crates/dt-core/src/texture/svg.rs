//! Panorama vector images (`.vsvg_c`). Read as a Panorama resource: DATA holds a CRC, an
//! (empty) image table and then the SVG text, the layout `hud::resource` already rewrites for
//! stylesheets. That layout is inferred from ValveResourceFormat's shared Panorama reader and
//! not yet checked against a real game file, so both directions refuse a container whose
//! text is not an SVG document rather than guess. `rasterize` (feature `svg`, resvg) draws
//! an icon for previews. `palette`, `remap` and `adjust` edit the colours in the text.

use std::collections::BTreeMap;

use crate::hud::resource::{self, Resource, ResourceError};
use crate::texture::adjust::{Adjust, Rgb, adjust_rgb};
#[cfg(feature = "svg")]
use crate::texture::png::RgbaImage;

#[derive(Debug, thiserror::Error)]
pub enum SvgError {
    #[error(transparent)]
    Resource(#[from] ResourceError),
    #[error("the game's file does not hold SVG text where expected: {0}")]
    Container(String),
    #[error("not a usable SVG: {0}")]
    Invalid(String),
}

/// The SVG root's coordinate box: its `viewBox`, else `0 0 width height`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ViewBox {
    pub min_x: f64,
    pub min_y: f64,
    pub width: f64,
    pub height: f64,
}

/// The SVG text of a compiled vector image.
pub fn svg_text(compiled: &[u8]) -> Result<String, SvgError> {
    let res = Resource::parse(compiled)?;
    let text = resource::style_text(&res)?;
    validate(text).map_err(|e| SvgError::Container(e.to_string()))?;
    Ok(text.to_string())
}

/// `original` (a whole `.vsvg_c`) carrying `svg` instead of its own text; every other block
/// and the image table stay the game's.
pub fn with_svg_text(original: &[u8], svg: &str) -> Result<Vec<u8>, SvgError> {
    let res = Resource::parse(original)?;
    validate(resource::style_text(&res)?).map_err(|e| SvgError::Container(e.to_string()))?;
    validate(svg)?;
    Ok(resource::with_style_text(&res, svg)?.to_bytes())
}

mod colors;

use colors::Paint;

/// One colour the icon uses and how many places use it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Swatch {
    pub rgb: Rgb,
    pub uses: usize,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Palette {
    pub colors: Vec<Swatch>,
    pub current_color: bool,
}

/// Every colour the SVG text names, most used first (ties by hex), plus whether
/// `currentColor` appears.
pub fn palette(svg: &str) -> Palette {
    let mut uses = BTreeMap::<Rgb, usize>::new();
    let mut current_color = false;
    for token in colors::tokens(svg) {
        match token.paint {
            Paint::Color(rgb) => *uses.entry(rgb).or_default() += 1,
            Paint::Current => current_color = true,
        }
    }
    let mut colors: Vec<Swatch> = uses
        .into_iter()
        .map(|(rgb, uses)| Swatch { rgb, uses })
        .collect();
    colors.sort_by_key(|s| std::cmp::Reverse(s.uses));
    Palette {
        colors,
        current_color,
    }
}

/// `svg` with every colour mapped through `f`; everything else byte for byte, and a colour
/// `f` returns unchanged keeps its original spelling.
pub fn map_colors(svg: &str, mut f: impl FnMut(Rgb) -> Rgb) -> String {
    let mut out = String::with_capacity(svg.len());
    let mut done = 0;
    for token in colors::tokens(svg) {
        let Paint::Color(rgb) = token.paint else {
            continue;
        };
        let new = f(rgb);
        if new != rgb {
            out.push_str(&svg[done..token.at.start]);
            out.push_str(&colors::rewrite(&svg[token.at.clone()], new));
            done = token.at.end;
        }
    }
    out.push_str(&svg[done..]);
    out
}

/// `svg` with each key colour replaced by its value (exact matches only).
pub fn remap(svg: &str, map: &BTreeMap<Rgb, Rgb>) -> String {
    map_colors(svg, |rgb| map.get(&rgb).copied().unwrap_or(rgb))
}

/// `svg` with every colour in it (fills, strokes, gradient stops, style attributes and
/// `<style>` rules) put through `list` in order via `adjust_rgb`, `Swap` entries matched
/// exactly, and `Opacity` multiplied into the root's `opacity`; everything else byte for
/// byte.
pub fn adjust(svg: &str, list: &[Adjust]) -> String {
    let mapped = map_colors(svg, |rgb| {
        list.iter().fold(rgb, |color, op| match *op {
            Adjust::Swap { from, to } if color == from => to,
            Adjust::Swap { .. } | Adjust::Opacity { .. } => color,
            _ => Rgb(adjust_rgb(color.0, op)),
        })
    });
    let factor: f64 = list
        .iter()
        .filter_map(|op| match *op {
            Adjust::Opacity { percent } => Some(f64::from(percent) / 100.0),
            _ => None,
        })
        .product();
    if factor == 1.0 {
        return mapped;
    }
    with_root_opacity(&mapped, factor)
}

/// `svg` with the root element's `opacity` multiplied by `factor` (added when absent),
/// rounded to three decimals.
fn with_root_opacity(svg: &str, factor: f64) -> String {
    let mut at = 0;
    let root = loop {
        let Some(open) = svg[at..].find('<').map(|i| at + i) else {
            return svg.to_string();
        };
        let rest = &svg[open..];
        if !(rest.starts_with("<?") || rest.starts_with("<!")) {
            break open + 1;
        }
        let end = if rest.starts_with("<!--") { "-->" } else { ">" };
        let Some(past) = rest.find(end) else {
            return svg.to_string();
        };
        at = open + past + end.len();
    };
    let Some(end) = tag_end(&svg[root - 1..]) else {
        return svg.to_string();
    };
    let tag = &svg[root..root - 1 + end];
    let old = colors::attributes(tag)
        .into_iter()
        .find(|(name, _)| *name == "opacity")
        .map(|(_, value)| root + value.start..root + value.end);
    let current = old.clone().map_or(1.0, |at| {
        let text = svg[at].trim();
        match text.strip_suffix('%') {
            Some(p) => p.trim().parse().map_or(1.0, |p: f64| p / 100.0),
            None => text.parse().unwrap_or(1.0),
        }
    });
    let value = ((current * factor).clamp(0.0, 1.0) * 1000.0).round() / 1000.0;
    match old {
        Some(at) => format!("{}{value}{}", &svg[..at.start], &svg[at.end..]),
        None => {
            let name = root + tag.find(char::is_whitespace).unwrap_or(tag.len());
            let name = name.min(root + tag.trim_end_matches('/').len());
            format!("{} opacity=\"{value}\"{}", &svg[..name], &svg[name..])
        }
    }
}

/// An SVG document wrapping `png` as an embedded image filling `view`, so a raster image can
/// stand in for a vector one. Whether Panorama's SVG renderer draws embedded rasters is not
/// known yet.
pub fn png_in_svg(png: &[u8], view: ViewBox) -> String {
    let data = format!("data:image/png;base64,{}", base64(png));
    let ViewBox {
        min_x,
        min_y,
        width,
        height,
    } = view;
    format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" xmlns:xlink=\"http://www.w3.org/1999/xlink\" \
         width=\"{width}\" height=\"{height}\" viewBox=\"{min_x} {min_y} {width} {height}\">\
         <image x=\"{min_x}\" y=\"{min_y}\" width=\"{width}\" height=\"{height}\" \
         preserveAspectRatio=\"xMidYMid meet\" href=\"{data}\" xlink:href=\"{data}\"/></svg>"
    )
}

/// Rasterises `svg` so its longer side is exactly `max_side` pixels (up or down; icons are
/// tiny), straight (not premultiplied) RGBA.
#[cfg(feature = "svg")]
pub fn rasterize(svg: &str, max_side: u32) -> Result<RgbaImage, SvgError> {
    use resvg::{tiny_skia, usvg};

    let tree = usvg::Tree::from_str(svg, &usvg::Options::default())
        .map_err(|e| SvgError::Invalid(e.to_string()))?;
    let (w, h) = (tree.size().width(), tree.size().height());
    let scale = max_side as f32 / w.max(h);
    let shorter = ((w.min(h) * scale).ceil() as u32).max(1);
    let (width, height) = if w >= h {
        (max_side, shorter)
    } else {
        (shorter, max_side)
    };
    let mut pixmap = tiny_skia::Pixmap::new(width, height)
        .ok_or_else(|| SvgError::Invalid("the image has no size".into()))?;
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

/// Checks `svg` is one well-formed element tree with an `<svg>` root that has a positive
/// size, and returns that size.
pub fn validate(svg: &str) -> Result<ViewBox, SvgError> {
    let bad = |what: &str| Err(SvgError::Invalid(what.to_string()));
    let mut stack: Vec<&str> = Vec::new();
    let mut root: Option<&str> = None;
    let mut rest = svg.trim_start_matches('\u{feff}');
    loop {
        let Some(open) = rest.find('<') else {
            if !rest.trim().is_empty() && root.is_none() {
                return bad("text before the <svg> element");
            }
            break;
        };
        if root.is_none() && !rest[..open].trim().is_empty() {
            return bad("text before the <svg> element");
        }
        rest = &rest[open..];
        let skip = |end: &str| rest.find(end).map(|at| at + end.len());
        if rest.starts_with("<!--") {
            let Some(at) = skip("-->") else {
                return bad("unterminated comment");
            };
            rest = &rest[at..];
            continue;
        }
        if rest.starts_with("<![CDATA[") {
            let Some(at) = skip("]]>") else {
                return bad("unterminated CDATA");
            };
            rest = &rest[at..];
            continue;
        }
        if rest.starts_with("<?") || rest.starts_with("<!") {
            if root.is_some() && rest.starts_with("<!") {
                return bad("a declaration inside the document");
            }
            let Some(at) = skip(">") else {
                return bad("unterminated declaration");
            };
            rest = &rest[at..];
            continue;
        }
        let end = tag_end(rest).ok_or(SvgError::Invalid("unterminated tag".into()))?;
        let tag = &rest[1..end];
        rest = &rest[end + 1..];
        if let Some(name) = tag.strip_prefix('/') {
            match stack.pop() {
                Some(open) if open == name.trim() => {}
                _ => return bad("mismatched closing tag"),
            }
            continue;
        }
        let self_closing = tag.ends_with('/');
        let body = tag.trim_end_matches('/');
        let name = body.split(|c: char| c.is_whitespace()).next().unwrap_or("");
        if name.is_empty() {
            return bad("an element without a name");
        }
        if stack.is_empty() {
            if root.is_some() {
                return bad("more than one root element");
            }
            if name != "svg" {
                return bad("the root element is not <svg>");
            }
            root = Some(body);
        }
        if !self_closing {
            stack.push(name);
        }
    }
    if !stack.is_empty() {
        return bad("unclosed elements");
    }
    let Some(root) = root else {
        return bad("no <svg> element");
    };
    root_size(root)
}

/// Offset of the `>` closing the tag at the start of `text`, skipping quoted values.
fn tag_end(text: &str) -> Option<usize> {
    let mut quote = None;
    for (i, c) in text.char_indices().skip(1) {
        match (quote, c) {
            (Some(q), c) if c == q => quote = None,
            (None, '"' | '\'') => quote = Some(c),
            (None, '>') => return Some(i),
            (None, '<') => return None,
            _ => {}
        }
    }
    None
}

fn attribute<'a>(tag: &'a str, name: &str) -> Option<&'a str> {
    let mut rest = tag;
    while let Some(at) = rest.find(name) {
        let before = rest[..at].chars().next_back();
        let after = rest[at + name.len()..].trim_start();
        rest = &rest[at + name.len()..];
        if !before.is_some_and(char::is_whitespace) {
            continue;
        }
        let Some(value) = after.strip_prefix('=') else {
            continue;
        };
        let value = value.trim_start();
        let quote = value.chars().next().filter(|c| *c == '"' || *c == '\'')?;
        let inner = &value[1..];
        return inner.find(quote).map(|end| &inner[..end]);
    }
    None
}

fn length(text: &str) -> Option<f64> {
    let n: f64 = text.trim().trim_end_matches("px").trim().parse().ok()?;
    (n.is_finite() && n > 0.0).then_some(n)
}

fn root_size(tag: &str) -> Result<ViewBox, SvgError> {
    if let Some(view) = attribute(tag, "viewBox") {
        let n: Vec<f64> = view
            .split(|c: char| c.is_whitespace() || c == ',')
            .filter(|s| !s.is_empty())
            .map(str::parse)
            .collect::<Result<_, _>>()
            .map_err(|_| SvgError::Invalid(format!("viewBox {view:?} is not four numbers")))?;
        return match n[..] {
            [min_x, min_y, width, height] if width > 0.0 && height > 0.0 => Ok(ViewBox {
                min_x,
                min_y,
                width,
                height,
            }),
            _ => Err(SvgError::Invalid(format!(
                "viewBox {view:?} needs four numbers with a positive size"
            ))),
        };
    }
    match (
        attribute(tag, "width").and_then(length),
        attribute(tag, "height").and_then(length),
    ) {
        (Some(width), Some(height)) => Ok(ViewBox {
            min_x: 0.0,
            min_y: 0.0,
            width,
            height,
        }),
        _ => Err(SvgError::Invalid(
            "the <svg> element has neither a viewBox nor a width and height".into(),
        )),
    }
}

fn base64(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let n = chunk
            .iter()
            .enumerate()
            .fold(0u32, |n, (i, &b)| n | u32::from(b) << (16 - 8 * i));
        for i in 0..4 {
            if i <= chunk.len() {
                out.push(ALPHABET[(n >> (18 - 6 * i) & 63) as usize] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::hud::crc32::crc32;
    use crate::hud::resource::Block;

    pub const GAME_SVG: &str = "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 24 24\"><path d=\"M0 0h24v24z\"/></svg>";

    /// A vsvg_c as the Panorama reader lays it out: RED2, then DATA with a CRC prefix whose
    /// source part is `0x1234_5678`, an empty image table and `svg`.
    pub fn compiled(svg: &str) -> Vec<u8> {
        let mut data = (0x1234_5678 ^ crc32(svg.as_bytes())).to_le_bytes().to_vec();
        data.extend_from_slice(&0u16.to_le_bytes());
        data.extend_from_slice(svg.as_bytes());
        Resource {
            header_version: 12,
            type_version: 3,
            blocks: vec![
                Block {
                    name: *b"RED2",
                    data: vec![9; 24],
                },
                Block {
                    name: *b"DATA",
                    data,
                },
            ],
        }
        .to_bytes()
    }

    #[test]
    fn swaps_the_text_and_keeps_the_rest() {
        let game = compiled(GAME_SVG);
        assert_eq!(svg_text(&game).unwrap(), GAME_SVG);
        let mine = "<?xml version=\"1.0\"?>\n<!-- mine -->\n<svg width=\"32px\" height=\"16\"><g><circle r=\"4\"/></g></svg>\n";
        let out = with_svg_text(&game, mine).unwrap();
        assert_eq!(svg_text(&out).unwrap(), mine);
        let (a, b) = (
            Resource::parse(&out).unwrap(),
            Resource::parse(&game).unwrap(),
        );
        assert_eq!(a.block(b"RED2"), b.block(b"RED2"));
        assert_eq!(a.type_version, b.type_version);
        assert_eq!(
            resource::source_crc(&a).unwrap(),
            resource::source_crc(&b).unwrap()
        );
        assert_eq!(with_svg_text(&out, GAME_SVG).unwrap(), game, "and back");
    }

    #[test]
    fn refuses_a_container_that_does_not_hold_svg() {
        let css = include_bytes!("../../tests/fixtures/hud/hud_abilities_small.vcss_c");
        assert!(matches!(svg_text(css), Err(SvgError::Container(_))));
        assert!(matches!(
            with_svg_text(css, GAME_SVG),
            Err(SvgError::Container(_))
        ));
        assert!(matches!(
            with_svg_text(&compiled(GAME_SVG), "<svg>"),
            Err(SvgError::Invalid(_))
        ));
        assert!(matches!(svg_text(&[0; 8]), Err(SvgError::Resource(_))));
    }

    #[test]
    fn validates_structure_and_size() {
        let ok = |s: &str| validate(s).unwrap();
        assert_eq!(
            ok("<svg viewBox=\"-1 2, 30 40\"></svg>"),
            ViewBox {
                min_x: -1.0,
                min_y: 2.0,
                width: 30.0,
                height: 40.0
            }
        );
        assert_eq!(ok("\u{feff}<svg width='8' height='4'/>").width, 8.0);
        assert_eq!(
            ok("<svg viewBox=\"0 0 1 1\"><text>a &lt; b</text><![CDATA[<x>]]></svg>").height,
            1.0
        );
        assert_eq!(
            ok("<svg viewBox=\"0 0 1 1\" data-x=\"a>b\"><g/></svg>").width,
            1.0
        );
        for (svg, why) in [
            ("", "no <svg>"),
            ("hello", "text before"),
            ("<html></html>", "not <svg>"),
            ("<svg viewBox=\"0 0 1 1\"><g></svg>", "mismatched"),
            ("<svg viewBox=\"0 0 1 1\"><g>", "unclosed"),
            (
                "<svg viewBox=\"0 0 1 1\"/><svg viewBox=\"0 0 1 1\"/>",
                "more than one",
            ),
            ("<svg viewBox=\"0 0 0 1\"/>", "positive size"),
            ("<svg viewBox=\"a b c d\"/>", "four numbers"),
            ("<svg width=\"8\"/>", "neither a viewBox"),
            ("<svg viewBox=\"0 0 1 1\"><!-- open", "comment"),
            ("<svg viewBox=\"0 0 1 1\"", "unterminated tag"),
        ] {
            let err = validate(svg).unwrap_err().to_string();
            assert!(err.contains(why), "{svg:?}: {err}");
        }
    }

    #[test]
    fn wraps_a_png_in_the_games_view_box() {
        let png = [0x89, b'P', b'N', b'G', 0, 255];
        let view = validate(GAME_SVG).unwrap();
        let svg = png_in_svg(&png, view);
        assert_eq!(validate(&svg).unwrap(), view);
        assert!(
            svg.contains("href=\"data:image/png;base64,iVBORwD/\""),
            "{svg}"
        );
        assert!(svg.contains("xlink:href=\"data:image/png;base64,iVBORwD/\""));
        assert!(svg.contains("width=\"24\" height=\"24\""));
    }

    const GRADIENT: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 32 32">
  <defs>
    <linearGradient id="g" x1="0" y1="0" x2="0" y2="1">
      <stop offset="0" stop-color="#E6A000"/>
      <stop offset="0.5" stop-color='#ffffff'/>
      <stop offset="1" style="stop-color:#fff;stop-opacity:1"/>
    </linearGradient>
  </defs>
  <path id="ffffff" d="M0 0h32v32H0z" fill="url(#g)" fill-opacity="0.5" fill-rule="evenodd"/>
</svg>"##;

    const ILLUSTRATOR: &str = r##"<?xml version="1.0" encoding="utf-8"?>
<!-- Generator: Adobe Illustrator 27.0.0, SVG Export Plug-In . SVG Version: 6.00 Build 0)  -->
<svg version="1.1" id="Layer_1" xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink" x="0px" y="0px"
	 viewBox="0 0 64 64" style="enable-background:new 0 0 64 64;" xml:space="preserve">
<style type="text/css">
	.st0{fill:#FFFFFF;}
	.st1{fill:none;stroke:#E6A000;stroke-width:2;stroke-miterlimit:10;}
</style>
<path class="st0" d="M32,4L4,60h56L32,4z"/>
<circle class="st1" cx="32" cy="36" r="12"/>
<text class="st0">white #fff</text>
</svg>
"##;

    const INKSCAPE: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" width="24" height="24" viewBox="0 0 24 24">
  <g id="layer1">
    <path style="fill:#ece8e1;fill-opacity:1;stroke:none;stroke-width:0.5" d="m 2,2 h 20 v 20 h -20 z"/>
    <rect style="fill:#ece8e1;stroke:#000000;stroke-opacity:0.8" x="6" y="6" width="12" height="12"/>
  </g>
</svg>"##;

    const MIXED: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 16 16" color="#333">
  <circle r="4" fill="white" stroke="Gold"/>
  <rect width="2" height="2" fill="rgb(255, 0, 0)" stroke="rgba(0,128,255,0.5)"/>
  <rect width="3" height="3" fill="rgb(100%, 50%, 0%)" stroke="#fff"/>
  <path d="M0 0" fill="currentColor" stroke="#11223380"/>
  <rect fill="none" stroke="transparent" flood-color="inherit" lighting-color="#0f08"/>
</svg>"##;

    const CURRENT: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24"><path fill="currentColor" d="M2 2h20v20H2z"/><path stroke="currentColor" fill="none" d="M4 4h16"/></svg>"#;

    const STYLED: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 8 8"><style><![CDATA[
/* fill: red; } */
.a:hover { fill: Gold !important; stroke : rgb(10%, 20%, 30%) }
@media (min-width: 1px) { .b{stop-color:#abc} }
.c{fill-opacity:.5;color:#AbCdEf}
]]></style><rect class="a" width="8" height="8"/></svg>"##;

    const NOT_COLOURS: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 8 8" class="red"><rect id="white" class="gold" fill="none" stroke="transparent" color="inherit" fill-opacity="0.5" fill-rule="evenodd" stroke-width="2" stroke-linecap="round" d="M0 0" data-fill="red"/><path fill="url(#red)" style="fill-opacity:0.5;stroke-width:2"/><text x="1">white #fff rgb(1,2,3)</text></svg>"##;

    const SHAPES: [&str; 7] = [
        GRADIENT,
        ILLUSTRATOR,
        INKSCAPE,
        MIXED,
        CURRENT,
        STYLED,
        NOT_COLOURS,
    ];

    fn rgb(hex: &str) -> Rgb {
        hex.parse().unwrap()
    }

    fn swatches(svg: &str) -> Vec<(String, usize)> {
        palette(svg)
            .colors
            .iter()
            .map(|s| (s.rgb.hex(), s.uses))
            .collect()
    }

    fn map(pairs: &[(&str, &str)]) -> BTreeMap<Rgb, Rgb> {
        pairs.iter().map(|(a, b)| (rgb(a), rgb(b))).collect()
    }

    fn replaced(svg: &str, pairs: &[(&str, &str)]) -> String {
        pairs.iter().fold(svg.to_string(), |text, (from, to)| {
            assert_eq!(text.matches(from).count(), 1, "{from}");
            text.replacen(from, to, 1)
        })
    }

    fn valid(svg: &str) -> &str {
        validate(svg).unwrap_or_else(|e| panic!("{e}: {svg}"));
        svg
    }

    #[test]
    fn palette_counts_every_colour_most_used_first() {
        let owned = |list: &[(&str, usize)]| -> Vec<(String, usize)> {
            list.iter().map(|(h, n)| (h.to_string(), *n)).collect()
        };
        assert_eq!(swatches(GRADIENT), owned(&[("#ffffff", 2), ("#e6a000", 1)]));
        assert_eq!(
            swatches(ILLUSTRATOR),
            owned(&[("#e6a000", 1), ("#ffffff", 1)]),
            "ties by hex"
        );
        assert_eq!(swatches(INKSCAPE), owned(&[("#ece8e1", 2), ("#000000", 1)]));
        assert_eq!(
            swatches(MIXED),
            owned(&[
                ("#ffffff", 2),
                ("#0080ff", 1),
                ("#00ff00", 1),
                ("#112233", 1),
                ("#333333", 1),
                ("#ff0000", 1),
                ("#ff8000", 1),
                ("#ffd700", 1),
            ])
        );
        assert_eq!(
            swatches(STYLED),
            owned(&[
                ("#1a334d", 1),
                ("#aabbcc", 1),
                ("#abcdef", 1),
                ("#ffd700", 1)
            ])
        );
        assert!(palette(MIXED).current_color);
        assert!(!palette(GRADIENT).current_color);
        assert_eq!(
            palette(CURRENT),
            Palette {
                colors: vec![],
                current_color: true
            }
        );
        assert_eq!(palette(NOT_COLOURS), Palette::default());
    }

    #[test]
    fn unchanged_colours_keep_their_bytes() {
        for svg in SHAPES {
            assert_eq!(remap(svg, &BTreeMap::new()), svg);
            assert_eq!(map_colors(svg, |c| c), svg);
            assert_eq!(adjust(svg, &[]), svg);
        }
        assert_eq!(
            map_colors(NOT_COLOURS, |_| Rgb::BLACK),
            NOT_COLOURS,
            "none, transparent, inherit, url(), lookalike names, ids, classes and text"
        );
        assert_eq!(map_colors(CURRENT, |_| Rgb::BLACK), CURRENT);
    }

    #[test]
    fn remap_rewrites_only_the_colour_tokens() {
        let out = remap(ILLUSTRATOR, &map(&[("#e6a000", "#00ff00")]));
        assert_eq!(out, replaced(ILLUSTRATOR, &[("#E6A000", "#00ff00")]));
        let out = remap(valid(&out), &map(&[("#ffffff", "#000000")]));
        assert_eq!(
            out,
            replaced(
                ILLUSTRATOR,
                &[("#E6A000", "#00ff00"), ("fill:#FFFFFF", "fill:#000000")]
            ),
            "the text's #fff stays"
        );

        let out = remap(GRADIENT, &map(&[("#ffffff", "#102030")]));
        assert_eq!(
            valid(&out),
            replaced(
                GRADIENT,
                &[("'#ffffff'", "'#102030'"), (":#fff;", ":#102030;")]
            )
        );

        let out = remap(
            INKSCAPE,
            &map(&[("#ece8e1", "#d4860b"), ("#000000", "#ffffff")]),
        );
        assert_eq!(
            valid(&out),
            INKSCAPE
                .replace("#ece8e1", "#d4860b")
                .replace("stroke:#000000", "stroke:#ffffff")
        );
    }

    #[test]
    fn named_short_and_functional_forms_are_written_back() {
        let out = remap(
            MIXED,
            &map(&[
                ("#ffffff", "#123456"),
                ("#ffd700", "#abcdef"),
                ("#ff0000", "#010203"),
                ("#0080ff", "#090909"),
                ("#00ff00", "#ff00ff"),
                ("#112233", "#445566"),
            ]),
        );
        assert_eq!(
            valid(&out),
            replaced(
                MIXED,
                &[
                    ("fill=\"white\"", "fill=\"#123456\""),
                    ("stroke=\"Gold\"", "stroke=\"#abcdef\""),
                    ("rgb(255, 0, 0)", "rgb(1, 2, 3)"),
                    ("rgba(0,128,255,0.5)", "rgba(9, 9, 9, 0.5)"),
                    ("stroke=\"#fff\"", "stroke=\"#123456\""),
                    ("#11223380", "#44556680"),
                    ("#0f08", "#ff00ff88"),
                ]
            )
        );
        let out = remap(MIXED, &map(&[("#ff8000", "#000000")]));
        assert_eq!(
            out,
            replaced(MIXED, &[("rgb(100%, 50%, 0%)", "rgb(0, 0, 0)")])
        );
    }

    #[test]
    fn style_rules_are_css() {
        let out = remap(
            STYLED,
            &map(&[
                ("#ffd700", "#000000"),
                ("#1a334d", "#ffffff"),
                ("#aabbcc", "#010101"),
                ("#abcdef", "#020202"),
            ]),
        );
        assert_eq!(
            valid(&out),
            replaced(
                STYLED,
                &[
                    ("Gold", "#000000"),
                    ("rgb(10%, 20%, 30%)", "rgb(255, 255, 255)"),
                    ("#abc}", "#010101}"),
                    ("#AbCdEf", "#020202"),
                ]
            ),
            "the commented-out red stays"
        );
    }

    #[test]
    fn opacity_multiplies_the_roots() {
        let half = [Adjust::Opacity { percent: 50 }];
        let once = adjust(INKSCAPE, &half);
        assert_eq!(
            valid(&once),
            INKSCAPE.replacen("<svg ", "<svg opacity=\"0.5\" ", 1)
        );
        let twice = adjust(&once, &half);
        assert_eq!(
            valid(&twice),
            INKSCAPE.replacen("<svg ", "<svg opacity=\"0.25\" ", 1)
        );
        assert_eq!(adjust(INKSCAPE, &[half[0], half[0]]), twice);
        let own = "<svg opacity='0.8' viewBox=\"0 0 1 1\"><path opacity='0.8'/></svg>";
        assert_eq!(
            adjust(own, &[Adjust::Opacity { percent: 33 }]),
            "<svg opacity='0.264' viewBox=\"0 0 1 1\"><path opacity='0.8'/></svg>"
        );
        let out = adjust(ILLUSTRATOR, &half);
        assert_eq!(
            valid(&out),
            ILLUSTRATOR.replacen("<svg version", "<svg opacity=\"0.5\" version", 1),
            "after the prolog and comment"
        );
        assert_eq!(
            adjust(INKSCAPE, &[Adjust::Opacity { percent: 100 }]),
            INKSCAPE
        );
    }

    #[test]
    fn swaps_through_adjust_match_remap_in_list_order() {
        let swap = |from: &str, to: &str| Adjust::Swap {
            from: rgb(from),
            to: rgb(to),
        };
        let out = adjust(
            MIXED,
            &[swap("#ffffff", "#ff0000"), swap("#ff0000", "#0000ff")],
        );
        assert_eq!(
            valid(&out),
            remap(
                MIXED,
                &map(&[("#ffffff", "#0000ff"), ("#ff0000", "#0000ff")])
            )
        );
        let out = adjust(
            GRADIENT,
            &[swap("#e6a000", "#00ff00"), Adjust::Opacity { percent: 50 }],
        );
        assert_eq!(
            valid(&out),
            remap(GRADIENT, &map(&[("#e6a000", "#00ff00")])).replacen(
                "<svg ",
                "<svg opacity=\"0.5\" ",
                1
            )
        );
    }

    #[test]
    fn colour_operations_go_through_adjust_rgb() {
        let out = adjust(INKSCAPE, &[Adjust::Invert]);
        assert_eq!(
            valid(&out),
            remap(
                INKSCAPE,
                &map(&[("#ece8e1", "#13171e"), ("#000000", "#ffffff")])
            )
        );
    }

    #[cfg(feature = "svg")]
    const RECT: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" width="8" height="8"><rect width="4" height="8" fill="#f00"/></svg>"##;

    #[cfg(feature = "svg")]
    #[test]
    fn rasterizes_at_native_size() {
        let img = rasterize(RECT, 8).unwrap();
        assert_eq!((img.width, img.height, img.pixels.len()), (8, 8, 256));
        assert_eq!(img.pixel(1, 4), [255, 0, 0, 255]);
        assert_eq!(img.pixel(6, 4)[3], 0);
    }

    #[cfg(feature = "svg")]
    #[test]
    fn scales_longer_side_to_max() {
        let img = rasterize(GAME_SVG, 64).unwrap();
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
        let [r, g, b, a] = rasterize(svg, 4).unwrap().pixel(2, 2);
        assert!(a.abs_diff(128) <= 2, "alpha {a}");
        assert!(r >= 250, "red {r}");
        assert_eq!((g, b), (0, 0));
    }

    #[cfg(feature = "svg")]
    #[test]
    fn malformed_is_an_error() {
        assert!(matches!(rasterize("<svg", 8), Err(SvgError::Invalid(_))));
        assert!(matches!(rasterize("not xml", 8), Err(SvgError::Invalid(_))));
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

    /// `DEADTUNE_GAME_SAMPLES=<snapshot folder>`: every `.vsvg_c` under `raw/panorama/images`
    /// must hold SVG text (and rasterise, with the feature), survive an empty `remap` byte for
    /// byte, have a palette when it names a hex colour, and stay valid with every colour
    /// inverted; prints a feature survey.
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
        let (mut colored, mut current) = (0, 0);
        let mut failures = Vec::new();
        for path in &files {
            let text = match svg_text(&std::fs::read(path).unwrap()) {
                Ok(text) => text,
                Err(e) => {
                    failures.push(format!("{}: {e}", path.display()));
                    continue;
                }
            };
            for (count, needle) in counts.iter_mut().zip(features) {
                *count += usize::from(text.contains(needle));
            }
            #[cfg(feature = "svg")]
            if let Err(e) = rasterize(&text, 64) {
                failures.push(format!("{}: {e}", path.display()));
            }
            if remap(&text, &BTreeMap::new()) != text {
                failures.push(format!("{}: an empty remap changed it", path.display()));
            }
            let found = palette(&text);
            let squeezed: String = text.chars().filter(|c| !c.is_whitespace()).collect();
            let hex_paint = ["fill", "stroke", "stop-color"].iter().any(|p| {
                ["=\"#", "='#", ":#"]
                    .iter()
                    .any(|sep| squeezed.contains(&format!("{p}{sep}")))
            });
            if hex_paint && found.colors.is_empty() {
                failures.push(format!("{}: a hex colour but no palette", path.display()));
            }
            colored += usize::from(!found.colors.is_empty());
            current += usize::from(found.current_color);
            let inverted = map_colors(&text, |c| Rgb(c.0.map(|v| 255 - v)));
            if let Err(e) = validate(&inverted) {
                failures.push(format!("{}: inverted: {e}", path.display()));
            }
            #[cfg(feature = "svg")]
            if let Err(e) = rasterize(&inverted, 64) {
                failures.push(format!("{}: inverted: {e}", path.display()));
            }
        }
        eprintln!("{} .vsvg_c files under {}", files.len(), images.display());
        eprintln!("  with colours    {colored}");
        eprintln!("  currentColor    {current}");
        for (needle, count) in features.iter().zip(counts) {
            eprintln!("  {needle:<16} {count}");
        }
        for failure in &failures {
            eprintln!("FAIL {failure}");
        }
        assert!(!files.is_empty(), "no .vsvg_c under {}", images.display());
        assert!(failures.is_empty(), "{} failures", failures.len());
    }

    #[test]
    fn base64_matches_the_rfc_vectors() {
        for (input, want) in [
            ("", ""),
            ("f", "Zg=="),
            ("fo", "Zm8="),
            ("foo", "Zm9v"),
            ("foob", "Zm9vYg=="),
            ("fooba", "Zm9vYmE="),
            ("foobar", "Zm9vYmFy"),
        ] {
            assert_eq!(base64(input.as_bytes()), want);
        }
    }
}
