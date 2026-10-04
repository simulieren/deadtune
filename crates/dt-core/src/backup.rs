//! Timestamped backups with sha256, a never-overwritten "original" snapshot, and atomic writes.

use std::path::{Path, PathBuf};

pub const KEEP_BACKUPS: usize = 20;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum FileKind {
    GameInfo,
    Video,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BackupEntry {
    pub kind: FileKind,
    pub path: PathBuf,
    pub sha256: String,
    pub created: chrono::DateTime<chrono::Utc>,
}

#[derive(Clone, Debug)]
pub struct BackupStore {
    pub root: PathBuf,
}

impl BackupStore {
    pub fn open(root: impl Into<PathBuf>) -> std::io::Result<BackupStore> {
        let _ = root;
        todo!()
    }

    /// Idempotent: the first snapshot wins forever.
    pub fn snapshot_original(&self, kind: FileKind, live: &Path) -> std::io::Result<BackupEntry> {
        let _ = (kind, live);
        todo!()
    }

    pub fn original(&self, kind: FileKind) -> Option<BackupEntry> {
        let _ = kind;
        todo!()
    }

    /// Copies `live` into a new timestamped backup, then prunes to `KEEP_BACKUPS`.
    pub fn backup(&self, kind: FileKind, live: &Path) -> std::io::Result<BackupEntry> {
        let _ = (kind, live);
        todo!()
    }

    /// Newest first.
    pub fn list(&self, kind: FileKind) -> std::io::Result<Vec<BackupEntry>> {
        let _ = kind;
        todo!()
    }

    pub fn restore(&self, entry: &BackupEntry, live: &Path) -> std::io::Result<()> {
        let _ = (entry, live);
        todo!()
    }
}

pub fn sha256_hex(bytes: &[u8]) -> String {
    let _ = bytes;
    todo!()
}

/// Temp file in the same directory + rename.
pub fn atomic_write(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let _ = (path, bytes);
    todo!()
}

/// Per-user data dir for DeadTune (backups, profiles, bench history).
pub fn data_dir() -> PathBuf {
    todo!()
}
