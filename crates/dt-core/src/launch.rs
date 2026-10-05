//! Start/stop the game through Steam. Never touches the game process beyond kill.

use std::process::Command;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use sysinfo::{Process, ProcessRefreshKind, ProcessesToUpdate, System};

use crate::locate::APP_ID;

/// The final command line handed to Steam. Build it from `launch_options::LaunchOptions::args`.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct LaunchArgs {
    pub args: Vec<String>,
}

/// The config `+exec` runs at start, and the flag that makes the game write `console.log`.
pub const BOOT_CFG: &str = "deadtune_boot";
pub const CONDEBUG: &str = "-condebug";

/// `steam://rungameid/1422450` or `steam://run/1422450//<args>/` when args are set.
///
/// The args are one command line: args containing whitespace are double-quoted, then everything
/// outside the URL unreserved set is percent-encoded so `+`, `/`, `&` and `%` survive the shell and Steam.
pub fn steam_url(opts: &LaunchArgs) -> String {
    if opts.args.is_empty() {
        return format!("steam://rungameid/{APP_ID}");
    }
    let mut encoded = String::new();
    for byte in command_line(&opts.args).bytes() {
        if byte.is_ascii_alphanumeric() || b"-_.~".contains(&byte) {
            encoded.push(byte as char);
        } else {
            encoded.push_str(&format!("%{byte:02X}"));
        }
    }
    format!("steam://run/{APP_ID}//{encoded}/")
}

/// Args joined with spaces, double-quoting any that contain whitespace: what Steam's Launch
/// Options box takes, and what `launch_options::split_command_line` reads back.
pub fn command_line(args: &[String]) -> String {
    args.iter()
        .map(|arg| {
            if arg.contains(char::is_whitespace) {
                format!("\"{arg}\"")
            } else {
                arg.clone()
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// The user's options plus what the ack loop needs: `+exec deadtune_boot`, `-condebug`, and
/// `-console` when asked. Arguments the user already lists are not repeated.
pub fn with_boot(args: &[String], console: bool) -> LaunchArgs {
    let mut args = args.to_vec();
    let has_exec_boot = args
        .windows(2)
        .any(|w| w[0] == "+exec" && w[1].trim_end_matches(".cfg") == BOOT_CFG);
    if !has_exec_boot {
        args.extend(["+exec".to_string(), BOOT_CFG.to_string()]);
    }
    for flag in [CONDEBUG].into_iter().chain(console.then_some("-console")) {
        if !args.iter().any(|a| a == flag) {
            args.push(flag.to_string());
        }
    }
    LaunchArgs { args }
}

pub fn is_game_running() -> bool {
    game_processes(&process_snapshot()).next().is_some()
}

/// Process start time of the running game, used by the relaunch countdown.
pub fn game_started_at() -> Option<SystemTime> {
    let system = process_snapshot();
    let earliest = game_processes(&system).map(|p| p.start_time()).min()?;
    Some(UNIX_EPOCH + Duration::from_secs(earliest))
}

pub fn kill_game() -> std::io::Result<bool> {
    let system = process_snapshot();
    let mut found = false;
    let mut failed = Vec::new();
    for process in game_processes(&system) {
        found = true;
        if !process.kill() {
            failed.push(process.pid().to_string());
        }
    }
    if failed.is_empty() {
        Ok(found)
    } else {
        Err(std::io::Error::other(format!(
            "could not kill game process {}",
            failed.join(", ")
        )))
    }
}

pub fn launch(opts: &LaunchArgs) -> std::io::Result<()> {
    let url = steam_url(opts);
    open_url(&url)
}

#[cfg(windows)]
fn open_url(url: &str) -> std::io::Result<()> {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    // raw_arg keeps Rust's argv quoting away from cmd's own parser; the empty "" is start's window title.
    Command::new("cmd")
        .raw_arg(format!("/C start \"\" \"{url}\""))
        .creation_flags(CREATE_NO_WINDOW)
        .status()
        .and_then(check_opener)
}

#[cfg(target_os = "macos")]
fn open_url(url: &str) -> std::io::Result<()> {
    Command::new("open")
        .arg(url)
        .status()
        .and_then(check_opener)
}

#[cfg(not(any(windows, target_os = "macos")))]
fn open_url(url: &str) -> std::io::Result<()> {
    Command::new("xdg-open")
        .arg(url)
        .status()
        .and_then(check_opener)
}

fn check_opener(status: std::process::ExitStatus) -> std::io::Result<()> {
    if status.success() {
        Ok(())
    } else {
        Err(std::io::Error::other(format!(
            "URL opener exited with {status}"
        )))
    }
}

/// Name and start time are always read; nothing else is needed.
fn process_snapshot() -> System {
    let mut system = System::new();
    system.refresh_processes_specifics(ProcessesToUpdate::All, true, ProcessRefreshKind::nothing());
    system
}

fn game_processes(system: &System) -> impl Iterator<Item = &Process> {
    system
        .processes()
        .values()
        .filter(|p| is_game_process(&p.name().to_string_lossy()))
}

/// Windows and Proton both report `deadlock.exe` (any case); a native Linux build would be `deadlock`.
fn is_game_process(name: &str) -> bool {
    name.eq_ignore_ascii_case("deadlock.exe") || name.eq_ignore_ascii_case("deadlock")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn opts(args: &[&str]) -> LaunchArgs {
        LaunchArgs { args: strs(args) }
    }

    fn strs(args: &[&str]) -> Vec<String> {
        args.iter().map(|a| a.to_string()).collect()
    }

    #[test]
    fn steam_url_without_args_uses_rungameid() {
        assert_eq!(steam_url(&opts(&[])), "steam://rungameid/1422450");
    }

    #[test]
    fn steam_url_with_args_uses_run_form_and_encodes_separators() {
        assert_eq!(
            steam_url(&opts(&["-novid", "-high", "+fps_max", "0"])),
            "steam://run/1422450//-novid%20-high%20%2Bfps_max%200/"
        );
    }

    #[test]
    fn steam_url_quotes_args_with_spaces_and_escapes_url_metacharacters() {
        assert_eq!(
            steam_url(&opts(&["+exec", "my cfg.cfg", "a/b&c"])),
            "steam://run/1422450//%2Bexec%20%22my%20cfg.cfg%22%20a%2Fb%26c/"
        );
    }

    #[test]
    fn with_boot_appends_the_boot_exec_and_log_flags_once() {
        assert_eq!(
            with_boot(&strs(&["-novid"]), false).args,
            ["-novid", "+exec", "deadtune_boot", "-condebug"]
        );
        assert_eq!(
            with_boot(&strs(&["-novid"]), true).args,
            ["-novid", "+exec", "deadtune_boot", "-condebug", "-console"]
        );
        assert_eq!(
            with_boot(
                &strs(&["+exec", "deadtune_boot", "-condebug", "-console"]),
                true
            )
            .args,
            ["+exec", "deadtune_boot", "-condebug", "-console"],
            "nothing is repeated"
        );
        assert_eq!(
            steam_url(&with_boot(&strs(&[]), false)),
            "steam://run/1422450//%2Bexec%20deadtune_boot%20-condebug/"
        );
    }

    #[test]
    fn command_line_quotes_args_with_spaces_and_is_steams_paste_text() {
        assert_eq!(
            command_line(&with_boot(&strs(&["-vulkan", "+exec", "my cfg"]), true).args),
            "-vulkan +exec \"my cfg\" +exec deadtune_boot -condebug -console"
        );
        assert_eq!(
            command_line(&with_boot(&[], false).args),
            "+exec deadtune_boot -condebug"
        );
    }

    #[test]
    fn process_snapshot_reads_name_and_start_time_of_this_process() {
        let system = process_snapshot();
        let me = system
            .process(sysinfo::get_current_pid().unwrap())
            .expect("own process listed");
        assert!(!me.name().is_empty(), "process name must be populated");
        assert!(
            me.start_time() > 1_600_000_000,
            "start time must be a real epoch, got {}",
            me.start_time()
        );
    }

    #[test]
    fn game_process_name_matches_windows_proton_and_native_names() {
        for name in ["deadlock.exe", "Deadlock.exe", "DEADLOCK.EXE", "deadlock"] {
            assert!(is_game_process(name), "{name} should count as the game");
        }
        for name in [
            "deadlock-mod-manager.exe",
            "steam.exe",
            "deadlockx.exe",
            "project8.exe",
            "",
        ] {
            assert!(!is_game_process(name), "{name} must not count as the game");
        }
    }
}
