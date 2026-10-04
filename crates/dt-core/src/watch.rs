//! Detects game updates and overwrites of our files.

use std::ffi::OsString;
use std::path::Path;
use std::sync::mpsc::Sender;

use notify::{Event, EventKind, RecursiveMode, Watcher as _};

use crate::backup::sha256_hex;
use crate::locate::{GamePaths, parse_buildid};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Fingerprint {
    pub buildid: Option<String>,
    pub gameinfo_sha: Option<String>,
    pub video_sha: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Change {
    GameUpdated {
        from: Option<String>,
        to: Option<String>,
    },
    GameInfoChanged,
    VideoChanged,
}

pub fn fingerprint(paths: &GamePaths) -> Fingerprint {
    let sha = |path: &Path| std::fs::read(path).ok().map(|bytes| sha256_hex(&bytes));
    Fingerprint {
        buildid: paths
            .appmanifest
            .as_ref()
            .and_then(|p| std::fs::read_to_string(p).ok())
            .and_then(|acf| parse_buildid(&acf)),
        gameinfo_sha: sha(&paths.gameinfo),
        video_sha: sha(&paths.video),
    }
}

pub fn diff(prev: &Fingerprint, now: &Fingerprint) -> Vec<Change> {
    let mut changes = Vec::new();
    if prev.buildid != now.buildid {
        changes.push(Change::GameUpdated {
            from: prev.buildid.clone(),
            to: now.buildid.clone(),
        });
    }
    if prev.gameinfo_sha != now.gameinfo_sha {
        changes.push(Change::GameInfoChanged);
    }
    if prev.video_sha != now.video_sha {
        changes.push(Change::VideoChanged);
    }
    changes
}

/// Keeps a `notify` watcher alive; dropping it stops watching.
pub struct Watcher {
    _inner: notify::RecommendedWatcher,
}

/// Sends the changes against `baseline` whenever a watched file is touched.
///
/// Each send is a diff against the last state sent, so one overwrite is reported once even when
/// the OS delivers several events for it.
pub fn spawn(
    paths: GamePaths,
    baseline: Fingerprint,
    tx: Sender<Vec<Change>>,
) -> notify::Result<Watcher> {
    let watched_names: Vec<OsString> = [
        Some(&paths.gameinfo),
        Some(&paths.video),
        paths.appmanifest.as_ref(),
    ]
    .into_iter()
    .flatten()
    .filter_map(|p| p.file_name().map(OsString::from))
    .collect();
    let mut dirs = vec![paths.citadel_dir.clone()];
    if paths.cfg_dir.is_dir() {
        dirs.push(paths.cfg_dir.clone());
    }
    if let Some(manifest_dir) = paths.appmanifest.as_ref().and_then(|p| p.parent()) {
        dirs.push(manifest_dir.to_path_buf());
    }

    let mut last = baseline;
    let mut inner = notify::recommended_watcher(move |event: notify::Result<Event>| {
        let Ok(event) = event else { return };
        // Our own fingerprint reads raise access events; reacting to them would loop forever.
        if matches!(event.kind, EventKind::Access(_)) {
            return;
        }
        let ours = event.paths.iter().any(|p| {
            p.file_name()
                .is_some_and(|name| watched_names.iter().any(|w| w == name))
        });
        if !ours {
            return;
        }
        let now = fingerprint(&paths);
        let changes = diff(&last, &now);
        if !changes.is_empty() && tx.send(changes).is_ok() {
            last = now;
        }
    })?;
    for dir in &dirs {
        inner.watch(dir, RecursiveMode::NonRecursive)?;
    }
    Ok(Watcher { _inner: inner })
}

#[cfg(test)]
mod tests {
    use std::sync::mpsc;
    use std::time::{Duration, Instant};

    use super::*;
    use crate::locate::from_game_root;

    const CLEAN_GAMEINFO: &str = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../research/configs/OptimizationLock/clean gameinfo.gi/gameinfo.gi"
    ));

    fn fake_install(buildid: &str) -> (tempfile::TempDir, GamePaths) {
        let steam = tempfile::tempdir().unwrap();
        let steamapps = steam.path().join("steamapps");
        let game_root = steamapps.join("common").join("Deadlock");
        let citadel = game_root.join("game").join("citadel");
        std::fs::create_dir_all(citadel.join("cfg")).unwrap();
        std::fs::write(citadel.join("gameinfo.gi"), CLEAN_GAMEINFO).unwrap();
        std::fs::write(citadel.join("cfg").join("video.txt"), "\"config\"\n{\n}\n").unwrap();
        write_manifest(&steamapps, buildid);
        let paths = from_game_root(&game_root).unwrap();
        (steam, paths)
    }

    fn write_manifest(steamapps: &Path, buildid: &str) {
        let acf = format!(
            "\"AppState\"\n{{\n\t\"appid\"\t\t\"1422450\"\n\t\"buildid\"\t\t\"{buildid}\"\n}}\n"
        );
        std::fs::write(steamapps.join("appmanifest_1422450.acf"), acf).unwrap();
    }

    fn fp(buildid: Option<&str>, gameinfo: Option<&str>, video: Option<&str>) -> Fingerprint {
        Fingerprint {
            buildid: buildid.map(String::from),
            gameinfo_sha: gameinfo.map(String::from),
            video_sha: video.map(String::from),
        }
    }

    #[test]
    fn fingerprint_hashes_files_and_reads_buildid() {
        let (_steam, paths) = fake_install("100");
        assert_eq!(
            fingerprint(&paths),
            fp(
                Some("100"),
                Some(&sha256_hex(CLEAN_GAMEINFO.as_bytes())),
                Some(&sha256_hex(b"\"config\"\n{\n}\n"))
            )
        );
    }

    #[test]
    fn fingerprint_of_missing_files_is_none() {
        let (_steam, paths) = fake_install("100");
        std::fs::remove_file(&paths.video).unwrap();
        let no_manifest = GamePaths {
            appmanifest: None,
            ..paths
        };
        let print = fingerprint(&no_manifest);
        assert_eq!(print.video_sha, None);
        assert_eq!(print.buildid, None);
        assert!(print.gameinfo_sha.is_some());
    }

    #[test]
    fn diff_of_identical_fingerprints_is_empty() {
        let a = fp(Some("1"), Some("g"), Some("v"));
        assert!(diff(&a, &a.clone()).is_empty());
    }

    #[test]
    fn diff_reports_each_change_in_order() {
        let prev = fp(Some("1"), Some("g"), Some("v"));
        assert_eq!(
            diff(&prev, &fp(Some("1"), Some("g2"), Some("v"))),
            vec![Change::GameInfoChanged]
        );
        assert_eq!(
            diff(&prev, &fp(Some("1"), Some("g"), None)),
            vec![Change::VideoChanged]
        );
        assert_eq!(
            diff(&prev, &fp(Some("2"), Some("g2"), Some("v2"))),
            vec![
                Change::GameUpdated {
                    from: Some("1".into()),
                    to: Some("2".into())
                },
                Change::GameInfoChanged,
                Change::VideoChanged,
            ]
        );
    }

    fn recv_until(rx: &mpsc::Receiver<Vec<Change>>, want: &Change) -> Vec<Vec<Change>> {
        let deadline = Instant::now() + Duration::from_secs(10);
        let mut seen = Vec::new();
        while let Some(left) = deadline.checked_duration_since(Instant::now()) {
            match rx.recv_timeout(left) {
                Ok(changes) => {
                    let hit = changes.contains(want);
                    seen.push(changes);
                    if hit {
                        return seen;
                    }
                }
                Err(_) => break,
            }
        }
        panic!("never received {want:?}; got {seen:?}");
    }

    #[test]
    fn spawn_reports_a_gameinfo_overwrite() {
        let (_steam, paths) = fake_install("100");
        let (tx, rx) = mpsc::channel();
        let _watcher = spawn(paths.clone(), fingerprint(&paths), tx).unwrap();
        std::fs::write(
            &paths.gameinfo,
            "\"GameInfo\"\n{\n\t// steam overwrote this\n}\n",
        )
        .unwrap();
        let batches = recv_until(&rx, &Change::GameInfoChanged);
        assert!(
            batches.iter().all(|b| !b.is_empty()),
            "empty diffs must not be sent"
        );
    }

    #[test]
    fn spawn_reports_a_buildid_bump_from_the_appmanifest() {
        let (steam, paths) = fake_install("100");
        let (tx, rx) = mpsc::channel();
        let _watcher = spawn(paths.clone(), fingerprint(&paths), tx).unwrap();
        write_manifest(&steam.path().join("steamapps"), "101");
        recv_until(
            &rx,
            &Change::GameUpdated {
                from: Some("100".into()),
                to: Some("101".into()),
            },
        );
    }
}
