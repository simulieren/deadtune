//! App settings persisted as `settings.toml` in the data dir.

use std::collections::BTreeSet;
use std::io;
use std::path::{Path, PathBuf};

use dt_core::launch_options::LaunchOptions;
use dt_core::power::PowerSource;
use dt_core::snapshot::Selection;

use crate::live::BridgeKind;
use crate::update::UpdateSettings;

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

/// The Game files page: what a snapshot saves, and whether one is taken after a game update.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct SnapshotSettings {
    pub auto: bool,
    pub selection: Selection,
}

impl Default for SnapshotSettings {
    fn default() -> SnapshotSettings {
        SnapshotSettings {
            auto: true,
            selection: Selection::default(),
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
    /// Every DeadTune pak stays out of the game folder until this is off again.
    pub safe_mode: bool,
    /// Advanced-view stars; the mini window lists them when opened from there.
    pub favourites: BTreeSet<String>,
    /// Simple-view pins; the mini window lists them under the key settings.
    pub pinned: BTreeSet<String>,
    pub launch: LaunchOptions,
    pub power: PowerProfiles,
    pub update: UpdateSettings,
    pub snapshots: SnapshotSettings,
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
            safe_mode: false,
            favourites: BTreeSet::new(),
            pinned: BTreeSet::new(),
            launch: LaunchOptions::default(),
            power: PowerProfiles::default(),
            update: UpdateSettings::default(),
            snapshots: SnapshotSettings::default(),
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
            update: UpdateSettings {
                enabled: false,
                channel: dt_core::update::Channel::Testing,
                skipped: Some(dt_core::update::Version(0, 3, 0)),
                last_check: chrono::DateTime::from_timestamp(1_800_000_000, 0),
            },
            ..Settings::default()
        };
        settings.save(dir.path()).unwrap();
        assert_eq!(Settings::load(dir.path()), settings);
        let text = std::fs::read_to_string(dir.path().join(FILE_NAME)).unwrap();
        for line in [
            "[update]",
            "channel = \"testing\"",
            "skipped = \"0.3.0\"",
            "last_check = \"2027-01-15T08:00:00Z\"",
        ] {
            assert!(text.contains(line), "{line} in\n{text}");
        }
    }

    #[test]
    fn a_file_from_before_updates_loads_with_update_defaults() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join(FILE_NAME),
            "onboarded = true\nbind_key = \"F9\"\n\n[power]\nenabled = true\n",
        )
        .unwrap();
        let settings = Settings::load(dir.path());
        assert!(settings.onboarded);
        assert_eq!(settings.bind_key, "F9");
        assert_eq!(settings.update, UpdateSettings::default());
        assert!(settings.update.enabled, "on unless turned off");
        std::fs::write(
            dir.path().join(FILE_NAME),
            "onboarded = true\n\n[update]\nchannel = \"testing\"\n",
        )
        .unwrap();
        let update = Settings::load(dir.path()).update;
        assert_eq!(update.channel, dt_core::update::Channel::Testing);
        assert!(update.enabled, "missing keys default too");
    }

    #[test]
    fn snapshot_settings_default_to_automatic_and_round_trip() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join(FILE_NAME), "onboarded = true\n").unwrap();
        let settings = Settings::load(dir.path());
        assert!(settings.snapshots.auto);
        assert_eq!(settings.snapshots.selection, Selection::default());
        std::fs::write(
            dir.path().join(FILE_NAME),
            "[snapshots]\nauto = false\n\n[snapshots.selection]\ncategories = [\"hud\"]\ndecode = false\n",
        )
        .unwrap();
        let settings = Settings::load(dir.path());
        assert!(!settings.snapshots.auto);
        assert_eq!(
            settings.snapshots.selection.categories,
            BTreeSet::from([dt_core::snapshot::Category::Hud])
        );
        assert!(!settings.snapshots.selection.decode);
        assert_eq!(
            settings.snapshots.selection.size_cap,
            Selection::default().size_cap,
            "missing keys keep their defaults"
        );
        settings.save(dir.path()).unwrap();
        assert_eq!(Settings::load(dir.path()), settings);
    }

    #[test]
    fn old_free_form_launch_args_load_into_the_launch_model() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join(FILE_NAME),
            "onboarded = true\n\n[launch]\nargs = [\"-high\", \"-novid\", \"-dx11\", \"+fps_max\", \"0\"]\n",
        )
        .unwrap();
        let launch = Settings::load(dir.path()).launch;
        assert_eq!(launch.args(), ["-dx11", "-novid", "-high", "+fps_max", "0"]);
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
