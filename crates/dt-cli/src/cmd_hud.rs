//! hud apply | remove | status | icon

use std::path::Path;

use dt_core::apply::{self, ApplyContext, Target};
use dt_core::backup;
use dt_core::catalog::Catalog;
use dt_core::doctor;
use dt_core::hud::elements::HUD_STYLE;
use dt_core::hud::icons::{self, IconOverride};
use dt_core::hud::install::{self, ADDON_FILE, InstalledState};
use dt_core::hud::{HudLayout, layout, searchpaths};
use dt_core::locate::GamePaths;
use dt_core::texture::encode::Fit;

use crate::args::{Args, CliResult, fail, usage};
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

/// `hud icon list|set|reset|reset-all`: edits the `icons` table of a layout file. Images are
/// stored in the data dir, so the source file can go; `hud apply` ships them.
pub fn icon(env: &Env, args: &Args) -> CliResult {
    let file = args.path("layout")?;
    let mut layout = match env::read_opt(&file)? {
        Some(text) => {
            toml::from_str(&text).map_err(|e| fail(format!("{}: {e}", file.display())))?
        }
        None => HudLayout::default(),
    };
    let ship = format!(
        "Run `hud apply --layout {}` to update the game.",
        file.display()
    );
    match args.pos.first().map(String::as_str) {
        Some("list") => {
            args.positionals::<1>("list")?;
            if layout.icons.is_empty() {
                println!("No icon overrides in {}.", file.display());
            }
            for (path, entry) in &layout.icons {
                println!(
                    "{path}  {}  {}",
                    icon_kind(entry),
                    entry.stored_at(&env.data_dir).display()
                );
            }
            return Ok(());
        }
        Some("set") => {
            let [_, game_path, image] =
                args.positionals("set <game_path> <image.png|image.svg>")?;
            let fit = match args.value("fit") {
                None => Fit::default(),
                Some(text) => Fit::parse(text).ok_or_else(|| usage("--fit is original or own"))?,
            };
            let bytes = std::fs::read(image).map_err(|e| fail(format!("{image}: {e}")))?;
            let entry = icons::set(&mut layout.icons, &env.data_dir, game_path, &bytes, fit)?;
            println!("{game_path}: {} set. {ship}", icon_kind(&entry));
        }
        Some("reset") => {
            let [_, game_path] = args.positionals("reset <game_path>")?;
            if icons::reset(&mut layout.icons, game_path) {
                println!("{game_path}: override removed. {ship}");
            } else {
                println!("{game_path}: no override to remove.");
                return Ok(());
            }
        }
        Some("reset-all") => {
            args.positionals::<1>("reset-all")?;
            let n = layout.icons.len();
            icons::reset_all(&mut layout.icons);
            println!("{n} icon override(s) removed. {ship}");
        }
        _ => return Err(usage("expected list, set, reset or reset-all")),
    }
    let text = toml::to_string(&layout).map_err(|e| fail(e.to_string()))?;
    backup::atomic_write(&file, text.as_bytes())
        .map_err(|e| fail(format!("{}: {e}", file.display())))?;
    Ok(())
}

fn icon_kind(entry: &IconOverride) -> String {
    match entry {
        IconOverride::Png { fit, .. } => format!("png, fit {}", fit.key()),
        IconOverride::Svg { .. } => "svg".into(),
        IconOverride::PngInSvg { .. } => "png in svg (experimental)".into(),
    }
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
        for (file, entry) in &patch.icons {
            println!("layout replaces {file} with your {}", icon_kind(entry));
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
    lines
}
