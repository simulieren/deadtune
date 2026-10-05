//! `deadtune-cli`: the DeadTune core as a small scriptable CLI.

mod args;
mod cmd_addons;
mod cmd_apply;
mod cmd_bench;
mod cmd_game;
mod cmd_hud;
mod cmd_images;
mod cmd_info;
mod cmd_presets;
mod cmd_profile;
mod cmd_snapshot;
mod cmd_texture;
mod cmd_update;
mod commands;
mod env;

use std::path::PathBuf;
use std::process::ExitCode;

use args::{Args, CliError, CliResult};
use commands::{COMMANDS, command_help, help};
use env::Env;

/// Pulls the global options out of argv, wherever they appear before `--`.
fn split_globals(argv: Vec<String>) -> Result<(Env, Vec<String>), CliError> {
    let mut game_dir = None;
    let mut data_dir = None;
    let mut rest = Vec::new();
    let mut iter = argv.into_iter();
    while let Some(arg) = iter.next() {
        if arg == "--" {
            rest.push(arg);
            rest.extend(iter);
            break;
        }
        let (name, inline) = match arg.split_once('=') {
            Some((n, v)) => (n.to_string(), Some(v.to_string())),
            None => (arg.clone(), None),
        };
        let slot = match name.as_str() {
            "--game-dir" => &mut game_dir,
            "--data-dir" => &mut data_dir,
            _ => {
                rest.push(arg);
                continue;
            }
        };
        let value = inline
            .or_else(|| iter.next())
            .ok_or_else(|| args::usage(format!("{name} needs a value")))?;
        *slot = Some(PathBuf::from(value));
    }
    let env = Env {
        game_dir,
        data_dir: data_dir.unwrap_or_else(env::default_data_dir),
    };
    Ok((env, rest))
}

fn run(argv: Vec<String>) -> CliResult {
    let (env, argv) = split_globals(argv)?;
    let words = argv.iter().take_while(|a| !a.starts_with('-'));
    let words: Vec<&str> = words.take(2).map(String::as_str).collect();
    let command = [
        words.join(" "),
        words.first().copied().unwrap_or("").to_string(),
    ]
    .iter()
    .find_map(|name| COMMANDS.iter().find(|c| c.name == name));
    let wants_help = argv
        .iter()
        .take_while(|a| *a != "--")
        .any(|a| a == "-h" || a == "--help");
    let Some(command) = command else {
        if wants_help || argv.is_empty() {
            println!("{}", help());
            return Ok(());
        }
        return Err(args::usage(format!(
            "unknown command {:?}; see --help",
            words.join(" ")
        )));
    };
    if wants_help {
        println!("{}", command_help(command));
        return Ok(());
    }
    let skip = command.name.split(' ').count();
    Args::parse(&argv[skip..], command.values, command.switches)
        .and_then(|parsed| (command.run)(&env, &parsed))
        .map_err(|e| match e {
            CliError::Usage(msg) => {
                CliError::Usage(format!("{msg}\nusage: deadtune-cli {}", command.usage))
            }
            other => other,
        })
}

fn main() -> ExitCode {
    match run(std::env::args().skip(1).collect()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(CliError::Usage(msg)) => {
            eprintln!("error: {msg}");
            ExitCode::from(2)
        }
        Err(CliError::Fail(e)) => {
            eprintln!("error: {e:#}");
            ExitCode::from(1)
        }
    }
}
