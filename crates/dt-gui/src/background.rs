//! `deadtune --background`, started at sign-in when "Start with Windows" is on: no window,
//! it only rebuilds DeadTune's paks after a game update (`hud::install` and
//! `addons::install` `refresh_after_update`), once Steam is done and the game is closed.
//! It exits when the setting is turned off.

use std::path::Path;
use std::time::Duration;

use dt_core::addons::install as addons_install;
use dt_core::hud::install::{self as hud_install, Refreshed};
use dt_core::locate::{self, GamePaths};

use crate::settings::Settings;

const POLL: Duration = Duration::from_secs(30);

/// When to rebuild: once at start, then whenever the game build moves, retried while the
/// game runs or Steam is still updating.
#[derive(Debug, Default)]
pub struct Schedule {
    /// The build the last finished check saw; `None` before the first.
    checked: Option<Option<String>>,
}

impl Schedule {
    pub fn due(&self, build: &Option<String>, game_running: bool) -> bool {
        !game_running && self.checked.as_ref() != Some(build)
    }

    pub fn finished(&mut self, build: Option<String>) {
        self.checked = Some(build);
    }
}

pub fn run(data_dir: &Path, settings: &Settings) {
    let paths = match &settings.game_dir {
        Some(dir) => locate::from_game_root(dir),
        None => locate::locate(),
    };
    let Ok(paths) = paths else {
        return;
    };
    let records = dt_core::backup::records_dir(data_dir);
    let mut schedule = Schedule::default();
    while Settings::load(data_dir).start_with_windows {
        let build = locate::buildid(&paths);
        if schedule.due(&build, dt_core::launch::is_game_running()) && refresh(&paths, &records) {
            schedule.finished(build);
        }
        std::thread::sleep(POLL);
    }
}

/// False while Steam is still updating, so the check runs again on the next poll.
fn refresh(paths: &GamePaths, records: &Path) -> bool {
    let hud = hud_install::refresh_after_update(paths, records);
    let addons = addons_install::refresh_after_update(paths, records);
    !matches!(hud, Ok(Refreshed::SteamBusy)) && !addons.is_ok_and(|a| a.steam_busy)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn checks_at_start_then_after_each_new_build_once_the_game_is_closed() {
        let mut schedule = Schedule::default();
        let one = Some("1".to_string());
        let two = Some("2".to_string());
        assert!(!schedule.due(&one, true), "never while the game runs");
        assert!(schedule.due(&one, false), "once at start");
        schedule.finished(one.clone());
        assert!(!schedule.due(&one, false));
        assert!(schedule.due(&two, false), "after an update");
        schedule.finished(two.clone());
        assert!(!schedule.due(&two, false));
    }
}
