//! The UI blur disabler, regenerated from the installed game. Upstream (Sqooky's
//! `pak97_dir.vpk`) ships a stub stylesheet that imports a copy of the vanilla
//! `citadel_base_styles.vcss_c` and then redefines `ingameHudBlur` and `menuBlur` as
//! `none`; that copy goes stale on every game update. We read the game's current
//! stylesheet from `pak01_dir.vpk`, rewrite those two `@define` values in its text,
//! and recompute the compiled prefix with `hud::resource`. One file, no `base/` copy.

use super::{AddonError, BlurOptions};
use crate::hud::resource::{self, Resource};
use crate::hud::vpk::VpkDir;

pub const STYLE: &str = "panorama/styles/citadel_base_styles.vcss_c";
const HUD_DEFINE: &str = "ingameHudBlur";
const MENU_DEFINE: &str = "menuBlur";

/// Sets `@define <name>: none;` for each chosen define, in place where the game defines
/// it and appended otherwise (upstream relies on a later define winning).
pub fn neutralise(text: &str, opts: &BlurOptions) -> String {
    let mut out = text.to_string();
    for (on, name) in [(opts.hud, HUD_DEFINE), (opts.menu, MENU_DEFINE)] {
        if !on {
            continue;
        }
        let key = format!("@define {name}:");
        let replacement = format!("{key} none;");
        match out.find(&key) {
            Some(start) => {
                let end = out[start..].find(';').map_or(out.len(), |i| start + i + 1);
                out.replace_range(start..end, &replacement);
            }
            None => out.push_str(&replacement),
        }
    }
    out
}

/// The compiled stylesheet for our pak, built from the game's own.
pub fn build(game_pak: &VpkDir, opts: &BlurOptions) -> Result<Vec<u8>, AddonError> {
    let res = Resource::parse(&game_pak.read(STYLE)?)?;
    let text = neutralise(resource::style_text(&res)?, opts);
    Ok(resource::with_style_text(&res, &text)?.to_bytes())
}

#[cfg(test)]
pub(crate) mod tests {
    use std::collections::BTreeMap;
    use std::path::Path;

    use super::*;
    use crate::hud::vpk;

    /// The vanilla stylesheet as upstream shipped it in its `base/` copy.
    pub fn vanilla_style() -> Vec<u8> {
        let p = Path::new(env!("CARGO_MANIFEST_DIR")).join(
            "../../research/configs/OptimizationLock/Various Addons Relating to Performance/Blur Disabler/pak97_dir.vpk",
        );
        VpkDir::open(&p)
            .unwrap()
            .read("panorama/styles/base/citadel_base_styles.vcss_c")
            .unwrap()
    }

    /// A game pak that carries only the stylesheet.
    pub fn fake_game_pak(dir: &Path) -> VpkDir {
        let files = BTreeMap::from([(STYLE.to_string(), vanilla_style())]);
        let p = dir.join("pak01_dir.vpk");
        std::fs::write(&p, vpk::write(&files)).unwrap();
        VpkDir::open(&p).unwrap()
    }

    #[test]
    fn rewrites_defines_in_place_or_appends() {
        let text = "@define a: 1;@define ingameHudBlur: gaussian(2,2,2);.x{world-blur: ingameHudBlur;}@define menuBlur: mipmapgaussian( 4, 4, 4);";
        assert_eq!(
            neutralise(text, &BlurOptions::default()),
            "@define a: 1;@define ingameHudBlur: none;.x{world-blur: ingameHudBlur;}@define menuBlur: none;"
        );
        assert_eq!(
            neutralise(
                text,
                &BlurOptions {
                    rebuild: true,
                    hud: false,
                    menu: true
                }
            ),
            "@define a: 1;@define ingameHudBlur: gaussian(2,2,2);.x{world-blur: ingameHudBlur;}@define menuBlur: none;"
        );
        assert_eq!(
            neutralise(
                ".x{}",
                &BlurOptions {
                    rebuild: true,
                    hud: true,
                    menu: false
                }
            ),
            ".x{}@define ingameHudBlur: none;"
        );
        let off = BlurOptions {
            rebuild: true,
            hud: false,
            menu: false,
        };
        assert_eq!(neutralise(text, &off), text);
    }

    #[test]
    fn build_from_vanilla_neutralises_both_defines_and_round_trips() {
        let dir = tempfile::tempdir().unwrap();
        let pak = fake_game_pak(dir.path());
        let out = build(&pak, &BlurOptions::default()).unwrap();
        let res = Resource::parse(&out).unwrap();
        assert_eq!(res.to_bytes(), out);
        let text = resource::style_text(&res).unwrap();
        assert!(text.contains("@define ingameHudBlur: none;"));
        assert!(text.contains("@define menuBlur: none;"));
        assert!(!text.contains("gaussian"));
        assert!(text.contains("world-blur: ingameHudBlur"));
        let vanilla = Resource::parse(&vanilla_style()).unwrap();
        assert_eq!(res.block(b"RED2"), vanilla.block(b"RED2"));

        let hud_only = build(
            &pak,
            &BlurOptions {
                rebuild: true,
                hud: true,
                menu: false,
            },
        )
        .unwrap();
        let text = resource::style_text(&Resource::parse(&hud_only).unwrap())
            .unwrap()
            .to_string();
        assert!(text.contains("@define menuBlur: mipmapgaussian( 4, 4, 4);"));
        assert_ne!(hud_only, out);
    }

    #[test]
    fn missing_style_in_game_pak_is_an_error() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("pak01_dir.vpk");
        std::fs::write(&p, vpk::write(&BTreeMap::new())).unwrap();
        let pak = VpkDir::open(&p).unwrap();
        assert!(matches!(
            build(&pak, &BlurOptions::default()),
            Err(AddonError::Vpk(_))
        ));
    }
}
