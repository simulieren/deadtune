//! `cfg/deadtune_boot.cfg`, run at game start through `+exec deadtune_boot`: binds the live key,
//! sets the profile's live convars and echoes a marker so the console log proves it ran.

use std::path::Path;

use super::ack::{BOOT, LOG_NAME};
use super::execfile::{ExecFileBridge, write_cfg};
use super::{BridgeError, ConsoleCmd};

pub const FILE_NAME: &str = "deadtune_boot.cfg";

/// Launch arguments that run the boot cfg and turn the console log on (`-condebug` writes
/// `console.log` next to gameinfo.gi in Source 2 games).
pub const LAUNCH_ARGS: [&str; 3] = ["+exec", "deadtune_boot", "-condebug"];

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BootCfg {
    pub bind_key: String,
    pub live: Vec<ConsoleCmd>,
    pub version: String,
}

impl BootCfg {
    pub fn lines(&self) -> Result<Vec<String>, BridgeError> {
        let mut lines = vec![
            format!("con_logfile {LOG_NAME}"),
            ExecFileBridge::bind_hint(&self.bind_key),
        ];
        lines.extend(super::lines(&self.live)?);
        lines.push(format!("echo {BOOT} {}", self.version));
        Ok(lines)
    }

    pub fn write(&self, cfg_dir: &Path) -> Result<(), BridgeError> {
        Ok(write_cfg(cfg_dir, FILE_NAME, &self.lines()?)?)
    }
}

/// What a player who starts the game from Steam pastes into its Launch Options.
pub fn steam_launch_options(console: bool) -> String {
    let mut text = LAUNCH_ARGS.join(" ");
    if console {
        text.push_str(" -console");
    }
    text
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bridge::cmd;

    fn cfg() -> BootCfg {
        BootCfg {
            bind_key: "F8".into(),
            live: vec![cmd("fps_max", "240"), cmd("r_name", "a b")],
            version: "0.1.0".into(),
        }
    }

    #[test]
    fn lines_bind_set_and_mark() {
        assert_eq!(
            cfg().lines().unwrap(),
            [
                "con_logfile deadtune_console.log",
                r#"bind F8 "exec deadtune_live""#,
                r#"fps_max "240""#,
                r#"r_name "a b""#,
                "echo DEADTUNE_BOOT 0.1.0",
            ]
        );
    }

    #[test]
    fn write_is_idempotent_and_atomic() {
        let dir = tempfile::tempdir().unwrap();
        cfg().write(dir.path()).unwrap();
        cfg().write(dir.path()).unwrap();
        let names: Vec<_> = std::fs::read_dir(dir.path())
            .unwrap()
            .map(|e| e.unwrap().file_name())
            .collect();
        assert_eq!(names, [FILE_NAME]);
        let text = std::fs::read_to_string(dir.path().join(FILE_NAME)).unwrap();
        assert!(text.contains("DEADTUNE_BOOT 0.1.0"), "{text}");
    }

    #[test]
    fn unsafe_value_is_refused_before_anything_is_written() {
        let dir = tempfile::tempdir().unwrap();
        let bad = BootCfg {
            live: vec![cmd("a", "1; quit")],
            ..cfg()
        };
        assert!(bad.write(dir.path()).is_err());
        assert!(!dir.path().join(FILE_NAME).exists());
    }

    #[test]
    fn steam_text_lists_the_launch_args() {
        assert_eq!(steam_launch_options(false), "+exec deadtune_boot -condebug");
        assert_eq!(
            steam_launch_options(true),
            "+exec deadtune_boot -condebug -console"
        );
    }
}
