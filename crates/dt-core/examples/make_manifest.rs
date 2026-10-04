//! Prints a release's `latest.json` for scripts/release-local.sh.
//!
//! cargo run -p dt-core --example make_manifest -- --channel stable|testing
//!   --version X.Y.Z --commit SHA --published RFC3339 --notes-url URL
//!   --asset <target>=<url>,<file>,<file.minisig> [--asset ...]

use std::collections::BTreeMap;
use std::error::Error;

use dt_core::backup::sha256_hex;
use dt_core::update::{Asset, Channel, Manifest, Target, Version};

const USAGE: &str = "usage: make_manifest --channel stable|testing --version X.Y.Z --commit SHA \
--published RFC3339 --notes-url URL --asset <target>=<url>,<file>,<file.minisig> [--asset ...]";

/// Parses a kebab-case enum value the way the manifest spells it.
fn enum_arg<T: serde::de::DeserializeOwned>(s: &str) -> Result<T, String> {
    serde_json::from_value(serde_json::Value::String(s.into()))
        .map_err(|_| format!("bad value {s:?}"))
}

fn asset_arg(spec: &str) -> Result<(Target, Asset), Box<dyn Error>> {
    let (target, rest) = spec.split_once('=').ok_or("--asset needs <target>=...")?;
    let [url, file, sig]: [&str; 3] = rest
        .split(',')
        .collect::<Vec<_>>()
        .try_into()
        .map_err(|_| "--asset needs <url>,<file>,<file.minisig>")?;
    let bytes = std::fs::read(file).map_err(|e| format!("{file}: {e}"))?;
    let asset = Asset {
        url: url.into(),
        size: bytes.len() as u64,
        sha256: sha256_hex(&bytes),
        minisig: std::fs::read_to_string(sig).map_err(|e| format!("{sig}: {e}"))?,
    };
    Ok((enum_arg(target)?, asset))
}

fn parse_args() -> Result<Manifest, Box<dyn Error>> {
    let (mut channel, mut version, mut commit, mut published, mut notes_url) =
        (None, None, None, None, None);
    let mut assets = BTreeMap::new();
    let mut it = std::env::args().skip(1);
    while let Some(flag) = it.next() {
        let value = it
            .next()
            .ok_or_else(|| format!("{flag} needs a value\n{USAGE}"))?;
        match flag.as_str() {
            "--channel" => channel = Some(enum_arg::<Channel>(&value)?),
            "--version" => version = Some(value.parse::<Version>()?),
            "--commit" => commit = Some(value),
            "--published" => published = Some(value),
            "--notes-url" => notes_url = Some(value),
            "--asset" => {
                let (target, asset) = asset_arg(&value)?;
                if assets.insert(target, asset).is_some() {
                    return Err(format!("duplicate --asset for {value}").into());
                }
            }
            _ => return Err(format!("unknown flag {flag}\n{USAGE}").into()),
        }
    }
    let missing = |name: &str| format!("missing {name}\n{USAGE}");
    if assets.is_empty() {
        return Err(missing("--asset").into());
    }
    Ok(Manifest {
        channel: channel.ok_or_else(|| missing("--channel"))?,
        version: version.ok_or_else(|| missing("--version"))?,
        commit: commit.ok_or_else(|| missing("--commit"))?,
        published: published.ok_or_else(|| missing("--published"))?,
        notes_url: notes_url.ok_or_else(|| missing("--notes-url"))?,
        assets,
    })
}

fn main() -> Result<(), Box<dyn Error>> {
    let manifest = parse_args()?;
    let json = serde_json::to_string_pretty(&manifest)?;
    let back: Manifest = serde_json::from_str(&json)?;
    assert_eq!(back, manifest, "latest.json does not round-trip");
    println!("{json}");
    Ok(())
}
