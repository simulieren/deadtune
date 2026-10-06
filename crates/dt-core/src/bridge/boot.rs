//! `cfg/deadtune_boot.cfg`, run at game start through `+exec deadtune_boot`: binds the live key,
//! sets the profile's live convars and echoes a marker so the console log proves it ran.

use std::path::Path;

use super::ack::{BOOT, LOG_NAME};
use super::execfile::{ExecFileBridge, write_cfg};
use super::{BridgeError, ConsoleCmd};
use crate::hud::live;

pub const FILE_NAME: &str = "deadtune_boot.cfg";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BootCfg {
    pub bind_key: String,
    pub live: Vec<ConsoleCmd>,
    pub version: String,
    /// The HUD pak carries the live script: the key also runs `exec deadtune_hud`, and the
    /// script's sliders start on probe values its hello line reports.
    pub live_hud: bool,
}

impl BootCfg {
    pub fn lines(&self) -> Result<Vec<String>, BridgeError> {
        let bind = if self.live_hud {
            format!(
                "bind {} \"exec deadtune_live; {}\"",
                self.bind_key,
                live::exec_line()
            )
        } else {
            ExecFileBridge::bind_hint(&self.bind_key)
        };
        let mut lines = vec![format!("con_logfile {LOG_NAME}"), bind];
        lines.extend(super::lines(&self.live)?);
        if self.live_hud {
            lines.extend(super::lines(&live::probe_cmds())?);
        }
        lines.push(format!("echo {BOOT} {}", self.version));
        Ok(lines)
    }

    pub fn write(&self, cfg_dir: &Path) -> Result<(), BridgeError> {
        Ok(write_cfg(cfg_dir, FILE_NAME, &self.lines()?)?)
    }
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
            live_hud: false,
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
    fn with_the_live_hud_the_key_also_reads_its_cfg_and_the_slots_get_probe_values() {
        use crate::hud::live;
        let lines = BootCfg {
            live_hud: true,
            ..cfg()
        }
        .lines()
        .unwrap();
        assert_eq!(
            lines[1],
            r#"bind F8 "exec deadtune_live; exec deadtune_hud""#
        );
        for probe in live::probe_cmds() {
            assert!(lines.contains(&probe.to_line().unwrap()), "{lines:?}");
        }
        assert_eq!(lines.last().unwrap(), "echo DEADTUNE_BOOT 0.1.0");
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
}
