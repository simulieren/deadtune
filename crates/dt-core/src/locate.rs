//! Finds Steam and Deadlock (appid 1422450).

use std::path::{Path, PathBuf};

pub const APP_ID: u32 = 1422450;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GamePaths {
    /// `.../steamapps/common/Deadlock`
    pub game_root: PathBuf,
    pub citadel_dir: PathBuf,
    pub gameinfo: PathBuf,
    pub video: PathBuf,
    pub cfg_dir: PathBuf,
    pub appmanifest: Option<PathBuf>,
    pub steam_root: Option<PathBuf>,
}

#[derive(Debug, thiserror::Error)]
pub enum LocateError {
    #[error("steam not found: {0}")]
    Steam(String),
    #[error("Deadlock (appid 1422450) not installed in any Steam library")]
    NotInstalled,
    #[error("not a Deadlock install: {0}")]
    BadPath(PathBuf),
}

pub fn locate() -> Result<GamePaths, LocateError> {
    let located = steamlocate::locate_all();
    let located_dirs: Vec<PathBuf> = match &located {
        Ok(dirs) => dirs.iter().map(|d| d.path().to_path_buf()).collect(),
        Err(_) => Vec::new(),
    };
    let candidates = located_dirs.into_iter().chain(extra_steam_dir());

    let mut steam_found = false;
    for root in candidates {
        let Ok(steam) = steamlocate::SteamDir::from_dir(&root) else {
            continue;
        };
        steam_found = true;
        let Ok(Some((app, library))) = steam.find_app(APP_ID) else {
            continue;
        };
        if let Ok(paths) = from_game_root(&library.resolve_app_dir(&app)) {
            return Ok(GamePaths {
                steam_root: Some(steam.path().to_path_buf()),
                ..paths
            });
        }
    }
    match (steam_found, located) {
        (true, _) => Err(LocateError::NotInstalled),
        (false, Err(e)) => Err(LocateError::Steam(e.to_string())),
        (false, Ok(_)) => Err(LocateError::Steam("no Steam directory found".into())),
    }
}

/// steamlocate does not probe this Flatpak layout; Deadlock Mod Manager users hit it.
#[cfg(target_os = "linux")]
fn extra_steam_dir() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .map(|home| PathBuf::from(home).join(".var/app/com.valvesoftware.Steam/data/Steam"))
}

#[cfg(not(target_os = "linux"))]
fn extra_steam_dir() -> Option<PathBuf> {
    None
}

/// For a user-picked `.../Deadlock` folder (manual override, Proton prefixes, etc.).
pub fn from_game_root(game_root: &Path) -> Result<GamePaths, LocateError> {
    let citadel_dir = game_root.join("game").join("citadel");
    let gameinfo = citadel_dir.join("gameinfo.gi");
    if !gameinfo.is_file() {
        return Err(LocateError::BadPath(game_root.to_path_buf()));
    }
    let cfg_dir = citadel_dir.join("cfg");
    let steamapps = game_root.parent().and_then(Path::parent);
    let appmanifest = steamapps
        .map(|dir| dir.join(format!("appmanifest_{APP_ID}.acf")))
        .filter(|p| p.is_file());
    // Only the main library holds `userdata`; secondary libraries leave the steam root unknown.
    let steam_root = steamapps
        .and_then(Path::parent)
        .filter(|lib| lib.join("userdata").is_dir())
        .map(Path::to_path_buf);
    Ok(GamePaths {
        game_root: game_root.to_path_buf(),
        gameinfo,
        video: cfg_dir.join("video.txt"),
        cfg_dir,
        citadel_dir,
        appmanifest,
        steam_root,
    })
}

pub fn parse_buildid(acf: &str) -> Option<String> {
    acf.lines().find_map(|line| {
        let mut fields = line.split('"').filter(|f| !f.trim().is_empty());
        match (fields.next(), fields.next()) {
            (Some(key), Some(value)) if key.eq_ignore_ascii_case("buildid") => {
                Some(value.to_string())
            }
            _ => None,
        }
    })
}

/// The installed build, read from the Steam appmanifest when there is one.
pub fn buildid(paths: &GamePaths) -> Option<String> {
    let acf = std::fs::read_to_string(paths.appmanifest.as_ref()?).ok()?;
    parse_buildid(&acf)
}

/// `userdata/<id>/760/remote/1422450/screenshots` for every Steam user.
pub fn screenshot_dirs(steam_root: &Path) -> Vec<PathBuf> {
    let Ok(users) = std::fs::read_dir(steam_root.join("userdata")) else {
        return Vec::new();
    };
    let mut dirs: Vec<PathBuf> = users
        .filter_map(|user| user.ok())
        .map(|user| {
            user.path()
                .join("760")
                .join("remote")
                .join(APP_ID.to_string())
                .join("screenshots")
        })
        .filter(|dir| dir.is_dir())
        .collect();
    dirs.sort();
    dirs
}

#[cfg(test)]
mod tests {
    use super::*;

    const APPMANIFEST: &str = include_str!("../tests/fixtures/appmanifest_1422450.acf");

    /// `<steam>/steamapps/common/Deadlock` with gameinfo, cfg dir, appmanifest and userdata.
    fn fake_steam() -> (tempfile::TempDir, PathBuf) {
        let steam = tempfile::tempdir().unwrap();
        let game_root = steam
            .path()
            .join("steamapps")
            .join("common")
            .join("Deadlock");
        let citadel = game_root.join("game").join("citadel");
        std::fs::create_dir_all(citadel.join("cfg")).unwrap();
        std::fs::write(citadel.join("gameinfo.gi"), "\"GameInfo\"\n{\n}\n").unwrap();
        std::fs::write(
            steam
                .path()
                .join("steamapps")
                .join("appmanifest_1422450.acf"),
            APPMANIFEST,
        )
        .unwrap();
        std::fs::create_dir_all(steam.path().join("userdata")).unwrap();
        (steam, game_root)
    }

    #[test]
    fn parse_buildid_reads_the_appmanifest_buildid() {
        assert_eq!(parse_buildid(APPMANIFEST).as_deref(), Some("20419345"));
    }

    #[test]
    fn parse_buildid_ignores_target_buildid_and_handles_missing_key() {
        let acf = "\"AppState\"\n{\n\t\"TargetBuildID\"\t\t\"99\"\n}\n";
        assert_eq!(parse_buildid(acf), None);
        assert_eq!(
            parse_buildid("\"AppState\"\r\n{\r\n\t\"buildid\"\t\t\"7\"\r\n}\r\n").as_deref(),
            Some("7")
        );
    }

    #[test]
    fn from_game_root_fills_every_path() {
        let (steam, game_root) = fake_steam();
        let paths = from_game_root(&game_root).unwrap();
        let citadel = game_root.join("game").join("citadel");
        assert_eq!(paths.game_root, game_root);
        assert_eq!(paths.citadel_dir, citadel);
        assert_eq!(paths.gameinfo, citadel.join("gameinfo.gi"));
        assert_eq!(paths.cfg_dir, citadel.join("cfg"));
        assert_eq!(paths.video, citadel.join("cfg").join("video.txt"));
        assert_eq!(
            paths.appmanifest,
            Some(
                steam
                    .path()
                    .join("steamapps")
                    .join("appmanifest_1422450.acf")
            )
        );
        assert_eq!(
            paths.steam_root,
            Some(steam.path().to_path_buf()),
            "library with userdata is the steam root"
        );
        assert_eq!(buildid(&paths).as_deref(), Some("20419345"));
        assert_eq!(
            buildid(&GamePaths {
                appmanifest: None,
                ..paths
            }),
            None
        );
    }

    #[test]
    fn from_game_root_without_manifest_or_userdata_leaves_them_none() {
        let dir = tempfile::tempdir().unwrap();
        let game_root = dir.path().join("Deadlock");
        std::fs::create_dir_all(game_root.join("game").join("citadel")).unwrap();
        std::fs::write(
            game_root.join("game").join("citadel").join("gameinfo.gi"),
            "",
        )
        .unwrap();
        let paths = from_game_root(&game_root).unwrap();
        assert_eq!(paths.appmanifest, None);
        assert_eq!(paths.steam_root, None);
    }

    #[test]
    fn from_game_root_rejects_a_folder_without_gameinfo() {
        let dir = tempfile::tempdir().unwrap();
        let err = from_game_root(dir.path()).unwrap_err();
        assert!(matches!(err, LocateError::BadPath(p) if p == dir.path()));
    }

    #[test]
    fn screenshot_dirs_finds_every_users_deadlock_screenshots() {
        let (steam, _) = fake_steam();
        let userdata = steam.path().join("userdata");
        let shots = |id: &str| {
            userdata
                .join(id)
                .join("760")
                .join("remote")
                .join("1422450")
                .join("screenshots")
        };
        std::fs::create_dir_all(shots("111")).unwrap();
        std::fs::create_dir_all(shots("222")).unwrap();
        std::fs::create_dir_all(
            userdata
                .join("333")
                .join("760")
                .join("remote")
                .join("730")
                .join("screenshots"),
        )
        .unwrap();
        assert_eq!(
            screenshot_dirs(steam.path()),
            vec![shots("111"), shots("222")]
        );
    }

    #[test]
    fn screenshot_dirs_without_userdata_is_empty() {
        let dir = tempfile::tempdir().unwrap();
        assert!(screenshot_dirs(dir.path()).is_empty());
    }
}
