//! Where an SVG's text names colours: the colour presentation attributes, the same
//! properties in `style` attributes and in `<style>` rules (CSS, CDATA-wrapped or not).

use std::ops::Range;

use crate::texture::adjust::Rgb;

const PROPERTIES: [&str; 6] = [
    "fill",
    "stroke",
    "stop-color",
    "flood-color",
    "lighting-color",
    "color",
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Paint {
    Color(Rgb),
    Current,
}

/// One colour as written: `at` is its byte range in the SVG text.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Token {
    pub at: Range<usize>,
    pub paint: Paint,
}

/// Every colour token in `svg`, in text order.
pub(super) fn tokens(svg: &str) -> Vec<Token> {
    let mut out = Vec::new();
    let mut at = 0;
    while let Some(open) = svg[at..].find('<') {
        let start = at + open;
        let rest = &svg[start..];
        let past = |end: &str| rest.find(end).map(|i| start + i + end.len());
        let next = if rest.starts_with("<!--") {
            past("-->")
        } else if rest.starts_with("<![CDATA[") {
            past("]]>")
        } else if rest.starts_with("<?") || rest.starts_with("<!") || rest.starts_with("</") {
            past(">")
        } else {
            super::tag_end(rest).map(|end| {
                let tag = &rest[1..end];
                for (name, value) in attributes(tag) {
                    let value = start + 1 + value.start..start + 1 + value.end;
                    if name == "style" {
                        declarations(svg, value, false, &mut out);
                    } else if PROPERTIES.contains(&name) {
                        values(svg, value, &mut out);
                    }
                }
                let after = start + end + 1;
                let element = tag.split(|c: char| c.is_whitespace() || c == '/').next();
                if element.is_some_and(|e| e == "style" || e.ends_with(":style"))
                    && !tag.ends_with('/')
                {
                    let close = svg[after..].find("</").map_or(svg.len(), |i| after + i);
                    declarations(svg, after..close, true, &mut out);
                    close
                } else {
                    after
                }
            })
        };
        let Some(next) = next else { break };
        at = next;
    }
    out
}

/// The attributes of `tag` (the text between `<` and `>`), each with its value's range
/// inside `tag`, quotes excluded.
pub(super) fn attributes(tag: &str) -> Vec<(&str, Range<usize>)> {
    let bytes = tag.as_bytes();
    let space = |i: usize| bytes.get(i).is_some_and(|b| b.is_ascii_whitespace());
    let mut out = Vec::new();
    let mut i = tag
        .find(|c: char| c.is_whitespace() || c == '/')
        .unwrap_or(tag.len());
    loop {
        while space(i) || bytes.get(i) == Some(&b'/') {
            i += 1;
        }
        let name_start = i;
        while bytes
            .get(i)
            .is_some_and(|b| !b.is_ascii_whitespace() && !matches!(b, b'=' | b'/'))
        {
            i += 1;
        }
        if name_start == i {
            break;
        }
        let name = &tag[name_start..i];
        while space(i) {
            i += 1;
        }
        if bytes.get(i) != Some(&b'=') {
            continue;
        }
        i += 1;
        while space(i) {
            i += 1;
        }
        let Some(&quote) = bytes.get(i).filter(|b| matches!(b, b'"' | b'\'')) else {
            break;
        };
        let start = i + 1;
        let Some(len) = tag[start..].find(char::from(quote)) else {
            break;
        };
        out.push((name, start..start + len));
        i = start + len + 1;
    }
    out
}

/// The colour declarations in `svg[within]`: a `<style>` sheet when `rules`, else the
/// declaration list of a `style` attribute.
fn declarations(svg: &str, within: Range<usize>, rules: bool, out: &mut Vec<Token>) {
    let bytes = svg.as_bytes();
    let mut depth = usize::from(!rules);
    let mut start = within.start;
    let mut i = within.start;
    while i < within.end {
        if bytes[i..within.end].starts_with(b"/*") {
            i = svg[i..within.end]
                .find("*/")
                .map_or(within.end, |end| i + end + 2);
            continue;
        }
        match bytes[i] {
            b'{' => depth += 1,
            b';' | b'}' if depth > 0 => declaration(svg, start..i, out),
            _ => {}
        }
        if bytes[i] == b'}' {
            depth = depth.saturating_sub(1);
        }
        if matches!(bytes[i], b'{' | b'}' | b';') {
            start = i + 1;
        }
        i += 1;
    }
    if depth > 0 {
        declaration(svg, start..within.end, out);
    }
}

fn declaration(svg: &str, at: Range<usize>, out: &mut Vec<Token>) {
    let mut body = svg[at.clone()].trim_start();
    while let Some(after) = body.strip_prefix("/*") {
        body = after
            .find("*/")
            .map_or("", |end| &after[end + 2..])
            .trim_start();
    }
    let Some(colon) = body.find(':') else {
        return;
    };
    let name = body[..colon].trim();
    if PROPERTIES.iter().any(|p| p.eq_ignore_ascii_case(name)) {
        let value = at.end - body.len() + colon + 1;
        values(svg, value..at.end, out);
    }
}

fn ident(b: u8) -> bool {
    b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_')
}

/// The colours in one property value: `#hex`, `rgb()`/`rgba()`, a named colour or
/// `currentColor`; `url()`, other functions and keywords are skipped.
fn values(svg: &str, within: Range<usize>, out: &mut Vec<Token>) {
    let bytes = svg.as_bytes();
    let mut i = within.start;
    while i < within.end {
        let b = bytes[i];
        if b == b'#' {
            let digits = bytes[i + 1..within.end]
                .iter()
                .take_while(|b| b.is_ascii_hexdigit())
                .count();
            let end = i + 1 + digits;
            let whole = end == within.end || !ident(bytes[end]);
            if whole && let Some(rgb) = hex(&svg[i + 1..end]) {
                out.push(Token {
                    at: i..end,
                    paint: Paint::Color(rgb),
                });
            }
            i = end;
        } else if b.is_ascii_alphabetic() || b == b'-' {
            let end = i + bytes[i..within.end]
                .iter()
                .take_while(|b| ident(**b))
                .count();
            let word = &svg[i..end];
            if bytes.get(end) == Some(&b'(') && end < within.end {
                let Some(close) = svg[end..within.end].find(')') else {
                    return;
                };
                let close = end + close + 1;
                let functional =
                    word.eq_ignore_ascii_case("rgb") || word.eq_ignore_ascii_case("rgba");
                if functional && let Some(rgb) = function(&svg[end + 1..close - 1]) {
                    out.push(Token {
                        at: i..close,
                        paint: Paint::Color(rgb),
                    });
                }
                i = close;
                continue;
            }
            let paint = if word.eq_ignore_ascii_case("currentcolor") {
                Some(Paint::Current)
            } else {
                named(word).map(Paint::Color)
            };
            if let Some(paint) = paint {
                out.push(Token { at: i..end, paint });
            }
            i = end;
        } else {
            i += 1;
        }
    }
}

fn hex(digits: &str) -> Option<Rgb> {
    let nibble = |i: usize| u8::from_str_radix(&digits[i..i + 1], 16).ok();
    let byte = |i: usize| u8::from_str_radix(&digits[i..i + 2], 16).ok();
    match digits.len() {
        3 | 4 => Some(Rgb([nibble(0)? * 17, nibble(1)? * 17, nibble(2)? * 17])),
        6 | 8 => Some(Rgb([byte(0)?, byte(2)?, byte(4)?])),
        _ => None,
    }
}

/// The colour in `rgb(...)`/`rgba(...)` arguments: three integers or percentages, then an
/// optional alpha.
fn function(args: &str) -> Option<Rgb> {
    let parts: Vec<&str> = args.split(',').map(str::trim).collect();
    if !matches!(parts.len(), 3 | 4) || parts.get(3).is_some_and(|a| a.is_empty()) {
        return None;
    }
    let channel = |text: &str| -> Option<u8> {
        let (number, full) = match text.strip_suffix('%') {
            Some(p) => (p.trim(), 100.0),
            None => (text, 255.0),
        };
        let n: f64 = number.parse().ok()?;
        n.is_finite()
            .then(|| (n * 255.0 / full).round().clamp(0.0, 255.0) as u8)
    };
    Some(Rgb([
        channel(parts[0])?,
        channel(parts[1])?,
        channel(parts[2])?,
    ]))
}

/// `rgb` written in the form of `original` (a token `tokens` found): hex keeps its alpha
/// digits (a short `#rgba` alpha is doubled), `rgb()`/`rgba()` keep the alpha text, and a
/// named colour becomes `#rrggbb`.
pub(super) fn rewrite(original: &str, rgb: Rgb) -> String {
    if let Some(digits) = original.strip_prefix('#') {
        let alpha = match digits.len() {
            4 => digits[3..].repeat(2),
            8 => digits[6..].to_string(),
            _ => String::new(),
        };
        return format!("{}{alpha}", rgb.hex());
    }
    let [r, g, b] = rgb.0;
    match original.split_once('(') {
        Some((_, args)) => match args.trim_end_matches(')').split(',').nth(3) {
            Some(alpha) => format!("rgba({r}, {g}, {b}, {})", alpha.trim()),
            None => format!("rgb({r}, {g}, {b})"),
        },
        None => rgb.hex(),
    }
}

fn named(word: &str) -> Option<Rgb> {
    let word = word.to_ascii_lowercase();
    let at = NAMED
        .binary_search_by(|(name, _)| name.cmp(&word.as_str()))
        .ok()?;
    let [_, r, g, b] = NAMED[at].1.to_be_bytes();
    Some(Rgb([r, g, b]))
}

/// CSS Color Level 4 named colours, sorted for binary search.
const NAMED: [(&str, u32); 148] = [
    ("aliceblue", 0xf0f8ff),
    ("antiquewhite", 0xfaebd7),
    ("aqua", 0x00ffff),
    ("aquamarine", 0x7fffd4),
    ("azure", 0xf0ffff),
    ("beige", 0xf5f5dc),
    ("bisque", 0xffe4c4),
    ("black", 0x000000),
    ("blanchedalmond", 0xffebcd),
    ("blue", 0x0000ff),
    ("blueviolet", 0x8a2be2),
    ("brown", 0xa52a2a),
    ("burlywood", 0xdeb887),
    ("cadetblue", 0x5f9ea0),
    ("chartreuse", 0x7fff00),
    ("chocolate", 0xd2691e),
    ("coral", 0xff7f50),
    ("cornflowerblue", 0x6495ed),
    ("cornsilk", 0xfff8dc),
    ("crimson", 0xdc143c),
    ("cyan", 0x00ffff),
    ("darkblue", 0x00008b),
    ("darkcyan", 0x008b8b),
    ("darkgoldenrod", 0xb8860b),
    ("darkgray", 0xa9a9a9),
    ("darkgreen", 0x006400),
    ("darkgrey", 0xa9a9a9),
    ("darkkhaki", 0xbdb76b),
    ("darkmagenta", 0x8b008b),
    ("darkolivegreen", 0x556b2f),
    ("darkorange", 0xff8c00),
    ("darkorchid", 0x9932cc),
    ("darkred", 0x8b0000),
    ("darksalmon", 0xe9967a),
    ("darkseagreen", 0x8fbc8f),
    ("darkslateblue", 0x483d8b),
    ("darkslategray", 0x2f4f4f),
    ("darkslategrey", 0x2f4f4f),
    ("darkturquoise", 0x00ced1),
    ("darkviolet", 0x9400d3),
    ("deeppink", 0xff1493),
    ("deepskyblue", 0x00bfff),
    ("dimgray", 0x696969),
    ("dimgrey", 0x696969),
    ("dodgerblue", 0x1e90ff),
    ("firebrick", 0xb22222),
    ("floralwhite", 0xfffaf0),
    ("forestgreen", 0x228b22),
    ("fuchsia", 0xff00ff),
    ("gainsboro", 0xdcdcdc),
    ("ghostwhite", 0xf8f8ff),
    ("gold", 0xffd700),
    ("goldenrod", 0xdaa520),
    ("gray", 0x808080),
    ("green", 0x008000),
    ("greenyellow", 0xadff2f),
    ("grey", 0x808080),
    ("honeydew", 0xf0fff0),
    ("hotpink", 0xff69b4),
    ("indianred", 0xcd5c5c),
    ("indigo", 0x4b0082),
    ("ivory", 0xfffff0),
    ("khaki", 0xf0e68c),
    ("lavender", 0xe6e6fa),
    ("lavenderblush", 0xfff0f5),
    ("lawngreen", 0x7cfc00),
    ("lemonchiffon", 0xfffacd),
    ("lightblue", 0xadd8e6),
    ("lightcoral", 0xf08080),
    ("lightcyan", 0xe0ffff),
    ("lightgoldenrodyellow", 0xfafad2),
    ("lightgray", 0xd3d3d3),
    ("lightgreen", 0x90ee90),
    ("lightgrey", 0xd3d3d3),
    ("lightpink", 0xffb6c1),
    ("lightsalmon", 0xffa07a),
    ("lightseagreen", 0x20b2aa),
    ("lightskyblue", 0x87cefa),
    ("lightslategray", 0x778899),
    ("lightslategrey", 0x778899),
    ("lightsteelblue", 0xb0c4de),
    ("lightyellow", 0xffffe0),
    ("lime", 0x00ff00),
    ("limegreen", 0x32cd32),
    ("linen", 0xfaf0e6),
    ("magenta", 0xff00ff),
    ("maroon", 0x800000),
    ("mediumaquamarine", 0x66cdaa),
    ("mediumblue", 0x0000cd),
    ("mediumorchid", 0xba55d3),
    ("mediumpurple", 0x9370db),
    ("mediumseagreen", 0x3cb371),
    ("mediumslateblue", 0x7b68ee),
    ("mediumspringgreen", 0x00fa9a),
    ("mediumturquoise", 0x48d1cc),
    ("mediumvioletred", 0xc71585),
    ("midnightblue", 0x191970),
    ("mintcream", 0xf5fffa),
    ("mistyrose", 0xffe4e1),
    ("moccasin", 0xffe4b5),
    ("navajowhite", 0xffdead),
    ("navy", 0x000080),
    ("oldlace", 0xfdf5e6),
    ("olive", 0x808000),
    ("olivedrab", 0x6b8e23),
    ("orange", 0xffa500),
    ("orangered", 0xff4500),
    ("orchid", 0xda70d6),
    ("palegoldenrod", 0xeee8aa),
    ("palegreen", 0x98fb98),
    ("paleturquoise", 0xafeeee),
    ("palevioletred", 0xdb7093),
    ("papayawhip", 0xffefd5),
    ("peachpuff", 0xffdab9),
    ("peru", 0xcd853f),
    ("pink", 0xffc0cb),
    ("plum", 0xdda0dd),
    ("powderblue", 0xb0e0e6),
    ("purple", 0x800080),
    ("rebeccapurple", 0x663399),
    ("red", 0xff0000),
    ("rosybrown", 0xbc8f8f),
    ("royalblue", 0x4169e1),
    ("saddlebrown", 0x8b4513),
    ("salmon", 0xfa8072),
    ("sandybrown", 0xf4a460),
    ("seagreen", 0x2e8b57),
    ("seashell", 0xfff5ee),
    ("sienna", 0xa0522d),
    ("silver", 0xc0c0c0),
    ("skyblue", 0x87ceeb),
    ("slateblue", 0x6a5acd),
    ("slategray", 0x708090),
    ("slategrey", 0x708090),
    ("snow", 0xfffafa),
    ("springgreen", 0x00ff7f),
    ("steelblue", 0x4682b4),
    ("tan", 0xd2b48c),
    ("teal", 0x008080),
    ("thistle", 0xd8bfd8),
    ("tomato", 0xff6347),
    ("turquoise", 0x40e0d0),
    ("violet", 0xee82ee),
    ("wheat", 0xf5deb3),
    ("white", 0xffffff),
    ("whitesmoke", 0xf5f5f5),
    ("yellow", 0xffff00),
    ("yellowgreen", 0x9acd32),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn named_colours_are_sorted_and_case_insensitive() {
        assert!(NAMED.windows(2).all(|w| w[0].0 < w[1].0));
        assert_eq!(named("RebeccaPurple"), Some(Rgb([0x66, 0x33, 0x99])));
        assert_eq!(named("aliceblue"), Some(Rgb([0xf0, 0xf8, 0xff])));
        assert_eq!(named("YellowGreen"), Some(Rgb([0x9a, 0xcd, 0x32])));
        for word in ["none", "transparent", "inherit", "important", "evenodd"] {
            assert_eq!(named(word), None, "{word}");
        }
    }

    #[test]
    fn attributes_are_whole_names() {
        let tag = "rect fill-rule = 'evenodd' fill=\"#fff\" hidden xlink:href=\"#a\"/";
        let got: Vec<(&str, &str)> = attributes(tag)
            .into_iter()
            .map(|(name, at)| (name, &tag[at]))
            .collect();
        assert_eq!(
            got,
            [
                ("fill-rule", "evenodd"),
                ("fill", "#fff"),
                ("xlink:href", "#a")
            ]
        );
    }

    #[test]
    fn values_skip_what_is_not_a_colour() {
        let svg = "url(#abc) #fffx #12345 var(--red) hsl(0, 0%, 0%) rgb(1, 2) red !important";
        let found: Vec<&str> = {
            let mut out = Vec::new();
            values(svg, 0..svg.len(), &mut out);
            out.into_iter().map(|t| &svg[t.at]).collect()
        };
        assert_eq!(found, ["red"]);
    }
}
