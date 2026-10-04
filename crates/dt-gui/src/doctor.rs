//! Local stand-in for `dt_core::doctor` with the agreed shape
//! (`Check { name, status: Pass|Warn|Fail, detail, fix }`). Delete this module and import
//! `dt_core::doctor` once it lands on main; callers only use `run`, `Check` and `CheckStatus`.

use dt_core::gi;
use dt_core::locate::GamePaths;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CheckStatus {
    Pass,
    Warn,
    Fail,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Check {
    pub name: String,
    pub status: CheckStatus,
    pub detail: String,
    pub fix: Option<String>,
}

fn check(name: &str, status: CheckStatus, detail: impl Into<String>, fix: Option<&str>) -> Check {
    Check {
        name: name.into(),
        status,
        detail: detail.into(),
        fix: fix.map(str::to_string),
    }
}

pub fn run(paths: &GamePaths) -> Vec<Check> {
    use CheckStatus::*;
    let mut out = vec![check(
        "Game folder",
        Pass,
        paths.game_root.display().to_string(),
        None,
    )];
    out.push(match std::fs::read_to_string(&paths.gameinfo) {
        Ok(text) => match gi::validate_braces(&text).and_then(|_| gi::read_convars(&text)) {
            Ok(_) => check("Game config file", Pass, "Readable and intact", None),
            Err(e) => check(
                "Game config file",
                Fail,
                e.to_string(),
                Some("Use \"Restore original game files\" or Steam's \"Verify integrity of game files\"."),
            ),
        },
        Err(e) => check(
            "Game config file",
            Fail,
            e.to_string(),
            Some("Run Steam's \"Verify integrity of game files\" for Deadlock."),
        ),
    });
    out.push(if paths.video.is_file() {
        check("Video settings file", Pass, "Found", None)
    } else {
        check(
            "Video settings file",
            Warn,
            "Not created yet",
            Some("Start Deadlock once and open the video settings, then come back."),
        )
    });
    out.push(if paths.appmanifest.is_some() {
        check("Update detection", Pass, "Steam manifest found", None)
    } else {
        check(
            "Update detection",
            Warn,
            "Steam manifest not found next to the game",
            Some("Game updates won't be detected; re-apply after each update."),
        )
    });
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fake_install_passes_and_broken_config_fails_with_a_fix() {
        let (_dir, paths) = crate::state::testutil::fake_install();
        let checks = run(&paths);
        assert!(
            checks.iter().all(|c| c.status != CheckStatus::Fail),
            "{checks:?}"
        );
        std::fs::write(&paths.gameinfo, "\"GameInfo\" {\n ConVars {\n").unwrap();
        let config = run(&paths)
            .into_iter()
            .find(|c| c.name == "Game config file")
            .unwrap();
        assert_eq!(config.status, CheckStatus::Fail);
        assert!(config.fix.is_some());
    }
}
