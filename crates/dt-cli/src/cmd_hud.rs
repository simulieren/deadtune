//! hud apply | remove | status

use std::path::Path;

use dt_core::apply::{self, ApplyContext, Target};
use dt_core::catalog::Catalog;
use dt_core::doctor;
use dt_core::hud::elements::HUD_STYLE;
use dt_core::hud::install::{self, ADDON_FILE, InstalledState};
use dt_core::hud::{HudLayout, layout, searchpaths};
use dt_core::locate::GamePaths;

use crate::args::{Args, CliResult, fail};
use crate::env::{self, Env};

fn load_layout(path: &Path) -> Result<HudLayout, crate::args::CliError> {
    toml::from_str(&env::read(path)?).map_err(|e| fail(format!("{}: {e}", path.display())))
}

pub fn apply(env: &Env, args: &Args) -> CliResult {
    args.positionals::<0>("no positional arguments")?;
    reconcile(env, args, &load_layout(&args.path("layout")?)?)
}

pub fn remove(env: &Env, args: &Args) -> CliResult {
    args.positionals::<0>("no positional arguments")?;
    reconcile(env, args, &HudLayout::default())
}

/// Brings the addon in line with `layout` through the apply pipeline, so the SearchPaths edit
/// gets the same backups and checks as a profile apply. ConVars are left as they are.
fn reconcile(env: &Env, args: &Args, layout: &HudLayout) -> CliResult {
    let paths = env.paths()?;
    let store = env.store()?;
    let Some(hud) = apply::hud_plan(&paths, layout, &store)? else {
        println!("HUD: vanilla layout and no DeadTune addon installed; nothing to do.");
        return Ok(());
    };
    let live = env::read(&paths.gameinfo)?;
    let gameinfo = if hud.needs_search_path {
        searchpaths::ensure_addons(&live)?
    } else {
        live.clone()
    };
    let target = Target {
        gameinfo,
        video: None,
        denied: Vec::new(),
        hud: Some(hud),
        addons: None,
        practice_record: None,
    };
    let plan = apply::plan(
        &paths,
        &live,
        None,
        &target,
        Catalog::embedded(),
        ApplyContext::default(),
    )?;
    env::print_plan(&plan, args.switch("diff"));
    if plan.is_empty() {
        return Ok(());
    }
    if !env::confirm(args, "Apply HUD changes?")? {
        println!("Cancelled.");
        return Ok(());
    }
    let report = apply::execute(&paths, &plan, &env.store()?, None)?;
    if report.hud_changed {
        println!("HUD addon updated. Restart the game to load it.");
    }
    Ok(())
}

pub fn status(env: &Env, args: &Args) -> CliResult {
    args.positionals::<0>("no positional arguments")?;
    let paths = env.paths()?;
    for line in report_lines(env, &paths) {
        println!("{line}");
    }
    if let Some(path) = args.value("layout") {
        let patch = layout::compile(&load_layout(Path::new(path))?)?;
        for (file, css) in patch.styles.iter().filter(|(_, css)| !css.is_empty()) {
            println!("layout patches {file}: {css}");
        }
        for (file, edit) in &patch.layouts {
            let includes: Vec<&str> = edit
                .style_includes
                .iter()
                .chain(&edit.script_includes)
                .map(String::as_str)
                .collect();
            println!("layout rebuilds {file} with {}", includes.join(", "));
        }
        for (file, text) in &patch.own_files {
            println!("layout adds {file} ({} bytes)", text.len());
        }
        if patch.is_empty() {
            println!("layout is vanilla: applying it removes the addon");
        }
    }
    Ok(())
}

/// One-line HUD facts for `hud status`, `status` and `doctor`.
pub fn report_lines(env: &Env, paths: &GamePaths) -> Vec<String> {
    let mut lines = Vec::new();
    let state = env
        .store()
        .ok()
        .map(|store| install::installed_state(paths, &store.root));
    lines.push(match state {
        Some(Ok(InstalledState::None)) => "HUD addon: not installed".to_string(),
        Some(Ok(InstalledState::Current(r))) => {
            format!("HUD addon: installed, current ({})", r.patched.join(", "))
        }
        Some(Ok(InstalledState::Stale(r))) => format!(
            "HUD addon: installed for build {}, game updated since; re-run `hud apply`",
            r.build_id.as_deref().unwrap_or("?")
        ),
        Some(Ok(InstalledState::Foreign)) => {
            format!("HUD addon: {ADDON_FILE} exists but is not ours")
        }
        Some(Err(e)) => format!("HUD addon: unreadable state: {e}"),
        None => "HUD addon: data dir unavailable".to_string(),
    });
    let mounted = std::fs::read_to_string(&paths.gameinfo)
        .ok()
        .and_then(|text| searchpaths::has_addons(&text).ok());
    lines.push(match mounted {
        Some(true) => "HUD search path: `Game citadel/addons` present".to_string(),
        Some(false) => "HUD search path: absent (added on first HUD apply)".to_string(),
        None => "HUD search path: SearchPaths block unreadable".to_string(),
    });
    for addon in doctor::hud_conflicts(paths) {
        lines.push(format!(
            "HUD conflict: {} overrides {HUD_STYLE}",
            addon.display()
        ));
    }
    for addon in doctor::settings_menu_conflicts(paths) {
        lines.push(format!(
            "Settings menu conflict: {} replaces {}",
            addon.display(),
            dt_core::hud::ingame::SETTINGS_LAYOUT
        ));
    }
    lines
}
