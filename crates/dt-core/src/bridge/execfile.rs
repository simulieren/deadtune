//! Writes `cfg/deadtune_live.cfg`; the player presses a bound key that runs `exec deadtune_live`.

use std::fs;
use std::io::Write;
use std::path::PathBuf;

use super::{Bridge, BridgeError, ConsoleCmd};

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

    fn contents(cmds: &[ConsoleCmd]) -> Result<String, BridgeError> {
        let mut out = String::new();
        for line in super::lines(cmds)? {
            out.push_str(&line);
            out.push_str(EOL);
        }
        out.push_str(&format!("echo \"DeadTune: applied {}\"{EOL}", cmds.len()));
        Ok(out)
    }
}

impl Bridge for ExecFileBridge {
    fn name(&self) -> &'static str {
        "exec file"
    }

    /// Temp file + rename in the same directory, so the game never `exec`s a half-written file.
    fn push(&mut self, cmds: &[ConsoleCmd]) -> Result<(), BridgeError> {
        let contents = Self::contents(cmds)?;
        let tmp = self.cfg_dir.join(format!(".{FILE_NAME}.tmp"));
        let mut file = fs::File::create(&tmp)?;
        file.write_all(contents.as_bytes())?;
        file.sync_all()?;
        drop(file);
        fs::rename(&tmp, self.cfg_dir.join(FILE_NAME))?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bridge::cmd;

    #[test]
    fn bind_hint_execs_the_live_file() {
        assert_eq!(
            ExecFileBridge::bind_hint("F8"),
            r#"bind F8 "exec deadtune_live""#
        );
    }

    #[test]
    fn push_writes_one_command_per_line_and_a_confirmation_echo() {
        let dir = tempfile::tempdir().unwrap();
        let mut bridge = ExecFileBridge {
            cfg_dir: dir.path().to_path_buf(),
        };
        bridge
            .push(&[cmd("fps_max", "240"), cmd("r_name", "a b")])
            .unwrap();
        let written = fs::read_to_string(dir.path().join(FILE_NAME)).unwrap();
        let expected = [
            r#"fps_max "240""#,
            r#"r_name "a b""#,
            r#"echo "DeadTune: applied 2""#,
        ]
        .map(|l| format!("{l}{EOL}"))
        .concat();
        assert_eq!(written, expected);
    }

    #[cfg(windows)]
    #[test]
    fn push_uses_crlf_on_windows() {
        let dir = tempfile::tempdir().unwrap();
        let mut bridge = ExecFileBridge {
            cfg_dir: dir.path().to_path_buf(),
        };
        bridge.push(&[cmd("fps_max", "240")]).unwrap();
        let written = fs::read_to_string(dir.path().join(FILE_NAME)).unwrap();
        assert_eq!(
            written,
            "fps_max \"240\"\r\necho \"DeadTune: applied 1\"\r\n"
        );
    }

    #[test]
    fn push_replaces_previous_file_and_leaves_no_temp_file() {
        let dir = tempfile::tempdir().unwrap();
        let mut bridge = ExecFileBridge {
            cfg_dir: dir.path().to_path_buf(),
        };
        bridge.push(&[cmd("a", "1"), cmd("b", "2")]).unwrap();
        bridge.push(&[cmd("c", "3")]).unwrap();
        let written = fs::read_to_string(dir.path().join(FILE_NAME)).unwrap();
        assert!(!written.contains("a \"1\""), "old batch is gone: {written}");
        assert!(written.contains("applied 1"));
        let names: Vec<_> = fs::read_dir(dir.path())
            .unwrap()
            .map(|e| e.unwrap().file_name())
            .collect();
        assert_eq!(names, [FILE_NAME], "only the live cfg remains");
    }

    #[test]
    fn unsafe_batch_leaves_existing_file_untouched() {
        let dir = tempfile::tempdir().unwrap();
        let mut bridge = ExecFileBridge {
            cfg_dir: dir.path().to_path_buf(),
        };
        bridge.push(&[cmd("a", "1")]).unwrap();
        let before = fs::read_to_string(dir.path().join(FILE_NAME)).unwrap();
        assert!(bridge.push(&[cmd("b", "1\nquit")]).is_err());
        let after = fs::read_to_string(dir.path().join(FILE_NAME)).unwrap();
        assert_eq!(before, after);
    }
}
