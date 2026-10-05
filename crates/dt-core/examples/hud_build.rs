//! Builds the DeadTune HUD addon from a layout TOML and prints the install plan.
//!
//! cargo run -p dt-core --example hud_build -- <game_root> <layout.toml> [--out <file>] [--install]

use std::error::Error;
use std::path::PathBuf;

use dt_core::hud::install::{self, GAME_PAK, HudAction};
use dt_core::hud::vpk::VpkDir;
use dt_core::hud::{HudLayout, searchpaths};
use dt_core::{backup, locate};

const USAGE: &str = "usage: hud_build <game_root> <layout.toml> [--out <file>] [--install]";

struct Args {
    game_root: PathBuf,
    layout: PathBuf,
    out: Option<PathBuf>,
    install: bool,
}

fn parse_args() -> Result<Args, String> {
    let mut positional = Vec::new();
    let mut out = None;
    let mut install = false;
    let mut it = std::env::args().skip(1);
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--out" => out = Some(PathBuf::from(it.next().ok_or("--out needs a path")?)),
            "--install" => install = true,
            "-h" | "--help" => return Err(USAGE.into()),
            flag if flag.starts_with("--") => return Err(format!("unknown flag {flag}\n{USAGE}")),
            _ => positional.push(PathBuf::from(arg)),
        }
    }
    let [game_root, layout]: [PathBuf; 2] = positional.try_into().map_err(|_| USAGE.to_string())?;
    Ok(Args {
        game_root,
        layout,
        out,
        install,
    })
}

fn main() -> Result<(), Box<dyn Error>> {
    let args = parse_args()?;
    let layout: HudLayout = toml::from_str(&std::fs::read_to_string(&args.layout)?)?;
    let paths = locate::from_game_root(&args.game_root)?;
    let state_dir = backup::data_dir().join("hud");
    let plan = install::plan(&paths, &layout, &state_dir)?;

    println!("addon:   {}", plan.addon_path.display());
    println!(
        "state:   {}",
        install::installed_state(&paths, &state_dir)
            .map_or_else(|e| e.to_string(), |s| format!("{s:?}"))
    );
    if !layout.minimap_colors.is_empty() {
        println!("experimental: minimap colours are untested in game");
    }
    match &plan.action {
        HudAction::Write(bytes) => println!("action:  write {} bytes", bytes.len()),
        HudAction::Remove => println!("action:  remove (layout is vanilla)"),
        HudAction::Nothing => println!("action:  nothing (already up to date)"),
    }
    for (file, css) in plan.patch.styles.iter().filter(|(_, css)| !css.is_empty()) {
        println!("\npatched: {file}\n{css}");
    }
    for (file, edit) in &plan.patch.layouts {
        println!("\nrebuilt: {file}\n{edit:?}");
    }
    for (file, text) in &plan.patch.own_files {
        println!("\nadded: {file} ({} bytes)", text.len());
    }
    for c in &plan.conflicts {
        println!(
            "\nconflict: {} also overrides {}; only one of them will apply",
            c.addon.display(),
            c.paths.join(", ")
        );
    }
    println!("\nneeds_search_path: {}", plan.needs_search_path);

    if let Some(out) = &args.out {
        let pak = VpkDir::open(&paths.citadel_dir.join(GAME_PAK))?;
        let bytes = install::build_addon(&pak, &plan.patch)?;
        backup::atomic_write(out, &bytes)?;
        println!("wrote {} ({} bytes)", out.display(), bytes.len());
    }
    if args.install {
        install::execute(&plan, &paths, &state_dir)?;
        let done = match plan.action {
            HudAction::Write(_) => "installed",
            HudAction::Remove => "removed",
            HudAction::Nothing => "unchanged",
        };
        println!(
            "{done}; record in {}",
            state_dir.join(install::RECORD_FILE).display()
        );
    }

    println!("\nnext steps:");
    if plan.needs_search_path {
        let gameinfo = std::fs::read_to_string(&paths.gameinfo)?;
        let ensured = searchpaths::ensure_addons(&gameinfo)?;
        println!(
            "- {} needs `Game {}` in SearchPaths or the addon never mounts. Apply this diff (back the file up first):\n",
            paths.gameinfo.display(),
            searchpaths::ADDONS_LINE_VALUE
        );
        print!(
            "{}",
            similar::TextDiff::from_lines(&gameinfo, &ensured)
                .unified_diff()
                .header("gameinfo.gi", "gameinfo.gi")
        );
    }
    match (args.install, &args.out) {
        (true, _) => println!("- launch Deadlock and check the HUD"),
        (false, Some(out)) => println!(
            "- copy {} to {} and launch Deadlock",
            out.display(),
            plan.addon_path.display()
        ),
        (false, None) => println!("- rerun with --install, or --out <file> to inspect the VPK"),
    }
    Ok(())
}
