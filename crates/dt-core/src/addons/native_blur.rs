//! The UI blur disabler the way Sqooky's `pak97_dir.vpk` does it. His stub replaces the
//! game's `citadel_base_styles.vcss_c`: it `@import`s a copy of the vanilla sheet kept at
//! `base/` and then re-`@define`s `ingameHudBlur` and `menuBlur` as `none`, the later define
//! winning. That copy goes stale on every game update. We ship the user's own current file
//! byte for byte as the `base/` copy, and Sqooky's compiled stub with its text re-set for the
//! chosen options, its DATA prefix recomputed by `hud::resource`.
//! Method by Sqooky, with Bytenode.

use std::collections::BTreeMap;

use super::{AddonError, BlurOptions};
use crate::hud::resource::{Resource, with_style_text};
use crate::hud::vpk::VpkDir;

pub const STYLE: &str = "panorama/styles/citadel_base_styles.vcss_c";
pub const BASE: &str = "panorama/styles/base/citadel_base_styles.vcss_c";
const STUB: &[u8] = include_bytes!("native_blur_stub.vcss_c");

/// Sqooky's compiled stub as published: what the shipped stub's RED2, SrMa and source CRC
/// must match.
pub fn stub() -> &'static [u8] {
    STUB
}

/// The stub's stylesheet text for `opts`; the default options give Sqooky's text exactly.
pub fn stub_text(opts: &BlurOptions) -> String {
    let mut text = format!("@import url(\"s2r://{BASE}\");");
    if opts.hud {
        text.push_str("@define ingameHudBlur: none;");
    }
    if opts.menu {
        text.push_str("@define menuBlur: none;");
    }
    text
}

/// Both files of the pak: the stub at [`STYLE`] and the game's own file, unchanged, at [`BASE`].
/// Empty when neither blur is turned off.
pub fn build(game: &VpkDir, opts: &BlurOptions) -> Result<BTreeMap<String, Vec<u8>>, AddonError> {
    if !opts.hud && !opts.menu {
        return Ok(BTreeMap::new());
    }
    let live = game.read(STYLE)?;
    // Parsed only to refuse a game file we cannot read; the bytes ship untouched.
    Resource::parse(&live)?;
    let stub = with_style_text(&Resource::parse(STUB)?, &stub_text(opts))?.to_bytes();
    Ok(BTreeMap::from([
        (STYLE.to_string(), stub),
        (BASE.to_string(), live),
    ]))
}

#[cfg(test)]
pub(crate) mod tests {
    use std::path::Path;

    use super::*;
    use crate::addons::verify::{Check, Expect, verify};
    use crate::hud::crc32::crc32;
    use crate::hud::resource::{image_table, source_crc, style_text};
    use crate::hud::vpk;

    const SQOOKY_TEXT: &str = "@import url(\"s2r://panorama/styles/base/citadel_base_styles.vcss_c\");@define ingameHudBlur: none;@define menuBlur: none;";
    const HUD_ONLY: BlurOptions = BlurOptions {
        hud: true,
        menu: false,
    };
    const MENU_ONLY: BlurOptions = BlurOptions {
        hud: false,
        menu: true,
    };

    fn pak97() -> VpkDir {
        VpkDir::open(
            &Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/hud/blur_pak97_dir.vpk"),
        )
        .unwrap()
    }

    /// The game's stylesheet as it was when Sqooky published pak97.
    pub fn vanilla_style() -> Vec<u8> {
        pak97().read(BASE).unwrap()
    }

    pub fn fake_game_pak(dir: &Path, style: &[u8]) -> VpkDir {
        let files = BTreeMap::from([(STYLE.to_string(), style.to_vec())]);
        let p = dir.join("pak01_dir.vpk");
        std::fs::write(&p, vpk::write(&files)).unwrap();
        VpkDir::open(&p).unwrap()
    }

    fn build_from(style: &[u8], opts: &BlurOptions) -> BTreeMap<String, Vec<u8>> {
        let dir = tempfile::tempdir().unwrap();
        build(&fake_game_pak(dir.path(), style), opts).unwrap()
    }

    fn data_prefix(res: &Resource) -> u32 {
        let d = &res.block(b"DATA").unwrap().data;
        u32::from_le_bytes([d[0], d[1], d[2], d[3]])
    }

    pub fn vanilla_with_images() -> Vec<u8> {
        let mut res = Resource::parse(&vanilla_style()).unwrap();
        assert_eq!(res.type_version, 3);
        let mut table = 2u16.to_le_bytes().to_vec();
        for name in ["file://{images}/a.png", "file://{images}/b.svg"] {
            table.extend_from_slice(name.as_bytes());
            table.push(0);
            table.extend_from_slice(&64u16.to_le_bytes());
            table.extend_from_slice(&32u16.to_le_bytes());
            table.extend_from_slice(&0xdeadbeefu32.to_le_bytes());
        }
        let data = &mut res
            .blocks
            .iter_mut()
            .find(|b| &b.name == b"DATA")
            .unwrap()
            .data;
        data.splice(4..6, table);
        res.to_bytes()
    }

    #[test]
    fn default_stub_text_is_sqookys() {
        let text = stub_text(&BlurOptions::default());
        assert_eq!(text.len(), 120);
        assert_eq!(text, SQOOKY_TEXT);
        assert_eq!(text, style_text(&Resource::parse(STUB).unwrap()).unwrap());
    }

    #[test]
    fn build_from_vanilla_reproduces_sqookys_pak97_byte_for_byte() {
        let out = build_from(&vanilla_style(), &BlurOptions::default());
        assert_eq!(out.keys().collect::<Vec<_>>(), [BASE, STYLE]);
        let pak = pak97();
        assert_eq!(out[STYLE], pak.read(STYLE).unwrap());
        assert_eq!(out[BASE], pak.read(BASE).unwrap());
    }

    #[test]
    fn hud_only_and_menu_only_rewrite_only_the_stub_text() {
        let stub = Resource::parse(STUB).unwrap();
        assert_eq!(source_crc(&stub).unwrap(), 0x244412b1);
        let default = build_from(&vanilla_style(), &BlurOptions::default());
        let mut stubs = vec![default[STYLE].clone()];
        for (opts, define) in [
            (HUD_ONLY, "@define ingameHudBlur: none;"),
            (MENU_ONLY, "@define menuBlur: none;"),
        ] {
            let out = build_from(&vanilla_style(), &opts);
            let res = Resource::parse(&out[STYLE]).unwrap();
            let text = style_text(&res).unwrap();
            assert_eq!(text, format!("@import url(\"s2r://{BASE}\");{define}"));
            assert_eq!(res.block(b"RED2"), stub.block(b"RED2"));
            assert_eq!(res.block(b"SrMa"), stub.block(b"SrMa"));
            assert_eq!(source_crc(&res).unwrap(), 0x244412b1);
            assert_eq!(data_prefix(&res), 0x244412b1 ^ crc32(text.as_bytes()));
            assert_eq!(out[BASE], vanilla_style());
            assert!(!stubs.contains(&out[STYLE]), "{define} stub is distinct");
            stubs.push(out[STYLE].clone());
        }
    }

    #[test]
    fn neither_define_builds_nothing() {
        let off = BlurOptions {
            hud: false,
            menu: false,
        };
        assert!(build_from(&vanilla_style(), &off).is_empty());
    }

    #[test]
    fn live_file_with_an_image_table_ships_verbatim() {
        let live = vanilla_with_images();
        let out = build_from(&live, &BlurOptions::default());
        assert_eq!(out[BASE], live);
        let res = Resource::parse(&out[BASE]).unwrap();
        let vanilla = Resource::parse(&vanilla_style()).unwrap();
        assert_eq!(style_text(&res).unwrap(), style_text(&vanilla).unwrap());
        assert!(image_table(&res).unwrap().len() > 2);
    }

    #[test]
    fn pak_passes_verify_against_the_games_file() {
        let cases = [
            (vanilla_style(), BlurOptions::default()),
            (vanilla_style(), HUD_ONLY),
            (vanilla_style(), MENU_ONLY),
            (vanilla_with_images(), BlurOptions::default()),
        ];
        for (live, opts) in cases {
            let out = build_from(&live, &opts);
            let pak = VpkDir::in_memory(vpk::write(&out)).unwrap();
            let expect = Expect {
                particle_stub: None,
                checks: BTreeMap::from([
                    (STYLE.to_string(), Check::StyleFrom(STUB.to_vec())),
                    (BASE.to_string(), Check::Bytes(live.clone())),
                ]),
            };
            let got = verify(&pak, &expect);
            assert!(got.is_ok(), "{opts:?}: {got}");
        }
    }

    #[test]
    fn prefix_rule_holds_on_sqookys_files() {
        let pak = pak97();
        for path in [STYLE, BASE] {
            let res = Resource::parse(&pak.read(path).unwrap()).unwrap();
            let source = source_crc(&res).unwrap().to_le_bytes();
            let red2 = &res.block(b"RED2").unwrap().data;
            assert!(red2.windows(4).any(|w| w == source), "{path}");
            let text = style_text(&res).unwrap().as_bytes();
            let mut table_text = image_table(&res).unwrap().to_vec();
            table_text.extend_from_slice(text);
            let prefix = data_prefix(&res);
            assert_ne!(prefix, crc32(text), "{path}");
            assert_ne!(prefix, crc32(&table_text), "{path}");
        }
    }

    #[test]
    fn missing_style_in_game_pak_is_an_error() {
        let pak = VpkDir::in_memory(vpk::write(&BTreeMap::new())).unwrap();
        assert!(matches!(
            build(&pak, &BlurOptions::default()),
            Err(AddonError::Vpk(_))
        ));
    }

    #[test]
    fn unreadable_style_in_game_pak_is_an_error() {
        let dir = tempfile::tempdir().unwrap();
        let pak = fake_game_pak(dir.path(), b"not a resource");
        assert!(matches!(
            build(&pak, &BlurOptions::default()),
            Err(AddonError::Resource(_))
        ));
    }
}
