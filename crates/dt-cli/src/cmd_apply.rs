//! diff, apply, ranked-safe, restore, push.

use dt_core::apply::{self, ApplyContext, ApplyPlan, ApplyReport};
use dt_core::backup::FileKind;
use dt_core::bridge::{ConsoleCmd, clipboard};
use dt_core::catalog::{ApplyClass, Catalog};
use dt_core::hud::HudLayout;
use dt_core::locate::GamePaths;

use crate::args::{Args, CliResult, fail, usage};
use crate::env::{self, BridgeChoice, Env};

pub fn diff(env: &Env, args: &Args) -> CliResult {
    args.positionals::<0>("no positional arguments")?;
    let paths = env.paths()?;
    let profile = env::load_profile(&args.path("profile")?)?;
    let plan = env::profile_plan(env, &paths, &profile, args.switch("sandbox"))?;
    env::print_plan(&plan, true);
    Ok(())
}

pub fn apply(env: &Env, args: &Args) -> CliResult {
    args.positionals::<0>("no positional arguments")?;
    let paths = env.paths()?;
    let profile = env::load_profile(&args.path("profile")?)?;
    let running = dt_core::launch::is_game_running();
    let bridge = match args.value("bridge") {
        Some(text) => env::parse_bridge(text, true, false)?,
        None if running => BridgeChoice::ExecFile,
        None => BridgeChoice::None,
    };
    let plan = env::profile_plan(env, &paths, &profile, args.switch("sandbox"))?;
    println!(
        "Profile {:?} (base {})",
        profile.name,
        env::base_label(&profile.base)
    );
    run_plan(env, args, &paths, &plan, bridge, running)
}

pub fn ranked_safe(env: &Env, args: &Args) -> CliResult {
    args.positionals::<0>("no positional arguments")?;
    let paths = env.paths()?;
    let store = env.store()?;
    let live = env::read(&paths.gameinfo)?;
    let hud = apply::hud_plan(&paths, &HudLayout::default(), &store)?;
    let target = apply::ranked_safe_target(&live, &store, hud)?;
    let plan = apply::plan(
        &paths,
        &live,
        None,
        &target,
        Catalog::embedded(),
        ApplyContext::default(),
    )?;
    println!("Ranked-safe: stock ConVars block, video.txt kept, DeadTune HUD addon removed");
    run_plan(
        env,
        args,
        &paths,
        &plan,
        BridgeChoice::None,
        dt_core::launch::is_game_running(),
    )
}

fn run_plan(
    env: &Env,
    args: &Args,
    paths: &GamePaths,
    plan: &ApplyPlan,
    bridge: BridgeChoice,
    running: bool,
) -> CliResult {
    env::print_plan(plan, false);
    if plan.is_empty() {
        return Ok(());
    }
    if args.switch("dry-run") {
        println!("Dry run: nothing written.");
        return Ok(());
    }
    if !env::confirm(args, "Apply these changes?")? {
        println!("Cancelled.");
        return Ok(());
    }
    let mut bridge = env::open_bridge(bridge, paths)?;
    let report = apply::execute(
        paths,
        plan,
        &env.store()?,
        bridge
            .as_mut()
            .map(|b| &mut **b as &mut dyn dt_core::bridge::Bridge),
    )?;
    print_report(&report, running);
    match report.bridge_error {
        Some(e) => Err(fail(format!(
            "files written, but the live push failed: {e}. Retry with `deadtune-cli push`, or restart the game"
        ))),
        None => Ok(()),
    }
}

fn print_report(report: &ApplyReport, running: bool) {
    let wrote: Vec<&str> = [
        (report.wrote_gameinfo, "gameinfo.gi"),
        (report.wrote_video, "video.txt"),
        (report.hud_changed, "HUD addon"),
    ]
    .into_iter()
    .filter_map(|(done, name)| done.then_some(name))
    .collect();
    if wrote.is_empty() {
        println!("Wrote nothing.");
    } else {
        println!("Wrote {} (backups kept).", wrote.join(", "));
    }
    if report.pushed_live > 0 {
        println!("Pushed {} live command(s).", report.pushed_live);
    }
    if report.needs_restart {
        let when = if running {
            "Restart the game"
        } else {
            "Launch the game"
        };
        println!("{when} for the remaining changes to take effect.");
    }
}

pub fn restore(env: &Env, args: &Args) -> CliResult {
    args.positionals::<0>("no positional arguments")?;
    let paths = env.paths()?;
    let store = env.store()?;
    let kinds: Vec<FileKind> = match args.value("kind") {
        None => vec![FileKind::GameInfo, FileKind::Video],
        Some("gameinfo") => vec![FileKind::GameInfo],
        Some("video") => vec![FileKind::Video],
        Some(other) => return Err(usage(format!("unknown --kind {other:?}"))),
    };
    let modes = ["original", "latest", "list"];
    let chosen: Vec<&str> = modes.into_iter().filter(|m| args.switch(m)).collect();
    let [mode] = chosen[..] else {
        return Err(usage("pass exactly one of --original, --latest, --list"));
    };
    for kind in kinds {
        let live = match kind {
            FileKind::GameInfo => &paths.gameinfo,
            FileKind::Video => &paths.video,
        };
        let label = kind_label(kind);
        if mode == "list" {
            match store.original(kind) {
                Some(o) => println!(
                    "{label} original  {}  {}",
                    o.created.format("%Y-%m-%d %H:%M:%S"),
                    o.path.display()
                ),
                None => println!("{label} original  (none yet)"),
            }
            for b in store.list(kind)? {
                println!(
                    "{label} backup    {}  {}",
                    b.created.format("%Y-%m-%d %H:%M:%S"),
                    b.path.display()
                );
            }
            continue;
        }
        let entry = match mode {
            "original" => store.original(kind),
            _ => store.list(kind)?.into_iter().next(),
        };
        let Some(entry) = entry else {
            println!("{label}: no {mode} copy to restore");
            continue;
        };
        if live.is_file() {
            store.backup(kind, live)?;
        }
        store.restore(&entry, live)?;
        println!(
            "{label}: restored {} -> {}",
            entry.path.display(),
            live.display()
        );
    }
    Ok(())
}

fn kind_label(kind: FileKind) -> &'static str {
    match kind {
        FileKind::GameInfo => "gameinfo.gi",
        FileKind::Video => "video.txt",
    }
}

pub fn push(env: &Env, args: &Args) -> CliResult {
    if args.pos.is_empty() {
        return Err(usage("expected one or more name=value"));
    }
    let catalog = Catalog::embedded();
    let mut cmds = Vec::new();
    for item in &args.pos {
        let (name, value) = item
            .split_once('=')
            .ok_or_else(|| usage(format!("expected name=value, got {item:?}")))?;
        if catalog.is_denied(name) {
            return Err(fail(format!("{name} is on the denylist")));
        }
        if catalog.apply_class(name) == ApplyClass::Restart {
            eprintln!("note: {name} is restart-only; the console will likely reject it");
        }
        cmds.push(ConsoleCmd {
            name: name.to_string(),
            value: value.to_string(),
        });
    }
    let choice = env::parse_bridge(args.value("bridge").unwrap_or("execfile"), false, true)?;
    if choice == BridgeChoice::Clipboard {
        println!("{}", clipboard::batch_string(&cmds)?);
        return Ok(());
    }
    let paths = env.paths()?;
    let mut bridge = env::open_bridge(choice, &paths)?.expect("execfile and netcon open a bridge");
    bridge.push(&cmds)?;
    println!("Pushed {} command(s) via {}.", cmds.len(), bridge.name());
    if choice == BridgeChoice::ExecFile {
        println!(
            "Run `exec deadtune_live` in the console, or bind it once: {}",
            dt_core::bridge::execfile::ExecFileBridge::bind_hint("F8")
        );
    }
    Ok(())
}
