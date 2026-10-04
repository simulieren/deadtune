//! Tails the game's console log for the ack loop.
//!
//! Where the log lands is unverified on Windows: Source 1 wrote `con_logfile` relative to the mod
//! dir (`game/citadel`), CS2 has no `con_logfile` convar at all and writes `console.log` to the
//! mod dir under `-condebug`. So every plausible location is tailed and whichever one the
//! markers show up in wins.

use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

use super::ack::LOG_NAME;
use crate::locate::GamePaths;

const CONDEBUG_NAME: &str = "console.log";
/// How far back an existing log is read on start, enough to catch this session's boot marker.
const BACKLOG: u64 = 64 * 1024;

/// `deadtune_console.log` and `console.log` under the mod dir, the `game` dir and the install root.
pub fn candidates(paths: &GamePaths) -> Vec<PathBuf> {
    let mut roots: Vec<&Path> = vec![&paths.citadel_dir];
    if let Some(game) = paths.citadel_dir.parent() {
        roots.push(game);
    }
    roots.push(&paths.game_root);
    roots
        .iter()
        .flat_map(|root| [root.join(LOG_NAME), root.join(CONDEBUG_NAME)])
        .collect()
}

struct TailFile {
    path: PathBuf,
    offset: Option<u64>,
    partial: Vec<u8>,
}

impl TailFile {
    /// New complete lines since the last poll. A shrunken file (truncated by `-conclearlog` or
    /// rotated) is read again from the start.
    fn poll(&mut self) -> Vec<String> {
        // std opens with FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE on Windows,
        // so the game's own handle on the log does not block this read.
        let Ok(mut file) = File::open(&self.path) else {
            return Vec::new();
        };
        let Ok(len) = file.metadata().map(|m| m.len()) else {
            return Vec::new();
        };
        let offset = match self.offset {
            Some(offset) if offset <= len => offset,
            Some(_) => 0,
            None => len.saturating_sub(BACKLOG),
        };
        let mut buf = Vec::new();
        if file.seek(SeekFrom::Start(offset)).is_err() || file.read_to_end(&mut buf).is_err() {
            return Vec::new();
        }
        self.offset = Some(offset + buf.len() as u64);
        self.partial.extend_from_slice(&buf);
        let mut lines = Vec::new();
        while let Some(nl) = self.partial.iter().position(|b| *b == b'\n') {
            let line: Vec<u8> = self.partial.drain(..=nl).collect();
            let text = String::from_utf8_lossy(&line[..nl]);
            lines.push(text.trim_end_matches('\r').to_string());
        }
        lines
    }
}

pub struct LogTail {
    files: Vec<TailFile>,
}

impl LogTail {
    pub fn new(paths: impl IntoIterator<Item = PathBuf>) -> LogTail {
        LogTail {
            files: paths
                .into_iter()
                .map(|path| TailFile {
                    path,
                    offset: None,
                    partial: Vec::new(),
                })
                .collect(),
        }
    }

    pub fn for_game(paths: &GamePaths) -> LogTail {
        LogTail::new(candidates(paths))
    }

    /// New lines from every candidate file, in candidate order.
    pub fn poll(&mut self) -> Vec<String> {
        self.files.iter_mut().flat_map(TailFile::poll).collect()
    }

    pub fn paths(&self) -> impl Iterator<Item = &Path> {
        self.files.iter().map(|f| f.path.as_path())
    }
}

#[cfg(test)]
mod tests {
    use std::fs::OpenOptions;
    use std::io::Write;

    use super::*;

    fn append(path: &Path, text: &str) {
        let mut f = OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .unwrap();
        f.write_all(text.as_bytes()).unwrap();
    }

    #[test]
    fn reads_new_lines_only_and_keeps_partial_lines() {
        let dir = tempfile::tempdir().unwrap();
        let log = dir.path().join("console.log");
        append(&log, "old line\r\n");
        let mut tail = LogTail::new([log.clone()]);
        assert_eq!(tail.poll(), ["old line"], "backlog is read once");
        assert!(tail.poll().is_empty());
        append(&log, "DEADTUNE_ACK 1 1\r\nfps_max = 2");
        assert_eq!(tail.poll(), ["DEADTUNE_ACK 1 1"]);
        append(&log, "40\nDEADTUNE_END 1\n");
        assert_eq!(tail.poll(), ["fps_max = 240", "DEADTUNE_END 1"]);
    }

    #[test]
    fn truncated_file_is_read_from_the_start_again() {
        let dir = tempfile::tempdir().unwrap();
        let log = dir.path().join("console.log");
        append(&log, "first session line\n");
        let mut tail = LogTail::new([log.clone()]);
        tail.poll();
        std::fs::write(&log, "x\n").unwrap();
        assert_eq!(tail.poll(), ["x"]);
    }

    #[test]
    fn missing_files_are_skipped_until_they_appear() {
        let dir = tempfile::tempdir().unwrap();
        let a = dir.path().join("a.log");
        let b = dir.path().join("b.log");
        let mut tail = LogTail::new([a.clone(), b.clone()]);
        assert!(tail.poll().is_empty());
        append(&b, "hello\n");
        assert_eq!(tail.poll(), ["hello"]);
        append(&a, "late\n");
        append(&b, "more\n");
        assert_eq!(tail.poll(), ["late", "more"]);
    }

    #[test]
    fn candidates_cover_mod_game_and_root_dirs() {
        let root = PathBuf::from("/g/Deadlock");
        let citadel = root.join("game/citadel");
        let paths = GamePaths {
            game_root: root.clone(),
            citadel_dir: citadel.clone(),
            gameinfo: citadel.join("gameinfo.gi"),
            video: citadel.join("cfg/video.txt"),
            cfg_dir: citadel.join("cfg"),
            appmanifest: None,
            steam_root: None,
        };
        assert_eq!(
            candidates(&paths),
            [
                citadel.join("deadtune_console.log"),
                citadel.join("console.log"),
                root.join("game/deadtune_console.log"),
                root.join("game/console.log"),
                root.join("deadtune_console.log"),
                root.join("console.log"),
            ]
        );
    }
}
