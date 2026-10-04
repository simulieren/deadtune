//! Detects game updates and overwrites of our files.

use std::sync::mpsc::Sender;

use crate::locate::GamePaths;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Fingerprint {
    pub buildid: Option<String>,
    pub gameinfo_sha: Option<String>,
    pub video_sha: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Change {
    GameUpdated { from: Option<String>, to: Option<String> },
    GameInfoChanged,
    VideoChanged,
}

pub fn fingerprint(paths: &GamePaths) -> Fingerprint {
    let _ = paths;
    todo!()
}

pub fn diff(prev: &Fingerprint, now: &Fingerprint) -> Vec<Change> {
    let _ = (prev, now);
    todo!()
}

/// Keeps a `notify` watcher alive; dropping it stops watching.
pub struct Watcher {
    _inner: notify::RecommendedWatcher,
}

/// Sends the changes against `baseline` whenever a watched file is touched.
pub fn spawn(paths: GamePaths, baseline: Fingerprint, tx: Sender<Vec<Change>>) -> notify::Result<Watcher> {
    let _ = (paths, baseline, tx);
    todo!()
}
