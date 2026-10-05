//! The addons DeadTune rebuilds from the player's own `pak01_dir.vpk`: each [`Native`]
//! names the game files its builder reads (their CRCs fingerprint the build, so a game
//! update triggers a rebuild), the paths its pak overrides, the options that change the
//! output, the builder itself and what the written pak must match when read back.

use std::collections::BTreeMap;

use super::verify::{Check, Expect};
use super::{
    AddonError, AddonsConfig, native_blur, native_particles, native_scope, native_sinner, particles,
};
use crate::hud::vpk::VpkDir;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Native {
    /// Laund's screen-space particle disabler: the game's empty particle at the hidden paths.
    Particles,
    /// Sqooky's blur disabler: his stub over the game's current base stylesheet.
    Blur,
    /// HoppCX's Sinner's Sacrifice light fix: the mask's top mip only, the model's LODs pinned.
    Sinner,
    /// Tamara Mochaccinae's Vindicta scope downscale: the scope texture resampled smaller.
    Scope,
}

impl Native {
    /// The pak01 entries the build reads. Missing means the addon cannot be built.
    pub fn game_files(self) -> &'static [&'static str] {
        match self {
            Native::Particles => &[native_particles::EMPTY_PARTICLE],
            Native::Blur => &[native_blur::STYLE],
            Native::Sinner => &[native_sinner::MASK, native_sinner::MODEL],
            Native::Scope => &[native_scope::TEXTURE],
        }
    }

    /// The player's choices that change the output, as one fingerprint line.
    pub fn options(self, config: &AddonsConfig) -> String {
        match self {
            Native::Particles => particles::hidden_paths(&config.keep_particles).join("\n"),
            Native::Blur => format!("hud={} menu={}", config.blur.hud, config.blur.menu),
            Native::Sinner => String::new(),
            Native::Scope => format!("side={}", config.scope.side),
        }
    }

    /// The game paths our pak overrides, for the conflict scan. Empty when the options
    /// leave nothing to install.
    pub fn ships(self, config: &AddonsConfig) -> Vec<String> {
        let own = |paths: &[&str]| paths.iter().map(|p| p.to_string()).collect();
        match self {
            Native::Particles => own(&particles::hidden_paths(&config.keep_particles)),
            Native::Blur if config.blur.hud || config.blur.menu => {
                own(&[native_blur::STYLE, native_blur::BASE])
            }
            Native::Blur => Vec::new(),
            Native::Sinner => own(&[native_sinner::MASK, native_sinner::MODEL]),
            Native::Scope => own(&[native_scope::TEXTURE]),
        }
    }

    /// Pak path -> bytes. Empty means nothing to install (every particle group kept, or
    /// neither blur turned off).
    pub fn build(
        self,
        game: &VpkDir,
        config: &AddonsConfig,
    ) -> Result<BTreeMap<String, Vec<u8>>, AddonError> {
        match self {
            Native::Particles => native_particles::build(game, &config.keep_particles),
            Native::Blur => native_blur::build(game, &config.blur),
            Native::Sinner => native_sinner::build(game),
            Native::Scope => native_scope::build(game, &config.scope),
        }
    }

    /// What the built pak must match, taken from the game's own files, so the check is
    /// independent of the builder's output.
    pub fn expect(self, game: &VpkDir) -> Result<Expect, AddonError> {
        let mut expect = Expect::default();
        match self {
            Native::Particles => {
                expect.particle_stub = Some(game.read(native_particles::EMPTY_PARTICLE)?);
            }
            Native::Blur => {
                expect.checks.insert(
                    native_blur::STYLE.to_string(),
                    Check::StyleFrom(native_blur::stub().to_vec()),
                );
                expect.checks.insert(
                    native_blur::BASE.to_string(),
                    Check::Bytes(game.read(native_blur::STYLE)?),
                );
            }
            Native::Sinner => {
                expect.checks.insert(
                    native_sinner::MASK.to_string(),
                    Check::TopMipOf(game.read(native_sinner::MASK)?),
                );
                expect.checks.insert(
                    native_sinner::MODEL.to_string(),
                    Check::PinnedLodsOf(game.read(native_sinner::MODEL)?),
                );
            }
            Native::Scope => {
                expect.checks.insert(
                    native_scope::TEXTURE.to_string(),
                    Check::ResampledFrom(game.read(native_scope::TEXTURE)?),
                );
            }
        }
        Ok(expect)
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use std::collections::BTreeSet;

    use super::*;
    use crate::addons::verify::verify;
    use crate::addons::{BlurOptions, ScopeOptions};
    use crate::hud::vpk;

    const ALL: [Native; 4] = [
        Native::Particles,
        Native::Blur,
        Native::Sinner,
        Native::Scope,
    ];

    /// A pak01 carrying every file the four builders read.
    pub fn fake_game_files() -> BTreeMap<String, Vec<u8>> {
        BTreeMap::from([
            (
                native_particles::EMPTY_PARTICLE.to_string(),
                native_particles::tests::EMPTY.to_vec(),
            ),
            (
                native_blur::STYLE.to_string(),
                native_blur::tests::vanilla_style(),
            ),
            (
                native_sinner::MASK.to_string(),
                native_sinner::tests::stock_mask(),
            ),
            (
                native_sinner::MODEL.to_string(),
                native_sinner::tests::stock_model(),
            ),
            (
                native_scope::TEXTURE.to_string(),
                native_scope::tests::synthetic_original(512),
            ),
        ])
    }

    pub fn fake_game() -> VpkDir {
        VpkDir::in_memory(vpk::write(&fake_game_files())).unwrap()
    }

    #[test]
    fn every_builder_reads_only_the_files_it_declares_and_passes_its_own_check() {
        let game = fake_game();
        let config = AddonsConfig {
            scope: ScopeOptions { side: 256 },
            ..AddonsConfig::default()
        };
        for native in ALL {
            let files = native.build(&game, &config).unwrap();
            assert!(!files.is_empty(), "{native:?}");
            let mut shipped: Vec<&str> = files.keys().map(String::as_str).collect();
            shipped.sort_unstable();
            let mut declared = native.ships(&config);
            declared.sort_unstable();
            assert_eq!(shipped, declared, "{native:?}");
            let pak = VpkDir::in_memory(vpk::write(&files)).unwrap();
            let got = verify(&pak, &native.expect(&game).unwrap());
            assert!(got.is_ok(), "{native:?}: {got}");

            let only_declared: BTreeMap<String, Vec<u8>> = native
                .game_files()
                .iter()
                .map(|p| (p.to_string(), game.read(p).unwrap()))
                .collect();
            let partial = VpkDir::in_memory(vpk::write(&only_declared)).unwrap();
            assert_eq!(
                native.build(&partial, &config).unwrap(),
                files,
                "{native:?}"
            );
            native.expect(&partial).unwrap();
        }
    }

    #[test]
    fn a_game_missing_a_declared_file_fails_to_build() {
        let empty = VpkDir::in_memory(vpk::write(&BTreeMap::new())).unwrap();
        for native in ALL {
            assert!(
                matches!(
                    native.build(&empty, &AddonsConfig::default()),
                    Err(AddonError::Vpk(_))
                ),
                "{native:?}"
            );
            assert!(native.expect(&empty).is_err(), "{native:?}");
        }
    }

    #[test]
    fn options_that_leave_nothing_to_install_build_and_ship_nothing() {
        let game = fake_game();
        let mut config = AddonsConfig {
            keep_particles: particles::GROUPS.iter().map(|g| g.id.to_string()).collect(),
            blur: BlurOptions {
                hud: false,
                menu: false,
            },
            ..AddonsConfig::default()
        };
        for native in [Native::Particles, Native::Blur] {
            assert!(native.build(&game, &config).unwrap().is_empty());
            assert!(native.ships(&config).is_empty());
        }
        assert_eq!(Native::Sinner.options(&config), "");
        assert_eq!(Native::Scope.options(&config), "side=1080");
        config.keep_particles = BTreeSet::from(["low_health".to_string()]);
        assert!(!Native::Particles.options(&config).contains("low_health"));
        assert_eq!(Native::Particles.ships(&config).len(), 108 - 4);
    }
}
