//! What a snapshot holds: categories as path rules over the pak tree, the loose files,
//! and the player's selection.

use std::collections::BTreeSet;
use std::path::Path;

use super::{SnapshotError, dependencies};
use crate::hud::crc32::crc32;
use crate::hud::install::GAME_PAK;
use crate::hud::vpk::VpkDir;
use crate::locate::{APP_ID, GamePaths};

#[derive(
    Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Serialize, serde::Deserialize,
)]
#[serde(rename_all = "snake_case")]
pub enum Category {
    Hud,
    Settings,
    Menu,
    Panorama,
    Deadtune,
    Config,
}

impl Category {
    pub const ALL: [Category; 6] = [
        Category::Hud,
        Category::Settings,
        Category::Menu,
        Category::Panorama,
        Category::Deadtune,
        Category::Config,
    ];

    /// The CLI and manifest name.
    pub fn key(self) -> &'static str {
        match self {
            Category::Hud => "hud",
            Category::Settings => "settings",
            Category::Menu => "menu",
            Category::Panorama => "panorama",
            Category::Deadtune => "deadtune",
            Category::Config => "config",
        }
    }

    pub fn parse(text: &str) -> Option<Category> {
        Category::ALL.into_iter().find(|c| c.key() == text)
    }

    pub fn label(self) -> &'static str {
        match self {
            Category::Hud => "HUD",
            Category::Settings => "Settings menu",
            Category::Menu => "Main menu",
            Category::Panorama => "Every interface file",
            Category::Deadtune => "Files DeadTune builds from",
            Category::Config => "Game settings files",
        }
    }

    pub fn blurb(self) -> &'static str {
        match self {
            Category::Hud => {
                "In-game HUD, top bar, minimap and health bar layouts, styles and scripts."
            }
            Category::Settings => "The game's settings menu.",
            Category::Menu => "The main menu and dashboard.",
            Category::Panorama => {
                "Every layout, stylesheet and script of the game's interface (text only)."
            }
            Category::Deadtune => {
                "Every game file a DeadTune feature reads: the addons, the HUD pages and the planned settings rows."
            }
            Category::Config => "gameinfo.gi, the cfg folder and the Steam build id.",
        }
    }
}

/// Where a file comes from.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Source {
    Pak01,
    /// Under `game/citadel`.
    Loose,
    /// The Steam `appmanifest`, stored under `steam/`.
    Steam,
}

const PANORAMA_DIRS: [&str; 3] = ["panorama/layout/", "panorama/styles/", "panorama/scripts/"];
const PANORAMA_EXTS: [&str; 3] = [".vxml_c", ".vcss_c", ".vjs_c"];
const HUD_STEMS: [&str; 5] = ["hud", "citadel_hud", "minimap", "top_bar", "health"];
const MENU_STEMS: [&str; 1] = ["base"];
const MENU_PARTS: [&str; 2] = ["dashboard", "main_menu"];
const SETTINGS_PARTS: [&str; 1] = ["settings"];
const CONFIG_EXTS: [&str; 3] = [".cfg", ".vcfg", ".txt"];

fn file_name(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or(path)
}

fn is_panorama(path: &str) -> bool {
    PANORAMA_DIRS.iter().any(|d| path.starts_with(d))
        && PANORAMA_EXTS.iter().any(|e| path.ends_with(e))
}

/// The categories a pak entry belongs to. `deadtune` is the dependency path set.
pub fn categories_of(path: &str, deadtune: &BTreeSet<&str>) -> Vec<Category> {
    let mut out = Vec::new();
    if is_panorama(path) {
        let name = file_name(path);
        if HUD_STEMS.iter().any(|s| name.starts_with(s)) {
            out.push(Category::Hud);
        }
        if SETTINGS_PARTS.iter().any(|s| name.contains(s)) {
            out.push(Category::Settings);
        }
        if MENU_STEMS.iter().any(|s| name.starts_with(s))
            || MENU_PARTS.iter().any(|s| name.contains(s))
        {
            out.push(Category::Menu);
        }
        out.push(Category::Panorama);
    }
    if deadtune.contains(path) {
        out.push(Category::Deadtune);
    }
    out
}

/// What the player chose to save.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct Selection {
    pub categories: BTreeSet<Category>,
    /// Write decoded text next to the raw bytes.
    pub decode: bool,
    /// Files above this many bytes are listed but not stored (`None` stores everything).
    pub size_cap: Option<u64>,
}

pub const DEFAULT_SIZE_CAP: u64 = 8 << 20;

impl Default for Selection {
    fn default() -> Selection {
        Selection {
            categories: Category::ALL.into_iter().collect(),
            decode: true,
            size_cap: Some(DEFAULT_SIZE_CAP),
        }
    }
}

impl Selection {
    pub fn includes(&self, categories: &[Category]) -> bool {
        categories.iter().any(|c| self.categories.contains(c))
    }
}

/// One file a snapshot could hold.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Candidate {
    pub path: String,
    pub source: Source,
    pub size: u64,
    pub crc: u32,
    pub categories: Vec<Category>,
}

/// Everything a snapshot could hold, with sizes and CRCs from the pak tree (no file data
/// is read for pak entries).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Inventory {
    pub files: Vec<Candidate>,
    pub pak_entries: usize,
}

impl Inventory {
    pub fn scan(paths: &GamePaths) -> Result<Inventory, SnapshotError> {
        let pak_path = paths.citadel_dir.join(GAME_PAK);
        if !pak_path.is_file() {
            return Err(SnapshotError::MissingGamePak(pak_path));
        }
        Ok(Inventory::from_pak(
            &VpkDir::open(&pak_path)?,
            loose_files(paths)?,
        ))
    }

    pub fn from_pak(pak: &VpkDir, loose: Vec<Candidate>) -> Inventory {
        let deadtune: BTreeSet<&str> = dependencies()
            .into_iter()
            .flat_map(|(_, paths)| paths)
            .collect();
        let mut files: Vec<Candidate> = pak
            .entries
            .iter()
            .filter_map(|(path, e)| {
                let categories = categories_of(path, &deadtune);
                (!categories.is_empty()).then(|| Candidate {
                    path: path.clone(),
                    source: Source::Pak01,
                    size: e.length as u64 + e.preload.len() as u64,
                    crc: e.crc,
                    categories,
                })
            })
            .collect();
        files.extend(loose);
        Inventory {
            files,
            pak_entries: pak.entries.len(),
        }
    }

    /// Files and bytes in one category.
    pub fn totals(&self, category: Category) -> (usize, u64) {
        self.files
            .iter()
            .filter(|c| c.categories.contains(&category))
            .fold((0, 0), |(n, b), c| (n + 1, b + c.size))
    }

    pub fn selected<'a>(&'a self, selection: &'a Selection) -> impl Iterator<Item = &'a Candidate> {
        self.files
            .iter()
            .filter(move |c| selection.includes(&c.categories))
    }

    /// Files and bytes the selection would store (the size cap applied).
    pub fn selected_totals(&self, selection: &Selection) -> (usize, u64) {
        self.selected(selection).fold((0, 0), |(n, b), c| {
            let stored = selection.size_cap.is_none_or(|cap| c.size <= cap);
            (n + 1, b + if stored { c.size } else { 0 })
        })
    }
}

/// `gameinfo.gi`, the cfg files and the Steam appmanifest, CRCs computed here.
pub fn loose_files(paths: &GamePaths) -> Result<Vec<Candidate>, SnapshotError> {
    let mut out = Vec::new();
    let mut push = |path: String, source: Source, file: &Path| -> Result<(), SnapshotError> {
        let bytes = std::fs::read(file)?;
        out.push(Candidate {
            path,
            source,
            size: bytes.len() as u64,
            crc: crc32(&bytes),
            categories: vec![Category::Config],
        });
        Ok(())
    };
    push(super::GAMEINFO.into(), Source::Loose, &paths.gameinfo)?;
    if let Ok(dir) = std::fs::read_dir(&paths.cfg_dir) {
        let mut names: Vec<String> = dir
            .filter_map(|e| e.ok())
            .filter(|e| e.path().is_file())
            .filter_map(|e| e.file_name().into_string().ok())
            .filter(|n| CONFIG_EXTS.iter().any(|x| n.ends_with(x)))
            .collect();
        names.sort();
        for name in names {
            push(
                format!("cfg/{name}"),
                Source::Loose,
                &paths.cfg_dir.join(&name),
            )?;
        }
    }
    if let Some(acf) = &paths.appmanifest {
        push(
            format!("steam/appmanifest_{APP_ID}.acf"),
            Source::Steam,
            acf,
        )?;
    }
    Ok(out)
}

/// Where a candidate's bytes are on disk, for loose files.
pub fn loose_path(paths: &GamePaths, candidate: &Candidate) -> Option<std::path::PathBuf> {
    match candidate.source {
        Source::Pak01 => None,
        Source::Loose => Some(paths.citadel_dir.join(&candidate.path)),
        Source::Steam => paths.appmanifest.clone(),
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use std::collections::BTreeMap;

    use super::*;
    use crate::hud::vpk;

    /// A pak list shaped like the game's: interface files, DeadTune targets and noise.
    pub fn fake_list() -> BTreeMap<String, Vec<u8>> {
        [
            "panorama/layout/hud.vxml_c",
            "panorama/layout/citadel_hud_top_bar.vxml_c",
            "panorama/layout/hud_minimap.vxml_c",
            "panorama/layout/popups/popup_settings.vxml_c",
            "panorama/layout/base_dashboard.vxml_c",
            "panorama/layout/shop/shop.vxml_c",
            "panorama/styles/hud.vcss_c",
            "panorama/styles/hud_health.vcss_c",
            "panorama/styles/citadel_base_styles.vcss_c",
            "panorama/styles/popups/popup_settings.vcss_c",
            "panorama/scripts/settings_video.vjs_c",
            "panorama/scripts/minimap_markers.vjs_c",
            "panorama/scripts/main_menu_news.vjs_c",
            "panorama/images/hud/icon_psd.vtex_c",
            "panorama/images/hud/crosshair/scope_common_psd.vtex_c",
            "particles/empty.vpcf_c",
            "models/heroes/x/x.vmdl_c",
            "maps/dl_midtown.vpk",
            "sounds/ui/click.vsnd_c",
        ]
        .into_iter()
        .map(|p| (p.to_string(), p.as_bytes().to_vec()))
        .collect()
    }

    fn inventory() -> Inventory {
        let pak = VpkDir::in_memory(vpk::write(&fake_list())).unwrap();
        Inventory::from_pak(&pak, Vec::new())
    }

    fn paths_in(inv: &Inventory, cat: Category) -> Vec<&str> {
        inv.files
            .iter()
            .filter(|c| c.categories.contains(&cat))
            .map(|c| c.path.as_str())
            .collect()
    }

    #[test]
    fn rules_pick_interface_files_and_deadtune_targets_only() {
        let inv = inventory();
        assert_eq!(inv.pak_entries, 19);
        assert_eq!(
            paths_in(&inv, Category::Hud),
            [
                "panorama/layout/citadel_hud_top_bar.vxml_c",
                "panorama/layout/hud.vxml_c",
                "panorama/layout/hud_minimap.vxml_c",
                "panorama/scripts/minimap_markers.vjs_c",
                "panorama/styles/hud.vcss_c",
                "panorama/styles/hud_health.vcss_c",
            ]
        );
        assert_eq!(
            paths_in(&inv, Category::Settings),
            [
                "panorama/layout/popups/popup_settings.vxml_c",
                "panorama/scripts/settings_video.vjs_c",
                "panorama/styles/popups/popup_settings.vcss_c",
            ]
        );
        assert_eq!(
            paths_in(&inv, Category::Menu),
            [
                "panorama/layout/base_dashboard.vxml_c",
                "panorama/scripts/main_menu_news.vjs_c",
            ]
        );
        assert_eq!(inv.totals(Category::Panorama).0, 13);
        assert_eq!(
            paths_in(&inv, Category::Deadtune),
            [
                "panorama/images/hud/crosshair/scope_common_psd.vtex_c",
                "panorama/layout/citadel_hud_top_bar.vxml_c",
                "panorama/layout/hud_minimap.vxml_c",
                "panorama/layout/popups/popup_settings.vxml_c",
                "panorama/styles/citadel_base_styles.vcss_c",
                "panorama/styles/hud.vcss_c",
                "panorama/styles/hud_health.vcss_c",
                "particles/empty.vpcf_c",
            ]
        );
        let all: Vec<&str> = inv.files.iter().map(|c| c.path.as_str()).collect();
        for noise in [
            "models/heroes/x/x.vmdl_c",
            "maps/dl_midtown.vpk",
            "sounds/ui/click.vsnd_c",
            "panorama/images/hud/icon_psd.vtex_c",
            "panorama/layout/shop/shop.vxml_c",
        ] {
            assert_eq!(
                all.contains(&noise),
                noise.ends_with("shop.vxml_c"),
                "{noise}"
            );
        }
        assert!(
            inv.files
                .iter()
                .all(|c| c.size == c.path.len() as u64 && c.crc == crc32(c.path.as_bytes()))
        );
    }

    #[test]
    fn selection_counts_the_union_and_applies_the_cap() {
        let inv = inventory();
        let mut sel = Selection {
            categories: BTreeSet::from([Category::Hud, Category::Deadtune]),
            ..Selection::default()
        };
        let (n, bytes) = inv.selected_totals(&sel);
        assert_eq!(n, 10, "hud and deadtune overlap on four files");
        assert!(bytes > 0);
        sel.size_cap = Some(20);
        let (n_capped, bytes_capped) = inv.selected_totals(&sel);
        assert_eq!(n_capped, n, "capped files stay listed");
        assert!(bytes_capped < bytes);
        assert_eq!(Selection::default().categories.len(), 6);
        assert_eq!(Category::parse("deadtune"), Some(Category::Deadtune));
        assert_eq!(Category::parse("nope"), None);
        assert_eq!(
            toml::to_string(&Selection::default())
                .unwrap()
                .lines()
                .next(),
            Some(
                "categories = [\"hud\", \"settings\", \"menu\", \"panorama\", \"deadtune\", \"config\"]"
            )
        );
    }

    #[test]
    fn loose_files_cover_gameinfo_cfg_and_the_appmanifest() {
        let (_dir, paths) = crate::snapshot::export::tests::fake_install(b"", "100");
        let loose = loose_files(&paths).unwrap();
        let names: Vec<&str> = loose.iter().map(|c| c.path.as_str()).collect();
        assert_eq!(
            names,
            [
                "gameinfo.gi",
                "cfg/autoexec.cfg",
                "cfg/video.txt",
                "steam/appmanifest_1422450.acf"
            ]
        );
        assert!(loose.iter().all(|c| c.categories == [Category::Config]));
        assert_eq!(loose[3].source, Source::Steam);
        assert_eq!(
            loose_path(&paths, &loose[1]),
            Some(paths.cfg_dir.join("autoexec.cfg"))
        );
    }
}
