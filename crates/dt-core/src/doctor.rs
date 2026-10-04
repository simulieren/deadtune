//! Read-only self test for a new machine, shared by `deadtune-cli doctor` and the GUI.
//! The only writes are scratch files that are deleted again: one in the game's cfg folder,
//! one in DeadTune's data folder.

use std::path::{Path, PathBuf};

use crate::backup::{BackupStore, FileKind};
use crate::bridge::netcon::{DEFAULT_PORT, NetconBridge};
use crate::gi::{self, Eol, Overrides};
use crate::hud::elements::HUD_STYLE;
use crate::hud::install::{self, ADDON_FILE, GAME_PAK, InstalledState};
use crate::hud::searchpaths;
use crate::hud::vpk::VpkDir;
use crate::locate::{GamePaths, parse_buildid};
use crate::{launch, video};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CheckStatus {
    Pass,
    Warn,
    Fail,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Check {
    pub name: &'static str,
    pub status: CheckStatus,
    pub detail: String,
    /// A plain-English next step for a player; set whenever the status is not `Pass`.
    pub fix: Option<String>,
    /// A Windows Settings page (`ms-settings:` URI) where the player can make the change.
    pub link: Option<&'static str>,
}

const VERIFY_FILES: &str = "In Steam, right-click Deadlock > Properties > Installed Files > Verify integrity of game files.";

pub(crate) fn pass(name: &'static str, detail: impl Into<String>) -> Check {
    Check {
        name,
        status: CheckStatus::Pass,
        detail: detail.into(),
        fix: None,
        link: None,
    }
}

pub(crate) fn warn(name: &'static str, detail: impl Into<String>, fix: &str) -> Check {
    Check {
        name,
        status: CheckStatus::Warn,
        detail: detail.into(),
        fix: Some(fix.to_string()),
        link: None,
    }
}

pub(crate) fn fail(name: &'static str, detail: impl Into<String>, fix: &str) -> Check {
    Check {
        name,
        status: CheckStatus::Fail,
        detail: detail.into(),
        fix: Some(fix.to_string()),
        link: None,
    }
}

pub fn run(paths: Option<&GamePaths>, data_dir: &Path) -> Vec<Check> {
    let mut checks = Vec::new();
    match paths {
        Some(paths) => {
            checks.push(pass("Find Deadlock", paths.game_root.display().to_string()));
            game_checks(paths, data_dir, &mut checks);
        }
        None => checks.push(fail(
            "Find Deadlock",
            "Deadlock was not found in any Steam library",
            "Start Steam once and make sure Deadlock is installed, then press Retry. \
             If it lives somewhere unusual, choose the Deadlock folder by hand.",
        )),
    }
    checks.push(match probe_writable(data_dir) {
        Ok(()) => pass("DeadTune data folder", data_dir.display().to_string()),
        Err(e) => fail(
            "DeadTune data folder",
            format!("{}: {e}", data_dir.display()),
            "DeadTune cannot save its backups. Make sure your user folder is not full or read-only.",
        ),
    });
    checks.push(match BackupStore::open(data_dir) {
        Ok(store) => {
            let original = if store.original(FileKind::GameInfo).is_some() {
                "original saved"
            } else {
                "original is saved on first apply"
            };
            let count = store.list(FileKind::GameInfo).map_or(0, |l| l.len());
            pass("Backups", format!("{original}, {count} gameinfo backup(s)"))
        }
        Err(e) => fail(
            "Backups",
            e.to_string(),
            "DeadTune cannot save its backups. Make sure your user folder is not full or read-only.",
        ),
    });
    checks.push(if NetconBridge::probe(DEFAULT_PORT) {
        pass(
            "Live console (netcon)",
            format!("listening on port {DEFAULT_PORT}"),
        )
    } else {
        pass(
            "Live console (netcon)",
            "off (optional; DeadTune uses a key bind instead)",
        )
    });
    checks.push(pass(
        "Game running",
        if launch::is_game_running() {
            "yes: file changes take effect next launch"
        } else {
            "no"
        },
    ));
    checks.extend(crate::winfps::checks(paths));
    checks
}

fn game_checks(paths: &GamePaths, data_dir: &Path, checks: &mut Vec<Check>) {
    match std::fs::read_to_string(&paths.gameinfo) {
        Ok(text) => {
            checks.push(pass("Read gameinfo.gi", format!("{} bytes", text.len())));
            gameinfo_checks(&text, checks);
        }
        Err(e) => checks.push(fail("Read gameinfo.gi", e.to_string(), VERIFY_FILES)),
    }

    checks.push(match std::fs::read_to_string(&paths.video) {
        Ok(text) => match video::read_settings(&text) {
            Ok(settings) => pass("Read video.txt", format!("{} settings", settings.len())),
            Err(e) => warn(
                "Read video.txt",
                e.to_string(),
                "Open Deadlock's video settings, change any option and change it back so the game rewrites the file.",
            ),
        },
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => warn(
            "Read video.txt",
            "not created yet",
            "Launch Deadlock once so it creates its video settings, then press Retry.",
        ),
        Err(e) => warn("Read video.txt", e.to_string(), VERIFY_FILES),
    });

    let build = paths
        .appmanifest
        .as_ref()
        .and_then(|p| std::fs::read_to_string(p).ok())
        .and_then(|acf| parse_buildid(&acf));
    checks.push(match build {
        Some(build) => pass("Game build id", build),
        None => warn(
            "Game build id",
            "Steam's appmanifest for Deadlock was not found",
            "DeadTune cannot tell when the game updates. Start Steam and let any Deadlock update finish.",
        ),
    });

    checks.push(if !paths.cfg_dir.is_dir() {
        fail(
            "Write cfg folder",
            format!("{} is missing", paths.cfg_dir.display()),
            VERIFY_FILES,
        )
    } else {
        match probe_writable(&paths.cfg_dir) {
            Ok(()) => pass("Write cfg folder", paths.cfg_dir.display().to_string()),
            Err(e) => fail(
                "Write cfg folder",
                e.to_string(),
                "DeadTune cannot write into the game folder. Close any program that locks it, or run DeadTune from a normal (non-admin) account that owns the Steam folder.",
            ),
        }
    });

    hud_checks(paths, data_dir, checks);
    addon_checks(paths, data_dir, checks);
}

fn addon_checks(paths: &GamePaths, data_dir: &Path, checks: &mut Vec<Check>) {
    use crate::addons::{self, InstalledState as Addon};
    let states = match addons::install::installed_state(paths, data_dir) {
        Ok(states) => states,
        Err(e) => {
            checks.push(warn(
                "Performance addons",
                e.to_string(),
                "DeadTune could not read its addons record. Turn every addon off, Apply, then turn them on again.",
            ));
            return;
        }
    };
    let name = |id: &addons::AddonId| addons::info(*id).name;
    let current: Vec<&str> = states
        .iter()
        .filter(|(_, s)| matches!(s, Addon::Current(_)))
        .map(|(id, _)| name(id))
        .collect();
    let stale: Vec<&str> = states
        .iter()
        .filter(|(_, s)| matches!(s, Addon::Stale(_)))
        .map(|(id, _)| name(id))
        .collect();
    let foreign: Vec<String> = states
        .iter()
        .filter_map(|(id, s)| match s {
            Addon::Foreign(file) => Some(format!("{} ({file})", name(id))),
            _ => None,
        })
        .collect();
    checks.push(
        if current.is_empty() && stale.is_empty() && foreign.is_empty() {
            pass("Performance addons", "none installed")
        } else {
            pass(
                "Performance addons",
                format!("installed: {}", current.join(", ")),
            )
        },
    );
    if !stale.is_empty() {
        checks.push(warn(
            "Addons after update",
            format!("built for an older game version: {}", stale.join(", ")),
            "The game updated. Press Apply (or Build for the texture downscaler) to rebuild them from the new game files.",
        ));
    }
    if !foreign.is_empty() {
        checks.push(warn(
            "Addon files",
            format!("not what DeadTune wrote: {}", foreign.join(", ")),
            "Another program replaced an addon file DeadTune installed. DeadTune will not touch it; delete it by hand if you want DeadTune's version back.",
        ));
    }
}

fn gameinfo_checks(text: &str, checks: &mut Vec<Check>) {
    let eol = match gi::detect_eol(text) {
        Eol::CrLf => "Windows (CRLF)",
        Eol::Lf => "Unix (LF)",
    };
    checks.push(pass("Line endings", eol));
    checks.push(match gi::validate_braces(text) {
        Ok(()) => pass("Brace balance", "balanced"),
        Err(e) => fail(
            "Brace balance",
            e.to_string(),
            &format!(
                "gameinfo.gi is damaged. Use Restore original in DeadTune, or: {VERIFY_FILES}"
            ),
        ),
    });
    checks.push(match gi::apply_overrides(text, &Overrides::new()) {
        Ok(out) if out.text == text => pass("Lossless edit", "an empty edit leaves every byte alone"),
        Ok(_) => fail(
            "Lossless edit",
            "an empty edit changed the file",
            "Please report this to DeadTune with your gameinfo.gi attached; do not apply until fixed.",
        ),
        Err(e) => fail("Lossless edit", e.to_string(), VERIFY_FILES),
    });
    checks.push(match gi::read_convars(text) {
        Ok(entries) => pass(
            "Read convars",
            format!(
                "{} convars, {} active",
                entries.len(),
                entries.iter().filter(|e| !e.commented).count()
            ),
        ),
        Err(e) => fail("Read convars", e.to_string(), VERIFY_FILES),
    });
}

fn hud_checks(paths: &GamePaths, data_dir: &Path, checks: &mut Vec<Check>) {
    let pak = paths.citadel_dir.join(GAME_PAK);
    checks.push(if !pak.is_file() {
        warn(
            "Game archive (HUD)",
            format!("{GAME_PAK} not found"),
            &format!(
                "HUD editing is unavailable until the game files are complete. {VERIFY_FILES}"
            ),
        )
    } else {
        match VpkDir::open(&pak).and_then(|dir| dir.read(HUD_STYLE)) {
            Ok(bytes) => pass(
                "Game archive (HUD)",
                format!("{HUD_STYLE} found ({} bytes)", bytes.len()),
            ),
            Err(e) => fail(
                "Game archive (HUD)",
                e.to_string(),
                &format!("HUD editing cannot read the game archive. {VERIFY_FILES}"),
            ),
        }
    });

    checks.push(match install::installed_state(paths, data_dir) {
        Ok(InstalledState::None) => pass("HUD addon", "not installed"),
        Ok(InstalledState::Current(_)) => pass("HUD addon", "installed and up to date"),
        Ok(InstalledState::Stale(_)) => warn(
            "HUD addon",
            "built for an older game version",
            "The game updated. Press Apply on your HUD layout to rebuild it.",
        ),
        Ok(InstalledState::Foreign) => warn(
            "HUD addon",
            format!("{ADDON_FILE} belongs to another mod"),
            "Another mod uses DeadTune's addon slot. DeadTune will not touch it; remove that mod to use HUD editing.",
        ),
        Err(e) => warn(
            "HUD addon",
            e.to_string(),
            "DeadTune could not read its HUD record. Press Remove on the HUD tab, then Apply again.",
        ),
    });

    if let Ok(text) = std::fs::read_to_string(&paths.gameinfo) {
        checks.push(match searchpaths::has_addons(&text) {
            Ok(true) => pass("HUD search path", "addons are mounted"),
            Ok(false) => pass("HUD search path", "added when you first apply a HUD layout"),
            Err(e) => warn(
                "HUD search path",
                e.to_string(),
                &format!("HUD addons cannot be mounted. {VERIFY_FILES}"),
            ),
        });
    }

    let conflicts = hud_conflicts(paths);
    checks.push(if conflicts.is_empty() {
        pass("HUD conflicts", "no other HUD mods")
    } else {
        let names: Vec<String> = conflicts
            .iter()
            .filter_map(|p| p.file_name().map(|n| n.to_string_lossy().into_owned()))
            .collect();
        warn(
            "HUD conflicts",
            format!("also changing the HUD: {}", names.join(", ")),
            "Another HUD mod is installed. Only one HUD mod can win; disable the other one if your layout does not show up.",
        )
    });
}

/// Other addons that ship their own `hud.vcss_c`; only one of them can win.
pub fn hud_conflicts(paths: &GamePaths) -> Vec<PathBuf> {
    let Ok(dir) = std::fs::read_dir(install::addons_dir(paths)) else {
        return Vec::new();
    };
    let mut found: Vec<PathBuf> = dir
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.ends_with("_dir.vpk") && n != ADDON_FILE)
        })
        .filter(|p| VpkDir::open(p).is_ok_and(|v| v.contains(HUD_STYLE)))
        .collect();
    found.sort();
    found
}

/// Writes and deletes a scratch file in `dir`, creating `dir` first (only ever our data dir;
/// the cfg dir is checked for existence before this runs).
fn probe_writable(dir: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dir)?;
    let probe = dir.join(".deadtune_doctor.tmp");
    std::fs::write(&probe, b"deadtune")?;
    std::fs::remove_file(&probe)
}
