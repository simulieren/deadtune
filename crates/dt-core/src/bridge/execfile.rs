//! Writes `cfg/deadtune_live.cfg`; the player presses a bound key that runs `exec deadtune_live`.

use std::path::PathBuf;

use super::{Bridge, BridgeError, ConsoleCmd};

pub const FILE_NAME: &str = "deadtune_live.cfg";

pub struct ExecFileBridge {
    pub cfg_dir: PathBuf,
}

impl ExecFileBridge {
    /// Console line the user binds once, e.g. `bind F8 "exec deadtune_live"`.
    pub fn bind_hint(key: &str) -> String {
        let _ = key;
        todo!()
    }
}

impl Bridge for ExecFileBridge {
    fn name(&self) -> &'static str {
        "exec file"
    }

    fn push(&mut self, cmds: &[ConsoleCmd]) -> Result<(), BridgeError> {
        let _ = cmds;
        todo!()
    }
}
