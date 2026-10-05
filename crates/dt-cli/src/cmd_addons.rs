//! addons list | enable | disable | import | fetch | build

use std::path::Path;

use dt_core::addons::install::{self, Action, InstalledState};
use dt_core::addons::{self, AddonId, Kind, Native, Source, sources, textures};
use dt_core::locate::GamePaths;

use crate::args::{Args, CliError, CliResult, fail, usage};
use crate::env::{self, Env};

fn parse_id(text: &str) -> Result<AddonId, CliError> {
    AddonId::parse(text).ok_or_else(|| {
        let ids: Vec<&str> = AddonId::ALL.iter().map(|id| id.key()).collect();
        usage(format!("unknown addon {text:?}; one of {}", ids.join(", ")))
    })
}

fn source_status(env: &Env, id: AddonId) -> String {
    match addons::info(id).source {
        Source::Generated => "nothing to download".to_string(),
        Source::Upstream { url, .. } => {
            match sources::cached(&sources::cache_dir(&env.data_dir), id) {
                Ok(Some(_)) => "downloaded".to_string(),
                Ok(None) => format!("needs download: {url}"),
                Err(e) => format!("cache unreadable: {e}"),
            }
        }
    }
}

pub fn list(env: &Env, args: &Args) -> CliResult {
    args.positionals::<0>("no positional arguments")?;
    let profile = args
        .value("profile")
        .map(|f| env::load_profile(Path::new(f)))
        .transpose()?;
    let states = env
        .paths()
        .ok()
        .and_then(|paths| install::installed_state(&paths, &env.data_dir).ok())
        .unwrap_or_default();
    for a in addons::all() {
        let enabled = match &profile {
            Some(p) if p.addons.is_enabled(a.id) => "on ",
            Some(_) => "off",
            None => "   ",
        };
        let installed = match states.get(&a.id) {
            None | Some(InstalledState::None) => "not installed",
            Some(InstalledState::Current(_)) => "installed",
            Some(InstalledState::Stale(_)) => "installed, stale after game update",
            Some(InstalledState::Foreign(_)) => "file replaced by another program",
        };
        let kind = match a.kind {
            Kind::Toggle => "on/off, the author's file",
            Kind::Native(Native::Particles) => "per effect group, rebuilt from your game files",
            Kind::Native(Native::Blur) => "HUD and menu blur, rebuilt from your game files",
            Kind::Native(Native::Sinner) => "on/off, rebuilt from your game files",
            Kind::Native(Native::Scope) => "scope size, rebuilt from your game files",
            Kind::Native(Native::Clutter) => "per effect group, rebuilt from your game files",
            Kind::Textures => "built on demand",
        };
        println!("{enabled} {:<20} {:<40} {}", a.id.key(), a.name, a.credit());
        println!(
            "    {kind}; {installed}; {}\n    {}",
            source_status(env, a.id),
            a.description
        );
    }
    println!(
        "\nParticle groups (addons enable particle_disabler, then --keep <group> to leave one visible):"
    );
    for g in addons::particles::GROUPS {
        println!("    {:<12} {}", g.id, g.label);
    }
    println!(
        "\nClutter groups (addons enable clutter_remover --hide <group>[,<group>]; {} when none given):",
        addons::clutter::CITY
    );
    for g in &addons::clutter::GROUPS {
        println!("    {:<12} {}: {}", g.id, g.label, g.detail);
        if let Some(warning) = g.warning {
            println!("    {:<12} {warning}", "");
        }
    }
    Ok(())
}

fn set_enabled(args: &Args, on: bool) -> CliResult {
    let [id] = args.positionals("<addon id>")?;
    let id = parse_id(id)?;
    let path = args.path("profile")?;
    let mut profile = env::load_profile(&path)?;
    profile.addons.set_enabled(id, on);
    if on && id == AddonId::ParticleDisabler {
        for group in args.value("keep").into_iter().flat_map(|k| k.split(',')) {
            let group = group.trim();
            if addons::particles::group(group).is_none() {
                return Err(usage(format!("unknown particle group {group:?}")));
            }
            profile.addons.keep_particles.insert(group.to_string());
        }
    }
    if on && id == AddonId::ClutterRemover {
        let picked: Vec<&str> = args
            .value("hide")
            .into_iter()
            .flat_map(|k| k.split(','))
            .map(str::trim)
            .collect();
        if let Some(bad) = picked.iter().find(|g| addons::clutter::group(g).is_none()) {
            return Err(usage(format!("unknown clutter group {bad:?}")));
        }
        if !picked.is_empty() {
            profile.addons.hide_clutter = picked.into_iter().map(String::from).collect();
        }
    }
    let text = profile.to_toml()?;
    std::fs::write(&path, text).map_err(|e| fail(format!("{}: {e}", path.display())))?;
    println!(
        "{} {} in {}. Run `deadtune-cli apply --profile {}` to {} it.",
        addons::info(id).name,
        if on { "enabled" } else { "disabled" },
        path.display(),
        path.display(),
        if on { "install" } else { "remove" }
    );
    Ok(())
}

pub fn enable(_: &Env, args: &Args) -> CliResult {
    set_enabled(args, true)
}

pub fn disable(_: &Env, args: &Args) -> CliResult {
    set_enabled(args, false)
}

pub fn import(env: &Env, args: &Args) -> CliResult {
    let [file] = args.positionals("<file or folder>")?;
    let ids = sources::import(&sources::cache_dir(&env.data_dir), Path::new(file))?;
    for id in ids {
        println!("imported {} ({})", addons::info(id).name, id.key());
    }
    Ok(())
}

#[cfg(feature = "fetch")]
pub fn fetch(env: &Env, args: &Args) -> CliResult {
    let [which] = args.positionals("<addon id> or all")?;
    let ids: Vec<AddonId> = if which == "all" {
        AddonId::ALL.to_vec()
    } else {
        vec![parse_id(which)?]
    };
    let cache = sources::cache_dir(&env.data_dir);
    for id in ids {
        if !matches!(addons::info(id).source, Source::Upstream { .. }) {
            continue;
        }
        let got = sources::fetch(&cache, id)?;
        println!(
            "fetched {} -> {}",
            addons::info(id).name,
            got.path.display()
        );
    }
    Ok(())
}

#[cfg(not(feature = "fetch"))]
pub fn fetch(_: &Env, _: &Args) -> CliResult {
    Err(fail(
        "this build has no downloader; only the soul container needs one: download its pak01_dir.vpk from https://github.com/Sqooky/OptimizationLock and run `addons import <file>`",
    ))
}

/// Builds the texture downscaler pak for the profile's settings, printing progress.
pub fn build(env: &Env, args: &Args) -> CliResult {
    args.positionals::<0>("no positional arguments")?;
    let profile = env::load_profile(&args.path("profile")?)?;
    let paths = env.paths()?;
    let mut last = 0;
    let stats = install::build_textures(&paths, &profile.addons, &env.data_dir, &mut |p| {
        if p.done * 20 / p.total.max(1) != last {
            last = p.done * 20 / p.total.max(1);
            println!("{}/{} {}", p.done, p.total, p.current);
        }
        std::ops::ControlFlow::Continue(())
    })?;
    println!("{}", textures::summary(&stats));
    for (reason, n) in &stats.skipped {
        println!("    skipped {n}: {reason:?}");
    }
    Ok(())
}

/// Reads every installed DeadTune pak back; exits 1 when one fails its check.
pub fn verify(env: &Env, args: &Args) -> CliResult {
    args.positionals::<0>("no positional arguments")?;
    let paths = env.paths()?;
    let reports = addons::verify::verify_installed(&paths, &env.data_dir);
    if reports.is_empty() {
        println!("no DeadTune paks installed");
        return Ok(());
    }
    let mut failed = 0;
    for r in &reports {
        println!("{r}");
        if !r.result.as_ref().is_ok_and(|v| v.is_ok()) {
            failed += 1;
        }
    }
    if failed > 0 {
        return Err(fail(format!(
            "{failed} pak(s) failed; remove them from the Addons page (or `addons disable` + apply) and rebuild"
        )));
    }
    println!("All {} pak(s) read back clean.", reports.len());
    Ok(())
}

/// One-line addon facts for `status` and `doctor`.
pub fn report_lines(env: &Env, paths: &GamePaths) -> Vec<String> {
    let states = match install::installed_state(paths, &env.data_dir) {
        Ok(s) => s,
        Err(e) => return vec![format!("addons: unreadable state: {e}")],
    };
    if states.is_empty() {
        return vec!["addons: none installed".to_string()];
    }
    states
        .iter()
        .map(|(id, state)| {
            let what = match state {
                InstalledState::None => "record only, file missing".to_string(),
                InstalledState::Current(r) => format!("installed as {}", r.file),
                InstalledState::Stale(r) => {
                    format!("installed as {}, stale after game update; re-apply", r.file)
                }
                InstalledState::Foreign(file) => format!("{file} is not what DeadTune wrote"),
            };
            format!("addon {}: {what}", id.key())
        })
        .collect()
}

pub fn describe_action(action: &Action) -> String {
    match action {
        Action::Write(_) => "write".to_string(),
        Action::Remove => "remove".to_string(),
        Action::Keep => "unchanged".to_string(),
        Action::Build => "needs `addons build`".to_string(),
        Action::Unavailable(blocker) => match blocker {
            install::Blocker::NotDownloaded => {
                "needs download (`addons fetch` or `addons import`)".to_string()
            }
            install::Blocker::GameFiles(e) => format!("game files unreadable: {e}"),
            install::Blocker::Invalid(e) => {
                format!("built pak failed its check, not installed: {e}")
            }
        },
    }
}
