//! VPK v2 archives: read the game's split `pak01_dir.vpk` (+ `pak01_NNN.vpk`), and
//! write single-file addons where every entry's data follows the tree (index 0x7fff).
//! Format notes: research/hud/vpk-and-compiled-resources.md section 4.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

pub const SIGNATURE: u32 = 0x55AA_1234;
/// Archive index meaning "data is in the dir file, after the tree".
pub const EMBEDDED: u16 = 0x7fff;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VpkEntry {
    pub crc: u32,
    pub preload: Vec<u8>,
    pub archive_index: u16,
    pub offset: u32,
    pub length: u32,
}

/// Parsed directory of a `*_dir.vpk`. Paths use forward slashes and include the
/// extension (`panorama/styles/hud.vcss_c`).
#[derive(Clone, Debug)]
pub struct VpkDir {
    pub dir_path: PathBuf,
    pub entries: BTreeMap<String, VpkEntry>,
    /// Absolute offset of the embedded data section in the dir file (28 + tree size).
    data_start: u64,
}

#[derive(Debug, thiserror::Error)]
pub enum VpkError {
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("not a VPK v2 file: {0}")]
    BadHeader(String),
    #[error("malformed tree: {0}")]
    BadTree(String),
    #[error("no entry {0}")]
    Missing(String),
    #[error("crc mismatch for {0}")]
    Crc(String),
}

impl VpkDir {
    pub fn open(dir_path: &Path) -> Result<VpkDir, VpkError> {
        todo!()
    }

    /// Parses a dir file already in memory. `dir_path` is only used to find sibling
    /// `_NNN.vpk` archives on `read`.
    pub fn parse(dir_path: &Path, bytes: &[u8]) -> Result<VpkDir, VpkError> {
        todo!()
    }

    /// Full file bytes (preload + archive data), CRC-checked.
    pub fn read(&self, path: &str) -> Result<Vec<u8>, VpkError> {
        todo!()
    }

    pub fn contains(&self, path: &str) -> bool {
        self.entries.contains_key(path)
    }
}

/// Builds a single-file VPK v2. Deterministic: same input, same bytes. The MD5
/// section is zero-filled (QoL Lite ships this way and loads).
pub fn write(files: &BTreeMap<String, Vec<u8>>) -> Vec<u8> {
    todo!()
}
