//! watch, launch, kill

use std::sync::mpsc;

use dt_core::hud::install::{self, InstalledState};
use dt_core::launch::{self, LaunchOptions};
use dt_core::watch::{self, Change};

use crate::args::{Args, CliResult, fail};
use crate::env::Env;

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

pub fn launch(_: &Env, args: &Args) -> CliResult {
    args.positionals::<0>("game arguments after --")?;
    let opts = LaunchOptions {
        args: args.rest.clone(),
    };
    println!("Opening {}", launch::steam_url(&opts));
    launch::launch(&opts)?;
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
