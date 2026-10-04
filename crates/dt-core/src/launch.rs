//! Start/stop the game through Steam. Never touches the game process beyond kill.

#[derive(Clone, Debug, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub struct LaunchOptions {
    pub args: Vec<String>,
}

/// `steam://rungameid/1422450` or `steam://run/1422450//<args>/` when args are set.
pub fn steam_url(opts: &LaunchOptions) -> String {
    let _ = opts;
    todo!()
}

pub fn is_game_running() -> bool {
    todo!()
}

/// Process start time of the running game, used by the relaunch countdown.
pub fn game_started_at() -> Option<std::time::SystemTime> {
    todo!()
}

pub fn kill_game() -> std::io::Result<bool> {
    todo!()
}

pub fn launch(opts: &LaunchOptions) -> std::io::Result<()> {
    let _ = opts;
    todo!()
}
