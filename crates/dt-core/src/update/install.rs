use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::time::Duration;

use super::UpdateError;

/// `deadtune.exe` -> `deadtune.exe.<suffix>` in the same directory.
fn sibling(exe: &Path, suffix: &str) -> PathBuf {
    let mut name = exe.file_name().expect("exe path names a file").to_owned();
    name.push(".");
    name.push(suffix);
    exe.with_file_name(name)
}

/// A free `.old` slot. A previous `.old` can't be deleted while the process it came from
/// still runs (Windows refuses with "Access is denied"), so try `.old1`, `.old2`, ...
fn free_old_slot(exe: &Path) -> io::Result<PathBuf> {
    let mut last = None;
    for n in 0..10 {
        let slot = sibling(
            exe,
            &if n == 0 {
                "old".into()
            } else {
                format!("old{n}")
            },
        );
        match fs::remove_file(&slot) {
            Ok(()) => return Ok(slot),
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(slot),
            Err(e) => last = Some(e),
        }
    }
    Err(last.expect("ten attempts"))
}

/// Antivirus scanners hold a freshly written exe for a moment; retry instead of failing.
fn rename_with_retry(from: &Path, to: &Path) -> io::Result<()> {
    let mut tries = 0;
    loop {
        match fs::rename(from, to) {
            Err(e) if e.kind() == io::ErrorKind::PermissionDenied && tries < 10 => {
                tries += 1;
                std::thread::sleep(Duration::from_millis(200));
            }
            result => return result,
        }
    }
}

/// Replaces `exe` with `new_bytes`: write `exe.new`, rename the running `exe` to
/// `exe.old` (allowed on Windows while running), rename `exe.new` into place.
/// Idempotent across crashes: rerunning converges, and on failure the original is
/// restored. Returns the `.old` path for [`cleanup`] on next start.
pub fn install(exe: &Path, new_bytes: &[u8]) -> Result<PathBuf, UpdateError> {
    let new = sibling(exe, "new");

    let mut file = fs::File::create(&new)?;
    file.write_all(new_bytes)?;
    file.sync_all()?;
    drop(file);

    // A crash between the two renames leaves no `exe` and the original in `.old`;
    // keep that `.old` and only finish the second rename.
    let old = if exe.exists() {
        fs::set_permissions(&new, fs::metadata(exe)?.permissions())?;
        let old = free_old_slot(exe)?;
        rename_with_retry(exe, &old)?;
        old
    } else {
        sibling(exe, "old")
    };
    if let Err(e) = rename_with_retry(&new, exe) {
        let _ = fs::rename(&old, exe);
        return Err(e.into());
    }
    Ok(old)
}

/// Deletes leftovers (`exe.new`, `exe.old`, `exe.old1`...) from previous updates. Best effort.
pub fn cleanup(exe: &Path) {
    let _ = fs::remove_file(sibling(exe, "new"));
    for n in 0..10 {
        let suffix = if n == 0 {
            "old".into()
        } else {
            format!("old{n}")
        };
        let _ = fs::remove_file(sibling(exe, &suffix));
    }
}
