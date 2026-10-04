#![cfg_attr(windows, windows_subsystem = "windows")]

mod app;
mod bench;
mod chart;
mod friendly;
mod live;
mod png;
mod profiles;
mod relaunch;
#[cfg(feature = "remote")]
mod remote;
mod settings;
mod simple;
mod state;
mod views;

use std::path::PathBuf;

#[derive(Debug, Default, PartialEq, Eq)]
pub struct Args {
    pub game_dir: Option<PathBuf>,
    /// Load this profile TOML instead of the last one (handy for screenshots and scripts).
    pub profile: Option<PathBuf>,
    pub compact: bool,
}

const USAGE: &str =
    "usage: deadtune [--game-dir <.../Deadlock>] [--profile <file.toml>] [--compact]";

fn parse_args(mut args: impl Iterator<Item = String>) -> Result<Args, String> {
    let mut out = Args::default();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--game-dir" => out.game_dir = Some(args.next().ok_or(USAGE)?.into()),
            "--profile" => out.profile = Some(args.next().ok_or(USAGE)?.into()),
            "--compact" => out.compact = true,
            _ => return Err(format!("unknown argument {arg}\n{USAGE}")),
        }
    }
    Ok(out)
}

fn env_path(name: &str) -> Option<PathBuf> {
    std::env::var_os(name)
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
}

fn main() -> anyhow::Result<()> {
    let args = parse_args(std::env::args().skip(1)).map_err(anyhow::Error::msg)?;
    let data_dir = env_path("DEADTUNE_DATA_DIR").unwrap_or_else(dt_core::backup::data_dir);
    let mut settings = settings::Settings::load(&data_dir);
    if let Some(dir) = args
        .game_dir
        .clone()
        .or_else(|| env_path("DEADTUNE_GAME_DIR"))
    {
        settings.game_dir = Some(dir);
    }
    let screenshot = env_path("DEADTUNE_SCREENSHOT");
    let compact = args.compact;
    let app = app::App::new(data_dir, settings, args, screenshot);
    let size = if compact {
        app::COMPACT_SIZE
    } else {
        app::FULL_SIZE
    };
    let mut viewport = eframe::egui::ViewportBuilder::default()
        .with_title("DeadTune")
        .with_inner_size(size)
        .with_min_inner_size([300.0, 300.0]);
    if compact {
        viewport = viewport.with_always_on_top();
    }
    let options = eframe::NativeOptions {
        viewport,
        ..Default::default()
    };
    eframe::run_native(
        "DeadTune",
        options,
        Box::new(|cc| Ok(Box::new(app.started(&cc.egui_ctx)))),
    )
    .map_err(|e| anyhow::anyhow!("{e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(list: &[&str]) -> Result<Args, String> {
        parse_args(list.iter().map(|s| s.to_string()))
    }

    #[test]
    fn parses_flags() {
        assert_eq!(
            args(&["--game-dir", "/g/Deadlock", "--compact"]).unwrap(),
            Args {
                game_dir: Some("/g/Deadlock".into()),
                profile: None,
                compact: true
            }
        );
        assert!(args(&["--game-dir"]).is_err(), "missing value");
        assert!(args(&["--bogus"]).unwrap_err().contains("--bogus"));
    }
}
