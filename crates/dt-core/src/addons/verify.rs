//! Reads a pak back the way the engine will before it goes into the game folder: every
//! entry at its recorded length with a matching CRC, every compiled resource we generate
//! parsing back to the same bytes, and every generated stylesheet carrying the game's own
//! RED2, SrMa, image table and source CRC. A pak that fails here is never installed.

use std::collections::BTreeMap;
use std::fmt;
use std::path::Path;

use super::install::read_record;
use super::{AddonId, Kind, blur, info, native_particles, particles, sources};
use crate::hud::crc32::crc32;
use crate::hud::install::{ADDON_FILE as HUD_ADDON_FILE, GAME_PAK, addons_dir};
use crate::hud::resource::{self, Resource};
use crate::hud::vpk::VpkDir;
use crate::locate::GamePaths;
use crate::texture::vtex::{Layout, Vtex};

/// What a pak's content must match beyond being readable.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Expect {
    /// Every `.vpcf_c` entry must be exactly these bytes (the upstream empty-particle stub).
    pub particle_stub: Option<Vec<u8>>,
    /// The game's own compiled file that a generated `.vcss_c` at the same path came from.
    pub originals: BTreeMap<String, Vec<u8>>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Verified {
    pub entries: usize,
    pub bytes: u64,
    /// `path: reason`, one per failing entry.
    pub problems: Vec<String>,
    /// `path: facts` for every compiled stylesheet, failing or not: block sizes, image
    /// table size, the DATA prefix and the CRCs it could be, for the diagnostic report.
    pub notes: Vec<String>,
}

impl Verified {
    pub fn is_ok(&self) -> bool {
        self.problems.is_empty()
    }
}

impl fmt::Display for Verified {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.is_ok() {
            write!(f, "ok: {} entries, {} bytes", self.entries, self.bytes)
        } else {
            write!(
                f,
                "{} of {} entries failed: {}",
                self.problems.len(),
                self.entries,
                self.problems.join("; ")
            )
        }
    }
}

pub fn verify(pak: &VpkDir, expect: &Expect) -> Verified {
    let mut out = Verified::default();
    for (path, entry) in &pak.entries {
        out.entries += 1;
        let data = match pak.read(path) {
            Ok(data) => data,
            Err(e) => {
                out.problems.push(format!("{path}: {e}"));
                continue;
            }
        };
        out.bytes += data.len() as u64;
        let want = entry.preload.len() + entry.length as usize;
        if data.len() != want {
            out.problems.push(format!(
                "{path}: read {} bytes, expected {want}",
                data.len()
            ));
            continue;
        }
        if path.ends_with(".vcss_c")
            && let Ok(note) = style_facts(&data)
        {
            out.notes.push(format!("{path}: {note}"));
        }
        if let Err(reason) = check_content(path, &data, expect) {
            out.problems.push(format!("{path}: {reason}"));
        }
    }
    out
}

fn style_facts(data: &[u8]) -> Result<String, String> {
    let res = Resource::parse(data).map_err(|e| e.to_string())?;
    let blocks: Vec<String> = res
        .blocks
        .iter()
        .map(|b| format!("{}:{}", String::from_utf8_lossy(&b.name), b.data.len()))
        .collect();
    let d = &res.block(b"DATA").ok_or("no DATA block")?.data;
    if d.len() < 6 {
        return Err("DATA shorter than its prefix".into());
    }
    let prefix = u32::from_le_bytes([d[0], d[1], d[2], d[3]]);
    let images = u16::from_le_bytes([d[4], d[5]]);
    let text = resource::style_text(&res).map_err(|e| e.to_string())?;
    let table = resource::image_table(&res).map_err(|e| e.to_string())?;
    Ok(format!(
        "v{} blocks [{}] images {images} (table {} bytes) text {} bytes prefix {prefix:08x} crc32(text) {:08x} crc32(table+text) {:08x} source {:08x}",
        res.type_version,
        blocks.join(" "),
        table.len(),
        text.len(),
        crc32(text.as_bytes()),
        crc32(&d[4..]),
        prefix ^ crc32(text.as_bytes())
    ))
}

fn check_content(path: &str, data: &[u8], expect: &Expect) -> Result<(), String> {
    match path.rsplit_once('.').map_or("", |(_, ext)| ext) {
        "vcss_c" => check_style(data, expect.originals.get(path)),
        "vtex_c" => check_texture(data),
        "vpcf_c" => match &expect.particle_stub {
            Some(stub) if data != stub => Err("not the upstream empty-particle stub".into()),
            _ => Ok(()),
        },
        _ => Ok(()),
    }
}

fn check_style(data: &[u8], original: Option<&Vec<u8>>) -> Result<(), String> {
    let res = Resource::parse(data).map_err(|e| e.to_string())?;
    if res.to_bytes() != data {
        return Err("does not serialise back to the same bytes".into());
    }
    let text = resource::style_text(&res).map_err(|e| e.to_string())?;
    let again = resource::with_style_text(&res, text).map_err(|e| e.to_string())?;
    if again != res {
        return Err("DATA image table does not survive a rewrite".into());
    }
    let Some(original) = original else {
        return Ok(());
    };
    let orig = Resource::parse(original).map_err(|e| format!("game's original: {e}"))?;
    let names = |r: &Resource| r.blocks.iter().map(|b| b.name).collect::<Vec<_>>();
    if names(&res) != names(&orig) {
        return Err("block list differs from the game's file".into());
    }
    for name in [b"RED2", b"SrMa"] {
        if res.block(name) != orig.block(name) {
            return Err(format!(
                "{} block differs from the game's file",
                String::from_utf8_lossy(name)
            ));
        }
    }
    let table = |r: &Resource| resource::image_table(r).map(<[u8]>::to_vec);
    if table(&res).ok() != table(&orig).ok() {
        return Err("image table differs from the game's file".into());
    }
    let source = |r: &Resource| resource::source_crc(r).map_err(|e| e.to_string());
    if source(&res)? != source(&orig)? {
        return Err("DATA prefix CRC is not consistent with the game's file".into());
    }
    Ok(())
}

fn check_texture(data: &[u8]) -> Result<(), String> {
    let v = Vtex::parse(data).map_err(|e| e.to_string())?;
    if v.width == 0 || v.height == 0 {
        return Err(format!("zero size {}x{}", v.width, v.height));
    }
    if v.mips.is_empty() {
        return Err("no mip levels".into());
    }
    let sized = v.format.layout().is_some_and(|l| l != Layout::Encoded);
    if sized && v.pixel_start() + v.pixel_len() != data.len() {
        return Err(format!(
            "pixel data is {} bytes, header accounts for {}",
            data.len() - v.pixel_start().min(data.len()),
            v.pixel_len()
        ));
    }
    Ok(())
}

/// What `id`'s pak must match, from the upstream cache and the game's own files. A source
/// that cannot be read leaves that check out; the pak's own integrity is always checked.
pub fn expect_for(id: AddonId, paths: &GamePaths, state_dir: &Path) -> Expect {
    let mut expect = Expect::default();
    match info(id).kind {
        Kind::ParticleGroups => {
            expect.particle_stub = sources::cached(&sources::cache_dir(state_dir), id)
                .ok()
                .flatten()
                .and_then(|src| VpkDir::open(&src.path).ok())
                .and_then(|pak| particles::stub_from_upstream(&pak).ok())
                .map(|stub| stub.particle);
        }
        Kind::Blur => {
            let rebuilt = read_record(state_dir)
                .ok()
                .and_then(|r| r.installed.get(id.key()).map(|i| i.from_game))
                .unwrap_or(false);
            if rebuilt && let Some(bytes) = game_file(paths, blur::STYLE) {
                expect.originals.insert(blur::STYLE.to_string(), bytes);
            }
        }
        Kind::Clutter => {
            expect.particle_stub = game_file(paths, native_particles::EMPTY_PARTICLE);
        }
        Kind::Toggle | Kind::Textures => {}
    }
    expect
}

/// The HUD pak patches whichever stylesheets the layout touches; each must come from the
/// game's file at the same path.
pub fn expect_for_hud(paths: &GamePaths, pak: &VpkDir) -> Expect {
    let originals = pak
        .entries
        .keys()
        .filter(|p| p.ends_with(".vcss_c"))
        .filter_map(|p| game_file(paths, p).map(|b| (p.clone(), b)))
        .collect();
    Expect {
        particle_stub: None,
        originals,
    }
}

fn game_file(paths: &GamePaths, path: &str) -> Option<Vec<u8>> {
    VpkDir::open(&paths.citadel_dir.join(GAME_PAK))
        .ok()?
        .read(path)
        .ok()
}

/// One installed pak and what its check found.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PakReport {
    pub label: String,
    pub file: String,
    pub result: Result<Verified, String>,
}

impl fmt::Display for PakReport {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.result {
            Ok(v) => {
                write!(f, "{} ({}): {v}", self.label, self.file)?;
                for note in &v.notes {
                    write!(f, "\n    {note}")?;
                }
                Ok(())
            }
            Err(e) => write!(f, "{} ({}): unreadable: {e}", self.label, self.file),
        }
    }
}

/// Checks every pak DeadTune's records name: the performance addons and the HUD addon.
pub fn verify_installed(paths: &GamePaths, state_dir: &Path) -> Vec<PakReport> {
    let dir = addons_dir(paths);
    let mut out = Vec::new();
    let record = read_record(state_dir).unwrap_or_default();
    for (key, rec) in &record.installed {
        let Some(id) = AddonId::parse(key) else {
            continue;
        };
        let path = dir.join(&rec.file);
        let result = VpkDir::open(&path)
            .map(|pak| verify(&pak, &expect_for(id, paths, state_dir)))
            .map_err(|e| e.to_string());
        out.push(PakReport {
            label: info(id).name.to_string(),
            file: rec.file.clone(),
            result,
        });
    }
    let hud = dir.join(HUD_ADDON_FILE);
    if hud.is_file()
        && !matches!(
            crate::hud::install::installed_state(paths, state_dir),
            Ok(crate::hud::install::InstalledState::Foreign)
        )
    {
        let result = VpkDir::open(&hud)
            .map(|pak| verify(&pak, &expect_for_hud(paths, &pak)))
            .map_err(|e| e.to_string());
        out.push(PakReport {
            label: "HUD layout".into(),
            file: HUD_ADDON_FILE.into(),
            result,
        });
    }
    out
}

/// The texture pak just built, read back from disk (it may span several chunk files).
pub fn verify_built_file(path: &Path) -> Result<Verified, super::AddonError> {
    Ok(verify(&VpkDir::open(path)?, &Expect::default()))
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::*;
    use crate::addons::BlurOptions;
    use crate::addons::install;
    use crate::hud::vpk;
    use crate::texture::vtex::tests::{COLOR, MASK};

    fn upstream(dir: &str, file: &str) -> VpkDir {
        VpkDir::open(&sources::tests::research(dir, file)).unwrap()
    }

    #[test]
    fn every_upstream_file_passes() {
        for (dir, file) in [
            ("Sinner Light Fix Mod", "pak26_dir.vpk"),
            ("Vindicta Scope Downscale", "pak89_dir.vpk"),
            ("Optimized Soul Container", "pak01_dir.vpk"),
            ("Blur Disabler", "pak97_dir.vpk"),
        ] {
            let pak = upstream(dir, file);
            let got = verify(&pak, &Expect::default());
            assert!(got.is_ok(), "{file}: {got}");
            assert_eq!(got.entries, pak.entries.len());
        }
        let particles = upstream("Screenspace Particle Disabler", "pak02_dir.vpk");
        let stub = particles::stub_from_upstream(&particles).unwrap();
        let expect = Expect {
            particle_stub: Some(stub.particle),
            originals: BTreeMap::new(),
        };
        let got = verify(&particles, &expect);
        assert!(got.is_ok(), "{got}");
        assert_eq!(got.entries, 109);
    }

    #[test]
    fn generated_blur_pak_passes_against_the_games_stylesheet() {
        let tmp = tempfile::tempdir().unwrap();
        let game = blur::tests::fake_game_pak(tmp.path());
        let built = blur::build(&game, &BlurOptions::default()).unwrap();
        let pak = VpkDir::in_memory(vpk::write(&BTreeMap::from([(
            blur::STYLE.to_string(),
            built,
        )])))
        .unwrap();
        let expect = Expect {
            particle_stub: None,
            originals: BTreeMap::from([(blur::STYLE.to_string(), game.read(blur::STYLE).unwrap())]),
        };
        let got = verify(&pak, &expect);
        assert!(got.is_ok(), "{got}");
        assert_eq!(
            got.to_string(),
            format!("ok: 1 entries, {} bytes", got.bytes)
        );
    }

    #[test]
    fn corrupted_crc_and_truncated_data_fail() {
        let files = BTreeMap::from([
            ("a/one.txt".to_string(), b"first".to_vec()),
            ("b/two.txt".to_string(), vec![9u8; 300]),
        ]);
        let good = vpk::write(&files);
        assert!(
            verify(
                &VpkDir::in_memory(good.clone()).unwrap(),
                &Expect::default()
            )
            .is_ok()
        );

        let mut flipped = good.clone();
        let tree_size = u32::from_le_bytes(flipped[8..12].try_into().unwrap()) as usize;
        flipped[28 + tree_size] ^= 0xFF;
        let got = verify(&VpkDir::in_memory(flipped).unwrap(), &Expect::default());
        assert_eq!(got.problems, ["a/one.txt: crc mismatch for a/one.txt"]);
        assert_eq!(got.entries, 2);

        let mut cut = good;
        cut.truncate(cut.len() - 48 - 100);
        let got = verify(&VpkDir::in_memory(cut).unwrap(), &Expect::default());
        assert_eq!(got.problems.len(), 1, "{got}");
        assert!(got.problems[0].starts_with("b/two.txt:"), "{got}");
        assert!(!got.is_ok());
    }

    #[test]
    fn a_stylesheet_with_a_damaged_image_table_fails() {
        let mut res = Resource::parse(&blur::tests::vanilla_style()).unwrap();
        let data = &mut res
            .blocks
            .iter_mut()
            .find(|b| &b.name == b"DATA")
            .unwrap()
            .data;
        data[4..6].copy_from_slice(&7u16.to_le_bytes());
        let pak = VpkDir::in_memory(vpk::write(&BTreeMap::from([(
            blur::STYLE.to_string(),
            res.to_bytes(),
        )])))
        .unwrap();
        let got = verify(&pak, &Expect::default());
        assert_eq!(got.problems.len(), 1, "{got}");
        assert!(
            got.problems[0].contains("image"),
            "names the image table: {got}"
        );
    }

    #[test]
    fn a_stylesheet_from_another_source_fails_the_original_comparison() {
        let vanilla = blur::tests::vanilla_style();
        let other = include_bytes!("../../tests/fixtures/hud/hud_vanilla.vcss_c").to_vec();
        let pak = VpkDir::in_memory(vpk::write(&BTreeMap::from([(
            blur::STYLE.to_string(),
            other,
        )])))
        .unwrap();
        let expect = Expect {
            particle_stub: None,
            originals: BTreeMap::from([(blur::STYLE.to_string(), vanilla)]),
        };
        let got = verify(&pak, &expect);
        assert_eq!(got.problems.len(), 1, "{got}");
        assert!(
            got.problems[0].contains("differs from the game's file"),
            "{got}"
        );
    }

    #[test]
    fn a_particle_pak_without_the_stub_fails() {
        let upstream = upstream("Screenspace Particle Disabler", "pak02_dir.vpk");
        let stub = particles::stub_from_upstream(&upstream).unwrap();
        let mut files = BTreeMap::from([
            (particles::STUB_PATH.to_string(), stub.particle.clone()),
            (particles::DEBUG_TEXTURE.to_string(), stub.texture.clone()),
        ]);
        let expect = Expect {
            particle_stub: Some(stub.particle.clone()),
            originals: BTreeMap::new(),
        };
        let pak = VpkDir::in_memory(vpk::write(&files)).unwrap();
        assert!(verify(&pak, &expect).is_ok());

        let mut wrong = stub.particle.clone();
        wrong.push(0);
        files.insert(
            "particles/abilities/lash/lash_final_strike_screen.vpcf_c".into(),
            wrong,
        );
        let pak = VpkDir::in_memory(vpk::write(&files)).unwrap();
        let got = verify(&pak, &expect);
        assert_eq!(
            got.problems,
            [
                "particles/abilities/lash/lash_final_strike_screen.vpcf_c: not the upstream empty-particle stub"
            ]
        );
    }

    #[test]
    fn textures_need_sane_dimensions_and_matching_pixel_length() {
        let mut files = BTreeMap::from([
            ("a/color.vtex_c".to_string(), COLOR.to_vec()),
            ("a/mask.vtex_c".to_string(), MASK.to_vec()),
        ]);
        let pak = VpkDir::in_memory(vpk::write(&files)).unwrap();
        assert!(verify(&pak, &Expect::default()).is_ok());

        let mut short = COLOR.to_vec();
        short.truncate(short.len() - 10);
        files.insert("a/short.vtex_c".into(), short);
        files.insert("a/junk.vtex_c".into(), b"junk".to_vec());
        let pak = VpkDir::in_memory(vpk::write(&files)).unwrap();
        let got = verify(&pak, &Expect::default());
        assert_eq!(got.problems.len(), 2, "{got}");
        assert!(got.problems.iter().any(|p| p.starts_with("a/junk.vtex_c:")));
        assert!(
            got.problems
                .iter()
                .any(|p| p.starts_with("a/short.vtex_c:") && p.contains("pixel data")),
            "{got}"
        );
    }

    #[test]
    fn verify_installed_covers_addons_and_the_hud_pak() {
        let (steam, paths) = install::tests::fake_install("1");
        let state = steam.path().join("data");
        assert!(verify_installed(&paths, &state).is_empty());
        sources::import(
            &sources::cache_dir(&state),
            &sources::tests::research("Sinner Light Fix Mod", "pak26_dir.vpk"),
        )
        .unwrap();
        sources::import(
            &sources::cache_dir(&state),
            &sources::tests::research("Blur Disabler", "pak97_dir.vpk"),
        )
        .unwrap();
        let config = super::super::AddonsConfig {
            enabled: [AddonId::SinnerLightFix, AddonId::BlurDisabler]
                .into_iter()
                .collect(),
            ..Default::default()
        };
        install::execute(
            &install::plan(&paths, &config, &state).unwrap(),
            &paths,
            &state,
        )
        .unwrap();
        let reports = verify_installed(&paths, &state);
        assert_eq!(reports.len(), 2, "{reports:?}");
        assert!(
            reports
                .iter()
                .all(|r| r.result.as_ref().is_ok_and(Verified::is_ok)),
            "{reports:?}"
        );
        let text: Vec<String> = reports.iter().map(ToString::to_string).collect();
        assert!(
            text[0].starts_with("UI blur disabler (pak72_dir.vpk): ok"),
            "{text:?}"
        );
        let facts: Vec<&str> = text[0].lines().skip(1).collect();
        assert_eq!(facts.len(), 2, "one fact line per stylesheet: {text:?}");
        assert!(
            facts[0].contains("base/citadel_base_styles.vcss_c: v3 blocks [RED2:787 DATA:98558 SrMa:97775] images 0 (table 2 bytes)")
                && facts[0].contains("prefix a14d7c62 crc32(text) 40574f7b"),
            "{facts:?}"
        );

        std::fs::write(addons_dir(&paths).join("pak73_dir.vpk"), b"broken").unwrap();
        let reports = verify_installed(&paths, &state);
        assert!(reports[1].result.is_err(), "{reports:?}");
    }
}
