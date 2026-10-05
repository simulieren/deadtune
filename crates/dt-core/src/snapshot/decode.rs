//! Compiled Panorama resources to something readable: stylesheets through
//! `resource::style_text`, scripts from their DATA block, layouts through
//! `inject::layout_text` (KV3 to XML), textures to PNG, vector icons to their SVG text.
//! What cannot be decoded gets its readable strings, so a format change never hides a file.

use super::store::Decoded;
use crate::hud::inject;
use crate::hud::resource::{self, Resource};
use crate::texture;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Style,
    Layout,
    Script,
    Texture,
    Vector,
}

impl Kind {
    pub fn of(path: &str) -> Option<Kind> {
        if path.ends_with(".vcss_c") {
            Some(Kind::Style)
        } else if path.ends_with(".vxml_c") {
            Some(Kind::Layout)
        } else if path.ends_with(".vjs_c") {
            Some(Kind::Script)
        } else if path.ends_with(".vtex_c") {
            Some(Kind::Texture)
        } else if path.ends_with(".vsvg_c") {
            Some(Kind::Vector)
        } else {
            None
        }
    }

    fn extension(self) -> &'static str {
        match self {
            Kind::Style => "css",
            Kind::Layout => "xml",
            Kind::Script => "js",
            Kind::Texture => "png",
            Kind::Vector => "svg",
        }
    }

    /// What the manifest records when `decode` succeeds.
    pub fn decoded(self) -> Decoded {
        match self {
            Kind::Texture => Decoded::Image,
            _ => Decoded::Text,
        }
    }

    /// `panorama/styles/hud.vcss_c` -> `panorama/styles/hud.css`.
    pub fn text_path(self, path: &str) -> String {
        let stem = path.rsplit_once('.').map_or(path, |(stem, _)| stem);
        format!("{stem}.{}", self.extension())
    }
}

/// The strings fallback lives next to the text it stands in for.
pub fn strings_path(path: &str) -> String {
    format!("{path}.strings.txt")
}

/// The readable form: UTF-8 text, or PNG bytes for a texture.
pub fn decode(kind: Kind, bytes: &[u8]) -> Result<Vec<u8>, String> {
    let text = |t: String| t.into_bytes();
    match kind {
        Kind::Style => {
            let res = Resource::parse(bytes).map_err(|e| e.to_string())?;
            resource::style_text(&res)
                .map(|t| text(t.to_string()))
                .map_err(|e| e.to_string())
        }
        Kind::Layout => inject::layout_text(bytes)
            .map(text)
            .map_err(|e| e.to_string()),
        Kind::Script => {
            let res = Resource::parse(bytes).map_err(|e| e.to_string())?;
            if res.type_version >= inject::SCRIPT_TYPE_VERSION {
                let data = res
                    .block(b"DATA")
                    .ok_or_else(|| "missing DATA block".to_string())?;
                return std::str::from_utf8(&data.data)
                    .map(|t| text(t.to_string()))
                    .map_err(|_| "script text is not UTF-8".to_string());
            }
            resource::style_text(&res)
                .map(|t| text(t.to_string()))
                .map_err(|e| e.to_string())
        }
        Kind::Texture => texture::decode(bytes)
            .map(|image| texture::png::write(&image))
            .map_err(|e| e.to_string()),
        Kind::Vector => texture::svg::decode_svg(bytes)
            .map(text)
            .map_err(|e| e.to_string()),
    }
}

const MIN_RUN: usize = 4;

/// Runs of at least four printable ASCII bytes, one per line, repeats dropped, under a
/// header that says why this is not the decoded text.
pub fn strings(bytes: &[u8], error: &str) -> String {
    let mut out = format!(
        "DeadTune could not decode this file ({error}). Its readable strings follow, one per line.\n\n"
    );
    let mut seen = std::collections::HashSet::new();
    for run in bytes.split(|b| !(b.is_ascii_graphic() || *b == b' ' || *b == b'\t')) {
        if run.len() < MIN_RUN {
            continue;
        }
        let text = String::from_utf8_lossy(run);
        if seen.insert(text.to_string()) {
            out.push_str(&text);
            out.push('\n');
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const STYLE: &[u8] = include_bytes!("../../tests/fixtures/hud/hud_abilities_small.vcss_c");
    const LAYOUT: &[u8] = include_bytes!("../../tests/fixtures/hud/top_bar_vanilla.vxml_c");

    fn text(kind: Kind, bytes: &[u8]) -> String {
        String::from_utf8(decode(kind, bytes).unwrap()).unwrap()
    }

    #[test]
    fn each_kind_decodes_to_its_text() {
        let css = text(Kind::Style, STYLE);
        assert!(css.contains('{'), "{css}");
        let xml = text(Kind::Layout, LAYOUT);
        assert!(xml.starts_with("<root>"));
        assert!(xml.contains("CitadelHudTopBar"));
        let js = text(
            Kind::Script,
            &inject::script_resource("(function(){ $.Msg('x'); })();"),
        );
        assert_eq!(js, "(function(){ $.Msg('x'); })();");
        let prefixed = text(Kind::Script, &inject::style_resource("var a = 1;"));
        assert_eq!(
            prefixed, "var a = 1;",
            "older scripts carry the style prefix"
        );
        let svg = "<svg xmlns=\"http://www.w3.org/2000/svg\"/>";
        assert_eq!(text(Kind::Vector, &inject::style_resource(svg)), svg);
    }

    #[test]
    fn textures_decode_to_png() {
        let png = decode(Kind::Texture, crate::texture::vtex::tests::COLOR).unwrap();
        assert!(png.starts_with(b"\x89PNG"));
        let image = texture::png::read(&png).unwrap();
        assert_eq!((image.width, image.height), (512, 512));
        assert_eq!(Kind::Texture.decoded(), Decoded::Image);
        assert_eq!(Kind::Vector.decoded(), Decoded::Text);
        assert!(decode(Kind::Texture, b"nope").is_err());
    }

    #[test]
    fn paths_and_kinds() {
        assert_eq!(Kind::of("panorama/styles/hud.vcss_c"), Some(Kind::Style));
        assert_eq!(Kind::of("panorama/layout/hud.vxml_c"), Some(Kind::Layout));
        assert_eq!(Kind::of("panorama/scripts/x.vjs_c"), Some(Kind::Script));
        assert_eq!(
            Kind::of("panorama/images/minimap/gold_psd.vtex_c"),
            Some(Kind::Texture)
        );
        assert_eq!(
            Kind::of("panorama/images/hud/top_bar/icon_ultimate.vsvg_c"),
            Some(Kind::Vector)
        );
        assert_eq!(Kind::of("particles/empty.vpcf_c"), None);
        assert_eq!(
            Kind::Layout.text_path("panorama/layout/popups/popup_settings.vxml_c"),
            "panorama/layout/popups/popup_settings.xml"
        );
        assert_eq!(
            Kind::Texture.text_path("panorama/images/minimap/gold_psd.vtex_c"),
            "panorama/images/minimap/gold_psd.png"
        );
        assert_eq!(
            Kind::Vector.text_path("panorama/images/hud/top_bar/icon_ultimate.vsvg_c"),
            "panorama/images/hud/top_bar/icon_ultimate.svg"
        );
        assert_eq!(
            strings_path("panorama/layout/x.vxml_c"),
            "panorama/layout/x.vxml_c.strings.txt"
        );
    }

    #[test]
    fn undecodable_bytes_fall_back_to_strings() {
        let err = decode(Kind::Layout, b"nope").unwrap_err();
        assert!(!err.is_empty());
        let mut bytes =
            b"\x00\x01abc\x00CitadelHudTopBar\x00\x02CitadelHudTopBar\x00tab\there\x00".to_vec();
        bytes.extend_from_slice(&[0xff; 8]);
        let text = strings(&bytes, &err);
        let lines: Vec<&str> = text.lines().collect();
        assert!(lines[0].contains(&err));
        assert_eq!(&lines[2..], ["CitadelHudTopBar", "tab\there"]);
    }
}
