//! locate, status, presets, catalog, doctor

use std::path::{Path, PathBuf};

use dt_core::backup::FileKind;
use dt_core::catalog::Catalog;
use dt_core::doctor::{self, CheckStatus};
use dt_core::locate::{GamePaths, parse_buildid};
use dt_core::{launch, power, preset, watch};

use crate::args::{Args, CliResult, fail};
use crate::cmd_hud;
use crate::env::{self, Env};

fn buildid(paths: &GamePaths) -> Option<String> {
    let acf = std::fs::read_to_string(paths.appmanifest.as_ref()?).ok()?;
    parse_buildid(&acf)
}

fn opt_path(p: &Option<PathBuf>) -> String {
    p.as_ref()
        .map_or("(not found)".into(), |p| p.display().to_string())
}

pub fn locate(env: &Env, args: &Args) -> CliResult {
    args.positionals::<0>("no positional arguments")?;
    let paths = env.paths()?;
    println!("game root:   {}", paths.game_root.display());
    println!("gameinfo:    {}", paths.gameinfo.display());
    println!("video:       {}", paths.video.display());
    println!("cfg dir:     {}", paths.cfg_dir.display());
    println!("appmanifest: {}", opt_path(&paths.appmanifest));
    println!("steam root:  {}", opt_path(&paths.steam_root));
    println!(
        "buildid:     {}",
        buildid(&paths).unwrap_or_else(|| "(unknown)".into())
    );
    println!("data dir:    {}", env.data_dir.display());
    Ok(())
}

pub fn status(env: &Env, args: &Args) -> CliResult {
    args.positionals::<0>("no positional arguments")?;
    let paths = env.paths()?;
    let store = env.store()?;
    let fp = watch::fingerprint(&paths);
    let short = |sha: &Option<String>| {
        sha.as_deref()
            .map_or("(missing)".into(), |s| s[..12].to_string())
    };
    println!(
        "game running: {}",
        if launch::is_game_running() {
            "yes"
        } else {
            "no"
        }
    );
    println!("power source: {:?}", power::power_source());
    println!(
        "buildid:      {}",
        fp.buildid.as_deref().unwrap_or("(unknown)")
    );
    println!("gameinfo:     sha256 {}", short(&fp.gameinfo_sha));
    println!("video:        sha256 {}", short(&fp.video_sha));
    let original = match store.original(FileKind::GameInfo) {
        None => "no original snapshot yet (taken on first apply)".to_string(),
        Some(o) if Some(&o.sha256) == fp.gameinfo_sha.as_ref() => {
            "live gameinfo matches the original".into()
        }
        Some(_) => "live gameinfo differs from the original".into(),
    };
    println!("original:     {original}");
    for line in cmd_hud::report_lines(env, &paths) {
        println!("{line}");
    }
    if let Some(file) = args.value("profile") {
        let profile = env::load_profile(Path::new(file))?;
        println!("pending for profile {:?}:", profile.name);
        let plan = env::profile_plan(env, &paths, &profile, args.switch("sandbox"))?;
        env::print_plan(&plan, false);
    }
    Ok(())
}

pub fn presets(_: &Env, args: &Args) -> CliResult {
    args.positionals::<0>("no positional arguments")?;
    for p in preset::all() {
        let video = if p.pinned_video.is_some() {
            "  +video.txt"
        } else {
            ""
        };
        println!("{:<22} {:<28} by {}{video}", p.id.key(), p.label, p.author);
    }
    Ok(())
}

pub fn catalog(_: &Env, args: &Args) -> CliResult {
    let query = args.pos.join(" ");
    let mut n = 0;
    for (name, e) in Catalog::embedded().search(&query) {
        n += 1;
        let deny = if e.denylist { "  DENYLIST" } else { "" };
        println!(
            "{name:<48} {:<10} impact={:<7} default={:<10} {}{deny}",
            format!("{:?}", e.apply),
            format!("{:?}", e.impact),
            e.default.as_deref().unwrap_or("?"),
            e.category,
        );
    }
    println!("{n} convar(s)");
    Ok(())
}

/// Prints the shared `dt_core::doctor` checks; exits 1 if any failed.
pub fn doctor(env: &Env, args: &Args) -> CliResult {
    args.positionals::<0>("no positional arguments")?;
    println!(
        "deadtune-cli {} doctor on {}",
        env!("CARGO_PKG_VERSION"),
        std::env::consts::OS
    );
    let paths = env
        .paths()
        .map_err(|e| eprintln!("locate: {}", e.message()))
        .ok();
    let checks = doctor::run(paths.as_ref(), &env.data_dir);
    for c in &checks {
        let tag = match c.status {
            CheckStatus::Pass => "PASS",
            CheckStatus::Warn => "WARN",
            CheckStatus::Fail => "FAIL",
        };
        println!("{tag}  {:<24} {}", c.name, c.detail);
        if let Some(fix) = &c.fix {
            println!("      fix: {fix}");
        }
        if let Some(link) = c.link {
            println!("      settings: {link}");
        }
    }
    if checks.iter().any(|c| c.status == CheckStatus::Fail) {
        Err(fail("doctor found failures"))
    } else {
        println!("No failures.");
        Ok(())
    }
}
