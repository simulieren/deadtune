//! Snapshot folders under `<data dir>/game-files`: the manifest, listing, lookup, delete.

use std::collections::BTreeSet;
use std::io;
use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};

use super::SnapshotError;
use super::spec::{Category, Source};
use crate::backup::atomic_write;

pub const DIR: &str = "game-files";
pub const MANIFEST: &str = "manifest.toml";
pub const PAK_LIST: &str = "pak01.tsv";
pub const RAW: &str = "raw";
pub const TEXT: &str = "text";
/// Folder name part when the Steam appmanifest is missing.
pub const NO_BUILD: &str = "unknown";
const STAMP: &str = "%Y%m%d-%H%M%S";

pub fn dir(data_dir: &Path) -> PathBuf {
    data_dir.join(DIR)
}

/// `<buildid>-<YYYYMMDD-HHMMSS>`.
pub fn folder_name(buildid: Option<&str>, taken: DateTime<Utc>) -> String {
    format!("{}-{}", buildid.unwrap_or(NO_BUILD), taken.format(STAMP))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Stored {
    Full,
    /// A texture over the size cap: everything before the pixel data, at `raw/<path>.header`.
    /// Only DeadTune 0.9.0 wrote these (the scope texture); later takes store such files in
    /// full and replace the header when they retake the build.
    Header,
    None,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Decoded {
    Text,
    /// Decoding failed; `text` points at the readable strings.
    Strings,
    None,
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct FileEntry {
    pub path: String,
    pub source: Source,
    pub size: u64,
    /// Eight hex digits: the VPK tree's CRC, or crc32 of the bytes for loose files.
    pub crc: String,
    pub sha256: String,
    pub categories: Vec<Category>,
    pub stored: Stored,
    /// Relative to `text/`.
    pub text: Option<String>,
    pub decoded: Decoded,
}

impl FileEntry {
    pub fn raw_path(&self, folder: &Path) -> Option<PathBuf> {
        match self.stored {
            Stored::Full => Some(folder.join(RAW).join(&self.path)),
            Stored::Header => Some(folder.join(RAW).join(format!("{}.header", self.path))),
            Stored::None => None,
        }
    }

    pub fn text_path(&self, folder: &Path) -> Option<PathBuf> {
        self.text.as_ref().map(|t| folder.join(TEXT).join(t))
    }
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Manifest {
    pub buildid: Option<String>,
    pub taken: DateTime<Utc>,
    pub deadtune: String,
    pub game_root: PathBuf,
    pub decode: bool,
    pub size_cap: Option<u64>,
    pub categories: BTreeSet<Category>,
    pub files: Vec<FileEntry>,
}

impl Manifest {
    pub fn load(folder: &Path) -> Result<Manifest, SnapshotError> {
        let path = folder.join(MANIFEST);
        let text = std::fs::read_to_string(&path)
            .map_err(|_| SnapshotError::NotASnapshot(folder.to_path_buf()))?;
        toml::from_str(&text).map_err(|e| SnapshotError::Toml(e.to_string()))
    }

    pub fn save(&self, folder: &Path) -> Result<(), SnapshotError> {
        let text = toml::to_string(self).map_err(|e| SnapshotError::Toml(e.to_string()))?;
        std::fs::create_dir_all(folder)?;
        atomic_write(&folder.join(MANIFEST), text.as_bytes())?;
        Ok(())
    }

    pub fn entry(&self, path: &str) -> Option<&FileEntry> {
        self.files.iter().find(|f| f.path == path)
    }

    /// Bytes the folder holds for the listed files.
    pub fn bytes_stored(&self) -> u64 {
        self.files
            .iter()
            .filter(|f| f.stored == Stored::Full)
            .map(|f| f.size)
            .sum()
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SnapshotInfo {
    pub folder: PathBuf,
    pub name: String,
    pub buildid: Option<String>,
    pub taken: Option<DateTime<Utc>>,
    pub files: usize,
    pub bytes: u64,
    /// `manifest.toml` is written last; without it the folder is a cancelled or crashed take.
    pub complete: bool,
    /// Report files in the folder (`diff-<old>-to-<new>.md`), newest first.
    pub reports: Vec<String>,
}

impl SnapshotInfo {
    fn read(folder: PathBuf) -> SnapshotInfo {
        let name = folder
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        let manifest = Manifest::load(&folder).ok();
        let mut reports: Vec<String> = std::fs::read_dir(&folder)
            .map(|dir| {
                dir.filter_map(|e| e.ok())
                    .filter_map(|e| e.file_name().into_string().ok())
                    .filter(|n| n.starts_with("diff-") && n.ends_with(".md"))
                    .collect()
            })
            .unwrap_or_default();
        reports.sort();
        reports.reverse();
        let from_name = name
            .rsplit_once('-')
            .and_then(|(rest, _)| rest.rsplit_once('-'))
            .map(|(build, _)| build.to_string())
            .filter(|b| b != NO_BUILD);
        SnapshotInfo {
            buildid: manifest.as_ref().map_or(from_name, |m| m.buildid.clone()),
            taken: manifest.as_ref().map(|m| m.taken),
            files: manifest.as_ref().map_or(0, |m| m.files.len()),
            bytes: manifest.as_ref().map_or(0, Manifest::bytes_stored),
            complete: manifest.is_some(),
            reports,
            folder,
            name,
        }
    }
}

/// Every snapshot folder, newest first.
pub fn list(dir: &Path) -> io::Result<Vec<SnapshotInfo>> {
    let entries = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(e),
    };
    let mut out: Vec<SnapshotInfo> = entries
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.is_dir() && (p.join(MANIFEST).is_file() || p.join(RAW).is_dir()))
        .map(SnapshotInfo::read)
        .collect();
    out.sort_by(|a, b| {
        stamp_of(&b.name)
            .cmp(&stamp_of(&a.name))
            .then(b.name.cmp(&a.name))
    });
    Ok(out)
}

/// The `YYYYMMDD-HHMMSS` tail of a folder name, what the listing sorts by.
fn stamp_of(name: &str) -> String {
    let mut parts = name.rsplitn(3, '-');
    let (time, date) = (parts.next().unwrap_or(""), parts.next().unwrap_or(""));
    format!("{date}-{time}")
}

/// `latest`, `previous`, a folder name or a build id.
pub fn find(dir: &Path, name: &str) -> Result<SnapshotInfo, SnapshotError> {
    let all = list(dir)?;
    let complete = || all.iter().filter(|s| s.complete);
    let found = match name {
        "latest" => complete().next(),
        "previous" => complete().nth(1),
        _ => all
            .iter()
            .find(|s| s.name == name)
            .or_else(|| complete().find(|s| s.buildid.as_deref() == Some(name))),
    };
    found
        .cloned()
        .ok_or_else(|| SnapshotError::NoSuch(name.to_string()))
}

/// Removes a snapshot folder. Refuses anything that does not look like one.
pub fn delete(folder: &Path) -> Result<(), SnapshotError> {
    if !(folder.join(MANIFEST).is_file() || folder.join(RAW).is_dir()) {
        return Err(SnapshotError::NotASnapshot(folder.to_path_buf()));
    }
    std::fs::remove_dir_all(folder)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stamp(secs: i64) -> DateTime<Utc> {
        DateTime::from_timestamp(1_800_000_000 + secs, 0).unwrap()
    }

    fn manifest(buildid: Option<&str>, secs: i64) -> Manifest {
        Manifest {
            buildid: buildid.map(str::to_string),
            taken: stamp(secs),
            deadtune: "0.0.0".into(),
            game_root: PathBuf::from("/g"),
            decode: true,
            size_cap: None,
            categories: BTreeSet::from([Category::Hud]),
            files: vec![FileEntry {
                path: "panorama/styles/hud.vcss_c".into(),
                source: Source::Pak01,
                size: 10,
                crc: "0000000a".into(),
                sha256: "x".into(),
                categories: vec![Category::Hud],
                stored: Stored::Full,
                text: Some("panorama/styles/hud.css".into()),
                decoded: Decoded::Text,
            }],
        }
    }

    #[test]
    fn folder_names_sort_by_time_and_carry_the_build() {
        assert_eq!(
            folder_name(Some("20419345"), stamp(0)),
            "20419345-20270115-080000"
        );
        assert_eq!(folder_name(None, stamp(0)), "unknown-20270115-080000");
    }

    #[test]
    fn manifest_round_trips_and_lists_newest_first() {
        let tmp = tempfile::tempdir().unwrap();
        let root = dir(tmp.path());
        assert!(list(&root).unwrap().is_empty(), "missing dir is empty");
        for (build, secs) in [(Some("100"), 0), (Some("101"), 60), (None, 120)] {
            let m = manifest(build, secs);
            let folder = root.join(folder_name(build, m.taken));
            m.save(&folder).unwrap();
            assert_eq!(Manifest::load(&folder).unwrap(), m);
        }
        std::fs::create_dir_all(root.join("101-20270115-090000/raw")).unwrap();
        std::fs::write(root.join("101-20270115-080100/diff-100-to-101.md"), "x").unwrap();
        std::fs::create_dir_all(root.join("not-a-snapshot")).unwrap();
        let all = list(&root).unwrap();
        let names: Vec<&str> = all.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(
            names,
            [
                "101-20270115-090000",
                "unknown-20270115-080200",
                "101-20270115-080100",
                "100-20270115-080000",
            ]
        );
        assert!(!all[0].complete);
        assert_eq!(
            all[0].buildid.as_deref(),
            Some("101"),
            "from the folder name"
        );
        assert_eq!(all[1].buildid, None);
        assert_eq!(all[2].reports, ["diff-100-to-101.md"]);
        assert_eq!((all[2].files, all[2].bytes), (1, 10));

        assert_eq!(
            find(&root, "latest").unwrap().name,
            "unknown-20270115-080200"
        );
        assert_eq!(find(&root, "previous").unwrap().name, "101-20270115-080100");
        assert_eq!(find(&root, "100").unwrap().name, "100-20270115-080000");
        assert_eq!(
            find(&root, "101-20270115-090000").unwrap().name,
            "101-20270115-090000"
        );
        assert!(matches!(find(&root, "7"), Err(SnapshotError::NoSuch(_))));

        assert!(matches!(
            delete(&root.join("not-a-snapshot")),
            Err(SnapshotError::NotASnapshot(_))
        ));
        delete(&all[3].folder).unwrap();
        assert_eq!(list(&root).unwrap().len(), 3);
        assert!(matches!(
            Manifest::load(&all[3].folder),
            Err(SnapshotError::NotASnapshot(_))
        ));
    }
}
