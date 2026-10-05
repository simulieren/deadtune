//! Presets whose licence lets us link to them but not ship them. The player downloads (or
//! imports) the author's file; DeadTune keeps it in its cache and checks it against a pinned
//! sha256, so an upstream change is never used without the player accepting it.
//!
//! Cache layout, per preset: `gameinfo.gi` is the file in use, `pending.gi` an upstream file
//! that differs from both the pinned one and the one in use and is waiting for review.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use crate::backup::{atomic_write, sha256_hex};
use crate::gi;
use crate::zip::{self, ZipError};

use super::Remote;

#[derive(Debug, thiserror::Error)]
pub enum RemoteError {
    #[error("{label} isn't downloaded yet. Download it or import cfg.zip or gameinfo.gi.")]
    Missing { label: &'static str },
    #[error("that file is not a gameinfo.gi with game settings in it")]
    NotGameinfo,
    #[error("zip: {0}")]
    Zip(#[from] ZipError),
    #[error("there is no upstream update to accept")]
    NothingPending,
    #[error("cache: {0}")]
    Io(#[from] io::Error),
    #[cfg(feature = "fetch")]
    #[error("download: {0}")]
    Http(#[from] ureq::Error),
    #[cfg(feature = "fetch")]
    #[error("GameBanana listed no downloadable file")]
    NoUpstreamFile,
}

/// The file in use.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Active {
    /// Byte-identical to the file DeadTune pinned.
    Pinned,
    /// An upstream update the player accepted.
    Accepted,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Status {
    pub active: Option<Active>,
    /// sha256 of an upstream file waiting for review.
    pub pending: Option<String>,
}

impl Status {
    pub fn ready(&self) -> bool {
        self.active.is_some()
    }
}

/// One setting an upstream update changes, against the file in use (`None`: not set).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Change {
    pub name: String,
    pub before: Option<String>,
    pub after: Option<String>,
}

pub fn dir(remote: &Remote, presets_dir: &Path) -> PathBuf {
    presets_dir.join(remote.id.key())
}

fn active_path(dir: &Path) -> PathBuf {
    dir.join("gameinfo.gi")
}

fn pending_path(dir: &Path) -> PathBuf {
    dir.join("pending.gi")
}

fn read_opt(path: &Path) -> io::Result<Option<Vec<u8>>> {
    match fs::read(path) {
        Ok(bytes) => Ok(Some(bytes)),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e),
    }
}

fn remove_opt(path: &Path) -> io::Result<()> {
    match fs::remove_file(path) {
        Err(e) if e.kind() != io::ErrorKind::NotFound => Err(e),
        _ => Ok(()),
    }
}

pub fn status(remote: &Remote, dir: &Path) -> io::Result<Status> {
    let active = read_opt(&active_path(dir))?.map(|bytes| {
        if sha256_hex(&bytes) == remote.sha256 {
            Active::Pinned
        } else {
            Active::Accepted
        }
    });
    let pending = read_opt(&pending_path(dir))?.map(|bytes| sha256_hex(&bytes));
    Ok(Status { active, pending })
}

/// The gameinfo.gi in use.
pub fn read(remote: &Remote, dir: &Path) -> Result<String, RemoteError> {
    match fs::read_to_string(active_path(dir)) {
        Ok(text) => Ok(text),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Err(RemoteError::Missing {
            label: super::info(remote.id).label,
        }),
        Err(e) => Err(e.into()),
    }
}

/// Takes a downloaded or imported cfg.zip or gameinfo.gi. The pinned file, or the one already
/// in use, goes straight into use; anything else waits in `pending.gi` for [`accept`].
pub fn stage(remote: &Remote, dir: &Path, bytes: &[u8]) -> Result<Status, RemoteError> {
    let gameinfo = if zip::is_zip(bytes) {
        zip::extract(bytes, "gameinfo.gi")?
    } else {
        bytes.to_vec()
    };
    let text = std::str::from_utf8(&gameinfo).map_err(|_| RemoteError::NotGameinfo)?;
    if gi::read_convars(text).map_or(true, |c| c.is_empty()) {
        return Err(RemoteError::NotGameinfo);
    }
    fs::create_dir_all(dir)?;
    let sha = sha256_hex(&gameinfo);
    let in_use = read_opt(&active_path(dir))?.map(|b| sha256_hex(&b));
    if sha == remote.sha256 || in_use.as_deref() == Some(sha.as_str()) {
        atomic_write(&active_path(dir), &gameinfo)?;
        remove_opt(&pending_path(dir))?;
    } else {
        atomic_write(&pending_path(dir), &gameinfo)?;
    }
    Ok(status(remote, dir)?)
}

/// What the pending file would change, against the file in use. Empty when nothing is pending.
pub fn pending_changes(dir: &Path) -> Result<Vec<Change>, RemoteError> {
    let Some(pending) = read_opt(&pending_path(dir))? else {
        return Ok(Vec::new());
    };
    let values = |bytes: &[u8]| {
        std::str::from_utf8(bytes)
            .ok()
            .and_then(|t| gi::effective_values(t).ok())
            .unwrap_or_default()
    };
    let after = values(&pending);
    let before = read_opt(&active_path(dir))?
        .map(|b| values(&b))
        .unwrap_or_default();
    let names: std::collections::BTreeSet<&String> = before.keys().chain(after.keys()).collect();
    Ok(names
        .into_iter()
        .filter(|n| before.get(*n) != after.get(*n))
        .map(|n| Change {
            name: n.clone(),
            before: before.get(n).cloned(),
            after: after.get(n).cloned(),
        })
        .collect())
}

/// Puts the pending upstream file into use.
pub fn accept(dir: &Path) -> Result<(), RemoteError> {
    let Some(pending) = read_opt(&pending_path(dir))? else {
        return Err(RemoteError::NothingPending);
    };
    atomic_write(&active_path(dir), &pending)?;
    remove_opt(&pending_path(dir))?;
    Ok(())
}

/// Keeps the file in use and drops the pending one.
pub fn discard(dir: &Path) -> io::Result<()> {
    remove_opt(&pending_path(dir))
}

/// Asks GameBanana for the mod's newest file, downloads it and [`stage`]s it.
#[cfg(feature = "fetch")]
pub fn fetch(remote: &Remote, dir: &Path) -> Result<Status, RemoteError> {
    let page: serde_json::Value =
        serde_json::from_str(&ureq::get(remote.api).call()?.body_mut().read_to_string()?)
            .map_err(|_| RemoteError::NoUpstreamFile)?;
    let url = newest_download(&page).ok_or(RemoteError::NoUpstreamFile)?;
    let bytes = ureq::get(&url).call()?.body_mut().read_to_vec()?;
    stage(remote, dir, &bytes)
}

/// `_sDownloadUrl` of the newest entry in a GameBanana ProfilePage's `_aFiles`.
#[cfg_attr(not(feature = "fetch"), allow(dead_code))]
fn newest_download(page: &serde_json::Value) -> Option<String> {
    page.get("_aFiles")?
        .as_array()?
        .iter()
        .filter(|f| f.get("_bIsArchived").and_then(|v| v.as_bool()) != Some(true))
        .max_by_key(|f| f.get("_tsDateAdded").and_then(|v| v.as_i64()))?
        .get("_sDownloadUrl")?
        .as_str()
        .map(str::to_string)
}

#[cfg(test)]
mod tests {
    use super::*;

    const GI: &str = include_str!("../../tests/fixtures/sidelock_standin/gameinfo.gi");
    const ZIP: &[u8] = include_bytes!("../../tests/fixtures/sidelock_standin/cfg.zip");

    fn standin() -> Remote {
        Remote {
            sha256: Box::leak(sha256_hex(GI.as_bytes()).into_boxed_str()),
            ..*super::super::remote(super::super::PresetId::SideLock)
        }
    }

    fn updated() -> String {
        GI.replace("\"r_ssao\" \"false\"", "\"r_ssao\" \"true\"")
            .replace("\"cl_ragdoll_limit\" \"0\"", "\"r_new_thing\" \"2\"")
    }

    #[test]
    fn nothing_cached_is_missing_with_a_human_message() {
        let tmp = tempfile::tempdir().unwrap();
        let remote = standin();
        assert_eq!(
            status(&remote, tmp.path()).unwrap(),
            Status {
                active: None,
                pending: None
            }
        );
        let err = read(&remote, tmp.path()).unwrap_err();
        assert_eq!(
            err.to_string(),
            "SideLock isn't downloaded yet. Download it or import cfg.zip or gameinfo.gi."
        );
    }

    #[test]
    fn the_pinned_file_goes_straight_into_use_from_a_zip_or_a_bare_file() {
        let remote = standin();
        for bytes in [ZIP, GI.as_bytes()] {
            let tmp = tempfile::tempdir().unwrap();
            let s = stage(&remote, tmp.path(), bytes).unwrap();
            assert_eq!(s.active, Some(Active::Pinned));
            assert_eq!(s.pending, None);
            assert_eq!(read(&remote, tmp.path()).unwrap(), GI, "byte for byte");
        }
    }

    #[test]
    fn a_different_upstream_file_waits_for_review_then_accept_puts_it_in_use() {
        let tmp = tempfile::tempdir().unwrap();
        let remote = standin();
        stage(&remote, tmp.path(), GI.as_bytes()).unwrap();
        let new = updated();
        let s = stage(&remote, tmp.path(), new.as_bytes()).unwrap();
        assert_eq!(s.active, Some(Active::Pinned), "old file still in use");
        assert_eq!(s.pending, Some(sha256_hex(new.as_bytes())));
        assert_eq!(read(&remote, tmp.path()).unwrap(), GI);

        let changes = pending_changes(tmp.path()).unwrap();
        let names: Vec<&str> = changes.iter().map(|c| c.name.as_str()).collect();
        assert_eq!(names, ["cl_ragdoll_limit", "r_new_thing", "r_ssao"]);
        assert_eq!(changes[2].before.as_deref(), Some("false"));
        assert_eq!(changes[2].after.as_deref(), Some("true"));

        accept(tmp.path()).unwrap();
        let s = status(&remote, tmp.path()).unwrap();
        assert_eq!(s.active, Some(Active::Accepted));
        assert_eq!(s.pending, None);
        assert_eq!(read(&remote, tmp.path()).unwrap(), new);
        assert!(pending_changes(tmp.path()).unwrap().is_empty());
        assert!(matches!(
            accept(tmp.path()),
            Err(RemoteError::NothingPending)
        ));

        let again = stage(&remote, tmp.path(), new.as_bytes()).unwrap();
        assert_eq!(again.pending, None, "the accepted file is not re-offered");
    }

    #[test]
    fn a_first_download_that_differs_is_not_used_until_accepted() {
        let tmp = tempfile::tempdir().unwrap();
        let remote = standin();
        let s = stage(&remote, tmp.path(), updated().as_bytes()).unwrap();
        assert!(!s.ready());
        assert!(s.pending.is_some());
        assert!(matches!(
            read(&remote, tmp.path()),
            Err(RemoteError::Missing { .. })
        ));
        assert_eq!(
            pending_changes(tmp.path()).unwrap().len(),
            7,
            "every setting is new"
        );
        discard(tmp.path()).unwrap();
        assert_eq!(status(&remote, tmp.path()).unwrap().pending, None);
    }

    #[test]
    fn staging_rejects_files_without_game_settings() {
        let tmp = tempfile::tempdir().unwrap();
        let remote = standin();
        for bytes in [&b"hello"[..], b"\xff\xfe", b"\"GameInfo\"\n{\n}\n"] {
            assert!(
                matches!(
                    stage(&remote, tmp.path(), bytes),
                    Err(RemoteError::NotGameinfo)
                ),
                "{bytes:?}"
            );
        }
        assert!(matches!(
            stage(&remote, tmp.path(), &ZIP[..ZIP.len() - 3]),
            Err(RemoteError::Zip(_))
        ));
        assert_eq!(status(&remote, tmp.path()).unwrap().active, None);
    }

    #[test]
    fn newest_download_picks_the_latest_unarchived_file() {
        let page = serde_json::json!({ "_aFiles": [
            { "_tsDateAdded": 10, "_sDownloadUrl": "https://gamebanana.com/dl/1" },
            { "_tsDateAdded": 30, "_sDownloadUrl": "https://gamebanana.com/dl/3", "_bIsArchived": true },
            { "_tsDateAdded": 20, "_sDownloadUrl": "https://gamebanana.com/dl/2" },
        ]});
        assert_eq!(
            newest_download(&page).as_deref(),
            Some("https://gamebanana.com/dl/2")
        );
        assert_eq!(newest_download(&serde_json::json!({})), None);
    }

    #[cfg(feature = "fetch")]
    #[test]
    #[ignore = "network"]
    fn fetch_downloads_the_pinned_sidelock_file() {
        let tmp = tempfile::tempdir().unwrap();
        let remote = super::super::remote(super::super::PresetId::SideLock);
        let s = fetch(remote, tmp.path()).unwrap();
        assert_eq!(
            s,
            Status {
                active: Some(Active::Pinned),
                pending: None
            },
            "upstream changed: review it and update the pinned sha256"
        );
        let text = read(remote, tmp.path()).unwrap();
        assert!(gi::effective_values(&text).unwrap().len() > 400);
    }
}
