//! The UI images page without a window: where the game's images come from (the player's
//! pak01, or a snapshot folder for previews on a machine without the game), the folder
//! list, the search and "Changed by me" filter, the selection, and every swap and reset.
//! Overrides live in `profile.hud.icons` (`dt_core::hud::icons`) and ship with Apply.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use dt_core::hud::art;
use dt_core::hud::icons::{self, IMAGES_ROOT, IconError, IconOverride, Target};
use dt_core::hud::install::GAME_PAK;
use dt_core::hud::vpk::VpkDir;
use dt_core::snapshot::ImageInfo;
use dt_core::texture::encode::Fit;
use dt_core::texture::{self, RgbaImage, png, svg};

use crate::state::AppState;

/// Folders shown first, in this order; the rest follow alphabetically.
const FIRST_FOLDERS: [&str; 6] = ["minimap", "hud", "heroes", "items", "upgrades", "icons"];
const EXPORTS_DIR: &str = "exports";

/// Where the game's images are read from.
pub enum ImageSource {
    Game(VpkDir),
    /// A snapshot's `raw/` folder (or any folder holding `panorama/images/`).
    Folder {
        root: PathBuf,
        label: String,
    },
    /// Decoded pictures (PNG, SVG) under game paths: a "Save all images" export or a
    /// snapshot's `text/`. `names` is the export manifest's game path -> files, if any.
    Decoded {
        root: PathBuf,
        names: BTreeMap<String, Vec<String>>,
        label: String,
    },
    /// Several sources asked in turn; the first that has a picture wins.
    Chain(Vec<ImageSource>),
}

impl ImageSource {
    /// A decoded-image folder, with its manifest's names when it has one.
    pub fn decoded(root: PathBuf) -> ImageSource {
        let names = std::fs::read_to_string(root.join(art::EXPORT_MANIFEST))
            .map(|json| art::manifest_names(&json))
            .unwrap_or_default();
        let label = root
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        ImageSource::Decoded { root, names, label }
    }

    /// The compiled game file at `path`.
    pub fn read(&self, path: &str) -> Result<Vec<u8>, String> {
        match self {
            ImageSource::Game(pak) => pak.read(path).map_err(|e| e.to_string()),
            ImageSource::Folder { root, .. } => {
                std::fs::read(root.join(path)).map_err(|e| e.to_string())
            }
            ImageSource::Decoded { .. } => Err("this folder holds decoded pictures only".into()),
            ImageSource::Chain(sources) => first_ok(sources, |s| s.read(path)),
        }
    }

    /// What the page header says about where the pictures come from.
    pub fn describe(&self) -> String {
        match self {
            ImageSource::Game(_) => "From your game files".into(),
            ImageSource::Folder { label, .. } => format!("Previewing snapshot {label}"),
            ImageSource::Decoded { label, .. } => format!("Previewing images from {label}"),
            ImageSource::Chain(sources) => sources
                .first()
                .map_or_else(|| "No image source".into(), ImageSource::describe),
        }
    }

    /// `game_path` decoded with its longer side at most `side` (vectors exactly that).
    fn game_picture(&self, game_path: &str, side: u32) -> Result<Rendered, String> {
        match self {
            ImageSource::Chain(sources) => first_ok(sources, |s| s.game_picture(game_path, side)),
            ImageSource::Decoded { root, names, .. } => {
                let files = names
                    .get(game_path)
                    .cloned()
                    .unwrap_or_else(|| art::export_names(game_path));
                let file = files
                    .iter()
                    .map(|f| root.join(f))
                    .find(|f| f.is_file())
                    .ok_or("not in the image folder")?;
                render_file(&file, side)
            }
            ImageSource::Game(_) | ImageSource::Folder { .. } => {
                let bytes = self.read(game_path)?;
                if game_path.ends_with(".vsvg_c") {
                    let text = svg::svg_text(&bytes).map_err(|e| e.to_string())?;
                    let facts = vector_facts(&text)?;
                    let image = svg::rasterize(&text, side).map_err(|e| e.to_string())?;
                    Ok(Rendered { image, facts })
                } else {
                    let info = ImageInfo::of(&bytes).ok_or("not a texture DeadTune can read")?;
                    let image = texture::thumbnail(&bytes, side).map_err(|e| e.to_string())?;
                    Ok(Rendered {
                        image,
                        facts: Facts {
                            width: info.width.into(),
                            height: info.height.into(),
                            format: info.format,
                        },
                    })
                }
            }
        }
    }

    fn paths(&self) -> Vec<String> {
        match self {
            ImageSource::Game(pak) => pak
                .entries
                .range(IMAGES_ROOT.to_string()..)
                .map(|(p, _)| p)
                .take_while(|p| p.starts_with(IMAGES_ROOT))
                .cloned()
                .collect(),
            ImageSource::Folder { root, .. } => {
                let mut out = Vec::new();
                walk(root, &root.join(IMAGES_ROOT), &mut out);
                out
            }
            ImageSource::Decoded { root, .. } => {
                let mut files = Vec::new();
                walk(root, &root.join(IMAGES_ROOT), &mut files);
                let vectors: std::collections::BTreeSet<String> = files
                    .iter()
                    .filter_map(|f| f.strip_suffix(".svg"))
                    .map(str::to_string)
                    .collect();
                files
                    .iter()
                    .filter_map(|f| match f.strip_suffix(".png") {
                        Some(stem) if !vectors.contains(stem) => Some(format!("{stem}.vtex_c")),
                        Some(_) => None,
                        None => f.strip_suffix(".svg").map(|stem| format!("{stem}.vsvg_c")),
                    })
                    .collect()
            }
            ImageSource::Chain(sources) => {
                let all: std::collections::BTreeSet<String> =
                    sources.iter().flat_map(ImageSource::paths).collect();
                all.into_iter().collect()
            }
        }
    }
}

fn first_ok<T>(
    sources: &[ImageSource],
    mut f: impl FnMut(&ImageSource) -> Result<T, String>,
) -> Result<T, String> {
    let mut last = Err("no image source".to_string());
    for source in sources {
        last = f(source);
        if last.is_ok() {
            break;
        }
    }
    last
}

/// A PNG or SVG file decoded with its longer side at most `side` (an SVG exactly that).
fn render_file(file: &Path, side: u32) -> Result<Rendered, String> {
    let bytes = std::fs::read(file).map_err(|e| e.to_string())?;
    if file.extension().is_some_and(|e| e == "svg") {
        let text = String::from_utf8(bytes).map_err(|_| "the SVG is not UTF-8")?;
        let facts = vector_facts(&text)?;
        let image = svg::rasterize(&text, side).map_err(|e| e.to_string())?;
        Ok(Rendered { image, facts })
    } else {
        let full = png::read(&bytes).map_err(|e| e.to_string())?;
        let facts = Facts {
            width: full.width,
            height: full.height,
            format: "PNG".into(),
        };
        Ok(Rendered {
            image: full.fit(side),
            facts,
        })
    }
}

fn walk(root: &Path, dir: &Path, out: &mut Vec<String>) {
    let Ok(read) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in read.flatten() {
        let path = entry.path();
        if path.is_dir() {
            walk(root, &path, out);
        } else if let Ok(rel) = path.strip_prefix(root) {
            let rel: Vec<_> = rel.iter().map(|s| s.to_string_lossy()).collect();
            out.push(rel.join("/"));
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ImageEntry {
    /// Full game path, `panorama/images/...`.
    pub path: String,
    pub kind: Target,
    lower: String,
}

impl ImageEntry {
    /// The path below `panorama/images/`.
    pub fn rel(&self) -> &str {
        &self.path[IMAGES_ROOT.len()..]
    }

    /// The folder below `panorama/images/`, empty at the top.
    pub fn folder(&self) -> &str {
        self.rel().rsplit_once('/').map_or("", |(dir, _)| dir)
    }

    /// The file name without the compiled extension or the `_psd`/`_png` source suffix.
    pub fn name(&self) -> &str {
        let file = self.rel().rsplit('/').next().unwrap_or_default();
        let stem = file
            .strip_suffix(".vtex_c")
            .or_else(|| file.strip_suffix(".vsvg_c"))
            .unwrap_or(file);
        stem.strip_suffix("_psd")
            .or_else(|| stem.strip_suffix("_png"))
            .unwrap_or(stem)
    }

    fn in_folder(&self, folder: &str) -> bool {
        self.rel()
            .strip_prefix(folder)
            .is_some_and(|rest| rest.starts_with('/'))
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Folder {
    /// Below `panorama/images/`, one or two levels deep.
    pub path: String,
    pub count: usize,
}

impl Folder {
    pub fn name(&self) -> &str {
        self.path.rsplit('/').next().unwrap_or(&self.path)
    }
}

/// Every replaceable image the source has, in display order, and its folders with counts.
pub struct Library {
    pub source: Arc<ImageSource>,
    pub entries: Vec<ImageEntry>,
    pub folders: Vec<Folder>,
}

fn folder_rank(top: &str) -> (usize, &str) {
    let rank = FIRST_FOLDERS
        .iter()
        .position(|f| *f == top)
        .unwrap_or(FIRST_FOLDERS.len());
    (rank, top)
}

impl Library {
    pub fn new(source: ImageSource) -> Library {
        let mut entries: Vec<ImageEntry> = source
            .paths()
            .into_iter()
            .filter_map(|path| {
                let kind = icons::target(&path).ok()?;
                Some(ImageEntry {
                    lower: path.to_lowercase(),
                    path,
                    kind,
                })
            })
            .collect();
        let top = |e: &ImageEntry| -> String {
            e.folder().split('/').next().unwrap_or_default().to_string()
        };
        entries.sort_by(|a, b| {
            let (ta, tb) = (top(a), top(b));
            folder_rank(&ta)
                .cmp(&folder_rank(&tb))
                .then_with(|| a.path.cmp(&b.path))
        });
        let mut counts: BTreeMap<String, usize> = BTreeMap::new();
        for entry in &entries {
            let parts: Vec<&str> = entry
                .folder()
                .split('/')
                .filter(|s| !s.is_empty())
                .collect();
            for depth in 1..=parts.len().min(2) {
                *counts.entry(parts[..depth].join("/")).or_default() += 1;
            }
        }
        let mut folders: Vec<Folder> = counts
            .into_iter()
            .map(|(path, count)| Folder { path, count })
            .collect();
        folders.sort_by(|a, b| {
            let top = |f: &Folder| f.path.split('/').next().unwrap_or_default().to_string();
            folder_rank(&top(a))
                .cmp(&folder_rank(&top(b)))
                .then_with(|| a.path.cmp(&b.path))
        });
        Library {
            source: Arc::new(source),
            entries,
            folders,
        }
    }

    pub fn entry(&self, path: &str) -> Option<&ImageEntry> {
        self.entries.iter().find(|e| e.path == path)
    }

    /// Top-level folders.
    pub fn top_folders(&self) -> impl Iterator<Item = &Folder> {
        self.folders.iter().filter(|f| !f.path.contains('/'))
    }

    /// The folders one level below `parent`.
    pub fn subfolders<'a>(&'a self, parent: &'a str) -> impl Iterator<Item = &'a Folder> {
        self.folders.iter().filter(move |f| {
            f.path
                .strip_prefix(parent)
                .and_then(|rest| rest.strip_prefix('/'))
                .is_some_and(|rest| !rest.is_empty() && !rest.contains('/'))
        })
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Zoom {
    #[default]
    Fit,
    Actual,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tone {
    Good,
    Warn,
    Bad,
}

/// The page's last message, shown under the image it is about.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Notice {
    pub path: String,
    pub tone: Tone,
    pub text: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExportKind {
    Png,
    Svg,
}

/// One picture the page may draw: a game image or the player's stored copy, at a size.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Picture {
    Game { path: String, side: u32 },
    Mine { file: PathBuf, side: u32 },
}

/// What a picture's source file is, before it was scaled for drawing.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Facts {
    pub width: u32,
    pub height: u32,
    pub format: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Rendered {
    pub image: RgbaImage,
    pub facts: Facts,
}

fn vector_facts(svg_text: &str) -> Result<Facts, String> {
    let view = svg::validate(svg_text).map_err(|e| e.to_string())?;
    Ok(Facts {
        width: view.width.round() as u32,
        height: view.height.round() as u32,
        format: "SVG".into(),
    })
}

/// Decodes `picture` with its longer side at most its `side` (vector images exactly that).
pub fn render(source: &ImageSource, picture: &Picture) -> Result<Rendered, String> {
    match picture {
        Picture::Game { path, side } => source.game_picture(path, *side),
        Picture::Mine { file, side } => render_file(file, *side),
    }
}

/// One sentence the player can act on, for a file that could not replace an image.
fn plain_error(error: &IconError) -> String {
    match error {
        IconError::UnknownImage => {
            "That file isn't a PNG or SVG. Save it as PNG or SVG and try again.".into()
        }
        IconError::Png(_) => {
            "That PNG couldn't be read. Export it again from your image editor and retry.".into()
        }
        IconError::Svg(_) => {
            "That SVG couldn't be read. Save it again from your vector editor (plain SVG) and retry."
                .into()
        }
        IconError::SvgForRaster(_) => {
            "This image is a picture, not a vector icon, so it needs a PNG. Export your SVG as a PNG and use that."
                .into()
        }
        IconError::Path(_) => "This image can't be replaced.".into(),
        IconError::Io(e) => {
            format!("Couldn't save a copy of your image ({e}). Check there is free disk space.")
        }
    }
}

/// The page's state: the library once loaded, filters, selection and the last message.
#[derive(Default)]
pub struct ImagesState {
    /// `None` until the page first opens.
    pub library: Option<Result<Library, String>>,
    /// A snapshot folder to preview instead of the game's pak (`DEADTUNE_IMAGES_FROM`).
    pub from: Option<PathBuf>,
    /// Below `panorama/images/`; `None` shows every folder.
    pub folder: Option<String>,
    pub search: String,
    pub changed_only: bool,
    pub selected: Option<String>,
    /// How the next picture dropped on a texture is sized.
    pub fit: Fit,
    pub zoom: Zoom,
    pub replace_path: String,
    pub notice: Option<Notice>,
    /// The last exported file, for "Open folder".
    pub exported: Option<PathBuf>,
    /// Decoded pictures and their workers, made when the page is first drawn.
    pub thumbs: Option<crate::thumbs::Thumbs>,
}

impl ImagesState {
    /// Entries passing the folder, search and "Changed by me" filters, in display order.
    pub fn visible<'a>(
        &self,
        library: &'a Library,
        icons: &BTreeMap<String, IconOverride>,
    ) -> Vec<&'a ImageEntry> {
        let query = self.search.trim().to_lowercase();
        library
            .entries
            .iter()
            .filter(|e| self.folder.as_deref().is_none_or(|f| e.in_folder(f)))
            .filter(|e| query.is_empty() || e.lower.contains(&query))
            .filter(|e| !self.changed_only || icons.contains_key(&e.path))
            .collect()
    }
}

impl AppState {
    /// Reads the image list on first use: the snapshot folder when one is set, else pak01.
    pub fn load_images(&mut self) {
        if self.images.library.is_some() {
            return;
        }
        let source = match &self.images.from {
            Some(dir) => {
                let raw = dir.join(dt_core::snapshot::store::RAW);
                let root = if raw.is_dir() { raw } else { dir.clone() };
                let label = dir
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default();
                Ok(ImageSource::Folder { root, label })
            }
            None => VpkDir::open(&self.paths.citadel_dir.join(GAME_PAK))
                .map(ImageSource::Game)
                .map_err(|e| format!("Couldn't read the game's images ({e}). Check the game folder in Safety & setup.")),
        };
        self.images.library = Some(source.map(Library::new));
    }

    pub fn image_library(&self) -> Option<&Library> {
        self.images.library.as_ref()?.as_ref().ok()
    }

    pub fn images_changed_count(&self) -> usize {
        self.profile.hud.icons.len()
    }

    pub fn image_override(&self, path: &str) -> Option<&IconOverride> {
        self.profile.hud.icons.get(path)
    }

    /// Where the player's copy of the image replacing `path` is stored.
    pub fn stored_image(&self, path: &str) -> Option<PathBuf> {
        self.image_override(path)
            .map(|o| o.stored_at(&self.store.root))
    }

    /// Why the last plan left `path`'s replacement out, if it did.
    pub fn image_problem(&self, path: &str) -> Option<&str> {
        let plan = self.preview.as_ref().ok()?.hud.as_ref()?;
        plan.icon_problems
            .iter()
            .find(|p| p.game_path == path)
            .map(|p| p.reason.as_str())
    }

    pub fn select_image(&mut self, path: Option<String>) {
        if let Some(IconOverride::Png { fit, .. }) =
            path.as_deref().and_then(|p| self.image_override(p))
        {
            self.images.fit = *fit;
        }
        if self.images.notice.as_ref().map(|n| &n.path) != path.as_ref() {
            self.images.notice = None;
        }
        self.images.selected = path;
    }

    fn notice(&mut self, path: &str, tone: Tone, text: impl Into<String>) {
        self.images.notice = Some(Notice {
            path: path.to_string(),
            tone,
            text: text.into(),
        });
    }

    /// Replaces the game image at `path` with `bytes` (a PNG or SVG) as a pending change.
    pub fn replace_image(&mut self, path: &str, bytes: &[u8]) {
        let fit = self.images.fit;
        match icons::set(
            &mut self.profile.hud.icons,
            &self.store.root,
            path,
            bytes,
            fit,
        ) {
            Ok(entry) => {
                self.refresh_preview();
                if entry.is_experimental() {
                    self.notice(
                        path,
                        Tone::Warn,
                        "Replaced, as a test. Press Apply, then check it shows in game.",
                    );
                } else {
                    self.notice(path, Tone::Good, "Replaced. Press Apply to use it in game.");
                }
            }
            Err(e) => self.notice(path, Tone::Bad, plain_error(&e)),
        }
    }

    /// The "Replace" box: reads the file at `file` and replaces the selected image with it.
    pub fn replace_selected_from_path(&mut self, file: &str) {
        let Some(path) = self.images.selected.clone() else {
            return;
        };
        let file = file.trim().trim_matches('"');
        match std::fs::read(file) {
            Ok(bytes) => {
                self.replace_image(&path, &bytes);
                if self
                    .images
                    .notice
                    .as_ref()
                    .is_some_and(|n| n.tone != Tone::Bad)
                {
                    self.images.replace_path.clear();
                }
            }
            Err(_) => self.notice(
                &path,
                Tone::Bad,
                "Couldn't open that file. Check the path and try again.",
            ),
        }
    }

    /// Files dropped on the window: the first goes to `target` (the tile under the pointer)
    /// or else the selected image.
    pub fn drop_image_files(
        &mut self,
        target: Option<String>,
        files: Vec<Result<Vec<u8>, String>>,
    ) {
        let Some(first) = files.into_iter().next() else {
            return;
        };
        let Some(path) = target.or_else(|| self.images.selected.clone()) else {
            self.images.notice = Some(Notice {
                path: String::new(),
                tone: Tone::Bad,
                text: "Pick an image first, then drop your file on it.".into(),
            });
            return;
        };
        self.select_image(Some(path.clone()));
        match first {
            Ok(bytes) => self.replace_image(&path, &bytes),
            Err(_) => self.notice(
                &path,
                Tone::Bad,
                "Couldn't read the dropped file. Copy it to a normal folder and drop it again.",
            ),
        }
    }

    /// Sets how textures are sized; an already replaced texture that is selected follows.
    pub fn set_image_fit(&mut self, fit: Fit) {
        self.images.fit = fit;
        let Some(path) = self.images.selected.clone() else {
            return;
        };
        if let Some(IconOverride::Png { fit: current, .. }) = self.profile.hud.icons.get_mut(&path)
            && *current != fit
        {
            *current = fit;
            self.refresh_preview();
        }
    }

    pub fn reset_image(&mut self, path: &str) {
        if icons::reset(&mut self.profile.hud.icons, path) {
            self.refresh_preview();
            self.notice(
                path,
                Tone::Good,
                "Back to the game's own image after Apply.",
            );
        }
    }

    pub fn reset_all_images(&mut self) {
        if self.profile.hud.icons.is_empty() {
            return;
        }
        icons::reset_all(&mut self.profile.hud.icons);
        self.images.changed_only = false;
        self.images.notice = None;
        self.refresh_preview();
    }

    /// Writes the game's image at `path` to `<data>/exports/`, says so under the image, and
    /// returns the file.
    pub fn export_image(&mut self, path: &str, kind: ExportKind) -> Result<PathBuf, String> {
        let result = self.write_export(path, kind);
        match &result {
            Ok(file) => {
                self.images.exported = Some(file.clone());
                self.notice(path, Tone::Good, format!("Saved {}", file.display()));
            }
            Err(e) => self.notice(path, Tone::Bad, format!("Couldn't save a copy ({e}).")),
        }
        result
    }

    fn write_export(&self, path: &str, kind: ExportKind) -> Result<PathBuf, String> {
        let source = self
            .image_library()
            .map(|l| l.source.clone())
            .ok_or("the images are not loaded")?;
        let bytes = source.read(path)?;
        let rel = &path[IMAGES_ROOT.len().min(path.len())..];
        let stem = rel
            .strip_suffix(".vtex_c")
            .or_else(|| rel.strip_suffix(".vsvg_c"))
            .unwrap_or(rel);
        let vector = path.ends_with(".vsvg_c");
        let (ext, out) = match (kind, vector) {
            (ExportKind::Svg, true) => (
                "svg",
                svg::svg_text(&bytes)
                    .map_err(|e| e.to_string())?
                    .into_bytes(),
            ),
            (ExportKind::Svg, false) => return Err("only vector icons export as SVG".into()),
            (ExportKind::Png, true) => {
                let text = svg::svg_text(&bytes).map_err(|e| e.to_string())?;
                let facts = vector_facts(&text)?;
                let side = facts.width.max(facts.height).max(256);
                let image = svg::rasterize(&text, side).map_err(|e| e.to_string())?;
                ("png", png::write(&image).map_err(|e| e.to_string())?)
            }
            (ExportKind::Png, false) => {
                let image = texture::decode(&bytes).map_err(|e| e.to_string())?;
                ("png", png::write(&image).map_err(|e| e.to_string())?)
            }
        };
        let file = self
            .data_dir
            .join(EXPORTS_DIR)
            .join(format!("{stem}.{ext}"));
        if let Some(dir) = file.parent() {
            std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        }
        std::fs::write(&file, out).map_err(|e| e.to_string())?;
        Ok(file)
    }

    pub fn exports_dir(&self) -> PathBuf {
        self.data_dir.join(EXPORTS_DIR)
    }
}

#[cfg(test)]
pub mod tests {
    use super::*;
    use crate::state::testutil;
    use dt_core::hud::crc32::crc32;
    use dt_core::hud::resource::{Block, Resource};
    use dt_core::hud::vpk;

    pub const TEXTURE: &str = "panorama/images/minimap/hero_ally_psd.vtex_c";
    pub const TOP_BAR: &str = "panorama/images/hud/top_bar/icon_ultimate.vsvg_c";
    pub const ITEM: &str = "panorama/images/items/stand_in_psd.vtex_c";
    pub const BROKEN: &str = "panorama/images/hud/broken_psd.vtex_c";
    pub const GAME_SVG: &str = "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 24 12\"><circle cx=\"6\" cy=\"6\" r=\"5\"/></svg>";
    const MY_SVG: &str = "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 10 10\"><rect width=\"10\" height=\"10\" fill=\"red\"/></svg>";

    pub fn vector(svg: &str) -> Vec<u8> {
        let mut data = crc32(svg.as_bytes()).to_le_bytes().to_vec();
        data.extend_from_slice(&0u16.to_le_bytes());
        data.extend_from_slice(svg.as_bytes());
        Resource {
            header_version: 12,
            type_version: 3,
            blocks: vec![
                Block {
                    name: *b"RED2",
                    data: vec![0; 16],
                },
                Block {
                    name: *b"DATA",
                    data,
                },
            ],
        }
        .to_bytes()
    }

    fn solid(width: u32, height: u32, rgba: [u8; 4]) -> RgbaImage {
        RgbaImage::new(width, height, rgba.repeat((width * height) as usize)).unwrap()
    }

    /// A Panorama texture of `width` x `height` in one colour, from the scope texture's
    /// container.
    pub fn texture(width: u32, height: u32, rgba: [u8; 4]) -> Vec<u8> {
        let scope = VpkDir::open(Path::new(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../research/configs/OptimizationLock/Various Addons Relating to Performance/Vindicta Scope Downscale/pak89_dir.vpk"
        )))
        .unwrap()
        .read(dt_core::addons::native_scope::TEXTURE)
        .unwrap();
        texture::encode::replace(&scope, &solid(width, height, rgba), Fit::Own).unwrap()
    }

    pub fn my_png(width: u32, height: u32) -> Vec<u8> {
        png::write(&solid(width, height, [255, 0, 0, 255])).unwrap()
    }

    /// The test state's game with three images, one undecodable file and a stylesheet.
    pub fn install_images(state: &AppState) {
        let files = BTreeMap::from([
            (TEXTURE.to_string(), texture(32, 16, [0, 0, 255, 255])),
            (TOP_BAR.to_string(), vector(GAME_SVG)),
            (ITEM.to_string(), texture(8, 8, [0, 255, 0, 255])),
            (BROKEN.to_string(), b"not a texture".to_vec()),
            ("panorama/styles/hud.vcss_c".to_string(), b"x".to_vec()),
        ]);
        std::fs::write(state.paths.citadel_dir.join(GAME_PAK), vpk::write(&files)).unwrap();
    }

    fn loaded() -> (tempfile::TempDir, AppState) {
        let (dir, mut state) = testutil::state();
        install_images(&state);
        state.load_images();
        (dir, state)
    }

    fn paths(entries: &[&ImageEntry]) -> Vec<String> {
        entries.iter().map(|e| e.path.clone()).collect()
    }

    #[test]
    fn library_lists_only_images_with_the_minimap_first_and_counts_folders() {
        let (_dir, state) = loaded();
        let lib = state.image_library().unwrap();
        let listed: Vec<&str> = lib.entries.iter().map(|e| e.path.as_str()).collect();
        assert_eq!(listed, [TEXTURE, BROKEN, TOP_BAR, ITEM]);
        let folders: Vec<(&str, usize)> = lib
            .folders
            .iter()
            .map(|f| (f.path.as_str(), f.count))
            .collect();
        assert_eq!(
            folders,
            [("minimap", 1), ("hud", 2), ("hud/top_bar", 1), ("items", 1)]
        );
        assert_eq!(lib.top_folders().count(), 3);
        let subs: Vec<&str> = lib.subfolders("hud").map(|f| f.path.as_str()).collect();
        assert_eq!(subs, ["hud/top_bar"]);
        assert_eq!(lib.entries[0].name(), "hero_ally");
        assert_eq!(lib.entries[2].name(), "icon_ultimate");
        assert_eq!(lib.entries[2].kind, Target::Vector);
    }

    #[test]
    fn a_missing_pak_is_one_plain_sentence() {
        let (_dir, mut state) = testutil::state();
        state.load_images();
        let err = state
            .images
            .library
            .as_ref()
            .unwrap()
            .as_ref()
            .err()
            .unwrap();
        assert!(err.starts_with("Couldn't read the game's images"), "{err}");
    }

    #[test]
    fn a_snapshot_folder_stands_in_for_the_game() {
        let (dir, mut state) = testutil::state();
        let snap = dir.path().join("2026-10-05_build1");
        let file = snap.join("raw").join(TOP_BAR);
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        std::fs::write(&file, vector(GAME_SVG)).unwrap();
        std::fs::write(snap.join("raw/panorama/images/hud/notes.txt"), "x").unwrap();
        state.images.from = Some(snap);
        state.load_images();
        let lib = state.image_library().unwrap();
        assert_eq!(paths(&lib.entries.iter().collect::<Vec<_>>()), [TOP_BAR]);
        assert_eq!(
            lib.source.describe(),
            "Previewing snapshot 2026-10-05_build1"
        );
        let shown = render(
            &lib.source,
            &Picture::Game {
                path: TOP_BAR.into(),
                side: 48,
            },
        )
        .unwrap();
        assert_eq!((shown.image.width, shown.image.height), (48, 24));
    }

    #[test]
    fn a_decoded_folder_and_a_chain_stand_in_for_the_game() {
        let (dir, state) = loaded();
        let export = dir.path().join("export");
        let png_file = export.join("panorama/images/items/stand_in_psd.png");
        std::fs::create_dir_all(png_file.parent().unwrap()).unwrap();
        std::fs::write(
            &png_file,
            png::write(&solid(20, 10, [9, 9, 9, 255])).unwrap(),
        )
        .unwrap();
        let svg_dir = export.join("panorama/images/hud/top_bar");
        std::fs::create_dir_all(&svg_dir).unwrap();
        std::fs::write(svg_dir.join("icon_ultimate.svg"), MY_SVG).unwrap();
        std::fs::write(svg_dir.join("icon_ultimate.png"), my_png(3, 3)).unwrap();
        let game = |side| Picture::Game {
            path: ITEM.into(),
            side,
        };
        let decoded = ImageSource::decoded(export.clone());
        let shown = render(&decoded, &game(10)).unwrap();
        assert_eq!(
            (shown.image.width, shown.image.pixel(0, 0)),
            (10, [9, 9, 9, 255])
        );
        let vector = render(
            &decoded,
            &Picture::Game {
                path: TOP_BAR.into(),
                side: 16,
            },
        )
        .unwrap();
        assert_eq!(vector.facts.format, "SVG", "the SVG wins over its PNG");
        let mut paths = decoded.paths();
        paths.sort();
        assert_eq!(paths, [TOP_BAR, ITEM]);

        let pak = VpkDir::open(&state.paths.citadel_dir.join(GAME_PAK)).unwrap();
        let chain = ImageSource::Chain(vec![decoded, ImageSource::Game(pak)]);
        assert_eq!(
            render(&chain, &game(10)).unwrap().image.pixel(0, 0),
            [9, 9, 9, 255]
        );
        let from_pak = Picture::Game {
            path: TEXTURE.into(),
            side: 8,
        };
        assert_eq!(
            render(&chain, &from_pak).unwrap().image.pixel(0, 0),
            [0, 0, 255, 255]
        );
        assert!(
            render(
                &chain,
                &Picture::Game {
                    path: "panorama/images/nope.vtex_c".into(),
                    side: 8
                }
            )
            .is_err()
        );

        std::fs::write(
            export.join(art::EXPORT_MANIFEST),
            format!(r#"{{"images":[{{"path":"{TEXTURE}","files":["panorama/images/items/stand_in_psd.png"]}}]}}"#),
        )
        .unwrap();
        let renamed = ImageSource::decoded(export);
        assert_eq!(
            render(&renamed, &from_pak).unwrap().image.pixel(0, 0),
            [9, 9, 9, 255],
            "the manifest's names win over the naming rule"
        );
    }

    #[test]
    fn folder_search_and_changed_filters_combine() {
        let (_dir, mut state) = loaded();
        let lib_entries = |state: &AppState| {
            let lib = state.image_library().unwrap();
            paths(&state.images.visible(lib, &state.profile.hud.icons))
        };
        state.images.folder = Some("hud".into());
        assert_eq!(lib_entries(&state), [BROKEN, TOP_BAR]);
        state.images.folder = Some("hud/top_bar".into());
        assert_eq!(lib_entries(&state), [TOP_BAR]);
        state.images.folder = Some("hu".into());
        assert!(
            lib_entries(&state).is_empty(),
            "folder match is by whole name"
        );
        state.images.folder = None;
        state.images.search = " HERO ".into();
        assert_eq!(lib_entries(&state), [TEXTURE]);
        state.images.search.clear();
        state.images.changed_only = true;
        assert!(lib_entries(&state).is_empty());
        state.replace_image(ITEM, &my_png(4, 4));
        assert_eq!(lib_entries(&state), [ITEM]);
    }

    #[test]
    fn renders_textures_vectors_and_the_players_copy_with_facts() {
        let (_dir, mut state) = loaded();
        let source = state.image_library().unwrap().source.clone();
        let tex = render(
            &source,
            &Picture::Game {
                path: TEXTURE.into(),
                side: 8,
            },
        )
        .unwrap();
        assert_eq!((tex.image.width, tex.image.height), (8, 4));
        assert_eq!(tex.image.pixel(0, 0), [0, 0, 255, 255]);
        assert_eq!((tex.facts.width, tex.facts.height), (32, 16));
        let vec = render(
            &source,
            &Picture::Game {
                path: TOP_BAR.into(),
                side: 96,
            },
        )
        .unwrap();
        assert_eq!((vec.image.width, vec.image.height), (96, 48));
        assert_eq!(vec.facts.format, "SVG");
        assert_eq!((vec.facts.width, vec.facts.height), (24, 12));
        let err = render(
            &source,
            &Picture::Game {
                path: BROKEN.into(),
                side: 96,
            },
        )
        .unwrap_err();
        assert!(!err.is_empty());
        state.replace_image(TEXTURE, &my_png(40, 20));
        let mine = render(
            &source,
            &Picture::Mine {
                file: state.stored_image(TEXTURE).unwrap(),
                side: 10,
            },
        )
        .unwrap();
        assert_eq!((mine.image.width, mine.image.height), (10, 5));
        assert_eq!((mine.facts.width, mine.facts.format.as_str()), (40, "PNG"));
    }

    #[test]
    fn a_dropped_png_replaces_a_texture_as_a_pending_change() {
        let (_dir, mut state) = loaded();
        state.select_image(Some(TEXTURE.into()));
        state.drop_image_files(None, vec![Ok(my_png(4, 4))]);
        assert!(matches!(
            state.image_override(TEXTURE),
            Some(IconOverride::Png {
                fit: Fit::Original,
                ..
            })
        ));
        assert!(state.stored_image(TEXTURE).unwrap().is_file());
        assert_eq!(state.images_changed_count(), 1);
        assert_eq!(state.images.notice.as_ref().unwrap().tone, Tone::Good);
        assert!(state.is_dirty());
        let hud = state.preview.as_ref().unwrap().hud.as_ref().unwrap();
        assert!(hud.patch.icons.contains_key(TEXTURE));
        assert!(hud.icon_problems.is_empty(), "{:?}", hud.icon_problems);
    }

    #[test]
    fn a_drop_on_a_tile_wins_over_the_selection() {
        let (_dir, mut state) = loaded();
        state.select_image(Some(TEXTURE.into()));
        state.drop_image_files(Some(TOP_BAR.into()), vec![Ok(MY_SVG.as_bytes().to_vec())]);
        assert!(matches!(
            state.image_override(TOP_BAR),
            Some(IconOverride::Svg { .. })
        ));
        assert!(state.image_override(TEXTURE).is_none());
        assert_eq!(state.images.selected.as_deref(), Some(TOP_BAR));
    }

    #[test]
    fn a_png_on_a_vector_icon_is_allowed_but_marked_as_a_test() {
        let (_dir, mut state) = loaded();
        state.drop_image_files(Some(TOP_BAR.into()), vec![Ok(my_png(4, 4))]);
        assert!(state.image_override(TOP_BAR).unwrap().is_experimental());
        let notice = state.images.notice.clone().unwrap();
        assert_eq!(notice.tone, Tone::Warn);
        assert!(notice.text.contains("as a test"));
    }

    #[test]
    fn bad_drops_change_nothing_and_say_what_to_do() {
        let (_dir, mut state) = loaded();
        state.drop_image_files(None, vec![Ok(my_png(4, 4))]);
        assert_eq!(
            state.images.notice.as_ref().unwrap().text,
            "Pick an image first, then drop your file on it."
        );
        let cases: [(&str, Vec<u8>, &str); 4] = [
            (TEXTURE, b"hello".to_vec(), "isn't a PNG or SVG"),
            (TEXTURE, MY_SVG.as_bytes().to_vec(), "needs a PNG"),
            (TOP_BAR, b"<svg><g></svg>".to_vec(), "SVG couldn't be read"),
            (
                TEXTURE,
                b"\x89PNG\r\n\x1a\nbroken".to_vec(),
                "PNG couldn't be read",
            ),
        ];
        for (path, bytes, says) in cases {
            state.drop_image_files(Some(path.into()), vec![Ok(bytes)]);
            let notice = state.images.notice.clone().unwrap();
            assert_eq!(notice.tone, Tone::Bad, "{says}");
            assert!(notice.text.contains(says), "{}", notice.text);
            assert_eq!(notice.path, path);
        }
        state.drop_image_files(Some(TEXTURE.into()), vec![Err("gone".into())]);
        assert!(
            state
                .images
                .notice
                .unwrap()
                .text
                .starts_with("Couldn't read the dropped file")
        );
        assert!(state.profile.hud.icons.is_empty());
        assert!(!state.data_dir.join("backups/icons").exists());
    }

    #[test]
    fn the_replace_box_reads_a_path_and_clears_on_success() {
        let (dir, mut state) = loaded();
        let file = dir.path().join("mine.png");
        std::fs::write(&file, my_png(4, 4)).unwrap();
        state.select_image(Some(ITEM.into()));
        state.images.replace_path = format!("\"{}\"", file.display());
        state.replace_selected_from_path(&state.images.replace_path.clone());
        assert!(state.image_override(ITEM).is_some());
        assert!(state.images.replace_path.is_empty());
        state.replace_selected_from_path("/nope/missing.png");
        assert!(
            state
                .images
                .notice
                .unwrap()
                .text
                .starts_with("Couldn't open that file")
        );
    }

    #[test]
    fn fit_follows_the_selected_override_and_edits_it() {
        let (_dir, mut state) = loaded();
        state.select_image(Some(TEXTURE.into()));
        state.set_image_fit(Fit::Own);
        state.replace_image(TEXTURE, &my_png(4, 4));
        state.select_image(Some(ITEM.into()));
        state.set_image_fit(Fit::Original);
        state.select_image(Some(TEXTURE.into()));
        assert_eq!(
            state.images.fit,
            Fit::Own,
            "selection shows the override's fit"
        );
        state.set_image_fit(Fit::Original);
        assert!(matches!(
            state.image_override(TEXTURE),
            Some(IconOverride::Png {
                fit: Fit::Original,
                ..
            })
        ));
    }

    #[test]
    fn reset_one_and_reset_all() {
        let (_dir, mut state) = loaded();
        state.replace_image(TEXTURE, &my_png(4, 4));
        state.replace_image(TOP_BAR, MY_SVG.as_bytes());
        state.reset_image(TEXTURE);
        assert!(state.image_override(TEXTURE).is_none());
        assert_eq!(state.images_changed_count(), 1);
        state.images.changed_only = true;
        state.reset_all_images();
        assert_eq!(state.images_changed_count(), 0);
        assert!(!state.images.changed_only);
    }

    #[test]
    fn hud_layout_presets_keep_replaced_images() {
        let (_dir, mut state) = loaded();
        state.replace_image(ITEM, &my_png(4, 4));
        state.apply_hud_preset(crate::state::HudPreset::Clean);
        assert_eq!(state.images_changed_count(), 1);
        assert_eq!(state.hud_preset(), Some(crate::state::HudPreset::Clean));
    }

    #[test]
    fn problems_from_the_plan_reach_the_tile() {
        let (_dir, mut state) = loaded();
        state.replace_image(TEXTURE, &my_png(4, 4));
        std::fs::remove_file(state.stored_image(TEXTURE).unwrap()).unwrap();
        state.replace_image(ITEM, &my_png(2, 2));
        let problem = state.image_problem(TEXTURE).unwrap();
        assert!(problem.contains("set it again"), "{problem}");
        assert!(state.image_problem(ITEM).is_none());
    }

    #[test]
    fn exports_land_in_the_data_folder() {
        let (_dir, mut state) = loaded();
        let png_file = state.export_image(TEXTURE, ExportKind::Png).unwrap();
        assert_eq!(
            png_file,
            state.data_dir.join("exports/minimap/hero_ally_psd.png")
        );
        let back = png::read(&std::fs::read(&png_file).unwrap()).unwrap();
        assert_eq!((back.width, back.height), (32, 16));
        let svg_file = state.export_image(TOP_BAR, ExportKind::Svg).unwrap();
        assert_eq!(std::fs::read_to_string(&svg_file).unwrap(), GAME_SVG);
        let raster = state.export_image(TOP_BAR, ExportKind::Png).unwrap();
        let back = png::read(&std::fs::read(raster).unwrap()).unwrap();
        assert_eq!((back.width, back.height), (256, 128));
        assert!(state.export_image(TEXTURE, ExportKind::Svg).is_err());
        assert!(state.export_image(BROKEN, ExportKind::Png).is_err());
        assert_eq!(state.images.exported, Some(raster_path(&state)));
        assert_eq!(
            state.images.notice.unwrap().tone,
            Tone::Bad,
            "the failed export says so"
        );
    }

    fn raster_path(state: &AppState) -> PathBuf {
        state.data_dir.join("exports/hud/top_bar/icon_ultimate.png")
    }
}
