//! Builds the DeadTune texture addon from the game's own pak01 and prints what it did.
//!
//! cargo run --release -p dt-core --example texture_downscale -- <game_root> [--quarter] [--all] [--lighting] [--out <file>] [--install] [--list]
//!
//! `--list` only classifies the textures (category, lighting) and prints counts with
//! sample paths, which is the quickest way to check the path rules against a real
//! install. `--install` writes straight to game/citadel/addons/pak78_dir.vpk.

use std::collections::BTreeMap;
use std::error::Error;
use std::ops::ControlFlow;
use std::path::PathBuf;
use std::time::Instant;

use dt_core::hud::install::GAME_PAK;
use dt_core::hud::searchpaths;
use dt_core::hud::vpk::VpkDir;
use dt_core::locate;
use dt_core::texture::select::{Category, Factor, category, is_lighting};
use dt_core::texture::{TextureDownscale, build_texture_addon};

const USAGE: &str = "usage: texture_downscale <game_root> [--quarter] [--all] [--lighting] [--out <file>] [--install] [--list]";
const ADDON_FILE: &str = "pak78_dir.vpk";

struct Args {
    game_root: PathBuf,
    cfg: TextureDownscale,
    out: Option<PathBuf>,
    install: bool,
    list: bool,
}

fn parse_args() -> Result<Args, String> {
    let mut game_root = None;
    let mut cfg = TextureDownscale::default();
    let mut out = None;
    let mut install = false;
    let mut list = false;
    let mut it = std::env::args().skip(1);
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--quarter" => cfg.factor = Factor::Quarter,
            "--all" => cfg.categories = Category::ALL.into_iter().collect(),
            "--lighting" => cfg.exclude_lighting = false,
            "--out" => out = Some(PathBuf::from(it.next().ok_or("--out needs a path")?)),
            "--install" => install = true,
            "--list" => list = true,
            "-h" | "--help" => return Err(USAGE.into()),
            flag if flag.starts_with("--") => return Err(format!("unknown flag {flag}\n{USAGE}")),
            _ if game_root.is_none() => game_root = Some(PathBuf::from(arg)),
            _ => return Err(USAGE.into()),
        }
    }
    Ok(Args {
        game_root: game_root.ok_or(USAGE)?,
        cfg,
        out,
        install,
        list,
    })
}

fn mb(bytes: u64) -> String {
    format!("{:.1} MB", bytes as f64 / 1e6)
}

fn list(pak: &VpkDir) {
    let mut groups: BTreeMap<(Category, bool), Vec<&str>> = BTreeMap::new();
    for path in pak.entries.keys().filter(|p| p.ends_with(".vtex_c")) {
        groups
            .entry((category(path), is_lighting(path)))
            .or_default()
            .push(path);
    }
    for ((cat, lighting), paths) in &groups {
        let tag = if *lighting { " (lighting)" } else { "" };
        println!("{:<9}{tag:<11} {:>7} textures", cat.label(), paths.len());
        for p in paths.iter().step_by((paths.len() / 3).max(1)).take(3) {
            println!("    {p}");
        }
    }
}

fn main() -> Result<(), Box<dyn Error>> {
    let args = parse_args()?;
    let paths = locate::from_game_root(&args.game_root)?;
    let game_pak = paths.citadel_dir.join(GAME_PAK);

    if args.list {
        list(&VpkDir::open(&game_pak)?);
        return Ok(());
    }

    let out = match (&args.out, args.install) {
        (Some(o), _) => o.clone(),
        (None, true) => paths.citadel_dir.join("addons").join(ADDON_FILE),
        (None, false) => PathBuf::from(ADDON_FILE),
    };
    println!("config: {:?}", args.cfg);
    println!("output: {}", out.display());

    let started = Instant::now();
    let mut last = Instant::now();
    let stats = build_texture_addon(&[game_pak], &args.cfg, &out, &mut |p| {
        if last.elapsed().as_secs() >= 2 || p.done == p.total {
            last = Instant::now();
            println!(
                "{}/{}  reduced {}  addon {}  {}",
                p.done,
                p.total,
                p.stats.reduced,
                mb(p.stats.bytes_after),
                p.path
            );
        }
        ControlFlow::Continue(())
    })?;

    println!(
        "\n{} textures, {} reduced in {:.0?}",
        stats.textures,
        stats.reduced,
        started.elapsed()
    );
    println!(
        "replaced {} of originals with {} of copies",
        mb(stats.bytes_before),
        mb(stats.bytes_after)
    );
    for (reason, n) in &stats.skipped {
        println!("skipped {n:>7}  {reason}");
    }

    println!("\nnext steps:");
    let gameinfo = std::fs::read_to_string(&paths.gameinfo)?;
    if searchpaths::ensure_addons(&gameinfo)? != gameinfo {
        println!(
            "- {} needs `Game {}` in SearchPaths or the addon never mounts",
            paths.gameinfo.display(),
            searchpaths::ADDONS_LINE_VALUE
        );
    }
    if args.install {
        println!(
            "- launch Deadlock; delete {} to restore full quality",
            out.display()
        );
    } else {
        println!(
            "- copy {} (and any {}_NNN.vpk next to it) into {}",
            out.display(),
            ADDON_FILE.trim_end_matches("_dir.vpk"),
            paths.citadel_dir.join("addons").display()
        );
    }
    Ok(())
}
