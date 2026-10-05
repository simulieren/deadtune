//! Panorama vector images (`.vsvg_c`). Read as a Panorama resource: DATA holds a CRC, an
//! (empty) image table and then the SVG text, the layout `hud::resource` already rewrites for
//! stylesheets. That layout is inferred from ValveResourceFormat's shared Panorama reader and
//! not yet checked against a real game file, so both directions refuse a container whose
//! text is not an SVG document rather than guess.

use crate::hud::resource::{self, Resource, ResourceError};

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
