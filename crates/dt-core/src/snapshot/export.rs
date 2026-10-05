//! Takes a snapshot: streams the selected files out of the game one at a time into
//! `raw/`, decodes them into `text/`, and writes `manifest.toml` last. A build that already
//! has a folder is reused; files already there with the same CRC are skipped.

use std::ops::ControlFlow;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use chrono::Utc;

use super::SnapshotError;
use super::decode::{self, Kind};
use super::spec::{self, Candidate, Inventory, Selection, Source};
use super::store::{self, Decoded, FileEntry, Manifest, Stored};
use crate::backup::{atomic_write, sha256_hex};
use crate::hud::install::GAME_PAK;
use crate::hud::vpk::VpkDir;
use crate::locate::{self, GamePaths};
use crate::texture::vtex::Vtex;

#[derive(Clone, Copy, Debug)]
pub struct Progress<'a> {
    pub done: usize,
    pub total: usize,
    /// The file just handled.
    pub path: &'a str,
    /// Bytes written so far.
    pub bytes: u64,
}

#[derive(Clone, Debug)]
pub struct Outcome {
    pub folder: PathBuf,
    pub manifest: Manifest,
    pub written: usize,
    /// Files already in the folder with the same CRC.
    pub reused: usize,
    pub bytes: u64,
    pub elapsed: Duration,
}

/// Removes a freshly created folder when the take does not finish.
struct Cleanup {
    folder: PathBuf,
    armed: bool,
}

impl Drop for Cleanup {
    fn drop(&mut self) {
        if self.armed {
            let _ = std::fs::remove_dir_all(&self.folder);
        }
    }
}

fn write_file(path: &Path, bytes: &[u8]) -> Result<(), SnapshotError> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    atomic_write(path, bytes)?;
    Ok(())
}

fn stored_kind(candidate: &Candidate, selection: &Selection) -> Stored {
    if selection.size_cap.is_none_or(|cap| candidate.size <= cap) {
        Stored::Full
    } else if candidate.path.ends_with(".vtex_c") {
        Stored::Header
    } else {
        Stored::None
    }
}

fn read_candidate(
    candidate: &Candidate,
    pak: &VpkDir,
    paths: &GamePaths,
) -> Result<Vec<u8>, SnapshotError> {
    match spec::loose_path(paths, candidate) {
        Some(path) => Ok(std::fs::read(path)?),
        None => Ok(pak.read(&candidate.path)?),
    }
}

/// Everything in `previous` for this file is still valid: same CRC, the same storage,
/// the files on disk, and text present when wanted.
fn reusable(
    previous: &FileEntry,
    folder: &Path,
    crc: &str,
    stored: Stored,
    want_text: bool,
) -> bool {
    previous.crc == crc
        && previous.stored == stored
        && previous.raw_path(folder).is_none_or(|p| p.is_file())
        && (!want_text || previous.text_path(folder).is_some_and(|p| p.is_file()))
}

pub fn take(
    paths: &GamePaths,
    data_dir: &Path,
    selection: &Selection,
    progress: &mut dyn FnMut(Progress) -> ControlFlow<()>,
) -> Result<Outcome, SnapshotError> {
    let started = Instant::now();
    let pak_path = paths.citadel_dir.join(GAME_PAK);
    if !pak_path.is_file() {
        return Err(SnapshotError::MissingGamePak(pak_path));
    }
    let pak = VpkDir::open(&pak_path)?;
    let inventory = Inventory::from_pak(&pak, spec::loose_files(paths)?);
    let buildid = locate::buildid(paths);
    let taken = Utc::now();
    let root = store::dir(data_dir);
    let existing = store::list(&root)?
        .into_iter()
        .find(|s| s.buildid == buildid);
    let (folder, fresh) = match existing {
        Some(s) => (s.folder, false),
        None => (
            root.join(store::folder_name(buildid.as_deref(), taken)),
            true,
        ),
    };
    let previous = Manifest::load(&folder).ok();
    let mut cleanup = Cleanup {
        folder: folder.clone(),
        armed: fresh,
    };
    std::fs::create_dir_all(folder.join(store::RAW))?;

    let selected: Vec<&Candidate> = inventory.selected(selection).collect();
    let total = selected.len();
    let mut files = Vec::with_capacity(total);
    let (mut written, mut reused, mut bytes) = (0, 0, 0u64);
    for (i, candidate) in selected.iter().enumerate() {
        let crc = format!("{:08x}", candidate.crc);
        let stored = stored_kind(candidate, selection);
        let kind = Kind::of(&candidate.path);
        let want_text = selection.decode && kind.is_some();
        let keep = previous
            .as_ref()
            .and_then(|m| m.entry(&candidate.path))
            .filter(|e| reusable(e, &folder, &crc, stored, want_text));
        match keep {
            Some(entry) => {
                files.push(entry.clone());
                reused += 1;
            }
            None => {
                let data = read_candidate(candidate, &pak, paths)?;
                let raw = folder.join(store::RAW).join(&candidate.path);
                match stored {
                    Stored::Full => write_file(&raw, &data)?,
                    Stored::Header => {
                        let end = Vtex::parse(&data)
                            .map(|v| v.pixel_start())
                            .unwrap_or(data.len());
                        let header = folder
                            .join(store::RAW)
                            .join(format!("{}.header", candidate.path));
                        write_file(&header, &data[..end])?;
                    }
                    Stored::None => {}
                }
                let (text, decoded) = match (want_text, kind) {
                    (true, Some(kind)) => match decode::decode(kind, &data) {
                        Ok(text) => {
                            let rel = kind.text_path(&candidate.path);
                            write_file(&folder.join(store::TEXT).join(&rel), text.as_bytes())?;
                            (Some(rel), Decoded::Text)
                        }
                        Err(error) => {
                            let rel = decode::strings_path(&candidate.path);
                            let text = decode::strings(&data, &error);
                            write_file(&folder.join(store::TEXT).join(&rel), text.as_bytes())?;
                            (Some(rel), Decoded::Strings)
                        }
                    },
                    _ => (None, Decoded::None),
                };
                files.push(FileEntry {
                    path: candidate.path.clone(),
                    source: candidate.source,
                    size: candidate.size,
                    crc,
                    sha256: sha256_hex(&data),
                    categories: candidate.categories.clone(),
                    stored,
                    text,
                    decoded,
                });
                written += 1;
                if stored == Stored::Full {
                    bytes += data.len() as u64;
                }
            }
        }
        let report = Progress {
            done: i + 1,
            total,
            path: &candidate.path,
            bytes,
        };
        if progress(report).is_break() {
            return Err(SnapshotError::Cancelled);
        }
    }

    write_file(&folder.join(store::PAK_LIST), pak_list(&pak).as_bytes())?;
    let manifest = Manifest {
        buildid,
        taken,
        deadtune: env!("CARGO_PKG_VERSION").into(),
        game_root: paths.game_root.clone(),
        decode: selection.decode,
        size_cap: selection.size_cap,
        categories: selection.categories.clone(),
        files,
    };
    manifest.save(&folder)?;
    cleanup.armed = false;
    Ok(Outcome {
        folder,
        manifest,
        written,
        reused,
        bytes,
        elapsed: started.elapsed(),
    })
}

/// One line per pak entry: path, size, crc, tab-separated.
pub fn pak_list(pak: &VpkDir) -> String {
    let mut out = String::new();
    for (path, e) in &pak.entries {
        out.push_str(path);
        out.push('\t');
        out.push_str(&(e.length as u64 + e.preload.len() as u64).to_string());
        out.push('\t');
        out.push_str(&format!("{:08x}\n", e.crc));
    }
    out
}

/// `Source::Pak01` candidates come from the pak; the rest from disk.
pub fn source_label(source: Source) -> &'static str {
    match source {
        Source::Pak01 => "game archive",
        Source::Loose => "game folder",
        Source::Steam => "Steam",
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use std::collections::BTreeMap;

    use super::*;
    use crate::addons::native_particles::EMPTY_PARTICLE;
    use crate::addons::native_scope::TEXTURE;
    use crate::hud::apples_tunnels::MINIMAP_LAYOUT;
    use crate::hud::elements::HUD_STYLE;
    use crate::hud::inject;
    use crate::hud::topbar::TOP_BAR_LAYOUT;
    use crate::hud::vpk;
    use crate::snapshot::Category;
    use crate::texture::vtex::tests::COLOR;

    const STYLE: &[u8] = include_bytes!("../../tests/fixtures/hud/hud_abilities_small.vcss_c");
    const TOP_BAR: &[u8] = include_bytes!("../../tests/fixtures/hud/top_bar_vanilla.vxml_c");
    const MINIMAP: &[u8] = include_bytes!("../../tests/fixtures/hud/hud_minimap_vanilla.vxml_c");
    const PARTICLE: &[u8] = include_bytes!("../../tests/fixtures/particles/empty.vpcf_c");
    const GAMEINFO: &str = include_str!("../../tests/fixtures/gameinfo_live_2026-09-29.gi");

    /// A pak carrying each decodable kind, a DeadTune target, a broken layout and a texture.
    pub fn fake_files() -> BTreeMap<String, Vec<u8>> {
        BTreeMap::from([
            (HUD_STYLE.to_string(), STYLE.to_vec()),
            (TOP_BAR_LAYOUT.to_string(), TOP_BAR.to_vec()),
            (MINIMAP_LAYOUT.to_string(), MINIMAP.to_vec()),
            (
                "panorama/scripts/hud_clock.vjs_c".to_string(),
                inject::script_resource("(function(){ $.Msg('tick'); })();"),
            ),
            (
                "panorama/layout/popups/popup_settings.vxml_c".to_string(),
                b"not a resource".to_vec(),
            ),
            (
                "panorama/scripts/hud_big.vjs_c".to_string(),
                inject::script_resource(&"// padding\n".repeat(3000)),
            ),
            (EMPTY_PARTICLE.to_string(), PARTICLE.to_vec()),
            (TEXTURE.to_string(), COLOR.to_vec()),
            ("models/heroes/x/x.vmdl_c".to_string(), vec![1, 2, 3]),
        ])
    }

    /// A Steam library with the game, `pak01_dir.vpk` from `pak`, and the given buildid.
    pub fn fake_install(pak: &[u8], buildid: &str) -> (tempfile::TempDir, GamePaths) {
        let tmp = tempfile::tempdir().unwrap();
        let steamapps = tmp.path().join("steamapps");
        let root = steamapps.join("common").join("Deadlock");
        let citadel = root.join("game").join("citadel");
        std::fs::create_dir_all(citadel.join("cfg")).unwrap();
        std::fs::write(citadel.join("gameinfo.gi"), GAMEINFO).unwrap();
        std::fs::write(citadel.join("cfg/video.txt"), "\"config\"\n{\n}\n").unwrap();
        std::fs::write(citadel.join("cfg/autoexec.cfg"), "fps_max 240\n").unwrap();
        std::fs::write(citadel.join("cfg/notes.md"), "not a cfg file").unwrap();
        std::fs::write(citadel.join(GAME_PAK), pak).unwrap();
        write_manifest(&steamapps, buildid);
        let paths = locate::from_game_root(&root).unwrap();
        (tmp, paths)
    }

    pub fn write_manifest(steamapps: &Path, buildid: &str) {
        let acf = format!(
            "\"AppState\"\n{{\n\t\"appid\"\t\t\"1422450\"\n\t\"buildid\"\t\t\"{buildid}\"\n}}\n"
        );
        std::fs::write(steamapps.join("appmanifest_1422450.acf"), acf).unwrap();
    }

    fn go(_: Progress) -> ControlFlow<()> {
        ControlFlow::Continue(())
    }

    #[test]
    fn take_writes_raw_text_and_manifest() {
        let (tmp, paths) = fake_install(&vpk::write(&fake_files()), "100");
        let data = tmp.path().join("data");
        let selection = Selection {
            size_cap: Some(20_000),
            ..Selection::default()
        };
        let mut seen = Vec::new();
        let out = take(&paths, &data, &selection, &mut |p| {
            seen.push((p.done, p.total));
            ControlFlow::Continue(())
        })
        .unwrap();
        assert_eq!(out.written, 12, "8 pak files and 4 loose ones");
        assert_eq!(out.reused, 0);
        assert_eq!(seen.last(), Some(&(12, 12)));
        assert!(out.folder.starts_with(data.join("game-files")));
        assert!(
            out.folder
                .file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with("100-")
        );
        let m = Manifest::load(&out.folder).unwrap();
        assert_eq!(m, out.manifest);
        assert_eq!(m.buildid.as_deref(), Some("100"));
        assert_eq!(m.categories.len(), 6);

        let raw = |p: &str| out.folder.join("raw").join(p);
        let text = |p: &str| std::fs::read_to_string(out.folder.join("text").join(p)).unwrap();
        assert_eq!(std::fs::read(raw(HUD_STYLE)).unwrap(), STYLE);
        assert_eq!(std::fs::read(raw(TOP_BAR_LAYOUT)).unwrap(), TOP_BAR);
        assert_eq!(
            std::fs::read(raw("gameinfo.gi")).unwrap(),
            GAMEINFO.as_bytes()
        );
        assert_eq!(
            std::fs::read_to_string(raw("cfg/autoexec.cfg")).unwrap(),
            "fps_max 240\n"
        );
        assert!(!raw("cfg/notes.md").exists());
        assert!(raw("steam/appmanifest_1422450.acf").is_file());
        assert!(!raw("models/heroes/x/x.vmdl_c").exists(), "not selected");

        assert!(text("panorama/styles/hud.css").contains('{'));
        assert!(text("panorama/layout/citadel_hud_top_bar.xml").starts_with("<root>"));
        assert_eq!(
            text("panorama/scripts/hud_clock.js"),
            "(function(){ $.Msg('tick'); })();"
        );
        let strings = text("panorama/layout/popups/popup_settings.vxml_c.strings.txt");
        assert!(
            strings.starts_with("DeadTune could not decode"),
            "{strings}"
        );
        assert!(strings.contains("not a resource"));

        let entry = |p: &str| m.entry(p).unwrap().clone();
        assert_eq!(entry(HUD_STYLE).decoded, Decoded::Text);
        assert_eq!(
            entry(HUD_STYLE).categories,
            [Category::Hud, Category::Panorama, Category::Deadtune]
        );
        assert_eq!(
            entry(HUD_STYLE).crc,
            format!("{:08x}", crate::hud::crc32::crc32(STYLE))
        );
        assert_eq!(entry(HUD_STYLE).sha256, sha256_hex(STYLE));
        let settings = entry("panorama/layout/popups/popup_settings.vxml_c");
        assert_eq!(settings.decoded, Decoded::Strings);
        assert_eq!(
            settings.text.as_deref(),
            Some("panorama/layout/popups/popup_settings.vxml_c.strings.txt")
        );
        assert_eq!(entry(EMPTY_PARTICLE).decoded, Decoded::None);
        assert_eq!(entry(EMPTY_PARTICLE).stored, Stored::Full);
        let big = entry("panorama/scripts/hud_big.vjs_c");
        assert_eq!(big.stored, Stored::None, "over the cap");
        assert!(!raw("panorama/scripts/hud_big.vjs_c").exists());
        assert_eq!(big.decoded, Decoded::Text, "text is still decoded");
        assert!(text("panorama/scripts/hud_big.js").starts_with("// padding"));
        let texture = entry(TEXTURE);
        assert_eq!(texture.stored, Stored::Header);
        let header = std::fs::read(raw(&format!("{TEXTURE}.header"))).unwrap();
        assert_eq!(header, &COLOR[..Vtex::parse(COLOR).unwrap().pixel_start()]);
        assert_eq!(
            texture.raw_path(&out.folder),
            Some(raw(&format!("{TEXTURE}.header")))
        );
        assert_eq!(entry("gameinfo.gi").source, Source::Loose);

        let list = std::fs::read_to_string(out.folder.join("pak01.tsv")).unwrap();
        assert_eq!(list.lines().count(), 9);
        assert!(list.contains("models/heroes/x/x.vmdl_c\t3\t"));
        assert_eq!(out.bytes, m.bytes_stored());
    }

    #[test]
    fn decode_off_and_fewer_categories_store_less() {
        let (tmp, paths) = fake_install(&vpk::write(&fake_files()), "100");
        let data = tmp.path().join("data");
        let selection = Selection {
            categories: [Category::Config].into_iter().collect(),
            decode: false,
            size_cap: None,
        };
        let out = take(&paths, &data, &selection, &mut go).unwrap();
        assert_eq!(out.written, 4);
        assert!(out.manifest.files.iter().all(|f| f.text.is_none()));
        assert!(!out.folder.join("text").exists());
        let selection = Selection {
            categories: [Category::Hud].into_iter().collect(),
            decode: false,
            size_cap: None,
        };
        let out = take(&paths, &data, &selection, &mut go).unwrap();
        assert_eq!(out.written, 5);
        assert!(out.manifest.files.iter().all(|f| f.text.is_none()));
        assert!(!out.folder.join("text").exists());
    }

    #[test]
    fn taking_the_same_build_again_reuses_the_folder_and_skips_files() {
        let (tmp, paths) = fake_install(&vpk::write(&fake_files()), "100");
        let data = tmp.path().join("data");
        let first = take(&paths, &data, &Selection::default(), &mut go).unwrap();
        let second = take(&paths, &data, &Selection::default(), &mut go).unwrap();
        assert_eq!(second.folder, first.folder);
        assert_eq!((second.written, second.reused), (0, 12));
        assert_eq!(second.manifest.files, first.manifest.files);
        assert_eq!(store::list(&store::dir(&data)).unwrap().len(), 1);

        let mut files = fake_files();
        files.insert(HUD_STYLE.to_string(), inject::style_resource("#x{}"));
        std::fs::write(paths.citadel_dir.join(GAME_PAK), vpk::write(&files)).unwrap();
        let third = take(&paths, &data, &Selection::default(), &mut go).unwrap();
        assert_eq!(third.folder, first.folder);
        assert_eq!((third.written, third.reused), (1, 11));
        assert_eq!(
            std::fs::read_to_string(third.folder.join("text/panorama/styles/hud.css")).unwrap(),
            "#x{}"
        );

        let narrow = Selection {
            categories: [Category::Config].into_iter().collect(),
            ..Selection::default()
        };
        let fourth = take(&paths, &data, &narrow, &mut go).unwrap();
        assert_eq!((fourth.written, fourth.reused), (0, 4));
        assert_eq!(fourth.manifest.files.len(), 4);
        assert!(
            fourth.folder.join("raw").join(HUD_STYLE).is_file(),
            "files of a wider take stay on disk"
        );

        write_manifest(&tmp.path().join("steamapps"), "101");
        let paths = locate::from_game_root(&paths.game_root).unwrap();
        let fifth = take(&paths, &data, &Selection::default(), &mut go).unwrap();
        assert_ne!(fifth.folder, first.folder);
        assert_eq!(fifth.written, 12);
        assert_eq!(store::list(&store::dir(&data)).unwrap().len(), 2);
    }

    #[test]
    fn cancelling_a_fresh_take_removes_its_folder() {
        let (tmp, paths) = fake_install(&vpk::write(&fake_files()), "100");
        let data = tmp.path().join("data");
        let err = take(&paths, &data, &Selection::default(), &mut |p| {
            if p.done == 3 {
                ControlFlow::Break(())
            } else {
                ControlFlow::Continue(())
            }
        })
        .unwrap_err();
        assert!(matches!(err, SnapshotError::Cancelled));
        assert!(store::list(&store::dir(&data)).unwrap().is_empty());
        assert_eq!(
            std::fs::read_dir(store::dir(&data)).unwrap().count(),
            0,
            "no folder left behind"
        );

        let first = take(&paths, &data, &Selection::default(), &mut go).unwrap();
        let err = take(&paths, &data, &Selection::default(), &mut |_| {
            ControlFlow::Break(())
        })
        .unwrap_err();
        assert!(matches!(err, SnapshotError::Cancelled));
        assert_eq!(
            Manifest::load(&first.folder).unwrap(),
            first.manifest,
            "a reused folder keeps its manifest"
        );
    }

    #[test]
    fn missing_pak_is_an_error_before_anything_is_written() {
        let (tmp, paths) = fake_install(b"", "100");
        std::fs::remove_file(paths.citadel_dir.join(GAME_PAK)).unwrap();
        let data = tmp.path().join("data");
        assert!(matches!(
            take(&paths, &data, &Selection::default(), &mut go),
            Err(SnapshotError::MissingGamePak(_))
        ));
        assert!(!data.exists());
    }
}
