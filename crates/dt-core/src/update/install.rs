use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use super::UpdateError;

/// `deadtune.exe` -> `deadtune.exe.<suffix>` in the same directory.
fn sibling(exe: &Path, suffix: &str) -> PathBuf {
    let mut name = exe.file_name().expect("exe path names a file").to_owned();
    name.push(".");
    name.push(suffix);
    exe.with_file_name(name)
}

/// Replaces `exe` with `new_bytes`: write `exe.new`, rename the running `exe` to
/// `exe.old` (allowed on Windows while running), rename `exe.new` into place.
/// Idempotent across crashes: rerunning converges, and on failure the original is
/// restored. Returns the `.old` path for [`cleanup`] on next start.
pub fn install(exe: &Path, new_bytes: &[u8]) -> Result<PathBuf, UpdateError> {
    let new = sibling(exe, "new");
    let old = sibling(exe, "old");

    let mut file = fs::File::create(&new)?;
    file.write_all(new_bytes)?;
    file.sync_all()?;
    drop(file);

    // A crash between the two renames leaves no `exe` and the original in `.old`;
    // keep that `.old` and only finish the second rename.
    if exe.exists() {
        fs::set_permissions(&new, fs::metadata(exe)?.permissions())?;
        match fs::remove_file(&old) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => return Err(e.into()),
            _ => {}
        }
        fs::rename(exe, &old)?;
    }
    if let Err(e) = fs::rename(&new, exe) {
        let _ = fs::rename(&old, exe);
        return Err(e.into());
    }
    Ok(old)
}

/// Deletes leftovers (`exe.old`, `exe.new`) from a previous update. Best effort.
pub fn cleanup(exe: &Path) {
    let _ = fs::remove_file(sibling(exe, "old"));
    let _ = fs::remove_file(sibling(exe, "new"));
}
