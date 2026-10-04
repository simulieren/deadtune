//! Addon VPKs only mount when `gameinfo.gi` SearchPaths lists `Game citadel/addons`.
//! Pure text edits; the apply pipeline decides when to write. Touches nothing but
//! the `SearchPaths` block, preserving EOL style and indentation.

pub const ADDONS_LINE_VALUE: &str = "citadel/addons";

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum SearchPathsError {
    #[error("no SearchPaths block in gameinfo.gi")]
    Missing,
    #[error("unbalanced braces in SearchPaths")]
    Unbalanced,
}

/// True if an uncommented `Game citadel/addons` (any whitespace, optional quotes) exists.
pub fn has_addons(gameinfo: &str) -> Result<bool, SearchPathsError> {
    todo!()
}

/// Inserts `Game citadel/addons` as the first line of SearchPaths (addons must win
/// over `Game citadel`). Returns the input unchanged if already present.
pub fn ensure_addons(gameinfo: &str) -> Result<String, SearchPathsError> {
    todo!()
}
