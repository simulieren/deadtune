//! Timestamped backups with sha256, a never-overwritten "original" snapshot, and atomic writes.

use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use chrono::{DateTime, NaiveDateTime, Utc};
use sha2::{Digest, Sha256};

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

/// Fixed width and free of `:` so names sort chronologically and are valid on Windows.
const STAMP_FORMAT: &str = "%Y%m%dT%H%M%S%6fZ";

impl FileKind {
    fn name(self) -> &'static str {
        match self {
            FileKind::GameInfo => "gameinfo",
            FileKind::Video => "video",
        }
    }

    fn ext(self) -> &'static str {
        match self {
            FileKind::GameInfo => "gi",
            FileKind::Video => "txt",
        }
    }
}

impl BackupStore {
    pub fn open(root: impl Into<PathBuf>) -> io::Result<BackupStore> {
        let root = root.into();
        fs::create_dir_all(root.join("original"))?;
        fs::create_dir_all(root.join("backups"))?;
        Ok(BackupStore { root })
    }

    /// Idempotent: the first snapshot wins forever.
    pub fn snapshot_original(&self, kind: FileKind, live: &Path) -> io::Result<BackupEntry> {
        if let Some(entry) = self.original(kind) {
            return Ok(entry);
        }
        let bytes = fs::read(live)?;
        atomic_write(&self.original_path(kind), &bytes)?;
        // The sidecar marks the snapshot complete; a crash before it means the next run snapshots again.
        atomic_write(
            &self.original_sha_path(kind),
            format!("{}\n", sha256_hex(&bytes)).as_bytes(),
        )?;
        self.original(kind)
            .ok_or_else(|| io::Error::other("original snapshot vanished right after writing it"))
    }

    pub fn original(&self, kind: FileKind) -> Option<BackupEntry> {
        let sha256 = fs::read_to_string(self.original_sha_path(kind))
            .ok()?
            .trim()
            .to_string();
        let path = self.original_path(kind);
        let modified = fs::metadata(&path).ok()?.modified().ok()?;
        Some(BackupEntry {
            kind,
            path,
            sha256,
            created: modified.into(),
        })
    }

    /// Copies `live` into a new timestamped backup, then prunes to `KEEP_BACKUPS`.
    pub fn backup(&self, kind: FileKind, live: &Path) -> io::Result<BackupEntry> {
        let bytes = fs::read(live)?;
        let dir = self.backup_dir(kind);
        fs::create_dir_all(&dir)?;

        let now = Utc::now();
        let mut created = DateTime::from_timestamp_micros(now.timestamp_micros()).unwrap_or(now);
        // Bumping past the newest keeps names unique within one microsecond and ordered if the clock steps back.
        if let Some((newest, _)) = self.stamped_files(kind)?.first() {
            created = created.max(*newest + chrono::Duration::microseconds(1));
        }
        let path = dir.join(format!("{}.{}", created.format(STAMP_FORMAT), kind.ext()));
        atomic_write(&path, &bytes)?;

        for (_, old) in self.stamped_files(kind)?.into_iter().skip(KEEP_BACKUPS) {
            fs::remove_file(old)?;
        }
        Ok(BackupEntry {
            kind,
            path,
            sha256: sha256_hex(&bytes),
            created,
        })
    }

    /// Newest first.
    pub fn list(&self, kind: FileKind) -> io::Result<Vec<BackupEntry>> {
        self.stamped_files(kind)?
            .into_iter()
            .map(|(created, path)| {
                let sha256 = sha256_hex(&fs::read(&path)?);
                Ok(BackupEntry {
                    kind,
                    path,
                    sha256,
                    created,
                })
            })
            .collect()
    }

    pub fn restore(&self, entry: &BackupEntry, live: &Path) -> io::Result<()> {
        let bytes = fs::read(&entry.path)?;
        if sha256_hex(&bytes) != entry.sha256 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!(
                    "backup {} no longer matches its sha256",
                    entry.path.display()
                ),
            ));
        }
        atomic_write(live, &bytes)
    }

    fn original_path(&self, kind: FileKind) -> PathBuf {
        self.root
            .join("original")
            .join(format!("{}.{}", kind.name(), kind.ext()))
    }

    fn original_sha_path(&self, kind: FileKind) -> PathBuf {
        self.root
            .join("original")
            .join(format!("{}.{}.sha256", kind.name(), kind.ext()))
    }

    fn backup_dir(&self, kind: FileKind) -> PathBuf {
        self.root.join("backups").join(kind.name())
    }

    /// Newest first. Files whose names do not parse (temp files, strays) are ignored.
    fn stamped_files(&self, kind: FileKind) -> io::Result<Vec<(DateTime<Utc>, PathBuf)>> {
        let entries = match fs::read_dir(self.backup_dir(kind)) {
            Ok(entries) => entries,
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(e) => return Err(e),
        };
        let suffix = format!(".{}", kind.ext());
        let mut files = Vec::new();
        for entry in entries {
            let path = entry?.path();
            let stamp = path
                .file_name()
                .and_then(|n| n.to_str())
                .and_then(|n| n.strip_suffix(&suffix))
                .and_then(|s| NaiveDateTime::parse_from_str(s, STAMP_FORMAT).ok());
            if let Some(stamp) = stamp {
                files.push((stamp.and_utc(), path));
            }
        }
        files.sort_by_key(|(stamp, _)| std::cmp::Reverse(*stamp));
        Ok(files)
    }
}

pub fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// Temp file in the same directory + rename.
///
/// `std::fs::rename` replaces an existing target on Windows as well (MoveFileExW, falling back to
/// SetFileInformationByHandle). It fails there if another process holds the target open without
/// FILE_SHARE_DELETE, and the error is returned rather than leaving a half-written file.
pub fn atomic_write(path: &Path, bytes: &[u8]) -> io::Result<()> {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let file_name = path.file_name().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "atomic_write needs a file path",
        )
    })?;
    let dir = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let tmp = dir.join(format!(
        ".{}.{}-{}.tmp",
        file_name.to_string_lossy(),
        std::process::id(),
        COUNTER.fetch_add(1, Ordering::Relaxed)
    ));
    let result = (|| {
        let mut file = fs::File::create_new(&tmp)?;
        file.write_all(bytes)?;
        file.sync_all()?;
        drop(file);
        fs::rename(&tmp, path)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&tmp);
    }
    result
}

/// Where the backup store and the addon, HUD, guard and practice records live inside the
/// data dir. The GUI and the CLI must agree on it or each misses what the other wrote.
pub fn records_dir(data_dir: &Path) -> PathBuf {
    data_dir.join("backups")
}

/// Per-user data dir for DeadTune (backups, profiles, bench history).
pub fn data_dir() -> PathBuf {
    let env_dir = |name: &str| {
        std::env::var_os(name)
            .map(PathBuf::from)
            .filter(|p| p.is_absolute())
    };
    #[cfg(windows)]
    let dir = env_dir("APPDATA").map(|d| d.join("DeadTune"));
    #[cfg(target_os = "macos")]
    let dir = env_dir("HOME").map(|h| h.join("Library/Application Support/DeadTune"));
    #[cfg(not(any(windows, target_os = "macos")))]
    let dir = env_dir("XDG_DATA_HOME")
        .or_else(|| env_dir("HOME").map(|h| h.join(".local/share")))
        .map(|d| d.join("deadtune"));
    dir.unwrap_or_else(|| PathBuf::from("DeadTune"))
}

#[cfg(test)]
mod tests {
    use super::*;

    const CRLF_GAMEINFO: &[u8] = include_bytes!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../research/configs/OptimizationLock/kaizuchanerus minimum spec/gameinfo.gi"
    ));

    fn store_with_live(bytes: &[u8]) -> (tempfile::TempDir, BackupStore, PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let store = BackupStore::open(dir.path().join("store")).unwrap();
        let live = dir.path().join("gameinfo.gi");
        std::fs::write(&live, bytes).unwrap();
        (dir, store, live)
    }

    #[test]
    fn sha256_hex_matches_known_vector() {
        assert_eq!(
            sha256_hex(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    #[test]
    fn atomic_write_creates_and_replaces_without_leaving_temp_files() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("video.txt");
        atomic_write(&path, b"first").unwrap();
        atomic_write(&path, b"second").unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), b"second");
        let names: Vec<_> = std::fs::read_dir(dir.path())
            .unwrap()
            .map(|e| e.unwrap().file_name())
            .collect();
        assert_eq!(
            names,
            vec![std::ffi::OsString::from("video.txt")],
            "temp file left behind"
        );
    }

    #[test]
    fn snapshot_original_keeps_the_first_snapshot_forever() {
        let (_dir, store, live) = store_with_live(CRLF_GAMEINFO);
        let first = store.snapshot_original(FileKind::GameInfo, &live).unwrap();
        std::fs::write(&live, b"modified").unwrap();
        let second = store.snapshot_original(FileKind::GameInfo, &live).unwrap();
        assert_eq!(first, second);
        assert_eq!(
            std::fs::read(&first.path).unwrap(),
            CRLF_GAMEINFO,
            "original must be byte-exact (CRLF kept)"
        );
        assert_eq!(first.sha256, sha256_hex(CRLF_GAMEINFO));
        assert_eq!(store.original(FileKind::GameInfo), Some(first));
        assert_eq!(store.original(FileKind::Video), None);
    }

    #[test]
    fn backups_in_the_same_second_are_unique_and_listed_newest_first() {
        let (_dir, store, live) = store_with_live(b"v1");
        let a = store.backup(FileKind::GameInfo, &live).unwrap();
        std::fs::write(&live, b"v2").unwrap();
        let b = store.backup(FileKind::GameInfo, &live).unwrap();
        std::fs::write(&live, b"v3").unwrap();
        let c = store.backup(FileKind::GameInfo, &live).unwrap();
        assert!(a.path != b.path && b.path != c.path);
        let list = store.list(FileKind::GameInfo).unwrap();
        assert_eq!(list, vec![c, b, a]);
        assert_eq!(list[0].sha256, sha256_hex(b"v3"));
        assert!(store.list(FileKind::Video).unwrap().is_empty());
    }

    #[test]
    fn backup_prunes_to_keep_backups() {
        let (_dir, store, live) = store_with_live(b"x");
        let mut made = Vec::new();
        for i in 0..KEEP_BACKUPS + 3 {
            std::fs::write(&live, format!("v{i}")).unwrap();
            made.push(store.backup(FileKind::Video, &live).unwrap());
        }
        let list = store.list(FileKind::Video).unwrap();
        assert_eq!(list.len(), KEEP_BACKUPS);
        assert_eq!(list[0], *made.last().unwrap());
        assert!(!made[2].path.exists(), "oldest backups must be deleted");
        assert!(
            made[3].path.exists(),
            "the newest KEEP_BACKUPS must survive"
        );
    }

    #[test]
    fn restore_writes_backup_bytes_back_exactly() {
        let (_dir, store, live) = store_with_live(CRLF_GAMEINFO);
        let entry = store.backup(FileKind::GameInfo, &live).unwrap();
        std::fs::write(&live, b"steam overwrote this").unwrap();
        store.restore(&entry, &live).unwrap();
        assert_eq!(std::fs::read(&live).unwrap(), CRLF_GAMEINFO);
    }

    #[test]
    fn restore_refuses_a_backup_whose_hash_no_longer_matches() {
        let (_dir, store, live) = store_with_live(b"good");
        let entry = store.backup(FileKind::GameInfo, &live).unwrap();
        std::fs::write(&entry.path, b"corrupted").unwrap();
        let err = store.restore(&entry, &live).unwrap_err();
        assert_eq!(err.kind(), std::io::ErrorKind::InvalidData);
        assert_eq!(std::fs::read(&live).unwrap(), b"good");
    }

    #[test]
    fn data_dir_ends_in_the_app_folder() {
        let name = data_dir()
            .file_name()
            .unwrap()
            .to_string_lossy()
            .to_lowercase();
        assert_eq!(name, "deadtune");
    }
}
