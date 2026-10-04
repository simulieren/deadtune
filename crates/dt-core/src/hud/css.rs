//! Just enough Panorama CSS for our needs: read the declarations the game ships for
//! a selector (vanilla values for the editor), and emit minified rules. Compiled
//! stylesheets are already minified; at-rules with blocks (`@keyframes`) are skipped.

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Rule {
    /// Selector list exactly as written, e.g. `.gDetailView #minimap_persp,.gScoreboardOpen #minimap_persp`.
    pub selectors: String,
    pub decls: Vec<(String, String)>,
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum CssError {
    #[error("unbalanced braces at byte {0}")]
    Unbalanced(usize),
    #[error("comment not closed")]
    OpenComment,
}

/// Replaces comments with spaces of equal byte length so reported positions stay valid.
/// Quoted strings are copied verbatim.
fn blank_comments(text: &str) -> Result<String, CssError> {
    let b = text.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        match b[i] {
            b'"' | b'\'' => {
                let end = skip_quoted(b, i);
                out.extend_from_slice(&b[i..end]);
                i = end;
            }
            b'/' if b.get(i + 1) == Some(&b'*') => {
                let close = find_bytes(&b[i + 2..], b"*/").ok_or(CssError::OpenComment)?;
                let end = i + 2 + close + 2;
                out.extend(std::iter::repeat_n(b' ', end - i));
                i = end;
            }
            c => {
                out.push(c);
                i += 1;
            }
        }
    }
    Ok(String::from_utf8_lossy(&out).into_owned())
}

fn find_bytes(hay: &[u8], needle: &[u8]) -> Option<usize> {
    hay.windows(needle.len()).position(|w| w == needle)
}

/// Index just past the closing quote of the string opening at `start` (or end of input).
fn skip_quoted(b: &[u8], start: usize) -> usize {
    let q = b[start];
    let mut i = start + 1;
    while i < b.len() {
        match b[i] {
            b'\\' => i += 2,
            c if c == q => return i + 1,
            _ => i += 1,
        }
    }
    b.len()
}

/// Index of the first byte in `stops` at paren depth 0 outside quotes, from `from`.
fn scan_to(b: &[u8], from: usize, stops: &[u8]) -> Option<usize> {
    let mut i = from;
    let mut parens = 0usize;
    while i < b.len() {
        match b[i] {
            b'"' | b'\'' => {
                i = skip_quoted(b, i);
                continue;
            }
            b'(' => parens += 1,
            b')' => parens = parens.saturating_sub(1),
            c if parens == 0 && stops.contains(&c) => return Some(i),
            _ => {}
        }
        i += 1;
    }
    None
}

/// Index of the `}` matching the `{` at `open`.
fn block_end(b: &[u8], open: usize) -> Option<usize> {
    let mut depth = 0usize;
    let mut i = open;
    while i < b.len() {
        match b[i] {
            b'"' | b'\'' => {
                i = skip_quoted(b, i);
                continue;
            }
            b'{' => depth += 1,
            b'}' => {
                depth -= 1;
                if depth == 0 {
                    return Some(i);
                }
            }
            _ => {}
        }
        i += 1;
    }
    None
}

fn parse_decls(body: &str) -> Vec<(String, String)> {
    let b = body.as_bytes();
    let mut decls = Vec::new();
    let mut start = 0;
    while start < b.len() {
        let end = scan_to(b, start, b";").unwrap_or(b.len());
        let part = &body[start..end];
        if let Some(c) = scan_to(part.as_bytes(), 0, b":") {
            let prop = part[..c].trim().to_ascii_lowercase();
            if !prop.is_empty() {
                decls.push((prop, part[c + 1..].trim().to_string()));
            }
        }
        start = end + 1;
    }
    decls
}

/// Top-level style rules in source order. `@define`/`@import` statements and
/// block at-rules are not returned.
pub fn parse_rules(text: &str) -> Result<Vec<Rule>, CssError> {
    let clean = blank_comments(text)?;
    let b = clean.as_bytes();
    let mut rules = Vec::new();
    let mut i = 0;
    while i < b.len() {
        if b[i].is_ascii_whitespace() {
            i += 1;
            continue;
        }
        let Some(stop) = scan_to(b, i, b"{;}") else {
            break;
        };
        match b[stop] {
            b'}' => return Err(CssError::Unbalanced(stop)),
            b';' => i = stop + 1,
            _ => {
                let close = block_end(b, stop).ok_or(CssError::Unbalanced(stop))?;
                let selectors = clean[i..stop].trim();
                if !selectors.starts_with('@') {
                    rules.push(Rule {
                        selectors: selectors.to_string(),
                        decls: parse_decls(&clean[stop + 1..close]),
                    });
                }
                i = close + 1;
            }
        }
    }
    Ok(rules)
}

fn normalize_ws(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Declarations of every rule whose selector list contains exactly `selector`
/// (e.g. `#minimap_persp`), later rules overriding earlier ones per property.
pub fn decls_for(text: &str, selector: &str) -> Result<Vec<(String, String)>, CssError> {
    let want = normalize_ws(selector);
    let mut out: Vec<(String, String)> = Vec::new();
    for rule in parse_rules(text)? {
        let b = rule.selectors.as_bytes();
        let mut matched = false;
        let mut start = 0;
        loop {
            let end = scan_to(b, start, b",").unwrap_or(b.len());
            if normalize_ws(&rule.selectors[start..end]) == want {
                matched = true;
                break;
            }
            if end >= b.len() {
                break;
            }
            start = end + 1;
        }
        if !matched {
            continue;
        }
        for (prop, val) in rule.decls {
            match out.iter_mut().find(|(p, _)| *p == prop) {
                Some(slot) => slot.1 = val,
                None => out.push((prop, val)),
            }
        }
    }
    Ok(out)
}

/// `sel{a:b;c:d;}` with no whitespace beyond what values need.
pub fn emit_rule(selector: &str, decls: &[(&str, String)]) -> String {
    let mut s = String::with_capacity(selector.len() + 2);
    s.push_str(selector);
    s.push('{');
    for (p, v) in decls {
        s.push_str(p);
        s.push(':');
        s.push_str(v);
        s.push(';');
    }
    s.push('}');
    s
}

/// Strips comments and collapses whitespace the way the Valve compiler does.
pub fn minify(text: &str) -> Result<String, CssError> {
    let clean = blank_comments(text)?;
    let b = clean.as_bytes();
    let mut out = String::with_capacity(b.len());
    let mut depth = 0usize;
    let mut pending_space = false;
    let mut i = 0;
    while i < b.len() {
        let c = b[i];
        if c.is_ascii_whitespace() {
            pending_space = true;
            i += 1;
            continue;
        }
        let tight_after = matches!(
            out.as_bytes().last(),
            None | Some(b'{' | b'}' | b':' | b';' | b',')
        );
        // At depth 0 a space before `:` separates a descendant from a pseudo-class (`.a :hover`).
        let tight_before = matches!(c, b'{' | b'}' | b';' | b',') || (c == b':' && depth > 0);
        if pending_space && !tight_after && !tight_before {
            out.push(' ');
        }
        pending_space = false;
        match c {
            b'"' | b'\'' => {
                let end = skip_quoted(b, i);
                out.push_str(&clean[i..end]);
                i = end;
                continue;
            }
            b'{' => depth += 1,
            b'}' => depth = depth.saturating_sub(1),
            _ => {}
        }
        // Push the whole UTF-8 character starting at i.
        let len = clean[i..].chars().next().map_or(1, char::len_utf8);
        out.push_str(&clean[i..i + len]);
        i += len;
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vanilla() -> String {
        let b = std::fs::read(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/fixtures/hud/hud_vanilla.vcss_c"
        ))
        .expect("fixture");
        let u32_at = |o: usize| u32::from_le_bytes([b[o], b[o + 1], b[o + 2], b[o + 3]]) as usize;
        let table = 8 + u32_at(8);
        for k in 0..u32_at(12) {
            let e = table + k * 12;
            if &b[e..e + 4] == b"DATA" {
                let off = e + 4 + u32_at(e + 4);
                let size = u32_at(e + 8);
                return String::from_utf8(b[off + 6..off + size].to_vec()).expect("utf8");
            }
        }
        panic!("no DATA block");
    }

    fn get(text: &str, sel: &str, prop: &str) -> Option<String> {
        decls_for(text, sel)
            .unwrap()
            .into_iter()
            .find(|(p, _)| p == prop)
            .map(|(_, v)| v)
    }

    #[test]
    fn parses_basic_rules_and_skips_at_rules() {
        let css = r#"
            /* c */ @define a: 1px;
            @import url("s2r://x.vcss_c");
            @keyframes k { 0% { opacity: 0; } 100% { opacity: 1; } }
            .A , .B { Width : 10px; background-image: url("a;b{c.png"); margin: 0px 4px }
            #x { }
        "#;
        let rules = parse_rules(css).unwrap();
        assert_eq!(rules.len(), 2);
        assert_eq!(rules[0].selectors, ".A , .B");
        assert_eq!(
            rules[0].decls,
            vec![
                ("width".to_string(), "10px".to_string()),
                (
                    "background-image".to_string(),
                    "url(\"a;b{c.png\")".to_string()
                ),
                ("margin".to_string(), "0px 4px".to_string()),
            ]
        );
        assert!(rules[1].decls.is_empty());
    }

    #[test]
    fn errors() {
        assert_eq!(parse_rules("a{b:c;"), Err(CssError::Unbalanced(1)));
        assert_eq!(parse_rules("a{b:c;}}"), Err(CssError::Unbalanced(7)));
        assert_eq!(parse_rules("a{} /* x"), Err(CssError::OpenComment));
        assert_eq!(minify("a{} /* x"), Err(CssError::OpenComment));
    }

    #[test]
    fn decls_for_overrides_in_place() {
        let css = ".a #m{width:1px;height:2px}.b, #m {width:3px;opacity:.5}#mm{width:9px}";
        assert_eq!(
            decls_for(css, "#m").unwrap(),
            vec![
                ("width".into(), "3px".into()),
                ("opacity".into(), ".5".into())
            ]
        );
        assert_eq!(
            decls_for(css, ".a   #m").unwrap(),
            vec![
                ("width".into(), "1px".into()),
                ("height".into(), "2px".into())
            ]
        );
        assert_eq!(get(css, "#m", "width").as_deref(), Some("3px"));
    }

    #[test]
    fn emit() {
        assert_eq!(
            emit_rule("#a", &[("opacity", "0.5".into())]),
            "#a{opacity:0.5;}"
        );
        assert_eq!(emit_rule("#a", &[]), "#a{}");
    }

    #[test]
    fn minify_cases() {
        let css = "/* hi */ .a :hover , #b {\n  margin : 0px 4px ;\n  transform: translateX(10px)  translateY(4px);\n  x: \"a ; { b\"; y: rgba(0, 0, 0, 1)\n}\n";
        assert_eq!(
            minify(css).unwrap(),
            ".a :hover,#b{margin:0px 4px;transform:translateX(10px) translateY(4px);x:\"a ; { b\";y:rgba(0,0,0,1)}"
        );
    }

    #[test]
    fn vanilla_roundtrip() {
        let text = vanilla();
        let min = minify(&text).unwrap();
        assert_eq!(minify(&min).unwrap(), min);
        // The compiler keeps `, ` inside values and `: ` after properties; minify drops the
        // comma spaces, so compare with those removed.
        let tight = |rules: Vec<Rule>| -> Vec<Rule> {
            rules
                .into_iter()
                .map(|mut r| {
                    r.selectors = r.selectors.replace(", ", ",");
                    for d in &mut r.decls {
                        d.1 = d.1.replace(", ", ",");
                    }
                    r
                })
                .collect()
        };
        assert_eq!(
            tight(parse_rules(&min).unwrap()),
            tight(parse_rules(&text).unwrap())
        );
        assert!(min.len() < text.len());
        assert!(parse_rules(&text).unwrap().len() > 100);
    }

    #[test]
    fn vanilla_values() {
        let t = vanilla();
        assert_eq!(get(&t, "#minimap_persp", "width").as_deref(), Some("440px"));
        assert_eq!(
            get(&t, "#minimap_persp", "height").as_deref(),
            Some("520px")
        );
        assert_eq!(
            get(&t, "#health_and_abilities_container", "width").as_deref(),
            Some("250px")
        );
    }

    #[test]
    #[ignore]
    fn dump_vanilla_values() {
        let t = vanilla();
        for sel in [
            "#minimap_persp",
            "#minimap_container",
            "#TopBar",
            "#hud_signature",
            "#ActiveAbilitiesMenu",
            "#hud_passive_items",
            "#StatsAndModsContainer",
            "#ammo_panel",
            "#DataFeed",
            "#Chat",
            "#AbilitiesContainer",
            "#health_and_abilities_container",
        ] {
            println!("{sel}: {:?}", decls_for(&t, sel).unwrap());
        }
    }
}
