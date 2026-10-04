//! App settings persisted as `settings.toml` in the data dir.

use std::collections::BTreeSet;
use std::io;
use std::path::{Path, PathBuf};

use dt_core::launch::LaunchOptions;
use dt_core::power::PowerSource;

use crate::live::BridgeKind;

pub const FILE_NAME: &str = "settings.toml";

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TargetSource {
    #[default]
    Profile,
    /// Stock ConVars block, video.txt untouched.
    RankedSafe,
}

/// Simple shows a few plain-language settings; Advanced shows every tab and every ConVar.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum View {
    #[default]
    Simple,
    Advanced,
}

#[derive(Clone, Debug, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct PowerProfiles {
    pub enabled: bool,
    pub ac: Option<String>,
    pub battery: Option<String>,
}

impl PowerProfiles {
    /// Profile to switch to for `source`, if auto switching is on and one is configured.
    pub fn choose(&self, source: PowerSource) -> Option<&str> {
        if !self.enabled {
            return None;
        }
        match source {
            PowerSource::Ac => self.ac.as_deref(),
            PowerSource::Battery => self.battery.as_deref(),
            PowerSource::Unknown => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct Settings {
    /// False until the welcome flow finished once.
    pub onboarded: bool,
    pub view: View,
    /// A live push was confirmed by the console log at least once, so the setup is known to work.
    pub live_verified: bool,
    /// Launch with `-console`, so the game opens its console window.
    pub console_window: bool,
    pub game_dir: Option<PathBuf>,
    pub last_profile: Option<String>,
    pub bridge: BridgeKind,
    pub bind_key: String,
    pub netcon_port: u16,
    pub source: TargetSource,
    /// Advanced-view stars; the mini window lists them when opened from there.
    pub favourites: BTreeSet<String>,
    /// Simple-view pins; the mini window lists them under the key settings.
    pub pinned: BTreeSet<String>,
    pub launch: LaunchOptions,
    pub power: PowerProfiles,
}

impl Default for Settings {
    fn default() -> Settings {
        Settings {
            onboarded: false,
            view: View::default(),
            live_verified: false,
            console_window: false,
            game_dir: None,
            last_profile: None,
            bridge: BridgeKind::default(),
            bind_key: "F8".into(),
            netcon_port: dt_core::bridge::netcon::DEFAULT_PORT,
            source: TargetSource::default(),
            favourites: BTreeSet::new(),
            pinned: BTreeSet::new(),
            launch: LaunchOptions {
                args: vec!["-novid".into()],
            },
            power: PowerProfiles::default(),
        }
    }
}

impl Settings {
    /// Missing or unreadable settings fall back to defaults; a broken file must not block startup.
    pub fn load(data_dir: &Path) -> Settings {
        std::fs::read_to_string(data_dir.join(FILE_NAME))
            .ok()
            .and_then(|text| toml::from_str(&text).ok())
            .unwrap_or_default()
    }

    pub fn save(&self, data_dir: &Path) -> io::Result<()> {
        std::fs::create_dir_all(data_dir)?;
        let text = toml::to_string(self).map_err(io::Error::other)?;
        dt_core::backup::atomic_write(&data_dir.join(FILE_NAME), text.as_bytes())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_through_the_data_dir() {
        let dir = tempfile::tempdir().unwrap();
        let settings = Settings {
            game_dir: Some(PathBuf::from("/games/Deadlock")),
            favourites: BTreeSet::from(["r_farz".to_string()]),
            pinned: BTreeSet::from(["r_shadows".to_string()]),
            bridge: BridgeKind::Netcon,
            power: PowerProfiles {
                enabled: true,
                ac: None,
                battery: Some("Battery".into()),
            },
            ..Settings::default()
        };
        settings.save(dir.path()).unwrap();
        assert_eq!(Settings::load(dir.path()), settings);
    }

    #[test]
    fn missing_or_broken_file_gives_defaults() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(Settings::load(dir.path()), Settings::default());
        std::fs::write(dir.path().join(FILE_NAME), "not = [toml").unwrap();
        assert_eq!(Settings::load(dir.path()), Settings::default());
    }

    #[test]
    fn power_choice_needs_enabled_and_a_known_source() {
        let mut power = PowerProfiles {
            enabled: false,
            ac: Some("Plugged in".into()),
            battery: Some("Battery".into()),
        };
        assert_eq!(power.choose(PowerSource::Battery), None, "disabled");
        power.enabled = true;
        assert_eq!(power.choose(PowerSource::Battery), Some("Battery"));
        assert_eq!(power.choose(PowerSource::Ac), Some("Plugged in"));
        assert_eq!(power.choose(PowerSource::Unknown), None);
    }
}
