//! "Start with Windows": a `Run` value under the player's own registry hive that starts
//! DeadTune with `--background` at sign-in, so paks are rebuilt right after Steam updates
//! the game even when nobody opens DeadTune first. Written and removed with `reg.exe`.

use std::path::Path;

pub const RUN_KEY: &str = r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run";
pub const VALUE: &str = "DeadTune";
pub const FLAG: &str = "--background";

/// What the `Run` value holds for `exe`.
pub fn command(exe: &Path) -> String {
    format!("\"{}\" {FLAG}", exe.display())
}

/// `reg.exe` arguments that turn the start on (`Some(exe)`) or off.
pub fn reg_args(exe: Option<&Path>) -> Vec<String> {
    let mut args: Vec<String> = match exe {
        Some(_) => vec!["add".into()],
        None => vec!["delete".into()],
    };
    args.extend([RUN_KEY.into(), "/v".into(), VALUE.into()]);
    if let Some(exe) = exe {
        args.extend(["/t".into(), "REG_SZ".into(), "/d".into(), command(exe)]);
    }
    args.push("/f".into());
    args
}

#[cfg(windows)]
pub fn set(exe: Option<&Path>) -> std::io::Result<()> {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    let out = std::process::Command::new("reg.exe")
        .args(reg_args(exe))
        .creation_flags(CREATE_NO_WINDOW)
        .output()?;
    // Deleting a value that is not there is already the wanted state.
    if out.status.success() || exe.is_none() {
        Ok(())
    } else {
        Err(std::io::Error::other(
            String::from_utf8_lossy(&out.stderr).trim().to_string(),
        ))
    }
}

#[cfg(not(windows))]
pub fn set(_exe: Option<&Path>) -> std::io::Result<()> {
    Err(std::io::Error::new(
        std::io::ErrorKind::Unsupported,
        "starting with the system is only available on Windows",
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_run_value_starts_deadtune_in_the_background() {
        let exe = Path::new(r"C:\Tools\DeadTune\deadtune.exe");
        assert_eq!(
            reg_args(Some(exe)),
            [
                "add",
                RUN_KEY,
                "/v",
                "DeadTune",
                "/t",
                "REG_SZ",
                "/d",
                r#""C:\Tools\DeadTune\deadtune.exe" --background"#,
                "/f"
            ]
        );
        assert_eq!(reg_args(None), ["delete", RUN_KEY, "/v", "DeadTune", "/f"]);
    }
}
