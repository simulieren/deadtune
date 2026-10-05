//! What a snapshot holds: categories as path rules over the pak tree, the loose files,
//! the image scope, and the player's selection.

use std::collections::BTreeSet;
use std::path::Path;

use super::decode::{self, Kind};
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
    /// `panorama/images/**`, chosen by [`ImageScope`] rather than a checkbox.
    Images,
}

impl Category {
    /// The checkbox categories; `Images` is selected through `Selection::images`.
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
            Category::Images => "images",
        }
    }

    /// A checkbox category by key.
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
            Category::Images => "Interface images",
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
            Category::Images => "The game's interface pictures and icons, saved as PNG and SVG.",
        }
    }
}

/// Which interface images a snapshot includes. The levels nest.
#[derive(
    Clone,
    Copy,
    Debug,
    Default,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    serde::Serialize,
    serde::Deserialize,
)]
#[serde(rename_all = "snake_case")]
pub enum ImageScope {
    #[default]
    None,
    /// `minimap/`, `hud/top_bar/` and every image the minimap and top bar layouts,
    /// stylesheets and scripts name.
    MinimapTopbar,
    /// All of `hud/` plus the minimap level.
    Hud,
    All,
}

const MINIMAP_TOPBAR_DIRS: [&str; 2] = ["panorama/images/minimap/", "panorama/images/hud/top_bar/"];
const HUD_DIRS: [&str; 2] = ["panorama/images/hud/", "panorama/images/minimap/"];

impl ImageScope {
    pub const ALL: [ImageScope; 4] = [
        ImageScope::None,
        ImageScope::MinimapTopbar,
        ImageScope::Hud,
        ImageScope::All,
    ];

    /// The CLI and manifest name.
    pub fn key(self) -> &'static str {
        match self {
            ImageScope::None => "none",
            ImageScope::MinimapTopbar => "minimap_topbar",
            ImageScope::Hud => "hud",
            ImageScope::All => "all",
        }
    }

    pub fn parse(text: &str) -> Option<ImageScope> {
        ImageScope::ALL.into_iter().find(|s| s.key() == text)
    }

    pub fn label(self) -> &'static str {
        match self {
            ImageScope::None => "No images",
            ImageScope::MinimapTopbar => "Minimap and top bar",
            ImageScope::Hud => "Whole HUD",
            ImageScope::All => "Every image",
        }
    }

    /// Whether an image path is in scope. `referenced` is `Inventory::referenced_images`.
    pub fn covers(self, path: &str, referenced: &BTreeSet<String>) -> bool {
        let under = |dirs: &[&str]| dirs.iter().any(|d| path.starts_with(d));
        match self {
            ImageScope::None => false,
            ImageScope::MinimapTopbar => under(&MINIMAP_TOPBAR_DIRS) || referenced.contains(path),
            ImageScope::Hud => under(&HUD_DIRS) || referenced.contains(path),
            ImageScope::All => true,
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
const IMAGES_DIR: &str = "panorama/images/";
const IMAGE_EXTS: [&str; 2] = [".vtex_c", ".vsvg_c"];
/// Source extensions an image reference may carry, with the compiled name's suffix.
const IMAGE_REF_EXTS: [(&str, &str); 6] = [
    (".vtex", ".vtex_c"),
    (".vsvg", ".vsvg_c"),
    (".png", "_png.vtex_c"),
    (".psd", "_psd.vtex_c"),
    (".tga", "_tga.vtex_c"),
    (".svg", ".vsvg_c"),
];
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

fn is_image(path: &str) -> bool {
    path.starts_with(IMAGES_DIR) && IMAGE_EXTS.iter().any(|e| path.ends_with(e))
}

/// Compiled image paths named in decoded layout, stylesheet or script text:
/// `s2r://panorama/images/minimap/x_psd.vtex` and `file://{images}/hud/y.png` both become
/// `panorama/images/.../..._c`.
pub fn image_refs(text: &str) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    for marker in [IMAGES_DIR, "{images}/"] {
        for (at, _) in text.match_indices(marker) {
            let rest = &text[at + marker.len()..];
            let end = rest
                .find(|c: char| matches!(c, '"' | '\'' | ')' | '<' | '>') || c.is_whitespace())
                .unwrap_or(rest.len());
            let name = &rest[..end];
            if let Some((ext, suffix)) = IMAGE_REF_EXTS.iter().find(|(e, _)| name.ends_with(e)) {
                out.insert(format!(
                    "{IMAGES_DIR}{}{suffix}",
                    &name[..name.len() - ext.len()]
                ));
            }
        }
    }
    out
}

/// Images the minimap and top bar layouts, stylesheets and scripts name: a dozen small
/// files read from the pak and decoded.
pub fn referenced_images(pak: &VpkDir) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    for path in pak.entries.keys() {
        let name = file_name(path);
        if !is_panorama(path) || !(name.contains("minimap") || name.contains("top_bar")) {
            continue;
        }
        let Some(kind) = Kind::of(path) else {
            continue;
        };
        let Ok(bytes) = pak.read(path) else {
            continue;
        };
        if let Ok(text) = decode::decode(kind, &bytes) {
            out.extend(image_refs(&String::from_utf8_lossy(&text)));
        }
    }
    out
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
    if is_image(path) {
        out.push(Category::Images);
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
    /// Images in scope are exempt.
    pub size_cap: Option<u64>,
    pub images: ImageScope,
}

pub const DEFAULT_SIZE_CAP: u64 = 8 << 20;

impl Default for Selection {
    fn default() -> Selection {
        Selection {
            categories: Category::ALL.into_iter().collect(),
            decode: true,
            size_cap: Some(DEFAULT_SIZE_CAP),
            images: ImageScope::None,
        }
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
    /// What the minimap and top bar interface files name; `ImageScope::MinimapTopbar`.
    pub referenced_images: BTreeSet<String>,
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
            referenced_images: referenced_images(pak),
        }
    }

    /// Files and bytes in one category.
    pub fn totals(&self, category: Category) -> (usize, u64) {
        self.files
            .iter()
            .filter(|c| c.categories.contains(&category))
            .fold((0, 0), |(n, b), c| (n + 1, b + c.size))
    }

    /// Files and bytes an image scope adds.
    pub fn scope_totals(&self, scope: ImageScope) -> (usize, u64) {
        self.files
            .iter()
            .filter(|c| self.in_scope(scope, c))
            .fold((0, 0), |(n, b), c| (n + 1, b + c.size))
    }

    /// An image the scope includes: stored whole whatever the size cap.
    pub fn in_scope(&self, scope: ImageScope, candidate: &Candidate) -> bool {
        candidate.categories.contains(&Category::Images)
            && scope.covers(&candidate.path, &self.referenced_images)
    }

    pub fn selected<'a>(&'a self, selection: &'a Selection) -> impl Iterator<Item = &'a Candidate> {
        self.files.iter().filter(move |c| {
            c.categories.iter().any(|cat| match cat {
                Category::Images => self.in_scope(selection.images, c),
                cat => selection.categories.contains(cat),
            })
        })
    }

    /// Files and bytes the selection would store (the size cap applied).
    pub fn selected_totals(&self, selection: &Selection) -> (usize, u64) {
        self.selected(selection).fold((0, 0), |(n, b), c| {
            let stored = self.in_scope(selection.images, c)
                || selection.size_cap.is_none_or(|cap| c.size <= cap);
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
            "panorama/images/hud/top_bar/icon_ultimate.vsvg_c",
            "panorama/images/minimap/gold_psd.vtex_c",
            "panorama/images/heroes/frog_mm_psd.vtex_c",
            "panorama/images/icons/icon_x.vsvg_c",
            "panorama/images/readme.txt",
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
        assert_eq!(inv.pak_entries, 24);
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
        assert_eq!(
            paths_in(&inv, Category::Images),
            [
                "panorama/images/heroes/frog_mm_psd.vtex_c",
                "panorama/images/hud/crosshair/scope_common_psd.vtex_c",
                "panorama/images/hud/icon_psd.vtex_c",
                "panorama/images/hud/top_bar/icon_ultimate.vsvg_c",
                "panorama/images/icons/icon_x.vsvg_c",
                "panorama/images/minimap/gold_psd.vtex_c",
            ]
        );
        let all: Vec<&str> = inv.files.iter().map(|c| c.path.as_str()).collect();
        for noise in [
            "models/heroes/x/x.vmdl_c",
            "maps/dl_midtown.vpk",
            "sounds/ui/click.vsnd_c",
            "panorama/images/readme.txt",
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
        assert_eq!(Selection::default().images, ImageScope::None);
        assert_eq!(Category::parse("deadtune"), Some(Category::Deadtune));
        assert_eq!(Category::parse("nope"), None);
        assert_eq!(Category::parse("images"), None, "a scope, not a checkbox");
        assert_eq!(
            ImageScope::parse("minimap_topbar"),
            Some(ImageScope::MinimapTopbar)
        );
        assert_eq!(ImageScope::parse("nope"), None);
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
    fn image_scopes_nest_and_exempt_their_files_from_the_cap() {
        let inv = inventory();
        let scope_texture = "panorama/images/hud/crosshair/scope_common_psd.vtex_c";
        let selected = |scope: ImageScope| -> Vec<String> {
            let sel = Selection {
                categories: BTreeSet::from([Category::Config]),
                images: scope,
                size_cap: Some(20),
                ..Selection::default()
            };
            inv.selected(&sel).map(|c| c.path.clone()).collect()
        };
        assert!(selected(ImageScope::None).is_empty());
        assert_eq!(
            selected(ImageScope::MinimapTopbar),
            [
                "panorama/images/hud/top_bar/icon_ultimate.vsvg_c",
                "panorama/images/minimap/gold_psd.vtex_c",
            ]
        );
        assert_eq!(
            selected(ImageScope::Hud),
            [
                scope_texture,
                "panorama/images/hud/icon_psd.vtex_c",
                "panorama/images/hud/top_bar/icon_ultimate.vsvg_c",
                "panorama/images/minimap/gold_psd.vtex_c",
            ]
        );
        assert_eq!(selected(ImageScope::All).len(), 6);
        assert_eq!(inv.scope_totals(ImageScope::MinimapTopbar).0, 2);
        assert_eq!(inv.scope_totals(ImageScope::All).0, 6);

        let over_cap = Selection {
            categories: BTreeSet::from([Category::Deadtune]),
            images: ImageScope::Hud,
            size_cap: Some(20),
            ..Selection::default()
        };
        let (_, bytes) = inv.selected_totals(&over_cap);
        assert_eq!(
            bytes,
            inv.scope_totals(ImageScope::Hud).1,
            "images in scope count whole; everything else is over the 20-byte cap"
        );
        let scope = inv.files.iter().find(|c| c.path == scope_texture).unwrap();
        assert!(inv.in_scope(ImageScope::Hud, scope));
        assert!(!inv.in_scope(ImageScope::MinimapTopbar, scope));
        let layout = inv
            .files
            .iter()
            .find(|c| c.path.ends_with(".vxml_c"))
            .unwrap();
        assert!(!inv.in_scope(ImageScope::All, layout), "not an image");
    }

    #[test]
    fn minimap_and_top_bar_files_pull_in_the_images_they_name() {
        use crate::hud::inject;
        let mut files = fake_list();
        files.insert(
            "panorama/styles/hud_minimap.vcss_c".into(),
            inject::style_resource(
                r#".a{background-image:url("s2r://panorama/images/heroes/frog_mm_psd.vtex")}.b{background-image:url("file://{images}/icons/icon_x.svg")}"#,
            ),
        );
        files.insert(
            "panorama/styles/shop.vcss_c".into(),
            inject::style_resource(
                r#".c{background-image:url("s2r://panorama/images/hud/icon_psd.vtex")}"#,
            ),
        );
        let inv = Inventory::from_pak(&VpkDir::in_memory(vpk::write(&files)).unwrap(), Vec::new());
        assert_eq!(
            inv.referenced_images,
            BTreeSet::from([
                "panorama/images/heroes/frog_mm_psd.vtex_c".to_string(),
                "panorama/images/icons/icon_x.vsvg_c".to_string(),
            ]),
            "the shop stylesheet is not a minimap or top bar file"
        );
        let sel = Selection {
            categories: BTreeSet::new(),
            images: ImageScope::MinimapTopbar,
            ..Selection::default()
        };
        let picked: Vec<&str> = inv.selected(&sel).map(|c| c.path.as_str()).collect();
        assert_eq!(
            picked,
            [
                "panorama/images/heroes/frog_mm_psd.vtex_c",
                "panorama/images/hud/top_bar/icon_ultimate.vsvg_c",
                "panorama/images/icons/icon_x.vsvg_c",
                "panorama/images/minimap/gold_psd.vtex_c",
            ]
        );
        assert_eq!(
            image_refs(
                r#"<Image src="s2r://panorama/images/minimap/player_cone_psd.vtex" /> url('file://{images}/hud/x.png') "panorama/images/hud/y.tga" panorama/images/z.psd
                panorama/images/other.vmat"#
            ),
            BTreeSet::from([
                "panorama/images/minimap/player_cone_psd.vtex_c".to_string(),
                "panorama/images/hud/x_png.vtex_c".to_string(),
                "panorama/images/hud/y_tga.vtex_c".to_string(),
                "panorama/images/z_psd.vtex_c".to_string(),
            ])
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
