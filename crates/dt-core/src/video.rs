//! `video.txt` reader/writer. Line-based like `gi`: keeps the header and unknown lines.

use std::collections::BTreeMap;

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum VideoError {
    #[error("video.txt has no settings block")]
    NoBlock,
}

/// `"setting.x" "v"` pairs in file order.
pub fn read_settings(text: &str) -> Result<Vec<(String, String)>, VideoError> {
    let _ = text;
    todo!()
}

/// Sets existing keys in place; missing keys are appended before the closing brace.
pub fn apply_settings(text: &str, settings: &BTreeMap<String, String>) -> Result<String, VideoError> {
    let _ = (text, settings);
    todo!()
}

/// Keeps `target`'s header (device id lines etc.) and takes every setting from `source`.
pub fn replace_settings(target: &str, source: &str) -> Result<String, VideoError> {
    let _ = (target, source);
    todo!()
}
