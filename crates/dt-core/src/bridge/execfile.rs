//! Writes `cfg/deadtune_live.cfg`; the player presses a bound key that runs `exec deadtune_live`.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use super::{Bridge, BridgeError};

pub const FILE_NAME: &str = "deadtune_live.cfg";

#[cfg(windows)]
const EOL: &str = "\r\n";
#[cfg(not(windows))]
const EOL: &str = "\n";

pub struct ExecFileBridge {
    pub cfg_dir: PathBuf,
}

impl ExecFileBridge {
    /// Console line the user binds once, e.g. `bind F8 "exec deadtune_live"`.
    pub fn bind_hint(key: &str) -> String {
        format!("bind {key} \"exec deadtune_live\"")
    }
}

/// Temp file + rename in the same directory, so the game never `exec`s a half-written file.
pub(crate) fn write_cfg(cfg_dir: &Path, name: &str, lines: &[String]) -> std::io::Result<()> {
    let contents: String = lines.iter().map(|l| l.clone() + EOL).collect();
    let tmp = cfg_dir.join(format!(".{name}.tmp"));
    let mut file = fs::File::create(&tmp)?;
    file.write_all(contents.as_bytes())?;
    file.sync_all()?;
    drop(file);
    fs::rename(&tmp, cfg_dir.join(name))
}

impl Bridge for ExecFileBridge {
    fn name(&self) -> &'static str {
        "exec file"
    }

    fn send(&mut self, lines: &[String]) -> Result<(), BridgeError> {
        Ok(write_cfg(&self.cfg_dir, FILE_NAME, lines)?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bridge::cmd;

    fn bridge(dir: &Path) -> ExecFileBridge {
        ExecFileBridge {
            cfg_dir: dir.to_path_buf(),
        }
    }

    fn written(dir: &Path) -> String {
        fs::read_to_string(dir.join(FILE_NAME)).unwrap()
    }

    #[test]
    fn bind_hint_execs_the_live_file() {
        assert_eq!(
            ExecFileBridge::bind_hint("F8"),
            r#"bind F8 "exec deadtune_live""#
        );
    }

    #[test]
    fn push_writes_one_command_per_line_with_the_ack_trailer() {
        let dir = tempfile::tempdir().unwrap();
        let receipt = bridge(dir.path())
            .push(&[cmd("fps_max", "240"), cmd("r_name", "a b")])
            .unwrap();
        let expected = [
            "con_logfile deadtune_console.log".to_string(),
            r#"fps_max "240""#.into(),
            r#"r_name "a b""#.into(),
            format!("echo DEADTUNE_ACK {} 2", receipt.nonce),
            "fps_max".into(),
            "r_name".into(),
            format!("echo DEADTUNE_END {}", receipt.nonce),
        ]
        .map(|l| format!("{l}{EOL}"))
        .concat();
        assert_eq!(written(dir.path()), expected);
        assert_eq!(receipt.queries, ["fps_max", "r_name"]);
    }

    #[cfg(windows)]
    #[test]
    fn push_uses_crlf_on_windows() {
        let dir = tempfile::tempdir().unwrap();
        bridge(dir.path()).push(&[cmd("fps_max", "240")]).unwrap();
        let text = written(dir.path());
        assert!(
            text.contains("fps_max \"240\"\r\necho DEADTUNE_ACK"),
            "{text}"
        );
        assert!(!text.replace("\r\n", "").contains('\n'));
    }

    #[test]
    fn push_replaces_previous_file_and_leaves_no_temp_file() {
        let dir = tempfile::tempdir().unwrap();
        let mut bridge = bridge(dir.path());
        bridge.push(&[cmd("a", "1"), cmd("b", "2")]).unwrap();
        let second = bridge.push(&[cmd("c", "3")]).unwrap();
        let text = written(dir.path());
        assert!(!text.contains("a \"1\""), "old batch is gone: {text}");
        assert!(text.contains(&format!("DEADTUNE_ACK {} 1", second.nonce)));
        let names: Vec<_> = fs::read_dir(dir.path())
            .unwrap()
            .map(|e| e.unwrap().file_name())
            .collect();
        assert_eq!(names, [FILE_NAME], "only the live cfg remains");
    }

    #[test]
    fn unsafe_batch_leaves_existing_file_untouched() {
        let dir = tempfile::tempdir().unwrap();
        let mut bridge = bridge(dir.path());
        bridge.push(&[cmd("a", "1")]).unwrap();
        let before = written(dir.path());
        assert!(bridge.push(&[cmd("b", "1\nquit")]).is_err());
        assert_eq!(before, written(dir.path()));
    }

    #[test]
    fn probe_writes_a_query_only_batch() {
        let dir = tempfile::tempdir().unwrap();
        let receipt = bridge(dir.path()).probe("fps_max").unwrap();
        let text = written(dir.path());
        assert!(!text.contains("fps_max \""), "no value is set: {text}");
        assert!(text.contains(&format!("DEADTUNE_ACK {} 1", receipt.nonce)));
        assert!(receipt.sent.is_empty());
    }
}
