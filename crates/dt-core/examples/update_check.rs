//! Builds every DeadTune HUD part and every addon made from game files against a game
//! snapshot (`snapshot take` folder), the way a player's DeadTune would after that update,
//! and reads each result back through `addons::verify`. Run it on each new build's
//! snapshot before players launch it.
//!
//! cargo run -p dt-core --example update_check -- <snapshot folder>

use std::collections::BTreeMap;
use std::error::Error;
use std::path::{Path, PathBuf};

use dt_core::addons::{self, AddonId, AddonsConfig, Kind, verify};
use dt_core::hud::health_style::HealthPreset;
use dt_core::hud::install;
use dt_core::hud::player_stats::StatsPreset;
use dt_core::hud::topbar::TopBarPreset;
use dt_core::hud::vpk::{self, VpkDir};
use dt_core::hud::{HudLayout, ingame, layout};

const USAGE: &str = "usage: update_check <snapshot folder>";

fn main() -> Result<(), Box<dyn Error>> {
    let folder = std::env::args().nth(1).map(PathBuf::from).ok_or(USAGE)?;
    let game = game_pak(&folder.join("raw"))?;
    let state = tempfile::tempdir()?;
    let mut failed = 0;
    for (name, layout) in hud_parts() {
        let result = build_hud(&game, &layout, state.path());
        failed += usize::from(result.is_err());
        report(&name, result);
    }
    for id in AddonId::ALL {
        let Kind::Native(native) = addons::info(id).kind else {
            continue;
        };
        let result = native
            .build(&game, &AddonsConfig::default())
            .map_err(|e| e.to_string())
            .and_then(|files| {
                if files.is_empty() {
                    return Ok("nothing to override with default settings".into());
                }
                let pak = VpkDir::in_memory(vpk::write(&files)).map_err(|e| e.to_string())?;
                let expect = native.expect(&game).map_err(|e| e.to_string())?;
                checked(&pak, &expect)
            });
        failed += usize::from(result.is_err());
        report(addons::info(id).name, result);
    }
    if failed > 0 {
        return Err(format!("{failed} part(s) do not build against this snapshot").into());
    }
    Ok(())
}

/// Every file the snapshot copied out of pak01, as one archive.
fn game_pak(raw: &Path) -> Result<VpkDir, Box<dyn Error>> {
    let mut files = BTreeMap::new();
    for dir in ["panorama", "particles", "models", "materials"] {
        collect(raw, &raw.join(dir), &mut files)?;
    }
    if files.is_empty() {
        return Err(format!("no game files under {}", raw.display()).into());
    }
    Ok(VpkDir::in_memory(vpk::write(&files))?)
}

fn collect(
    root: &Path,
    dir: &Path,
    files: &mut BTreeMap<String, Vec<u8>>,
) -> Result<(), Box<dyn Error>> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Ok(());
    };
    for entry in entries {
        let path = entry?.path();
        if path.is_dir() {
            collect(root, &path, files)?;
        } else {
            let rel = path
                .strip_prefix(root)?
                .to_string_lossy()
                .replace('\\', "/");
            files.insert(rel, std::fs::read(&path)?);
        }
    }
    Ok(())
}

fn hud_parts() -> Vec<(String, HudLayout)> {
    let mut parts = Vec::new();
    let mut map = HudLayout::default();
    map.apples_tunnels.apples.on = true;
    map.apples_tunnels.tunnels.on = true;
    map.apples_tunnels.clear_switching = true;
    parts.push(("Apples & tunnels".to_string(), map));
    let mut extras = HudLayout::default();
    extras.top_bar.spawn_timers = true;
    extras.top_bar.urn_lead = true;
    extras.top_bar.purchases = true;
    parts.push(("Top bar extras".to_string(), extras));
    let mut rows = HudLayout::default();
    rows.ingame.wide_fov = true;
    rows.ingame.performance = ingame::PERFORMANCE_ROWS
        .iter()
        .map(|r| r.convar.to_string())
        .collect();
    parts.push(("In-game settings rows".to_string(), rows));
    for preset in TopBarPreset::ALL {
        let hud = HudLayout {
            top_bar: preset.style(),
            ..HudLayout::default()
        };
        parts.push((format!("Top bar: {}", preset.label()), hud));
    }
    for preset in HealthPreset::ALL {
        let hud = HudLayout {
            health: preset.style(),
            ..HudLayout::default()
        };
        parts.push((format!("Health bar: {}", preset.label()), hud));
    }
    for preset in StatsPreset::ALL {
        let hud = HudLayout {
            player_stats: preset.style(),
            ..HudLayout::default()
        };
        parts.push((format!("Player stats: {}", preset.label()), hud));
    }
    parts
}

fn build_hud(game: &VpkDir, hud: &HudLayout, state: &Path) -> Result<String, String> {
    let patch = layout::compile(hud).map_err(|e| e.to_string())?;
    if patch.is_empty() {
        return Ok("nothing to override".into());
    }
    let built = install::build_addon(game, &patch, state).map_err(|e| e.to_string())?;
    let pak = VpkDir::in_memory(vpk::write(&built.files)).map_err(|e| e.to_string())?;
    checked(&pak, &verify::expect_for_hud(game, &pak))
}

fn checked(pak: &VpkDir, expect: &verify::Expect) -> Result<String, String> {
    let verified = verify::verify(pak, expect);
    if verified.is_ok() {
        Ok(verified.to_string())
    } else {
        Err(verified.to_string())
    }
}

fn report(name: &str, result: Result<String, String>) {
    match result {
        Ok(note) => println!("ok    {name}: {note}"),
        Err(why) => println!("FAIL  {name}: {why}"),
    }
}
