//! Profiles on disk: one TOML file per profile under `<data_dir>/profiles`.

use std::io;
use std::path::{Path, PathBuf};

use dt_core::profile::Profile;

pub fn dir(data_dir: &Path) -> PathBuf {
    data_dir.join("profiles")
}

/// Profile names are free text; file names keep only characters valid on every filesystem.
pub fn path_for(dir: &Path, name: &str) -> PathBuf {
    let stem: String = name
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect();
    dir.join(format!("{stem}.toml"))
}

pub fn save(dir: &Path, profile: &Profile) -> io::Result<PathBuf> {
    std::fs::create_dir_all(dir)?;
    let path = path_for(dir, &profile.name);
    let text = profile.to_toml().map_err(io::Error::other)?;
    dt_core::backup::atomic_write(&path, text.as_bytes())?;
    Ok(path)
}

/// Every parseable profile, sorted by name. Broken files are skipped, not fatal.
pub fn list(dir: &Path) -> Vec<Profile> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut out: Vec<Profile> = entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "toml"))
        .filter_map(|p| std::fs::read_to_string(p).ok())
        .filter_map(|text| Profile::from_toml(&text).ok())
        .collect();
    out.sort_by(|a, b| a.name.cmp(&b.name));
    out
}

pub fn load(dir: &Path, name: &str) -> Option<Profile> {
    list(dir).into_iter().find(|p| p.name == name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn save_list_load() {
        let dir = tempfile::tempdir().unwrap();
        let mut profiles = dt_core::profile::builtin_suggestions();
        for p in &profiles {
            save(dir.path(), p).unwrap();
        }
        std::fs::write(dir.path().join("broken.toml"), "nope").unwrap();
        profiles.sort_by(|a, b| a.name.cmp(&b.name));
        assert_eq!(list(dir.path()), profiles);
        assert_eq!(load(dir.path(), "Battery").unwrap().name, "Battery");
        assert_eq!(load(dir.path(), "missing"), None);
    }

    #[test]
    fn names_become_safe_file_names() {
        assert_eq!(
            path_for(Path::new("p"), "Laptop / battery"),
            Path::new("p").join("Laptop___battery.toml")
        );
    }
}
