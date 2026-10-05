//! profile new | show | import-overrides | export-overrides, and practice on | off

use std::collections::BTreeMap;
use std::path::Path;

use dt_core::addons::AddonsConfig;
use dt_core::catalog::Catalog;
use dt_core::gi::Override;
use dt_core::hud::HudLayout;
use dt_core::practice::{Group, PracticeMode};
use dt_core::profile::{self, BaseRef, ConVarEdits, Profile};

use crate::args::{Args, CliResult, fail, usage};
use crate::{cmd_presets, env};

fn new_profile(
    env: &env::Env,
    file: &Path,
    args: &Args,
    convars: ConVarEdits,
) -> Result<Profile, crate::args::CliError> {
    let name = file
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("profile")
        .to_string();
    let base = env::parse_base(args.required("base")?)?;
    if let BaseRef::Preset(id) = base {
        cmd_presets::check_ready(env, id)?;
    }
    Ok(Profile {
        name,
        base,
        base_rev: None,
        convars,
        video: BTreeMap::new(),
        hud: HudLayout::default(),
        addons: AddonsConfig::default(),
        practice: PracticeMode::default(),
    })
}

fn save(file: &Path, profile: &Profile) -> CliResult {
    env::save_new(file, &profile.to_toml()?)?;
    println!("Wrote {}", file.display());
    Ok(())
}

pub fn new(env: &env::Env, args: &Args) -> CliResult {
    let [file] = args.positionals("<file>")?;
    let file = Path::new(file);
    save(file, &new_profile(env, file, args, ConVarEdits::default())?)
}

pub fn show(_: &env::Env, args: &Args) -> CliResult {
    let [file] = args.positionals("<file>")?;
    let p = env::load_profile(Path::new(file))?;
    let catalog = Catalog::embedded();
    println!("name: {}", p.name);
    println!("base: {}", env::base_label(&p.base));
    if let Some(rev) = &p.base_rev {
        println!("base_rev: {rev}");
    }
    for (name, value) in &p.convars.set {
        println!("set      {name} = {value}{}", note(catalog, name));
    }
    for name in &p.convars.comment {
        println!("comment  {name}{}", note(catalog, name));
    }
    for (key, value) in &p.video {
        println!("video    {key} = {value}");
    }
    for (id, edit) in &p.hud.elements {
        println!("hud      {id:?}: {edit:?}");
    }
    if p.practice.any() {
        println!(
            "practice {}  (matchmaking may refuse to queue)",
            practice_line(p.practice)
        );
    }
    Ok(())
}

/// `practice on|off [--shadows] [--fog] [--batching] --profile <file>`: no group flag means
/// every group. Saves the profile; `apply --profile` writes it into the game.
pub fn practice(_: &env::Env, args: &Args) -> CliResult {
    let [switch] = args.positionals("on or off")?;
    let on = match switch {
        "on" => true,
        "off" => false,
        other => return Err(usage(format!("expected on or off, got {other:?}"))),
    };
    let path = args.path("profile")?;
    let mut profile = env::load_profile(&path)?;
    let groups: Vec<Group> = Group::ALL
        .into_iter()
        .filter(|g| args.switch(g.id()))
        .collect();
    profile.practice = next_practice(profile.practice, on, &groups);
    let text = profile.to_toml()?;
    std::fs::write(&path, text).map_err(|e| fail(format!("{}: {e}", path.display())))?;
    println!(
        "Practice mode in {}: {}",
        path.display(),
        practice_line(profile.practice)
    );
    if profile.practice.any() {
        println!(
            "Deadlock may refuse to find matches while this is on; `deadtune-cli ranked-safe` puts the stock values back."
        );
    }
    println!(
        "Run `deadtune-cli apply --profile {}` to write it; takes effect next launch. Off puts back what the keys held before; ranked-safe resets them to stock.",
        path.display()
    );
    Ok(())
}

fn next_practice(current: PracticeMode, on: bool, groups: &[Group]) -> PracticeMode {
    let mut next = current;
    for group in Group::ALL {
        if groups.is_empty() || groups.contains(&group) {
            next.set(group, on);
        }
    }
    next
}

fn practice_line(mode: PracticeMode) -> String {
    Group::ALL
        .iter()
        .map(|g| format!("{} {}", g.id(), if mode.get(*g) { "on" } else { "off" }))
        .collect::<Vec<_>>()
        .join(", ")
}

fn note(catalog: &Catalog, name: &str) -> String {
    match catalog.get(name) {
        Some(e) if e.denylist => "  (denylisted: will be refused)".into(),
        Some(e) if e.gameinfo_ignored => "  (the game ignores it in gameinfo.gi)".into(),
        Some(e) => format!("  ({:?})", e.apply),
        None => "  (not in catalog: restart)".into(),
    }
}

pub fn import_overrides(env: &env::Env, args: &Args) -> CliResult {
    let [overrides, file] = args.positionals("<overrides.gi> <file>")?;
    let parsed = profile::parse_overrides_gi(&env::read(Path::new(overrides))?)
        .map_err(|e| fail(format!("{overrides}: {e}")))?;
    let mut convars = ConVarEdits::default();
    for (name, o) in parsed {
        match o {
            Override::Set(value) => {
                convars.set.insert(name, value);
            }
            Override::Comment => convars.comment.push(name),
        }
    }
    let file = Path::new(file);
    save(file, &new_profile(env, file, args, convars)?)
}

pub fn export_overrides(_: &env::Env, args: &Args) -> CliResult {
    let [file, out] = args.positionals("<file> <overrides.gi>")?;
    let p = env::load_profile(Path::new(file))?;
    let out = Path::new(out);
    env::save_new(out, &profile::write_overrides_gi(&p.overrides()))?;
    println!("Wrote {}", out.display());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_group_flag_means_every_group_and_flags_leave_the_rest_alone() {
        let all = next_practice(PracticeMode::default(), true, &[]);
        assert_eq!(all, PracticeMode::ALL_ON);
        let fog = next_practice(PracticeMode::default(), true, &[Group::Fog]);
        assert_eq!(
            fog,
            PracticeMode {
                fog: true,
                ..PracticeMode::default()
            }
        );
        let fog_off_shadows = next_practice(fog, false, &[Group::Fog]);
        assert!(fog_off_shadows.is_off());
        let keep = next_practice(all, false, &[Group::Shadows, Group::Batching]);
        assert_eq!(
            keep,
            PracticeMode {
                fog: true,
                ..PracticeMode::default()
            }
        );
        assert!(next_practice(all, false, &[]).is_off());
        assert_eq!(practice_line(keep), "shadows off, fog on, batching off");
    }
}
