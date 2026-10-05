//! profile new | show | import-overrides | export-overrides

use std::collections::BTreeMap;
use std::path::Path;

use dt_core::addons::AddonsConfig;
use dt_core::catalog::Catalog;
use dt_core::gi::Override;
use dt_core::hud::HudLayout;
use dt_core::practice::PracticeMode;
use dt_core::profile::{self, ConVarEdits, Profile};

use crate::args::{Args, CliResult, fail};
use crate::env;

fn new_profile(
    file: &Path,
    args: &Args,
    convars: ConVarEdits,
) -> Result<Profile, crate::args::CliError> {
    let name = file
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("profile")
        .to_string();
    Ok(Profile {
        name,
        base: env::parse_base(args.required("base")?)?,
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

pub fn new(_: &env::Env, args: &Args) -> CliResult {
    let [file] = args.positionals("<file>")?;
    let file = Path::new(file);
    save(file, &new_profile(file, args, ConVarEdits::default())?)
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
    Ok(())
}

fn note(catalog: &Catalog, name: &str) -> String {
    match catalog.get(name) {
        Some(e) if e.denylist => "  (denylisted: will be refused)".into(),
        Some(e) if e.gameinfo_ignored => "  (the game ignores it in gameinfo.gi)".into(),
        Some(e) => format!("  ({:?})", e.apply),
        None => "  (not in catalog: restart)".into(),
    }
}

pub fn import_overrides(_: &env::Env, args: &Args) -> CliResult {
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
    save(file, &new_profile(file, args, convars)?)
}

pub fn export_overrides(_: &env::Env, args: &Args) -> CliResult {
    let [file, out] = args.positionals("<file> <overrides.gi>")?;
    let p = env::load_profile(Path::new(file))?;
    let out = Path::new(out);
    env::save_new(out, &profile::write_overrides_gi(&p.overrides()))?;
    println!("Wrote {}", out.display());
    Ok(())
}
