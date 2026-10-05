//! Read-only self test for a new machine, shared by `deadtune-cli doctor` and the GUI.
//! The only writes are scratch files that are deleted again: one in the game's cfg folder,
//! one in DeadTune's data folder.

use std::path::{Path, PathBuf};

use crate::addons::guard::{self, Guard, Pak, PakState};
use crate::backup::{BackupStore, FileKind};
use crate::bridge::netcon::{DEFAULT_PORT, NetconBridge};
use crate::gi::{self, Eol, Overrides};
use crate::hud::elements::HUD_STYLE;
use crate::hud::install::{self, ADDON_FILE, GAME_PAK, InstalledState};
use crate::hud::searchpaths;
use crate::hud::vpk::VpkDir;
use crate::locate::{GamePaths, parse_buildid};
use crate::{launch, practice, video};

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
            gameinfo_checks(&text, data_dir, checks);
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
    checks.push(other_mods(paths, data_dir));
    if let Some(steam) = &paths.steam_root {
        checks.push(launch_options_check(&crate::steamcfg::read_launch_options(
            steam,
        )));
    }
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

fn gameinfo_checks(text: &str, data_dir: &Path, checks: &mut Vec<Check>) {
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
    checks.push(matchmaking_check(text, data_dir));
}

/// Edits outside the ConVars block that the matchmaking check refuses: the practice values
/// DeadTune wrote (per its record), and anything another tool left in those sections.
fn matchmaking_check(text: &str, data_dir: &Path) -> Check {
    const NAME: &str = "Matchmaking sections";
    let drift = match practice::matchmaking_drift(text) {
        Ok(drift) => drift,
        Err(e) => return warn(NAME, e.to_string(), VERIFY_FILES),
    };
    if drift.is_empty() {
        return pass(NAME, "stock");
    }
    let record = practice::Record::load(data_dir).unwrap_or_default();
    let (ours, others): (Vec<_>, Vec<_>) = drift
        .iter()
        .partition(|d| d.managed() && record.prior(&d.key).is_some());
    let other_keys: Vec<String> = others
        .iter()
        .map(|d| format!("{}/{}", d.section, d.key))
        .collect();
    let other_note = match others.len() {
        0 => String::new(),
        _ => format!("not Valve's stock values: {}", other_keys.join(", ")),
    };
    if !ours.is_empty() {
        let fix = if others.is_empty() {
            "Practice mode is on, so matchmaking may refuse to queue. Turn on Ranked-safe mode (Safety & setup) or switch Practice mode off (Performance) and Apply before queueing.".to_string()
        } else {
            format!(
                "Practice mode is on and another tool changed these sections too, so matchmaking may refuse to queue. Ranked-safe mode resets practice mode's shadow, fog and batching keys; for the rest: {VERIFY_FILES}"
            )
        };
        let detail = if others.is_empty() {
            format!(
                "Practice mode on: matchmaking may refuse to queue ({} SceneSystem key(s))",
                ours.len()
            )
        } else {
            format!(
                "Practice mode on: matchmaking may refuse to queue ({} SceneSystem key(s); {other_note})",
                ours.len()
            )
        };
        return warn(NAME, detail, &fix);
    }
    warn(
        NAME,
        format!("matchmaking may refuse to queue: {other_note}"),
        &format!(
            "Another tool (SideLock, for example) changed these sections, so matchmaking may refuse to queue. DeadTune leaves them alone on Apply. Ranked-safe mode resets the shadow, fog and batching keys; for the rest: {VERIFY_FILES}"
        ),
    )
}

/// Mod paks in the addons folder that DeadTune did not install. Information, not a fault.
fn other_mods(paths: &GamePaths, data_dir: &Path) -> Check {
    const NAME: &str = "Other mods";
    let ours = addon_owners(paths, data_dir);
    let mut foreign: Vec<String> = std::fs::read_dir(install::addons_dir(paths))
        .into_iter()
        .flatten()
        .filter_map(|e| e.ok())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| {
            n.ends_with("_dir.vpk") && !ours.get(n).is_some_and(|o| o.starts_with("DeadTune:"))
        })
        .collect();
    foreign.sort();
    match foreign.len() {
        0 => pass(NAME, "none"),
        1 => pass(NAME, format!("1 mod: {}", foreign[0])),
        n => pass(NAME, format!("{n} mods: {}", foreign.join(", "))),
    }
}

/// Deadlock's Steam launch options, warning on `+` console options (other than DeadTune's
/// boot cfg): they run at every start and can override DeadTune's settings.
pub fn launch_options_check(options: &[String]) -> Check {
    const NAME: &str = "Steam launch options";
    if options.is_empty() {
        return pass(NAME, "none set");
    }
    let overrides: Vec<String> = options
        .iter()
        .flat_map(|o| crate::steamcfg::console_overrides(o))
        .collect();
    if overrides.is_empty() {
        pass(NAME, options.join(" | "))
    } else {
        warn(
            NAME,
            format!("{} (sets {})", options.join(" | "), overrides.join(", ")),
            "These run every time the game starts and can undo DeadTune's settings. Unless you set them on purpose, remove them: in Steam, right-click Deadlock > Properties > General > Launch options.",
        )
    }
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
    checks.extend(hud_launch_test(paths, data_dir));

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

    let names = |paks: &[PathBuf]| {
        paks.iter()
            .filter_map(|p| p.file_name().map(|n| n.to_string_lossy().into_owned()))
            .collect::<Vec<_>>()
            .join(", ")
    };
    let conflicts = hud_conflicts(paths);
    checks.push(if conflicts.is_empty() {
        pass("HUD conflicts", "no other HUD mods")
    } else {
        warn(
            "HUD conflicts",
            format!("also changing the HUD: {}", names(&conflicts)),
            "Another HUD mod is installed. Only one HUD mod can win; disable the other one if your layout does not show up.",
        )
    });
    let conflicts = settings_menu_conflicts(paths);
    checks.push(if conflicts.is_empty() {
        pass("Settings menu mods", "no other mod replaces the settings menu")
    } else {
        warn(
            "Settings menu mods",
            format!("also replacing the settings menu: {}", names(&conflicts)),
            "Another mod ships its own settings menu. DeadTune's in-game settings rows (Wide FOV, the DeadTune group) live in the same file, and only one copy loads; disable the other mod if the rows do not show up.",
        )
    });
}

/// Stylesheets DeadTune's HUD addon can replace; another pak shipping one clashes with it.
const PATCHED_STYLES: [&str; 4] = [
    HUD_STYLE,
    crate::hud::minimap_colors::MINIMAP_STYLE,
    crate::hud::health_style::HEALTH_STYLE,
    crate::hud::health_style::HEALTH_CONTAINER_STYLE,
];

/// Other addons that ship their own `hud.vcss_c`; only one of them can win.
pub fn hud_conflicts(paths: &GamePaths) -> Vec<PathBuf> {
    addons_shipping(paths, &PATCHED_STYLES)
}

/// Other addons that replace the whole settings menu, where our in-game settings rows go.
pub fn settings_menu_conflicts(paths: &GamePaths) -> Vec<PathBuf> {
    addons_shipping(paths, &[crate::hud::ingame::SETTINGS_LAYOUT])
}

/// Foreign `pakNN_dir.vpk` files in the addons folder carrying any of `files`, sorted.
fn addons_shipping(paths: &GamePaths, files: &[&str]) -> Vec<PathBuf> {
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
        .filter(|p| VpkDir::open(p).is_ok_and(|v| files.iter().any(|f| v.contains(f))))
        .collect();
    found.sort();
    found
}

/// How many console log lines the report quotes.
pub const REPORT_LOG_LINES: usize = 80;

/// Plain text for a bug report: versions, the SearchPaths block, what sits in the addons
/// folder and who owns it, DeadTune's records, the launch guard, the console log tail and
/// a read-back of every installed pak. Reads only.
pub fn report(
    paths: Option<&GamePaths>,
    data_dir: &Path,
    version: &str,
    launch_args: &[String],
) -> String {
    use std::fmt::Write as _;
    let mut out = String::new();
    let _ = writeln!(out, "DeadTune {version} diagnostic report");
    let _ = writeln!(
        out,
        "generated: {}  os: {}",
        chrono::Local::now().format("%Y-%m-%d %H:%M:%S"),
        std::env::consts::OS
    );
    let _ = writeln!(out, "data dir: {}", data_dir.display());
    let Some(paths) = paths else {
        out.push_str("game: not found\n");
        return out;
    };
    let _ = writeln!(out, "game root: {}", paths.game_root.display());
    let build = paths
        .appmanifest
        .as_ref()
        .and_then(|p| std::fs::read_to_string(p).ok())
        .and_then(|acf| parse_buildid(&acf));
    let _ = writeln!(out, "build id: {}", build.as_deref().unwrap_or("unknown"));
    let _ = writeln!(out, "last launch args: {}", launch_args.join(" "));
    let _ = writeln!(
        out,
        "game running now: {}",
        if launch::is_game_running() {
            "yes"
        } else {
            "no"
        }
    );

    section(&mut out, "gameinfo.gi SearchPaths");
    match std::fs::read_to_string(&paths.gameinfo) {
        Ok(text) => {
            match searchpaths::block_text(&text) {
                Ok(block) => out.push_str(block.trim_end_matches(['\r', '\n'])),
                Err(e) => {
                    let _ = write!(out, "unreadable: {e}");
                }
            }
            out.push('\n');
            let convars = gi::read_convars(&text).map(|c| c.len());
            let _ = writeln!(
                out,
                "ConVars lines: {}",
                convars.map_or_else(|e| e.to_string(), |n| n.to_string())
            );
        }
        Err(e) => {
            let _ = writeln!(out, "unreadable: {e}");
        }
    }

    section(&mut out, "game/citadel/addons");
    out.push_str(&addons_listing(paths, data_dir));

    for (title, file) in [
        (
            "addons record (addons.toml)",
            crate::addons::install::RECORD_FILE,
        ),
        ("HUD record (hud.toml)", install::RECORD_FILE),
        (
            "launch guard (guard.toml)",
            crate::addons::guard::RECORD_FILE,
        ),
        ("practice record (practice.toml)", practice::RECORD_FILE),
    ] {
        section(&mut out, title);
        match std::fs::read_to_string(data_dir.join(file)) {
            Ok(text) if text.trim().is_empty() => out.push_str("(empty)\n"),
            Ok(text) => {
                out.push_str(text.trim_end());
                out.push('\n');
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => out.push_str("(none)\n"),
            Err(e) => {
                let _ = writeln!(out, "unreadable: {e}");
            }
        }
    }

    section(&mut out, "launch guard, per pak");
    let lines = Guard::load(data_dir)
        .map(|g| g.report(&guard::installed_paks(paths, data_dir)))
        .unwrap_or_else(|e| vec![format!("unreadable: {e}")]);
    if lines.is_empty() {
        out.push_str("nothing installed or tested\n");
    }
    for line in lines {
        let _ = writeln!(out, "{line}");
    }

    section(&mut out, "installed pak read-back");
    let reports = crate::addons::verify::verify_installed(paths, data_dir);
    if reports.is_empty() {
        out.push_str("no DeadTune paks installed\n");
    }
    for r in reports {
        let _ = writeln!(out, "{r}");
    }

    section(
        &mut out,
        &format!("console.log, last {REPORT_LOG_LINES} lines"),
    );
    match crate::bridge::conlog::candidates(paths)
        .into_iter()
        .find(|p| p.is_file())
    {
        Some(log) => {
            let _ = writeln!(out, "file: {}", log.display());
            match std::fs::read(&log) {
                Ok(bytes) => {
                    let text = String::from_utf8_lossy(&bytes);
                    let lines: Vec<&str> = text.lines().collect();
                    let from = lines.len().saturating_sub(REPORT_LOG_LINES);
                    for line in &lines[from..] {
                        out.push_str(line);
                        out.push('\n');
                    }
                }
                Err(e) => {
                    let _ = writeln!(out, "unreadable: {e}");
                }
            }
        }
        None => out.push_str("no console.log found (launch with -condebug)\n"),
    }
    out
}

pub const HUD_LAUNCH_TEST: &str = "HUD launch test";

/// What the launch guard knows about the HUD pak; no row while it knows nothing.
fn hud_launch_test(paths: &GamePaths, data_dir: &Path) -> Option<Check> {
    let guard = Guard::load(data_dir).ok()?;
    let installed = guard::installed_paks(paths, data_dir);
    let sha = installed
        .iter()
        .find(|p| p.id == Pak::Hud)
        .map(|p| p.sha256.as_str());
    let state = guard.state_of(Pak::Hud, sha)?;
    Some(match state {
        PakState::Broke { .. } => warn(
            HUD_LAUNCH_TEST,
            state.describe(),
            "DeadTune's HUD changes stopped the game from starting, so they were turned off. \
             Your settings are kept; press Try again on the HUD page, or turn off the newest HUD change.",
        ),
        _ => pass(HUD_LAUNCH_TEST, state.describe()),
    })
}

fn section(out: &mut String, title: &str) {
    out.push_str("\n== ");
    out.push_str(title);
    out.push_str(" ==\n");
}

/// Files in the addons folder that DeadTune's records account for, with who they belong to.
fn addon_owners(paths: &GamePaths, data_dir: &Path) -> std::collections::BTreeMap<String, String> {
    use crate::addons::{self, InstalledState as Addon};
    let mut ours: std::collections::BTreeMap<String, String> = std::collections::BTreeMap::new();
    if let Ok(states) = addons::install::installed_state(paths, data_dir) {
        for (id, state) in states {
            let name = addons::info(id).name;
            match state {
                Addon::Current(r) | Addon::Stale(r) => {
                    ours.insert(r.file.clone(), format!("DeadTune: {name}"));
                    for chunk in r.chunks {
                        ours.insert(chunk, format!("DeadTune: {name} (chunk)"));
                    }
                }
                Addon::Foreign(file) => {
                    ours.insert(
                        file,
                        format!("was DeadTune's {name}, replaced by another program"),
                    );
                }
                Addon::None => {}
            }
        }
    }
    match install::installed_state(paths, data_dir) {
        Ok(InstalledState::Current(_) | InstalledState::Stale(_)) => {
            ours.insert(ADDON_FILE.into(), "DeadTune: HUD layout".into());
        }
        Ok(InstalledState::Foreign) => {
            ours.insert(
                ADDON_FILE.into(),
                "another mod in DeadTune's HUD slot".into(),
            );
        }
        _ => {}
    }
    ours
}

/// Every file in the addons folder with its size and whether DeadTune's records own it.
fn addons_listing(paths: &GamePaths, data_dir: &Path) -> String {
    let dir = install::addons_dir(paths);
    let ours = addon_owners(paths, data_dir);
    let Ok(entries) = std::fs::read_dir(&dir) else {
        return format!("{} does not exist\n", dir.display());
    };
    let mut rows: Vec<(String, u64)> = entries
        .filter_map(|e| e.ok())
        .filter_map(|e| {
            let len = e.metadata().ok()?.len();
            Some((e.file_name().to_string_lossy().into_owned(), len))
        })
        .collect();
    rows.sort();
    if rows.is_empty() {
        return "(empty)\n".into();
    }
    rows.iter()
        .map(|(name, len)| {
            let owner = ours.get(name).map_or("not DeadTune's", String::as_str);
            format!("{name:<20} {len:>12} bytes  {owner}\n")
        })
        .collect()
}

/// Writes and deletes a scratch file in `dir`, creating `dir` first (only ever our data dir;
/// the cfg dir is checked for existence before this runs).
fn probe_writable(dir: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dir)?;
    let probe = dir.join(".deadtune_doctor.tmp");
    std::fs::write(&probe, b"deadtune")?;
    std::fs::remove_file(&probe)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::addons::install::{self as addons_install, tests::fake_install};
    use crate::addons::{AddonId, AddonsConfig};

    #[test]
    fn launch_options_warn_only_on_console_overrides() {
        assert_eq!(launch_options_check(&[]).status, CheckStatus::Pass);
        let quiet = launch_options_check(&["-dx11 +exec deadtune_boot".into()]);
        assert_eq!(
            (quiet.status, quiet.detail.as_str()),
            (CheckStatus::Pass, "-dx11 +exec deadtune_boot")
        );
        let loud = launch_options_check(&["-high +fps_max 60".into()]);
        assert_eq!(loud.status, CheckStatus::Warn);
        assert!(loud.detail.contains("sets +fps_max 60"), "{}", loud.detail);
        assert!(loud.fix.unwrap().contains("Launch options"));
    }

    #[test]
    fn other_mods_lists_paks_deadtune_does_not_own() {
        let (steam, paths) = fake_install("4242");
        let data = steam.path().join("data");
        assert_eq!(other_mods(&paths, &data).detail, "none");
        let addons = install::addons_dir(&paths);
        std::fs::create_dir_all(&addons).unwrap();
        std::fs::write(addons.join("pak60_dir.vpk"), b"x").unwrap();
        std::fs::write(addons.join("pak60_000.vpk"), b"x").unwrap();
        let config = AddonsConfig {
            enabled: [AddonId::SinnerLightFix].into_iter().collect(),
            ..Default::default()
        };
        let plan = addons_install::plan(&paths, &config, &data).unwrap();
        addons_install::execute(&plan, &paths, &data).unwrap();
        let check = other_mods(&paths, &data);
        assert_eq!(
            (check.status, check.detail.as_str()),
            (CheckStatus::Pass, "1 mod: pak60_dir.vpk")
        );
    }

    #[test]
    fn the_hud_launch_test_shows_in_the_check_and_the_report() {
        use crate::addons::guard::{Guard, Pak, Verdict};
        let (steam, paths) = fake_install("4242");
        let data = steam.path().join("data");
        let mut checks = Vec::new();
        hud_checks(&paths, &data, &mut checks);
        assert!(!checks.iter().any(|c| c.name == HUD_LAUNCH_TEST));

        let mut guard = Guard::default();
        guard.verdicts.insert(
            Pak::Hud,
            Verdict::Failed {
                at: 1_800_000_000,
                fatal: Some("FATAL ERROR: x".into()),
                sha256: Some("bad".into()),
            },
        );
        guard.save(&data).unwrap();
        let mut checks = Vec::new();
        hud_checks(&paths, &data, &mut checks);
        let check = checks.iter().find(|c| c.name == HUD_LAUNCH_TEST).unwrap();
        assert_eq!(check.status, CheckStatus::Warn);
        assert!(
            check.detail.starts_with("stopped the game from starting"),
            "{}",
            check.detail
        );
        assert!(check.fix.as_deref().unwrap().contains("Try again"));

        let text = report(Some(&paths), &data, "9.9.9", &[]);
        assert!(
            text.contains(
                "== launch guard, per pak ==\nDeadTune's HUD changes: stopped the game from starting"
            ),
            "{text}"
        );
    }

    #[test]
    fn report_quotes_the_search_paths_records_paks_and_the_log_tail() {
        let (steam, paths) = fake_install("4242");
        let data = steam.path().join("data");
        let args = ["+exec".to_string(), "deadtune_boot".to_string()];
        let text = report(Some(&paths), &data, "9.9.9", &args);
        assert!(
            text.starts_with("DeadTune 9.9.9 diagnostic report\n"),
            "{text}"
        );
        assert!(text.contains("build id: 4242"));
        assert!(text.contains("last launch args: +exec deadtune_boot"));
        assert!(text.contains("SearchPaths"));
        assert!(text.contains("Game \"citadel\""), "{text}");
        assert!(text.contains("ConVars lines: "));
        assert!(text.contains("no DeadTune paks installed"));
        assert!(text.contains("== launch guard (guard.toml) ==\n(none)"));
        assert!(text.contains("no console.log found"));

        let config = AddonsConfig {
            enabled: [AddonId::SinnerLightFix].into_iter().collect(),
            ..Default::default()
        };
        let plan = addons_install::plan(&paths, &config, &data).unwrap();
        addons_install::execute(&plan, &paths, &data).unwrap();
        let addons = install::addons_dir(&paths);
        std::fs::write(addons.join("pak05_dir.vpk"), b"theirs").unwrap();
        let log: String = (0..100).map(|i| format!("line {i}\n")).collect();
        std::fs::write(paths.citadel_dir.join("console.log"), log).unwrap();

        let text = report(Some(&paths), &data, "9.9.9", &args);
        assert!(text.contains("pak73_dir.vpk"), "{text}");
        assert!(
            text.contains("DeadTune: Sinner's Sacrifice light fix"),
            "{text}"
        );
        assert!(text.contains("pak05_dir.vpk"), "{text}");
        assert!(text.contains("not DeadTune's"), "{text}");
        assert!(
            text.contains("[installed.sinner_light_fix]"),
            "record quoted: {text}"
        );
        assert!(
            text.contains("Sinner's Sacrifice light fix (pak73_dir.vpk): ok:"),
            "{text}"
        );
        assert!(!text.contains("line 19\n"), "only the last 80 lines");
        assert!(text.contains("line 20\n"));
        assert!(text.ends_with("line 99\n"));
        assert_eq!(report(None, &data, "1.0.0", &[]).lines().count(), 4);
    }
}
