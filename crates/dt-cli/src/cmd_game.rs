//! watch, launch, kill

use std::path::Path;
use std::sync::mpsc;

use dt_core::bridge::{ConsoleCmd, boot};
use dt_core::catalog::{ApplyClass, Catalog};
use dt_core::hud::install::{self, InstalledState};
use dt_core::launch;
use dt_core::launch_options::{self, LaunchOptions, Verdict};
use dt_core::watch::{self, Change};

use crate::args::{Args, CliResult, fail, usage};
use crate::env::{self, Env};

pub fn watch(env: &Env, args: &Args) -> CliResult {
    args.positionals::<0>("no positional arguments")?;
    let paths = env.paths()?;
    let baseline = watch::fingerprint(&paths);
    let (tx, rx) = mpsc::channel();
    let _watcher = watch::spawn(paths.clone(), baseline, tx)?;
    println!("Watching {} (Ctrl-C to stop)", paths.citadel_dir.display());
    for changes in rx {
        for change in changes {
            match change {
                Change::GameUpdated { from, to } => {
                    println!(
                        "game updated: build {} -> {}; re-apply your profile",
                        from.as_deref().unwrap_or("?"),
                        to.as_deref().unwrap_or("?")
                    );
                    let store = env.store()?;
                    if let Ok(InstalledState::Stale(_)) =
                        install::installed_state(&paths, &store.root)
                    {
                        println!(
                            "HUD addon was built for the old build; run `deadtune-cli hud apply`"
                        );
                    }
                }
                Change::GameInfoChanged => println!("gameinfo.gi changed"),
                Change::VideoChanged => println!("video.txt changed"),
            }
        }
    }
    Ok(())
}

/// Writes `cfg/deadtune_boot.cfg` and starts the game with `+exec deadtune_boot -condebug`.
/// With `--profile` the boot cfg carries that profile's live convars; without one an existing
/// boot cfg (the GUI's, with its convars) is kept and a bare one is written only if missing.
pub fn launch(env: &Env, args: &Args) -> CliResult {
    args.positionals::<0>("game arguments after --")?;
    let paths = env.paths()?;
    let profile = args
        .value("profile")
        .map(|p| env::load_profile(Path::new(p)))
        .transpose()?;
    let boot_path = paths.cfg_dir.join(boot::FILE_NAME);
    if profile.is_some() || !boot_path.exists() {
        let catalog = Catalog::embedded();
        let live = profile
            .iter()
            .flat_map(|p| p.convars.set.iter())
            .filter(|(name, _)| {
                catalog.apply_class(name) == ApplyClass::Live && !catalog.is_denied(name)
            })
            .map(|(name, value)| ConsoleCmd {
                name: name.clone(),
                value: value.clone(),
            })
            .collect();
        boot::BootCfg {
            bind_key: "F8".into(),
            live,
            version: env!("CARGO_PKG_VERSION").into(),
        }
        .write(&paths.cfg_dir)?;
        println!("Wrote {}", boot_path.display());
    }
    let opts = LaunchOptions::from_args(&args.rest);
    for checked in launch_options::check(&opts.args()) {
        if matches!(
            checked.verdict,
            Verdict::Unknown | Verdict::Conflicts { .. }
        ) {
            eprintln!(
                "warning: {}: {}",
                checked.arg.text(),
                checked.verdict.summary()
            );
        }
    }
    let opts = launch::with_boot(&opts.args(), args.switch("console"));
    println!("Opening {}", launch::steam_url(&opts));
    launch::launch(&opts)?;
    Ok(())
}

/// One line per option with its verdict; exits 1 when two options contradict each other.
pub fn launch_options_check(_: &Env, args: &Args) -> CliResult {
    let tokens = match (args.pos.as_slice(), args.rest.is_empty()) {
        ([line], true) => launch_options::split_command_line(line),
        ([], false) => args.rest.clone(),
        _ => return Err(usage("pass the options as one quoted string, or after --")),
    };
    let checked = launch_options::check(&tokens);
    let width = checked
        .iter()
        .map(|c| c.arg.text().len())
        .max()
        .unwrap_or(0);
    for c in &checked {
        println!("{:width$}  {}", c.arg.text(), c.verdict.summary());
    }
    if checked
        .iter()
        .any(|c| matches!(c.verdict, Verdict::Conflicts { .. }))
    {
        return Err(fail("some options contradict each other"));
    }
    Ok(())
}

pub fn kill(_: &Env, args: &Args) -> CliResult {
    args.positionals::<0>("no positional arguments")?;
    if launch::kill_game()? {
        println!("Deadlock stopped.");
        Ok(())
    } else {
        Err(fail("Deadlock is not running"))
    }
}
