//! Turns "desired state" into console commands + file writes, split by apply class.

use std::collections::BTreeMap;
use std::path::PathBuf;

use crate::bridge::ConsoleCmd;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FileWrite {
    pub path: PathBuf,
    pub before: String,
    pub after: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct ApplyContext {
    /// Player says they are in hideout/sandbox, where cheat convars are settable.
    pub in_sandbox: bool,
    pub game_running: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct ApplyPlan {
    pub live: Vec<ConsoleCmd>,
    pub queued_cheat: Vec<String>,
    pub restart: Vec<String>,
    pub denied: Vec<String>,
    pub gameinfo: Option<FileWrite>,
    pub video: Option<FileWrite>,
    pub video_changes: BTreeMap<String, String>,
}
