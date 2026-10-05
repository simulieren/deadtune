//! Lossless line-based editor for the scalar keys of one named top-level section of
//! `gameinfo.gi` (`SceneSystem`, `RenderSystem`, ...). Same rules as `gi`: never
//! re-serialize, touch only the lines asked for, keep every other byte.

use std::collections::BTreeMap;

use crate::gi::{self, GiError, Line, LineScan};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum KeyEdit {
    Set(String),
    Remove,
}

pub type SectionEdits = BTreeMap<String, KeyEdit>;

/// Active scalar keys of `section`, unquoted, with keys of nested blocks as `Block/Key`.
/// The last line wins for duplicates. `None` when the section is missing.
pub fn section_values(
    text: &str,
    section: &str,
) -> Result<Option<BTreeMap<String, String>>, GiError> {
    gi::validate_braces(text)?;
    let lines = gi::split_lines(text);
    let Some(block) = find_section(&lines, section) else {
        return Ok(None);
    };
    let mut values = BTreeMap::new();
    let mut stack: Vec<String> = Vec::new();
    let mut pending: Option<String> = None;
    for line in &lines[block.open_line + 1..block.close_line] {
        let scan = LineScan::new(line.content);
        if scan.opens > scan.closes {
            let name = block_name(line.content).or(pending.take());
            stack.push(name.unwrap_or_default());
            continue;
        }
        if scan.closes > scan.opens {
            stack.pop();
            pending = None;
            continue;
        }
        match Scalar::parse(line.content) {
            Some(s) => {
                let key = stack
                    .iter()
                    .map(String::as_str)
                    .chain([s.key()])
                    .collect::<Vec<_>>()
                    .join("/");
                values.insert(key, gi::unquote(s.value_raw).to_string());
                pending = None;
            }
            None => pending = bare_token(line.content).map(str::to_string),
        }
    }
    Ok(Some(values))
}

/// Sets or removes top-level scalar keys of `section`. A present key is rewritten in place
/// (indentation, padding, quoting and trailing comment kept); a missing one is inserted before
/// the closing brace in the style of the section's other keys. `Remove` drops every line of
/// the key. A missing section is an error for `Set` and a no-op for `Remove`.
pub fn edit_section(text: &str, section: &str, edits: &SectionEdits) -> Result<String, GiError> {
    gi::validate_braces(text)?;
    let lines = gi::split_lines(text);
    let Some(block) = find_section(&lines, section) else {
        if edits.values().any(|e| matches!(e, KeyEdit::Set(_))) {
            return Err(GiError::NoSection(section.to_string()));
        }
        return Ok(text.to_string());
    };
    let eol = gi::eol_str(gi::detect_eol(text));
    let mut out = String::with_capacity(text.len() + 64 * edits.len());
    for line in &lines[..=block.open_line] {
        gi::push_line(&mut out, line);
    }
    let mut seen: Vec<&str> = Vec::new();
    let mut style = Style {
        indent: format!("{}\t", indent_of(lines[block.open_line].content)),
        quoted: false,
    };
    let mut sub_depth = 0isize;
    for line in &lines[block.open_line + 1..block.close_line] {
        let scan = LineScan::new(line.content);
        let nested = sub_depth > 0 || scan.opens > scan.closes;
        sub_depth += scan.opens as isize - scan.closes as isize;
        let scalar = if nested {
            None
        } else {
            Scalar::parse(line.content)
        };
        let Some(s) = scalar else {
            gi::push_line(&mut out, line);
            continue;
        };
        style = Style {
            indent: s.indent.to_string(),
            quoted: s.value_raw.starts_with('"'),
        };
        match edits.get(s.key()) {
            Some(KeyEdit::Set(value)) => {
                seen.push(s.key());
                if gi::unquote(s.value_raw) == value {
                    gi::push_line(&mut out, line);
                } else {
                    let quoted =
                        s.value_raw.starts_with('"') || value.contains(char::is_whitespace);
                    out.extend([s.indent, s.key_raw, s.pad, &render(value, quoted), s.trail]);
                    out.push_str(line.eol);
                }
            }
            Some(KeyEdit::Remove) => {}
            None => gi::push_line(&mut out, line),
        }
    }
    for (key, edit) in edits {
        if let KeyEdit::Set(value) = edit
            && !seen.contains(&key.as_str())
        {
            let quoted = style.quoted || value.contains(char::is_whitespace);
            out.extend([style.indent.as_str(), key, " ", &render(value, quoted), eol]);
        }
    }
    for line in &lines[block.close_line..] {
        gi::push_line(&mut out, line);
    }
    gi::validate_braces(&out)?;
    Ok(out)
}

/// Indentation and value quoting of the section's keys, for lines we insert.
struct Style {
    indent: String,
    quoted: bool,
}

fn render(value: &str, quoted: bool) -> String {
    if quoted {
        format!("\"{value}\"")
    } else {
        value.to_string()
    }
}

struct Section {
    open_line: usize,
    close_line: usize,
}

/// A section is a bare `name` line at depth 1 (inside the root block) whose block opens on
/// that line or the next.
fn find_section(lines: &[Line], name: &str) -> Option<Section> {
    let scans: Vec<LineScan> = lines.iter().map(|l| LineScan::new(l.content)).collect();
    let mut depth = 0isize;
    let mut header = None;
    for (i, scan) in scans.iter().enumerate() {
        if depth == 1 && header.is_none() {
            let code = strip_comment(lines[i].content);
            let before_brace = code.split('{').next().unwrap_or("").trim();
            if gi::unquote(before_brace) == name && (scan.opens > 0 || code.trim() == before_brace)
            {
                header = Some(i);
            }
        }
        depth += scan.opens as isize - scan.closes as isize;
    }
    let header = header?;
    let open_line = (header..lines.len()).find(|&i| scans[i].opens > 0)?;
    if (header + 1..open_line).any(|i| !lines[i].content.trim().is_empty()) {
        return None;
    }
    let mut depth = 0isize;
    for (i, scan) in scans.iter().enumerate().skip(open_line) {
        depth += scan.opens as isize - scan.closes as isize;
        if depth <= 0 {
            return Some(Section {
                open_line,
                close_line: i,
            });
        }
    }
    None
}

/// `Name {` on one line names its own block.
fn block_name(content: &str) -> Option<String> {
    let code = strip_comment(content);
    let (before, _) = code.split_once('{')?;
    let token = before.trim();
    (!token.is_empty() && !token.contains(char::is_whitespace))
        .then(|| gi::unquote(token).to_string())
}

/// A line holding one word and nothing else: the name of the block that follows.
fn bare_token(content: &str) -> Option<&str> {
    let token = strip_comment(content).trim();
    (!token.is_empty() && !token.contains(char::is_whitespace) && !token.contains(['{', '}']))
        .then(|| gi::unquote(token))
}

/// Everything before a `//` that is not inside quotes.
fn strip_comment(content: &str) -> &str {
    let mut in_quote = false;
    let bytes = content.as_bytes();
    for i in 0..bytes.len() {
        match bytes[i] {
            b'"' => in_quote = !in_quote,
            b'/' if !in_quote && bytes.get(i + 1) == Some(&b'/') => return &content[..i],
            _ => {}
        }
    }
    content
}

fn indent_of(content: &str) -> &str {
    &content[..content.len() - content.trim_start_matches([' ', '\t']).len()]
}

/// `indent key pad value trail`; the key and the value may each be quoted.
#[derive(Debug, PartialEq, Eq)]
struct Scalar<'a> {
    indent: &'a str,
    key_raw: &'a str,
    pad: &'a str,
    value_raw: &'a str,
    trail: &'a str,
}

impl<'a> Scalar<'a> {
    fn parse(content: &'a str) -> Option<Scalar<'a>> {
        let indent = indent_of(content);
        let rest = &content[indent.len()..];
        if rest.starts_with("//") {
            return None;
        }
        let key_len = token_len(rest, true)?;
        let after_key = &rest[key_len..];
        let pad_len = after_key.len() - after_key.trim_start_matches([' ', '\t']).len();
        if pad_len == 0 {
            return None;
        }
        let after_pad = &after_key[pad_len..];
        let value_len = token_len(after_pad, false)?;
        let trail = &after_pad[value_len..];
        let after_blanks = trail.trim_start_matches([' ', '\t']);
        let trail_ok = after_blanks.is_empty()
            || (after_blanks.len() < trail.len() && after_blanks.starts_with("//"));
        if !trail_ok {
            return None;
        }
        Some(Scalar {
            indent,
            key_raw: &rest[..key_len],
            pad: &after_key[..pad_len],
            value_raw: &after_pad[..value_len],
            trail,
        })
    }

    fn key(&self) -> &'a str {
        gi::unquote(self.key_raw)
    }
}

/// A quoted string, or a bare run of non-blank characters (word characters only for keys).
fn token_len(s: &str, key: bool) -> Option<usize> {
    if let Some(inner) = s.strip_prefix('"') {
        return inner.find('"').map(|i| i + 2);
    }
    let len = s
        .bytes()
        .take_while(|b| {
            if key {
                b.is_ascii_alphanumeric() || *b == b'_'
            } else {
                !b.is_ascii_whitespace() && *b != b'{' && *b != b'}'
            }
        })
        .count();
    (len > 0).then_some(len)
}

#[cfg(test)]
mod tests {
    use super::*;

    const FILE: &str = "\"GameInfo\"\n{\n\tgame \"citadel\"\n\tRenderSystem\n\t{\n\t\tUseReverseDepth 1\n\t\t\"LowLatency\"\t\t\"1\" // nvidia\n\t\tModes\n\t\t{\n\t\t\tgame Default\n\t\t}\n\t\t// Skipped 0\n\t}\n\tSceneSystem\n\t{\n\t\tCSMCascadeResolution 2048\n\t\tFog\t\t1\n\t}\n\tConVars\n\t{\n\t\tr_farz \"1\"\n\t}\n}\n";

    fn edits(pairs: &[(&str, KeyEdit)]) -> SectionEdits {
        pairs
            .iter()
            .map(|(k, e)| (k.to_string(), e.clone()))
            .collect()
    }

    fn set(v: &str) -> KeyEdit {
        KeyEdit::Set(v.to_string())
    }

    #[test]
    fn scalar_parse_handles_bare_and_quoted_tokens() {
        let s = Scalar::parse("\t\t\"LowLatency\"\t\t\"1\" // nvidia").unwrap();
        assert_eq!(
            s,
            Scalar {
                indent: "\t\t",
                key_raw: "\"LowLatency\"",
                pad: "\t\t",
                value_raw: "\"1\"",
                trail: " // nvidia",
            }
        );
        assert_eq!(s.key(), "LowLatency");
        let s = Scalar::parse("    Game_UILanguage  citadel_*LANGUAGE*").unwrap();
        assert_eq!(s.value_raw, "citadel_*LANGUAGE*");
        for not_scalar in [
            "\tSceneSystem",
            "\t{",
            "\t}",
            "\t// Skipped 0",
            "",
            "\tA B C",
        ] {
            assert_eq!(Scalar::parse(not_scalar), None, "{not_scalar:?}");
        }
    }

    #[test]
    fn section_values_unquote_and_prefix_nested_keys() {
        let values = section_values(FILE, "RenderSystem").unwrap().unwrap();
        assert_eq!(
            values,
            BTreeMap::from([
                ("UseReverseDepth".to_string(), "1".to_string()),
                ("LowLatency".to_string(), "1".to_string()),
                ("Modes/game".to_string(), "Default".to_string()),
            ])
        );
        assert_eq!(section_values(FILE, "Particles").unwrap(), None);
        assert_eq!(
            section_values(FILE, "game").unwrap(),
            None,
            "a scalar is not a section"
        );
        assert_eq!(
            section_values(FILE, "Modes").unwrap(),
            None,
            "nested blocks are not top-level sections"
        );
    }

    #[test]
    fn set_in_place_keeps_layout_and_inserts_missing_keys_before_the_brace() {
        let out = edit_section(
            FILE,
            "SceneSystem",
            &edits(&[("Fog", set("0")), ("Batch", set("20"))]),
        )
        .unwrap();
        let expected = FILE.replace("\t\tFog\t\t1\n\t}", "\t\tFog\t\t0\n\t\tBatch 20\n\t}");
        assert_eq!(out, expected);
        let again = edit_section(
            &out,
            "SceneSystem",
            &edits(&[("Fog", set("0")), ("Batch", set("20"))]),
        )
        .unwrap();
        assert_eq!(again, out, "idempotent");
    }

    #[test]
    fn set_matches_the_quoting_of_the_line_and_of_the_section() {
        let out = edit_section(
            FILE,
            "RenderSystem",
            &edits(&[("LowLatency", set("0")), ("New", set("x"))]),
        )
        .unwrap();
        assert!(
            out.contains("\t\t\"LowLatency\"\t\t\"0\" // nvidia\n"),
            "{out}"
        );
        assert!(
            out.contains("\t\t// Skipped 0\n\t\tNew \"x\"\n\t}"),
            "last top-level scalar was quoted: {out}"
        );
        let quoted_section =
            FILE.replace("CSMCascadeResolution 2048", "CSMCascadeResolution \"2048\"");
        let out = edit_section(
            &quoted_section,
            "SceneSystem",
            &edits(&[("New", set("1 2"))]),
        )
        .unwrap();
        assert!(out.contains("\t\tFog\t\t1\n\t\tNew \"1 2\"\n\t}"), "{out}");
    }

    #[test]
    fn remove_drops_the_key_and_is_a_no_op_when_absent() {
        let out = edit_section(
            FILE,
            "SceneSystem",
            &edits(&[("Fog", KeyEdit::Remove), ("Nope", KeyEdit::Remove)]),
        )
        .unwrap();
        assert_eq!(out, FILE.replace("\t\tFog\t\t1\n", ""));
        assert_eq!(
            edit_section(FILE, "Missing", &edits(&[("Fog", KeyEdit::Remove)])).unwrap(),
            FILE
        );
    }

    #[test]
    fn set_in_a_missing_section_is_an_error() {
        assert_eq!(
            edit_section(FILE, "Missing", &edits(&[("Fog", set("0"))])),
            Err(GiError::NoSection("Missing".into()))
        );
    }

    #[test]
    fn nested_keys_and_the_convars_block_are_never_touched() {
        let out = edit_section(FILE, "RenderSystem", &edits(&[("game", set("X"))])).unwrap();
        assert!(out.contains("\t\t\tgame Default\n"), "{out}");
        assert!(
            out.contains("\t\t// Skipped 0\n\t\tgame \"X\"\n\t}"),
            "{out}"
        );
        let out = edit_section(FILE, "SceneSystem", &edits(&[("r_farz", set("9"))])).unwrap();
        assert!(out.contains("\t\tr_farz \"1\"\n"));
    }

    #[test]
    fn crlf_and_same_line_braces_round_trip() {
        let crlf = FILE.replace('\n', "\r\n");
        let out = edit_section(&crlf, "SceneSystem", &edits(&[("Batch", set("20"))])).unwrap();
        assert!(!out.replace("\r\n", "").contains('\n'));
        assert!(out.contains("\t\tFog\t\t1\r\n\t\tBatch 20\r\n\t}"), "{out}");
        let same_line = FILE.replace("\tSceneSystem\n\t{", "\tSceneSystem {");
        let values = section_values(&same_line, "SceneSystem").unwrap().unwrap();
        assert_eq!(values["Fog"], "1");
    }

    #[test]
    fn empty_edits_leave_every_byte_alone() {
        assert_eq!(
            edit_section(FILE, "SceneSystem", &SectionEdits::new()).unwrap(),
            FILE
        );
    }
}
