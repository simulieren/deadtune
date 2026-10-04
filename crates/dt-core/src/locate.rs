//! Finds Steam and Deadlock (appid 1422450).

use std::path::{Path, PathBuf};

pub const APP_ID: u32 = 1422450;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GamePaths {
    /// `.../steamapps/common/Deadlock`
    pub game_root: PathBuf,
    pub citadel_dir: PathBuf,
    pub gameinfo: PathBuf,
    pub video: PathBuf,
    pub cfg_dir: PathBuf,
    pub appmanifest: Option<PathBuf>,
    pub steam_root: Option<PathBuf>,
}

#[derive(Debug, thiserror::Error)]
pub enum LocateError {
    #[error("steam not found: {0}")]
    Steam(String),
    #[error("Deadlock (appid 1422450) not installed in any Steam library")]
    NotInstalled,
    #[error("not a Deadlock install: {0}")]
    BadPath(PathBuf),
}

pub fn locate() -> Result<GamePaths, LocateError> {
    todo!()
}

/// For a user-picked `.../Deadlock` folder (manual override, Proton prefixes, etc.).
pub fn from_game_root(game_root: &Path) -> Result<GamePaths, LocateError> {
    let _ = game_root;
    todo!()
}

pub fn parse_buildid(acf: &str) -> Option<String> {
    let _ = acf;
    todo!()
}

/// `userdata/<id>/760/remote/1422450/screenshots` for every Steam user.
pub fn screenshot_dirs(steam_root: &Path) -> Vec<PathBuf> {
    let _ = steam_root;
    todo!()
}
