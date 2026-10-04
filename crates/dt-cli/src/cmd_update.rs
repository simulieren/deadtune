//! self-update

use std::path::PathBuf;

use dt_core::update::Channel;

use crate::args::{Args, CliError, CliResult, fail, usage};
use crate::env::Env;

pub fn parse_channel(text: Option<&str>) -> Result<Channel, CliError> {
    match text {
        None | Some("stable") => Ok(Channel::Stable),
        Some("testing") => Ok(Channel::Testing),
        Some(other) => Err(usage(format!(
            "--channel is stable or testing, not {other:?}"
        ))),
    }
}

/// `DeadTune 0.2.0 (abc1234), stable channel`
fn build_line(channel: Channel) -> String {
    let channel = match channel {
        Channel::Stable => "stable",
        Channel::Testing => "testing",
    };
    let commit = option_env!("DEADTUNE_COMMIT").map_or("dev build", short);
    format!(
        "DeadTune {} ({commit}), {channel} channel",
        env!("CARGO_PKG_VERSION")
    )
}

fn short(commit: &str) -> &str {
    &commit[..commit.len().min(7)]
}

/// Releases publish the GUI executable, so that is what gets replaced: `--exe`, else
/// `deadtune` next to this CLI or one folder up (the release zip keeps the CLI in `tools/`).
#[cfg_attr(not(feature = "fetch"), allow(dead_code))]
fn gui_exe(args: &Args) -> Result<PathBuf, CliError> {
    if let Some(path) = args.value("exe") {
        return Ok(PathBuf::from(path));
    }
    let name = format!("deadtune{}", std::env::consts::EXE_SUFFIX);
    let cli = std::env::current_exe()?;
    cli.ancestors()
        .skip(1)
        .take(2)
        .map(|dir| dir.join(&name))
        .find(|p| p.is_file())
        .ok_or_else(|| {
            fail(format!(
                "couldn't find {name} next to deadtune-cli or one folder up; pass --exe <path>"
            ))
        })
}

#[cfg(feature = "fetch")]
pub fn self_update(_: &Env, args: &Args) -> CliResult {
    use dt_core::update::{self, Current, Offer};

    args.positionals::<0>("no positional arguments")?;
    let channel = parse_channel(args.value("channel"))?;
    println!("{}", build_line(channel));
    let current =
        Current::this_build().ok_or_else(|| fail("no updates are published for this platform"))?;
    let manifest = update::fetch_manifest(channel)?;
    let Offer::Available {
        version,
        commit,
        notes_url,
        asset,
    } = update::decide(&current, &manifest, None)
    else {
        println!(
            "Up to date (latest is {} {}).",
            manifest.version,
            short(&manifest.commit)
        );
        return Ok(());
    };
    println!(
        "DeadTune {version} ({}) is available: {notes_url}",
        short(&commit)
    );
    if args.switch("check") {
        return Ok(());
    }
    let exe = gui_exe(args)?;
    if !crate::env::confirm(
        args,
        &format!("Install DeadTune {version} over {}?", exe.display()),
    )? {
        println!("Cancelled.");
        return Ok(());
    }
    println!("Downloading {:.1} MB...", asset.size as f64 / 1e6);
    let bytes = update::download(&asset, |_, _| {})?;
    update::verify(
        &bytes,
        &asset,
        version,
        &commit,
        current.target,
        update::RELEASE_PUBKEY,
    )?;
    println!("Verified size, SHA-256 and the release signature.");
    let old = update::install(&exe, &bytes)?;
    println!(
        "Installed DeadTune {version} at {}. The previous version is at {} until DeadTune next starts.",
        exe.display(),
        old.display()
    );
    Ok(())
}

#[cfg(not(feature = "fetch"))]
pub fn self_update(_: &Env, args: &Args) -> CliResult {
    println!("{}", build_line(parse_channel(args.value("channel"))?));
    Err(fail(
        "this build has no downloader; get the latest DeadTune from https://github.com/simulieren/deadtune/releases",
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::COMMANDS;

    fn parse(items: &[&str]) -> Result<Args, CliError> {
        let command = COMMANDS.iter().find(|c| c.name == "self-update").unwrap();
        let argv: Vec<String> = items.iter().map(|s| s.to_string()).collect();
        Args::parse(&argv, command.values, command.switches)
    }

    #[test]
    fn self_update_takes_check_channel_exe_and_yes() {
        let args = parse(&["--check", "--channel", "testing", "--yes"])
            .ok()
            .unwrap();
        assert!(args.switch("check") && args.switch("yes"));
        assert_eq!(
            parse_channel(args.value("channel")).ok(),
            Some(Channel::Testing)
        );
        let args = parse(&["--exe=C:/DeadTune/deadtune.exe"]).ok().unwrap();
        assert_eq!(
            gui_exe(&args).ok(),
            Some(PathBuf::from("C:/DeadTune/deadtune.exe"))
        );
        assert_eq!(parse_channel(None).ok(), Some(Channel::Stable));
        assert!(
            matches!(parse_channel(Some("beta")), Err(CliError::Usage(m)) if m.contains("beta"))
        );
        assert!(matches!(parse(&["--force"]), Err(CliError::Usage(_))));
        assert!(matches!(parse(&["--check=1"]), Err(CliError::Usage(_))));
    }
}
