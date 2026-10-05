//! DeadTune core: everything except UI. No module here touches the game process;
//! all effects go through files, launch options and the official console.

pub mod addons;
pub mod apply;
pub mod backup;
pub mod bench;
pub mod bridge;
pub mod catalog;
pub mod doctor;
pub mod gi;
pub mod gi_sections;
pub mod hud;
pub mod launch;
pub mod launch_options;
pub mod locate;
pub mod power;
pub mod practice;
pub mod preset;
pub mod profile;
pub mod snapshot;
pub mod steamcfg;
pub mod texture;
pub mod update;
pub mod video;
pub mod watch;
pub mod winfps;
pub mod zip;

#[cfg(test)]
mod version_policy {
    /// Minor-only releases for now: every version is 0.Y.0 (scripts/release-local.sh minor).
    #[test]
    fn version_is_minor_only() {
        let parts: Vec<u64> = env!("CARGO_PKG_VERSION")
            .split('.')
            .map(|p| {
                p.parse()
                    .expect("plain numeric semver, no pre-release suffix")
            })
            .collect();
        assert!(
            matches!(parts[..], [0, _, 0]),
            "version {} must be 0.Y.0",
            env!("CARGO_PKG_VERSION")
        );
    }
}
