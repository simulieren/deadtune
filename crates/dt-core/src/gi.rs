//! Lossless editor for the `ConVars { ... }` block of `gameinfo.gi`.
//! Port of OptimizationLock's `gameinfo_updater.py`: line-based, never re-serializes.

use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Override {
    Set(String),
    Comment,
}

pub type Overrides = BTreeMap<String, Override>;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Eol {
    Lf,
    CrLf,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConVarEntry {
    pub name: String,
    pub value: String,
    pub commented: bool,
    pub comment: Option<String>,
    /// 0-based line index in the file.
    pub line: usize,
}

#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct CommentMeta {
    pub default: Option<String>,
    pub description: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ApplyOutcome {
    pub text: String,
    pub changed: Vec<String>,
    pub injected: Vec<String>,
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum GiError {
    #[error("no ConVars block found")]
    NoConVarsBlock,
    /// `line` is 1-based, unlike `ConVarEntry::line`.
    #[error("unbalanced braces near line {line}")]
    UnbalancedBraces { line: usize },
    #[error("no {0} section found")]
    NoSection(String),
}

pub fn detect_eol(text: &str) -> Eol {
    if text.contains("\r\n") {
        Eol::CrLf
    } else {
        Eol::Lf
    }
}

pub fn validate_braces(text: &str) -> Result<(), GiError> {
    let mut open_lines = Vec::new();
    for (i, line) in split_lines(text).iter().enumerate() {
        let scan = LineScan::new(line.content);
        open_lines.extend(std::iter::repeat_n(i + 1, scan.opens));
        for _ in 0..scan.closes {
            open_lines
                .pop()
                .ok_or(GiError::UnbalancedBraces { line: i + 1 })?;
        }
    }
    match open_lines.last() {
        Some(&line) => Err(GiError::UnbalancedBraces { line }),
        None => Ok(()),
    }
}

/// Every convar line in the ConVars block, commented or not, in file order.
pub fn read_convars(text: &str) -> Result<Vec<ConVarEntry>, GiError> {
    validate_braces(text)?;
    let lines = split_lines(text);
    let block = find_block(&lines)?;
    let first = block.open_line + 1;
    let entries = parse_body(&lines[first..block.close_line])
        .into_iter()
        .enumerate()
        .filter_map(|(i, cv)| {
            let cv = cv?;
            let comment = cv.trail.trim();
            Some(ConVarEntry {
                name: cv.name.to_string(),
                value: unquote(cv.value).to_string(),
                commented: cv.slashes.is_some(),
                comment: (!comment.is_empty()).then(|| comment.to_string()),
                line: first + i,
            })
        })
        .collect();
    Ok(entries)
}

/// Uncommented convars only: what the engine will actually load.
pub fn effective_values(text: &str) -> Result<BTreeMap<String, String>, GiError> {
    Ok(read_convars(text)?
        .into_iter()
        .filter(|e| !e.commented)
        .map(|e| (e.name, e.value))
        .collect())
}

/// `[def: "x"]` becomes `default`, the rest becomes `description`.
pub fn parse_comment_meta(comment: &str) -> CommentMeta {
    let text = comment.trim().trim_start_matches("//").trim();
    let Some(start) = ["[def:", "[default:"]
        .iter()
        .find_map(|tag| text.find(tag).map(|i| (i, tag.len())))
    else {
        return CommentMeta {
            default: None,
            description: text.to_string(),
        };
    };
    let (open, tag_len) = start;
    let rest = &text[open + tag_len..];
    let close = rest.find(']').unwrap_or(rest.len());
    let raw = rest[..close].trim().trim_matches('"');
    let default = (raw != "null").then(|| raw.to_string());
    let after = rest.get(close + 1..).unwrap_or("");
    let description = format!("{} {}", text[..open].trim(), after.trim())
        .trim()
        .to_string();
    CommentMeta {
        default,
        description,
    }
}

/// Applies overrides in place; unknown `Set`s go into the managed block, which is
/// stripped and regenerated on every call so repeated applies converge.
pub fn apply_overrides(text: &str, overrides: &Overrides) -> Result<ApplyOutcome, GiError> {
    validate_braces(text)?;
    let lines = split_lines(text);
    let block = find_block(&lines)?;
    let mut body = lines[block.open_line + 1..block.close_line].to_vec();
    strip_managed_blocks(&mut body);
    let parsed = parse_body(&body);

    let mut new_body = String::with_capacity(text.len());
    let mut changed: Vec<String> = Vec::new();
    let mut matched = BTreeSet::new();
    for (line, cv) in body.iter().zip(&parsed) {
        let Some(cv) = cv else {
            push_line(&mut new_body, line);
            continue;
        };
        let changes = match overrides.get(cv.name) {
            Some(Override::Set(value)) => {
                let quoted = quote(value);
                new_body.extend([cv.indent, cv.name, cv.pad, &quoted, cv.trail, line.eol]);
                matched.insert(cv.name);
                cv.slashes.is_some() || cv.value != quoted
            }
            Some(Override::Comment) if cv.slashes.is_none() => {
                new_body.extend([
                    cv.indent, "// ", cv.name, cv.pad, cv.value, cv.trail, line.eol,
                ]);
                true
            }
            _ => {
                push_line(&mut new_body, line);
                false
            }
        };
        if changes && !changed.iter().any(|n| n == cv.name) {
            changed.push(cv.name.to_string());
        }
    }

    let injected: Vec<(&String, &String)> = overrides
        .iter()
        .filter_map(|(name, o)| match o {
            Override::Set(value) if !matched.contains(name.as_str()) => Some((name, value)),
            _ => None,
        })
        .collect();

    let mut out = String::with_capacity(text.len());
    for line in &lines[..=block.open_line] {
        push_line(&mut out, line);
    }
    if !injected.is_empty() {
        let indent = parsed
            .iter()
            .flatten()
            .next()
            .map_or("        ", |cv| cv.indent);
        let eol = eol_str(detect_eol(text));
        out.extend([
            indent,
            MANAGED_HEADER,
            " -- do not edit this block manually =====",
            eol,
        ]);
        for (name, value) in &injected {
            out.extend([indent, name.as_str(), " ", &quote(value), eol]);
        }
        out.extend([indent, MANAGED_FOOTER, " =====", eol]);
    }
    out.push_str(&new_body);
    for line in &lines[block.close_line..] {
        push_line(&mut out, line);
    }
    validate_braces(&out)?;
    Ok(ApplyOutcome {
        text: out,
        changed,
        injected: injected.into_iter().map(|(name, _)| name.clone()).collect(),
    })
}

/// Replaces the ConVars body of `target` with the one from `source`, leaving every
/// other byte of `target` (SearchPaths edits by mod managers) untouched.
pub fn replace_convars_block(target: &str, source: &str) -> Result<String, GiError> {
    validate_braces(target)?;
    validate_braces(source)?;
    let target_lines = split_lines(target);
    let source_lines = split_lines(source);
    let target_block = find_block(&target_lines)?;
    let source_block = find_block(&source_lines)?;
    let eol = eol_str(detect_eol(target));
    let mut out = String::with_capacity(target.len() + source.len());
    for line in &target_lines[..=target_block.open_line] {
        push_line(&mut out, line);
    }
    for line in &source_lines[source_block.open_line + 1..source_block.close_line] {
        out.extend([line.content, eol]);
    }
    for line in &target_lines[target_block.close_line..] {
        push_line(&mut out, line);
    }
    Ok(out)
}

const MANAGED_HEADER: &str = "// ===== deadtune managed convars";
const MANAGED_FOOTER: &str = "// ===== end deadtune managed convars";
const MANAGED_TAGS: [(&str, &str); 2] = [
    (MANAGED_HEADER, MANAGED_FOOTER),
    (
        "// ===== gameinfo-updater added convars",
        "// ===== end gameinfo-updater added convars",
    ),
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Line<'a> {
    pub(crate) content: &'a str,
    pub(crate) eol: &'a str,
}

pub(crate) fn split_lines(text: &str) -> Vec<Line<'_>> {
    text.split_inclusive('\n')
        .map(|raw| {
            let content = raw
                .strip_suffix("\r\n")
                .or_else(|| raw.strip_suffix('\n'))
                .unwrap_or(raw);
            Line {
                content,
                eol: &raw[content.len()..],
            }
        })
        .collect()
}

pub(crate) fn push_line(out: &mut String, line: &Line) {
    out.push_str(line.content);
    out.push_str(line.eol);
}

pub(crate) fn eol_str(eol: Eol) -> &'static str {
    match eol {
        Eol::Lf => "\n",
        Eol::CrLf => "\r\n",
    }
}

#[derive(Debug, PartialEq, Eq)]
struct ConVarLine<'a> {
    indent: &'a str,
    slashes: Option<&'a str>,
    name: &'a str,
    pad: &'a str,
    value: &'a str,
    trail: &'a str,
}

impl ConVarLine<'_> {
    fn parse(content: &str) -> Option<ConVarLine<'_>> {
        let blank_end =
            |from: usize| content.len() - content[from..].trim_start_matches([' ', '\t']).len();
        let indent_end = blank_end(0);
        let (slashes, name_start) = if content[indent_end..].starts_with("//") {
            let end = blank_end(indent_end + 2);
            (Some(&content[indent_end..end]), end)
        } else {
            (None, indent_end)
        };
        let name_end = name_start
            + content[name_start..]
                .bytes()
                .take_while(|b| b.is_ascii_alphanumeric() || *b == b'_')
                .count();
        let value_start = blank_end(name_end);
        if name_end == name_start || value_start == name_end {
            return None;
        }
        let rest = &content[value_start..];
        let quoted = rest
            .strip_prefix('"')
            .and_then(|r| r.find('"'))
            .map(|i| i + 2);
        let bare = rest.find(char::is_whitespace).unwrap_or(rest.len());
        let value_len = quoted
            .filter(|&n| is_trail(&rest[n..]))
            .or(Some(bare).filter(|&n| n > 0 && is_trail(&rest[n..])))?;
        Some(ConVarLine {
            indent: &content[..indent_end],
            slashes,
            name: &content[name_start..name_end],
            pad: &content[name_end..value_start],
            value: &rest[..value_len],
            trail: &rest[value_len..],
        })
    }
}

/// Matches `([ \t]+//.*)?[ \t]*$`.
fn is_trail(s: &str) -> bool {
    let after_blanks = s.trim_start_matches([' ', '\t']);
    after_blanks.is_empty() || (after_blanks.len() < s.len() && after_blanks.starts_with("//"))
}

pub(crate) fn unquote(value: &str) -> &str {
    value
        .strip_prefix('"')
        .and_then(|v| v.strip_suffix('"'))
        .unwrap_or(value)
}

fn quote(value: &str) -> String {
    if value.len() >= 2 && value.starts_with('"') && value.ends_with('"') {
        value.to_string()
    } else {
        format!("\"{value}\"")
    }
}

/// Braces and the `ConVars` keyword count only outside `//` comments and quoted strings.
pub(crate) struct LineScan {
    pub(crate) opens: usize,
    pub(crate) closes: usize,
    mentions_convars: bool,
}

impl LineScan {
    pub(crate) fn new(content: &str) -> Self {
        let mut code = String::with_capacity(content.len());
        let (mut opens, mut closes, mut in_quote) = (0, 0, false);
        let mut chars = content.chars().peekable();
        while let Some(c) = chars.next() {
            match c {
                '"' => {
                    in_quote = !in_quote;
                    code.push(' ');
                }
                _ if in_quote => code.push(' '),
                '/' if chars.peek() == Some(&'/') => break,
                _ => {
                    opens += usize::from(c == '{');
                    closes += usize::from(c == '}');
                    code.push(c);
                }
            }
        }
        let is_word = |b: Option<&u8>| b.is_some_and(|b| b.is_ascii_alphanumeric() || *b == b'_');
        let bytes = code.as_bytes();
        let mentions_convars = code.match_indices("ConVars").any(|(i, m)| {
            !is_word(i.checked_sub(1).and_then(|j| bytes.get(j)))
                && !is_word(bytes.get(i + m.len()))
        });
        LineScan {
            opens,
            closes,
            mentions_convars,
        }
    }
}

struct Block {
    open_line: usize,
    close_line: usize,
}

fn find_block(lines: &[Line]) -> Result<Block, GiError> {
    let scans: Vec<LineScan> = lines.iter().map(|l| LineScan::new(l.content)).collect();
    let keyword = scans
        .iter()
        .position(|s| s.mentions_convars)
        .ok_or(GiError::NoConVarsBlock)?;
    let open_line = (keyword..lines.len())
        .find(|&i| scans[i].opens > 0)
        .ok_or(GiError::NoConVarsBlock)?;
    let mut depth = 0isize;
    for (i, scan) in scans.iter().enumerate().skip(open_line) {
        depth += scan.opens as isize - scan.closes as isize;
        if depth <= 0 {
            return Ok(Block {
                open_line,
                close_line: i,
            });
        }
    }
    Err(GiError::UnbalancedBraces {
        line: open_line + 1,
    })
}

/// Lines inside a nested `name { ... }` block yield `None`: those keys are not ConVars.
fn parse_body<'a>(body: &[Line<'a>]) -> Vec<Option<ConVarLine<'a>>> {
    let mut sub_depth = 0isize;
    body.iter()
        .map(|line| {
            let scan = LineScan::new(line.content);
            if sub_depth > 0 || scan.opens > scan.closes {
                sub_depth += scan.opens as isize - scan.closes as isize;
                return None;
            }
            ConVarLine::parse(line.content)
        })
        .collect()
}

/// A header with no footer after it is left alone so a hand-edited file never loses its body.
fn strip_managed_blocks(body: &mut Vec<Line>) {
    let starts = |line: &Line, tag: &str| line.content.trim().starts_with(tag);
    while let Some(range) = MANAGED_TAGS.iter().find_map(|&(header, footer)| {
        let start = body.iter().position(|l| starts(l, header))?;
        let end = start + body[start..].iter().position(|l| starts(l, footer))?;
        Some(start..=end)
    }) {
        body.drain(range);
    }
}

#[cfg(test)]
mod comment_meta_tests {
    use super::*;

    fn meta(default: Option<&str>, description: &str) -> CommentMeta {
        CommentMeta {
            default: default.map(str::to_string),
            description: description.to_string(),
        }
    }

    #[test]
    fn quoted_default_and_description() {
        assert_eq!(
            parse_comment_meta(r#"// Disables creep animations [def: "0"]"#),
            meta(Some("0"), "Disables creep animations")
        );
    }

    #[test]
    fn annotation_variants() {
        assert_eq!(parse_comment_meta(r#"[def:"1"]"#), meta(Some("1"), ""));
        assert_eq!(parse_comment_meta(r#"[def: "0]"#), meta(Some("0"), ""));
        assert_eq!(
            parse_comment_meta("[def: 0.55] tweak"),
            meta(Some("0.55"), "tweak")
        );
        assert_eq!(
            parse_comment_meta(r#"[default: "128"]"#),
            meta(Some("128"), "")
        );
        assert_eq!(parse_comment_meta("[def: null]"), meta(None, ""));
        assert_eq!(parse_comment_meta(r#"[def: "" ]"#), meta(Some(""), ""));
    }

    #[test]
    fn no_annotation() {
        assert_eq!(
            parse_comment_meta("  just words "),
            meta(None, "just words")
        );
    }
}

#[cfg(test)]
mod line_tests {
    use super::*;

    fn cv<'a>(
        indent: &'a str,
        slashes: Option<&'a str>,
        name: &'a str,
        pad: &'a str,
        value: &'a str,
        trail: &'a str,
    ) -> ConVarLine<'a> {
        ConVarLine {
            indent,
            slashes,
            name,
            pad,
            value,
            trail,
        }
    }

    #[test]
    fn split_lines_round_trips() {
        for text in ["", "a", "a\n", "a\r\nb\nc", "\r\n\n", "x\r"] {
            let lines = split_lines(text);
            assert_eq!(
                lines
                    .iter()
                    .map(|l| [l.content, l.eol].concat())
                    .collect::<String>(),
                text
            );
        }
        assert!(split_lines("").is_empty());
        assert_eq!(
            split_lines("a\r\nb"),
            vec![
                Line {
                    content: "a",
                    eol: "\r\n"
                },
                Line {
                    content: "b",
                    eol: ""
                }
            ]
        );
    }

    #[test]
    fn parse_line_shapes() {
        assert_eq!(
            ConVarLine::parse("\tname \"a b\" // c "),
            Some(cv("\t", None, "name", " ", "\"a b\"", " // c "))
        );
        assert_eq!(
            ConVarLine::parse("name \"a\"//c"),
            Some(cv("", None, "name", " ", "\"a\"//c", ""))
        );
        assert_eq!(
            ConVarLine::parse("//name 1"),
            Some(cv("", Some("//"), "name", " ", "1", ""))
        );
        assert_eq!(ConVarLine::parse("name value more"), None);
        for rejected in [
            "",
            "/",
            "// just prose words",
            "\"foo\" \"bar\"",
            "{",
            "rate",
            "rate  ",
        ] {
            assert_eq!(ConVarLine::parse(rejected), None, "{rejected:?}");
        }
    }
}
