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
    let _ = comment;
    todo!()
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
