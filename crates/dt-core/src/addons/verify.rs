//! Reads a pak back the way the engine will before it goes into the game folder: every
//! entry at its recorded length with a matching CRC, every compiled resource we generate
//! parsing back to the same bytes, and every rebuilt file matching the game's own file it
//! came from. A pak that fails here is never installed.

use std::collections::BTreeMap;
use std::fmt;
use std::path::Path;

use super::install::read_record;
use super::{AddonId, Kind, info, native_scope, native_sinner};
use crate::hud::crc32::crc32;
use crate::hud::inject;
use crate::hud::install::{ADDON_FILE as HUD_ADDON_FILE, GAME_PAK, addons_dir};
use crate::hud::resource::{self, Resource};
use crate::hud::vpk::VpkDir;
use crate::locate::GamePaths;
use crate::texture::encode;
use crate::texture::svg;
use crate::texture::vtex::{Flags, Layout, Vtex};

/// What one entry must match beyond being readable, taken from the game's own file.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Check {
    /// Exactly these bytes.
    Bytes(Vec<u8>),
    /// A compiled stylesheet generated from this one: same RED2, SrMa, image table and
    /// source CRC, only the text differs.
    StyleFrom(Vec<u8>),
    /// A layout rebuilt as text from this compiled one: every line of the original's XML
    /// is still there, in order.
    LayoutFrom(Vec<u8>),
    /// This texture with every level but the largest dropped and `NO_LOD` set.
    TopMipOf(Vec<u8>),
    /// This model with its LOD arrays pinned to the full-detail mesh, every other block
    /// the same.
    PinnedLodsOf(Vec<u8>),
    /// This texture area-averaged smaller with its aspect, format and flags kept.
    ResampledFrom(Vec<u8>),
    /// The player's image encoded into this game image: uncompressed BGRA8888, one mip,
    /// `NO_LOD`, the game file's RED2 block.
    ReplacedImageOf(Vec<u8>),
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Expect {
    /// Every `.vpcf_c` entry must be exactly these bytes (the game's empty particle).
    pub particle_stub: Option<Vec<u8>>,
    /// Per path.
    pub checks: BTreeMap<String, Check>,
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
        "vcss_c" => check_style(data)?,
        "vxml_c" => check_text_layout(data)?,
        "vjs_c" => check_script(data)?,
        "vtex_c" => check_texture(data)?,
        "vsvg_c" => {
            svg::svg_text(data).map_err(|e| e.to_string())?;
        }
        "vpcf_c" => {
            if let Some(stub) = &expect.particle_stub
                && data != stub
            {
                return Err("not the game's empty particle".into());
            }
        }
        _ => {}
    }
    match expect.checks.get(path) {
        None => Ok(()),
        Some(Check::Bytes(want)) if data == want => Ok(()),
        Some(Check::Bytes(_)) => Err("not the game's current file".into()),
        Some(Check::StyleFrom(original)) => style_from(data, original),
        Some(Check::LayoutFrom(original)) => layout_from(data, original),
        Some(Check::TopMipOf(original)) => top_mip_of(data, original),
        Some(Check::PinnedLodsOf(original)) => pinned_lods_of(data, original),
        Some(Check::ResampledFrom(original)) => resampled_from(data, original),
        Some(Check::ReplacedImageOf(original)) => replaced_image_of(data, original),
    }
}

fn check_style(data: &[u8]) -> Result<(), String> {
    let res = Resource::parse(data).map_err(|e| e.to_string())?;
    if res.to_bytes() != data {
        return Err("does not serialise back to the same bytes".into());
    }
    let text = resource::style_text(&res).map_err(|e| e.to_string())?;
    let again = resource::with_style_text(&res, text).map_err(|e| e.to_string())?;
    if again != res {
        return Err("DATA image table does not survive a rewrite".into());
    }
    Ok(())
}

/// A layout we wrote is text behind a prefix that is the text's own CRC.
fn check_text_layout(data: &[u8]) -> Result<(), String> {
    let res = Resource::parse(data).map_err(|e| e.to_string())?;
    if res.to_bytes() != data {
        return Err("does not serialise back to the same bytes".into());
    }
    if res.block(b"LaCo").is_some() {
        return Ok(());
    }
    let text = resource::style_text(&res).map_err(|e| e.to_string())?;
    if resource::source_crc(&res).map_err(|e| e.to_string())? != 0 {
        return Err("DATA prefix is not the crc32 of the layout text".into());
    }
    if !text.trim_start().starts_with("<!--") && !text.trim_start().starts_with("<root>") {
        return Err("layout text does not start with <root>".into());
    }
    Ok(())
}

fn check_script(data: &[u8]) -> Result<(), String> {
    let res = Resource::parse(data).map_err(|e| e.to_string())?;
    if res.type_version < 4 {
        return Err(format!(
            "script version {} keeps no plain text",
            res.type_version
        ));
    }
    let block = res.block(b"DATA").ok_or("no DATA block")?;
    std::str::from_utf8(&block.data).map_err(|_| "script is not UTF-8".to_string())?;
    Ok(())
}

fn layout_from(data: &[u8], original: &[u8]) -> Result<(), String> {
    let ours = inject::layout_text(data).map_err(|e| e.to_string())?;
    let theirs = inject::layout_text(original).map_err(|e| format!("game's original: {e}"))?;
    if !inject::extends(&ours, &theirs) {
        return Err("does not contain every line of the game's layout".into());
    }
    Ok(())
}

fn style_from(data: &[u8], original: &[u8]) -> Result<(), String> {
    let res = Resource::parse(data).map_err(|e| e.to_string())?;
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

fn parse_both(data: &[u8], original: &[u8]) -> Result<(Vtex, Vtex), String> {
    let ours = Vtex::parse(data).map_err(|e| e.to_string())?;
    let orig = Vtex::parse(original).map_err(|e| format!("game's original: {e}"))?;
    Ok((ours, orig))
}

fn top_mip_of(data: &[u8], original: &[u8]) -> Result<(), String> {
    let (ours, orig) = parse_both(data, original)?;
    if ours.mips.len() != 1 {
        return Err(format!(
            "{} mip levels; expected only the largest",
            ours.mips.len()
        ));
    }
    if !ours.flags.contains(Flags::NO_LOD) {
        return Err("NO_LOD flag is not set".into());
    }
    if (ours.width, ours.height, ours.format) != (orig.width, orig.height, orig.format) {
        return Err("size or format differs from the game's texture".into());
    }
    let top = orig.mips.first().map_or(0, |m| m.stored_len);
    if top > original.len()
        || data[ours.pixel_start().min(data.len())..] != original[original.len() - top..]
    {
        return Err("pixels are not the game texture's largest level".into());
    }
    Ok(())
}

fn pinned_lods_of(data: &[u8], original: &[u8]) -> Result<(), String> {
    let (masks, distances) = native_sinner::lod_fields(data).map_err(|e| e.to_string())?;
    let (orig_masks, orig_distances) =
        native_sinner::lod_fields(original).map_err(|e| format!("game's original: {e}"))?;
    let want_masks: Vec<u64> = orig_masks
        .iter()
        .enumerate()
        .map(|(i, _)| {
            if i == 0 {
                orig_masks.iter().fold(0, |a, m| a | m)
            } else {
                0
            }
        })
        .collect();
    let want_distances: Vec<f64> = orig_distances
        .iter()
        .enumerate()
        .map(|(i, d)| if i == 0 { *d } else { native_sinner::FAR })
        .collect();
    if masks != want_masks || distances != want_distances {
        return Err(format!(
            "LOD arrays are {masks:?} / {distances:?}, expected {want_masks:?} / {want_distances:?}"
        ));
    }
    let res = Resource::parse(data).map_err(|e| e.to_string())?;
    let orig = Resource::parse(original).map_err(|e| format!("game's original: {e}"))?;
    let others = |r: &Resource| {
        r.blocks
            .iter()
            .filter(|b| &b.name != b"DATA")
            .cloned()
            .collect::<Vec<_>>()
    };
    if others(&res) != others(&orig) {
        return Err("a block other than DATA differs from the game's model".into());
    }
    Ok(())
}

fn resampled_from(data: &[u8], original: &[u8]) -> Result<(), String> {
    let (ours, orig) = parse_both(data, original)?;
    if ours.format != orig.format || ours.flags != orig.flags {
        return Err("format or flags differ from the game's texture".into());
    }
    if ours.mips.len() != 1 {
        return Err(format!("{} mip levels; expected one", ours.mips.len()));
    }
    let side = ours.width.max(ours.height);
    let want = native_scope::target_dims(orig.width, orig.height, side);
    if side >= orig.width.max(orig.height) || (ours.width, ours.height) != want {
        return Err(format!(
            "{}x{} is not the game's {}x{} texture scaled to {side}",
            ours.width, ours.height, orig.width, orig.height
        ));
    }
    Ok(())
}

fn replaced_image_of(data: &[u8], original: &[u8]) -> Result<(), String> {
    let (ours, _) = parse_both(data, original)?;
    if ours.format.0 != encode::BGRA8888 || ours.mips.len() != 1 {
        return Err(format!(
            "{} with {} mip levels; expected one BGRA8888 level",
            ours.format.name(),
            ours.mips.len()
        ));
    }
    if !ours.flags.contains(Flags::NO_LOD) {
        return Err("NO_LOD flag is not set".into());
    }
    let want = usize::from(ours.width) * usize::from(ours.height) * 4;
    if data.len() != ours.pixel_start() + want {
        return Err(format!(
            "pixel data is {} bytes, {}x{} BGRA8888 needs {want}",
            data.len().saturating_sub(ours.pixel_start()),
            ours.width,
            ours.height
        ));
    }
    let red2 = |bytes: &[u8]| {
        Resource::parse(bytes)
            .ok()
            .and_then(|r| r.block(b"RED2").cloned())
    };
    if red2(data) != red2(original) {
        return Err("RED2 block differs from the game's image".into());
    }
    Ok(())
}

/// What `id`'s pak must match, from the game's own files. A game archive that cannot be
/// read leaves those checks out; the pak's own integrity is always checked.
pub fn expect_for(id: AddonId, paths: &GamePaths) -> Expect {
    match info(id).kind {
        Kind::Native(native) => VpkDir::open(&paths.citadel_dir.join(GAME_PAK))
            .ok()
            .and_then(|pak| native.expect(&pak).ok())
            .unwrap_or_default(),
        Kind::Toggle | Kind::Textures => Expect::default(),
    }
}

/// The HUD pak patches whichever stylesheets and layouts the layout touches and replaces
/// whichever images the player swapped; each must come from `game`'s file at the same path.
/// Our own files under `deadtune/` have no game original and get the content checks only.
pub fn expect_for_hud(game: &VpkDir, pak: &VpkDir) -> Expect {
    let checks = pak
        .entries
        .keys()
        .filter_map(|p| {
            let check = match p.rsplit_once('.').map_or("", |(_, ext)| ext) {
                "vcss_c" | "vsvg_c" => Check::StyleFrom,
                "vxml_c" => Check::LayoutFrom,
                "vtex_c" => Check::ReplacedImageOf,
                _ => return None,
            };
            game.read(p).ok().map(|b| (p.clone(), check(b)))
        })
        .collect();
    Expect {
        particle_stub: None,
        checks,
    }
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
            .map(|pak| verify(&pak, &expect_for(id, paths)))
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
        let game = VpkDir::open(&paths.citadel_dir.join(GAME_PAK)).ok();
        let result = VpkDir::open(&hud)
            .map(|pak| {
                let expect = game
                    .as_ref()
                    .map_or_else(Expect::default, |game| expect_for_hud(game, &pak));
                verify(&pak, &expect)
            })
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
    use crate::addons::native::tests::fake_game;
    use crate::addons::{AddonsConfig, Native, ScopeOptions, install, native_blur, sources};
    use crate::hud::vpk;
    use crate::texture::vtex::tests::{COLOR, MASK};

    fn upstream(dir: &str, file: &str) -> VpkDir {
        VpkDir::open(&sources::tests::research(dir, file)).unwrap()
    }

    fn one(path: &str, bytes: Vec<u8>) -> VpkDir {
        VpkDir::in_memory(vpk::write(&BTreeMap::from([(path.to_string(), bytes)]))).unwrap()
    }

    fn with_check(path: &str, check: Check) -> Expect {
        Expect {
            particle_stub: None,
            checks: BTreeMap::from([(path.to_string(), check)]),
        }
    }

    #[test]
    fn every_upstream_file_passes_the_generic_checks() {
        for (dir, file) in [
            ("Sinner Light Fix Mod", "pak26_dir.vpk"),
            ("Vindicta Scope Downscale", "pak89_dir.vpk"),
            ("Optimized Soul Container", "pak01_dir.vpk"),
            ("Blur Disabler", "pak97_dir.vpk"),
            ("Screenspace Particle Disabler", "pak02_dir.vpk"),
        ] {
            let pak = upstream(dir, file);
            let got = verify(&pak, &Expect::default());
            assert!(got.is_ok(), "{file}: {got}");
            assert_eq!(got.entries, pak.entries.len());
        }
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
        let mut res = Resource::parse(&native_blur::tests::vanilla_style()).unwrap();
        let data = &mut res
            .blocks
            .iter_mut()
            .find(|b| &b.name == b"DATA")
            .unwrap()
            .data;
        data[4..6].copy_from_slice(&7u16.to_le_bytes());
        let got = verify(&one(native_blur::STYLE, res.to_bytes()), &Expect::default());
        assert_eq!(got.problems.len(), 1, "{got}");
        assert!(
            got.problems[0].contains("image"),
            "names the image table: {got}"
        );
    }

    #[test]
    fn a_stylesheet_from_another_source_fails_the_original_comparison() {
        let vanilla = native_blur::tests::vanilla_style();
        let other = include_bytes!("../../tests/fixtures/hud/hud_vanilla.vcss_c").to_vec();
        let got = verify(
            &one(native_blur::STYLE, other),
            &with_check(native_blur::STYLE, Check::StyleFrom(vanilla)),
        );
        assert_eq!(got.problems.len(), 1, "{got}");
        assert!(
            got.problems[0].contains("differs from the game's file"),
            "{got}"
        );
    }

    #[test]
    fn the_blur_base_must_be_the_games_current_file() {
        let game = fake_game();
        let mut files = Native::Blur.build(&game, &AddonsConfig::default()).unwrap();
        let expect = Native::Blur.expect(&game).unwrap();
        assert!(verify(&VpkDir::in_memory(vpk::write(&files)).unwrap(), &expect).is_ok());
        files.insert(
            native_blur::BASE.to_string(),
            include_bytes!("../../tests/fixtures/hud/hud_vanilla.vcss_c").to_vec(),
        );
        let got = verify(&VpkDir::in_memory(vpk::write(&files)).unwrap(), &expect);
        assert_eq!(
            got.problems,
            [format!(
                "{}: not the game's current file",
                native_blur::BASE
            )]
        );
    }

    #[test]
    fn a_particle_pak_with_another_particle_fails() {
        let game = fake_game();
        let mut files = Native::Particles
            .build(&game, &AddonsConfig::default())
            .unwrap();
        let expect = Native::Particles.expect(&game).unwrap();
        assert!(verify(&VpkDir::in_memory(vpk::write(&files)).unwrap(), &expect).is_ok());
        let mut wrong = expect.particle_stub.clone().unwrap();
        wrong.push(0);
        files.insert(
            "particles/abilities/lash/lash_final_strike_screen.vpcf_c".into(),
            wrong,
        );
        let got = verify(&VpkDir::in_memory(vpk::write(&files)).unwrap(), &expect);
        assert_eq!(
            got.problems,
            [
                "particles/abilities/lash/lash_final_strike_screen.vpcf_c: not the game's empty particle"
            ]
        );
    }

    #[test]
    fn the_sinner_mask_must_be_the_games_top_level_alone_and_the_model_pinned() {
        let game = fake_game();
        let files = Native::Sinner
            .build(&game, &AddonsConfig::default())
            .unwrap();
        let expect = Native::Sinner.expect(&game).unwrap();
        let mask = game.read(native_sinner::MASK).unwrap();
        let got = verify(&one(native_sinner::MASK, mask), &expect);
        assert_eq!(got.problems.len(), 1, "{got}");
        assert!(got.problems[0].contains("mip levels"), "{got}");

        let mut ours = files[native_sinner::MASK].clone();
        let last = ours.len() - 1;
        ours[last] ^= 0xff;
        let got = verify(&one(native_sinner::MASK, ours), &expect);
        assert!(got.problems[0].contains("largest level"), "{got}");

        let model = game.read(native_sinner::MODEL).unwrap();
        let got = verify(&one(native_sinner::MODEL, model), &expect);
        assert_eq!(got.problems.len(), 1, "{got}");
        assert!(got.problems[0].contains("LOD arrays"), "{got}");
        let upstream = upstream("Sinner Light Fix Mod", "pak26_dir.vpk");
        assert!(
            verify(&upstream, &expect).is_ok(),
            "upstream is the same rebuild"
        );
    }

    #[test]
    fn the_scope_texture_must_be_the_games_scaled_with_its_aspect() {
        let game = fake_game();
        let config = AddonsConfig {
            scope: ScopeOptions { side: 256 },
            ..AddonsConfig::default()
        };
        let files = Native::Scope.build(&game, &config).unwrap();
        let expect = Native::Scope.expect(&game).unwrap();
        let pak = VpkDir::in_memory(vpk::write(&files)).unwrap();
        assert!(verify(&pak, &expect).is_ok());

        let ours = &files[native_scope::TEXTURE];
        let v = Vtex::parse(ours).unwrap();
        let mut res = Resource::parse(ours).unwrap();
        let data = &mut res
            .blocks
            .iter_mut()
            .find(|b| &b.name == b"DATA")
            .unwrap()
            .data;
        data[22..24].copy_from_slice(&128u16.to_le_bytes());
        let mut squashed = res.to_bytes();
        squashed.extend_from_slice(&ours[v.pixel_start()..v.pixel_start() + 256 * 128 * 4]);
        let got = verify(&one(native_scope::TEXTURE, squashed), &expect);
        assert_eq!(got.problems.len(), 1, "{got}");
        assert!(got.problems[0].contains("256x128"), "{got}");

        let upstream = upstream("Vindicta Scope Downscale", "pak89_dir.vpk")
            .read(native_scope::TEXTURE)
            .unwrap();
        let got = verify(&one(native_scope::TEXTURE, upstream), &expect);
        assert!(
            got.problems[0].contains("1080x1080"),
            "larger than this fake game's 512 original: {got}"
        );
        let copy = game.read(native_scope::TEXTURE).unwrap();
        let got = verify(&one(native_scope::TEXTURE, copy), &expect);
        assert!(
            got.problems[0].contains("512x512"),
            "an unchanged copy is not a downscale: {got}"
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
    fn verify_installed_covers_native_addons_and_the_hud_pak() {
        let (steam, paths) = install::tests::fake_install("1");
        let state = steam.path().join("data");
        assert!(verify_installed(&paths, &state).is_empty());
        let config = AddonsConfig {
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
        assert!(
            text[1].starts_with("Sinner's Sacrifice light fix (pak73_dir.vpk): ok: 2 entries"),
            "{text:?}"
        );

        let mut base = native_blur::tests::vanilla_style();
        base[100] ^= 1;
        std::fs::write(
            paths.citadel_dir.join(GAME_PAK),
            vpk::write(&BTreeMap::from([(native_blur::STYLE.to_string(), base)])),
        )
        .unwrap();
        let reports = verify_installed(&paths, &state);
        let blur = &reports[0].result.as_ref().unwrap().problems;
        assert_eq!(
            blur,
            &[format!(
                "{}: not the game's current file",
                native_blur::BASE
            )],
            "a game update shows up as a stale base"
        );

        std::fs::write(addons_dir(&paths).join("pak73_dir.vpk"), b"broken").unwrap();
        let reports = verify_installed(&paths, &state);
        assert!(reports[1].result.is_err(), "{reports:?}");
    }
}
