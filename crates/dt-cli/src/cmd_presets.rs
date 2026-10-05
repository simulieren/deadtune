//! presets, presets fetch | import | accept | discard

use std::path::{Path, PathBuf};

use dt_core::preset::remote::{self, Active, Status};
use dt_core::preset::{self, PresetId, Remote, Source};

use crate::args::{Args, CliError, CliResult, fail, usage};
use crate::env::Env;

fn remote_dir(env: &Env, r: &Remote) -> PathBuf {
    remote::dir(r, &preset::cache_dir(&env.data_dir))
}

fn state_label(status: &Status) -> &'static str {
    match (status.active, status.pending.is_some()) {
        (None, false) => "not downloaded",
        (None, true) => "downloaded, waiting for review",
        (Some(Active::Pinned), false) => "ready",
        (Some(Active::Accepted), false) => "ready (accepted update)",
        (Some(_), true) => "ready, update waiting for review",
    }
}

pub fn list(env: &Env, args: &Args) -> CliResult {
    args.positionals::<0>("no positional arguments")?;
    for p in preset::all() {
        let extra = match &p.source {
            Source::Pinned(x) if x.video.is_some() => "  +video.txt".to_string(),
            Source::Pinned(_) => String::new(),
            Source::Remote(r) => {
                let status = remote::status(r, &remote_dir(env, r))?;
                format!("  {}, {}: {}", r.licence, r.page, state_label(&status))
            }
        };
        println!("{:<22} {:<28} by {}{extra}", p.id.key(), p.label, p.author);
    }
    Ok(())
}

fn remote_arg(args: &Args, what: &str) -> Result<&'static Remote, CliError> {
    let name = args.pos.first().map(String::as_str).unwrap_or("");
    let remotes = || preset::all().iter().filter_map(|p| p.remote());
    remotes().find(|r| r.id.key() == name).ok_or_else(|| {
        let keys: Vec<_> = remotes().map(|r| r.id.key()).collect();
        usage(format!(
            "{what}: expected one of {}, got {name:?}",
            keys.join(", ")
        ))
    })
}

/// Errors for a profile base that cannot be resolved yet, with the command that fixes it.
pub fn check_ready(env: &Env, id: PresetId) -> CliResult {
    let Some(r) = preset::info(id).remote() else {
        return Ok(());
    };
    let status = remote::status(r, &remote_dir(env, r))?;
    let key = id.key();
    match (status.ready(), status.pending.is_some()) {
        (true, _) => Ok(()),
        (false, true) => Err(fail(format!(
            "{} has a file that differs from the one DeadTune checked: run `presets accept {key}` to use it, or `presets discard {key}`",
            preset::info(id).label
        ))),
        (false, false) => Err(fail(format!(
            "{} isn't downloaded yet: run `presets fetch {key}`, or download cfg.zip from {} and run `presets import {key} <cfg.zip or gameinfo.gi>`",
            preset::info(id).label,
            r.page
        ))),
    }
}

fn report(env: &Env, r: &Remote, status: &Status) -> CliResult {
    let key = r.id.key();
    let label = preset::info(r.id).label;
    if status.pending.is_none() {
        println!("{label}: {}", state_label(status));
        return Ok(());
    }
    let changes = remote::pending_changes(&remote_dir(env, r))?;
    match status.active {
        Some(_) => println!("{label} was updated upstream: review and accept."),
        None => {
            println!("This {label} file differs from the file DeadTune checked: review and accept.")
        }
    }
    println!("{} settings change:", changes.len());
    let show = |v: &Option<String>| v.clone().unwrap_or_else(|| "(not set)".into());
    for c in &changes {
        println!("  {}: {} -> {}", c.name, show(&c.before), show(&c.after));
    }
    println!(
        "Run `deadtune-cli presets accept {key}` to use it, or `deadtune-cli presets discard {key}` to keep {}.",
        if status.active.is_some() {
            "the current one"
        } else {
            "nothing"
        }
    );
    Ok(())
}

pub fn import(env: &Env, args: &Args) -> CliResult {
    let [_, file] = args.positionals("<preset> <cfg.zip or gameinfo.gi>")?;
    let r = remote_arg(args, "presets import")?;
    let bytes = std::fs::read(Path::new(file)).map_err(|e| fail(format!("{file}: {e}")))?;
    let status = remote::stage(r, &remote_dir(env, r), &bytes)?;
    report(env, r, &status)
}

#[cfg(feature = "fetch")]
pub fn fetch(env: &Env, args: &Args) -> CliResult {
    args.positionals::<1>("<preset>")?;
    let r = remote_arg(args, "presets fetch")?;
    let status = remote::fetch(r, &remote_dir(env, r))?;
    report(env, r, &status)
}

#[cfg(not(feature = "fetch"))]
pub fn fetch(_: &Env, args: &Args) -> CliResult {
    args.positionals::<1>("<preset>")?;
    let r = remote_arg(args, "presets fetch")?;
    Err(fail(format!(
        "this build has no downloader: download cfg.zip from {} and run `presets import {} <file>`",
        r.page,
        r.id.key()
    )))
}

pub fn accept(env: &Env, args: &Args) -> CliResult {
    args.positionals::<1>("<preset>")?;
    let r = remote_arg(args, "presets accept")?;
    remote::accept(&remote_dir(env, r))?;
    println!(
        "{}: now using the accepted update",
        preset::info(r.id).label
    );
    Ok(())
}

pub fn discard(env: &Env, args: &Args) -> CliResult {
    args.positionals::<1>("<preset>")?;
    let r = remote_arg(args, "presets discard")?;
    remote::discard(&remote_dir(env, r))?;
    let status = remote::status(r, &remote_dir(env, r))?;
    println!("{}: {}", preset::info(r.id).label, state_label(&status));
    Ok(())
}
