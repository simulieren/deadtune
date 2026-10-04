//! Lossless editor for the `ConVars { ... }` block of `gameinfo.gi`.
//! Port of OptimizationLock's `gameinfo_updater.py`: line-based, never re-serializes.

use std::collections::BTreeMap;

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
    #[error("unbalanced braces near line {line}")]
    UnbalancedBraces { line: usize },
}

pub fn detect_eol(text: &str) -> Eol {
    let _ = text;
    todo!()
}

pub fn validate_braces(text: &str) -> Result<(), GiError> {
    let _ = text;
    todo!()
}

/// Every convar line in the ConVars block, commented or not, in file order.
pub fn read_convars(text: &str) -> Result<Vec<ConVarEntry>, GiError> {
    let _ = text;
    todo!()
}

/// Uncommented convars only: what the engine will actually load.
pub fn effective_values(text: &str) -> Result<BTreeMap<String, String>, GiError> {
    let _ = text;
    todo!()
}

/// `[def: "x"]` becomes `default`, the rest becomes `description`.
pub fn parse_comment_meta(comment: &str) -> CommentMeta {
    let text = comment.trim().trim_start_matches("//").trim();
    let Some(start) = ["[def:", "[default:"].iter().find_map(|tag| text.find(tag).map(|i| (i, tag.len()))) else {
        return CommentMeta { default: None, description: text.to_string() };
    };
    let (open, tag_len) = start;
    let rest = &text[open + tag_len..];
    let close = rest.find(']').unwrap_or(rest.len());
    let raw = rest[..close].trim().trim_matches('"');
    let default = (raw != "null").then(|| raw.to_string());
    let after = rest.get(close + 1..).unwrap_or("");
    let description = format!("{} {}", text[..open].trim(), after.trim()).trim().to_string();
    CommentMeta { default, description }
}

/// Applies overrides in place; unknown `Set`s go into the managed block, which is
/// stripped and regenerated on every call so repeated applies converge.
pub fn apply_overrides(text: &str, overrides: &Overrides) -> Result<ApplyOutcome, GiError> {
    let _ = (text, overrides);
    todo!()
}

/// Replaces the ConVars body of `target` with the one from `source`, leaving every
/// other byte of `target` (SearchPaths edits by mod managers) untouched.
pub fn replace_convars_block(target: &str, source: &str) -> Result<String, GiError> {
    let _ = (target, source);
    todo!()
}

#[cfg(test)]
mod comment_meta_tests {
    use super::*;

    fn meta(default: Option<&str>, description: &str) -> CommentMeta {
        CommentMeta { default: default.map(str::to_string), description: description.to_string() }
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
        assert_eq!(parse_comment_meta("[def: 0.55] tweak"), meta(Some("0.55"), "tweak"));
        assert_eq!(parse_comment_meta(r#"[default: "128"]"#), meta(Some("128"), ""));
        assert_eq!(parse_comment_meta("[def: null]"), meta(None, ""));
        assert_eq!(parse_comment_meta(r#"[def: "" ]"#), meta(Some(""), ""));
    }

    #[test]
    fn no_annotation() {
        assert_eq!(parse_comment_meta("  just words "), meta(None, "just words"));
    }
}
