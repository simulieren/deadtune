//! Every UI image out of the game in one go: textures to PNG, vector icons to SVG plus a
//! drawn PNG, under `<data>/exports/ui-images-<build>-<date>/` at their game paths, with a
//! `manifest.json` that a HUD preview can load by game path, a `failures.txt`, and an
//! optional zip. File names come from `export_names`, the same names a snapshot's `text/`
//! folder uses (`decode::Kind::text_path`).

use std::collections::{BTreeMap, BTreeSet};
use std::io::BufWriter;
use std::ops::ControlFlow;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use chrono::{DateTime, NaiveDate, Utc};

use super::SnapshotError;
use super::decode::{self, Kind};
use super::export::Progress;
use super::store::{ImageInfo, Manifest, RAW};
use crate::backup::{atomic_write, sha256_hex};
use crate::hud::icons::{self, IMAGES_ROOT, Target};
use crate::hud::install::GAME_PAK;
use crate::hud::vpk::VpkDir;
use crate::locate::{self, GamePaths};
use crate::texture::svg;
use crate::zip::ZipWriter;

pub const EXPORTS_DIR: &str = "exports";
pub const MANIFEST_JSON: &str = "manifest.json";
pub const FAILURES: &str = "failures.txt";
/// Bumped when a field of `ImagesManifest` changes meaning or goes away.
pub const MANIFEST_FORMAT: u32 = 1;
/// The longer side of the PNG drawn from a vector icon is at least this.
pub const VECTOR_PNG_MIN_SIDE: u32 = 256;

/// Where the game's images are read from.
pub enum ImageSource {
    Game(VpkDir),
    /// A snapshot's `raw/` folder (or any folder holding `panorama/images/`).
    Folder {
        root: PathBuf,
        label: String,
    },
    /// Decoded pictures under their game paths: a "Save all images" export or a snapshot's
    /// `text/`. `names` is the export manifest's game path -> files, when it has one.
    Decoded {
        root: PathBuf,
        names: BTreeMap<String, Vec<String>>,
        label: String,
    },
    /// Several sources asked in turn; the first that has an image wins.
    Chain(Vec<ImageSource>),
}

/// Where a game image was found: its compiled file, or a decoded PNG or SVG on disk.
pub enum Found {
    Compiled(Vec<u8>),
    Decoded(PathBuf),
}

impl ImageSource {
    /// A decoded-image folder, reading file names from its `manifest.json` when it has one
    /// and from `export_names` otherwise.
    pub fn decoded(root: &Path) -> ImageSource {
        let names = ImagesManifest::load(root)
            .map(|m| {
                m.images
                    .into_iter()
                    .filter(|i| !i.files.is_empty())
                    .map(|i| (i.path, i.files))
                    .collect()
            })
            .unwrap_or_default();
        ImageSource::Decoded {
            root: root.to_path_buf(),
            names,
            label: root
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default(),
        }
    }

    /// The image at `game_path`, compiled or decoded, from the first source that has it.
    pub fn find(&self, game_path: &str) -> Result<Found, String> {
        match self {
            ImageSource::Chain(sources) => {
                let mut last = Err("no image source".to_string());
                for source in sources {
                    last = source.find(game_path);
                    if last.is_ok() {
                        break;
                    }
                }
                last
            }
            ImageSource::Decoded { root, names, .. } => names
                .get(game_path)
                .cloned()
                .unwrap_or_else(|| export_names(game_path))
                .iter()
                .map(|f| root.join(f))
                .find(|f| f.is_file())
                .map(Found::Decoded)
                .ok_or_else(|| "not in the image folder".to_string()),
            ImageSource::Game(_) | ImageSource::Folder { .. } => {
                self.read(game_path).map(Found::Compiled)
            }
        }
    }

    pub fn game(paths: &GamePaths) -> Result<ImageSource, SnapshotError> {
        let pak = paths.citadel_dir.join(GAME_PAK);
        if !pak.is_file() {
            return Err(SnapshotError::MissingGamePak(pak));
        }
        Ok(ImageSource::Game(VpkDir::open(&pak)?))
    }

    /// `dir` by what it holds: a "Save all images" export (its `manifest.json`) is read
    /// decoded, anything else as compiled files ([`ImageSource::folder`]).
    pub fn at(dir: &Path) -> ImageSource {
        if dir.join(MANIFEST_JSON).is_file() {
            ImageSource::decoded(dir)
        } else {
            ImageSource::folder(dir)
        }
    }

    /// A snapshot folder (its `raw/` is used when there is one) or a bare folder.
    pub fn folder(dir: &Path) -> ImageSource {
        let raw = dir.join(RAW);
        ImageSource::Folder {
            root: if raw.is_dir() { raw } else { dir.to_path_buf() },
            label: dir
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default(),
        }
    }

    pub fn read(&self, path: &str) -> Result<Vec<u8>, String> {
        match self {
            ImageSource::Game(pak) => pak.read(path).map_err(|e| e.to_string()),
            ImageSource::Folder { root, .. } => {
                std::fs::read(root.join(path)).map_err(|e| e.to_string())
            }
            ImageSource::Decoded { .. } => Err("this folder holds decoded pictures only".into()),
            ImageSource::Chain(sources) => sources
                .iter()
                .map(|s| s.read(path))
                .find(Result::is_ok)
                .unwrap_or_else(|| Err("no source has this file".into())),
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

    /// Every file under `panorama/images/`, images or not.
    pub fn paths(&self) -> Vec<String> {
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
                let vectors: BTreeSet<&str> = files
                    .iter()
                    .filter_map(|f| f.strip_suffix(".svg"))
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
            ImageSource::Chain(sources) => sources
                .iter()
                .flat_map(ImageSource::paths)
                .collect::<BTreeSet<String>>()
                .into_iter()
                .collect(),
        }
    }

    /// Every replaceable image, by game path, sorted.
    pub fn images(&self) -> Vec<(String, Target)> {
        let mut out: Vec<(String, Target)> = self
            .paths()
            .into_iter()
            .filter_map(|p| icons::target(&p).ok().map(|t| (p, t)))
            .collect();
        out.sort_by(|a, b| a.0.cmp(&b.0));
        out
    }

    /// The game's build for a pak; a snapshot folder's from its manifest.
    pub fn buildid(&self, paths: &GamePaths) -> Option<String> {
        match self {
            ImageSource::Game(_) => locate::buildid(paths),
            ImageSource::Folder { root, .. } => root
                .parent()
                .and_then(|p| Manifest::load(p).ok())
                .or_else(|| Manifest::load(root).ok())
                .and_then(|m| m.buildid),
            ImageSource::Decoded { root, .. } => {
                ImagesManifest::load(root).ok().and_then(|m| m.buildid)
            }
            ImageSource::Chain(sources) => sources.iter().find_map(|s| s.buildid(paths)),
        }
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

/// The files an image exports to, relative to the export folder: a texture
/// `x/y_psd.vtex_c` to `x/y_psd.png`; a vector `x/y.vsvg_c` to `x/y.svg` and the drawn
/// `x/y.png`. Anything else exports to nothing.
pub fn export_names(game_path: &str) -> Vec<String> {
    match Kind::of(game_path) {
        Some(Kind::Texture) => vec![Kind::Texture.text_path(game_path)],
        Some(Kind::Vector) => vec![
            Kind::Vector.text_path(game_path),
            Kind::Texture.text_path(game_path),
        ],
        _ => Vec::new(),
    }
}

/// How a stylesheet names the image: `s2r://panorama/images/x/y_psd.vtex`.
pub fn css_url(game_path: &str) -> String {
    format!(
        "s2r://{}",
        game_path.strip_suffix("_c").unwrap_or(game_path)
    )
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ImageKind {
    Texture,
    Vector,
}

impl From<Target> for ImageKind {
    fn from(t: Target) -> ImageKind {
        match t {
            Target::Raster => ImageKind::Texture,
            Target::Vector => ImageKind::Vector,
        }
    }
}

/// One game image in `manifest.json`.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ExportedImage {
    /// `panorama/images/...vtex_c` or `.vsvg_c`.
    pub path: String,
    pub kind: ImageKind,
    pub css_url: String,
    /// The texture's display size, or the vector icon's viewBox size; `None` when the file
    /// could not be read that far.
    pub width: Option<u32>,
    pub height: Option<u32>,
    /// The texture's pixel format (`BC7`, `RGBA8888`, ...) or `SVG`.
    pub format: Option<String>,
    /// Bytes of the game's compiled file.
    pub size: Option<u64>,
    /// Of the game's compiled file, so a later build's change shows.
    pub sha256: Option<String>,
    /// Written files, relative to the export folder, in `export_names` order.
    pub files: Vec<String>,
    /// Why some or all of `export_names` are missing.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ImagesManifest {
    pub format: u32,
    pub buildid: Option<String>,
    pub deadtune: String,
    /// "game" or "snapshot <folder name>".
    pub source: String,
    /// Below `panorama/images/`, when only one folder was exported.
    pub folder: Option<String>,
    pub exported_at: DateTime<Utc>,
    pub total: usize,
    pub exported: usize,
    pub failed: usize,
    /// Sorted by `path`.
    pub images: Vec<ExportedImage>,
}

impl ImagesManifest {
    pub fn load(folder: &Path) -> Result<ImagesManifest, SnapshotError> {
        let text = std::fs::read_to_string(folder.join(MANIFEST_JSON))?;
        serde_json::from_str(&text).map_err(|e| SnapshotError::Json(e.to_string()))
    }

    pub fn image(&self, path: &str) -> Option<&ExportedImage> {
        self.images.iter().find(|i| i.path == path)
    }

    pub fn failures(&self) -> impl Iterator<Item = &ExportedImage> {
        self.images.iter().filter(|i| i.error.is_some())
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ExportOptions {
    /// Below `panorama/images/`, e.g. `minimap` or `hud/top_bar`; `None` exports all.
    pub folder: Option<String>,
    /// Also write `<folder>.zip` next to the folder.
    pub zip: bool,
}

#[derive(Clone, Debug)]
pub struct ExportOutcome {
    pub folder: PathBuf,
    pub zip: Option<PathBuf>,
    pub manifest: ImagesManifest,
    /// Bytes written into the folder.
    pub bytes: u64,
    pub elapsed: Duration,
}

/// `ui-images-<build>-<date>`, with `-<folder>` when only one folder is exported.
pub fn folder_name(buildid: Option<&str>, date: NaiveDate, folder: Option<&str>) -> String {
    let mut name = format!(
        "ui-images-{}-{}",
        buildid.unwrap_or("unknown"),
        date.format("%Y-%m-%d")
    );
    if let Some(f) = folder
        .map(|f| f.trim_matches('/'))
        .filter(|f| !f.is_empty())
    {
        name.push('-');
        name.push_str(&f.replace('/', "-"));
    }
    name
}

/// Where an export made today goes.
pub fn default_folder(data_dir: &Path, buildid: Option<&str>, folder: Option<&str>) -> PathBuf {
    data_dir.join(EXPORTS_DIR).join(folder_name(
        buildid,
        chrono::Local::now().date_naive(),
        folder,
    ))
}

/// The files `game_path` exports to, with their bytes, and its manifest entry.
fn export_one(
    game_path: &str,
    kind: ImageKind,
    bytes: Result<Vec<u8>, String>,
) -> (ExportedImage, Vec<(String, Vec<u8>)>) {
    let mut entry = ExportedImage {
        path: game_path.to_string(),
        kind,
        css_url: css_url(game_path),
        width: None,
        height: None,
        format: None,
        size: None,
        sha256: None,
        files: Vec::new(),
        error: None,
    };
    let bytes = match bytes {
        Ok(b) => b,
        Err(e) => {
            entry.error = Some(format!("couldn't read the game's file: {e}"));
            return (entry, Vec::new());
        }
    };
    entry.size = Some(bytes.len() as u64);
    entry.sha256 = Some(sha256_hex(&bytes));
    let names = export_names(game_path);
    let mut out = Vec::new();
    match kind {
        ImageKind::Texture => {
            if let Some(info) = ImageInfo::of(&bytes) {
                entry.width = Some(info.width.into());
                entry.height = Some(info.height.into());
                entry.format = Some(info.format);
            }
            match decode::decode(Kind::Texture, &bytes) {
                Ok(png) => out.push((names[0].clone(), png)),
                Err(e) => entry.error = Some(format!("couldn't decode the texture: {e}")),
            }
        }
        ImageKind::Vector => match decode::decode(Kind::Vector, &bytes) {
            Ok(text) => {
                let svg_text = String::from_utf8_lossy(&text).into_owned();
                if let Ok(view) = svg::validate(&svg_text) {
                    entry.width = Some(view.width.round() as u32);
                    entry.height = Some(view.height.round() as u32);
                }
                entry.format = Some("SVG".into());
                out.push((names[0].clone(), text));
                match vector_png(&svg_text, entry.width, entry.height) {
                    Ok(png) => out.push((names[1].clone(), png)),
                    Err(e) => entry.error = Some(e),
                }
            }
            Err(e) => entry.error = Some(format!("couldn't read the vector icon: {e}")),
        },
    }
    entry.files = out.iter().map(|(name, _)| name.clone()).collect();
    (entry, out)
}

#[cfg(feature = "svg")]
fn vector_png(svg_text: &str, width: Option<u32>, height: Option<u32>) -> Result<Vec<u8>, String> {
    let side = width
        .unwrap_or(0)
        .max(height.unwrap_or(0))
        .max(VECTOR_PNG_MIN_SIDE);
    let image =
        svg::rasterize(svg_text, side).map_err(|e| format!("couldn't draw the PNG: {e}"))?;
    crate::texture::png::write(&image).map_err(|e| format!("couldn't write the PNG: {e}"))
}

#[cfg(not(feature = "svg"))]
fn vector_png(_: &str, _: Option<u32>, _: Option<u32>) -> Result<Vec<u8>, String> {
    Err("this build of DeadTune can't draw vector icons; the SVG is there".into())
}

fn write_file(path: &Path, bytes: &[u8]) -> Result<(), SnapshotError> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    atomic_write(path, bytes)?;
    Ok(())
}

fn failures_text(manifest: &ImagesManifest) -> String {
    let mut out = format!(
        "DeadTune could not export {} of {} images (build {}).\n",
        manifest.failed,
        manifest.total,
        manifest.buildid.as_deref().unwrap_or("unknown")
    );
    for image in manifest.failures() {
        out.push('\n');
        out.push_str(&image.path);
        out.push_str("\n    ");
        out.push_str(image.error.as_deref().unwrap_or_default());
        out.push('\n');
    }
    out
}

/// Removes what a run that does not finish leaves: a folder it created, its partial zip.
struct Cleanup {
    folder: Option<PathBuf>,
    partial_zip: Option<PathBuf>,
}

impl Drop for Cleanup {
    fn drop(&mut self) {
        if let Some(f) = &self.folder {
            let _ = std::fs::remove_dir_all(f);
        }
        if let Some(z) = &self.partial_zip {
            let _ = std::fs::remove_file(z);
        }
    }
}

/// Exports every image `source` has (or those under `options.folder`) into `out`. Running
/// it again overwrites the same files; a cancelled run leaves no new folder behind.
pub fn export_all(
    source: &ImageSource,
    out: &Path,
    buildid: Option<String>,
    options: &ExportOptions,
    progress: &mut dyn FnMut(Progress) -> ControlFlow<()>,
) -> Result<ExportOutcome, SnapshotError> {
    let started = Instant::now();
    let folder = options
        .folder
        .as_deref()
        .map(|f| f.trim_matches('/').to_string())
        .filter(|f| !f.is_empty());
    let prefix = folder.as_ref().map(|f| format!("{IMAGES_ROOT}{f}/"));
    let images: Vec<(String, Target)> = source
        .images()
        .into_iter()
        .filter(|(p, _)| prefix.as_ref().is_none_or(|pre| p.starts_with(pre)))
        .collect();
    if images.is_empty() {
        return Err(SnapshotError::NoImages(folder.map_or_else(
            || IMAGES_ROOT.to_string(),
            |f| format!("{IMAGES_ROOT}{f}"),
        )));
    }
    let mut cleanup = Cleanup {
        folder: (!out.exists()).then(|| out.to_path_buf()),
        partial_zip: None,
    };
    std::fs::create_dir_all(out)?;
    let now = Utc::now();
    let mut zip = None;
    if options.zip {
        let partial = with_suffix(out, ".zip.partial");
        let file = std::fs::File::create(&partial)?;
        cleanup.partial_zip = Some(partial);
        zip = Some(ZipWriter::new(
            BufWriter::new(file),
            now.with_timezone(&chrono::Local).naive_local(),
        ));
    }
    let total = images.len();
    let mut entries = Vec::with_capacity(total);
    let mut bytes = 0u64;
    for (i, (path, target)) in images.iter().enumerate() {
        let (entry, files) = export_one(path, (*target).into(), source.read(path));
        for (name, data) in &files {
            write_file(&out.join(name), data)?;
            if let Some(z) = zip.as_mut() {
                z.add(name, data)?;
            }
            bytes += data.len() as u64;
        }
        entries.push(entry);
        let report = Progress {
            done: i + 1,
            total,
            path,
            bytes,
        };
        if progress(report).is_break() {
            return Err(SnapshotError::Cancelled);
        }
    }
    let failed = entries.iter().filter(|e| e.error.is_some()).count();
    let manifest = ImagesManifest {
        format: MANIFEST_FORMAT,
        buildid,
        deadtune: env!("CARGO_PKG_VERSION").into(),
        source: match source {
            ImageSource::Game(_) => "game".into(),
            ImageSource::Folder { label, .. } => format!("snapshot {label}"),
            ImageSource::Decoded { label, .. } => format!("images {label}"),
            ImageSource::Chain(_) => "several".into(),
        },
        folder,
        exported_at: now,
        total,
        exported: total - failed,
        failed,
        images: entries,
    };
    let mut json =
        serde_json::to_string_pretty(&manifest).map_err(|e| SnapshotError::Json(e.to_string()))?;
    json.push('\n');
    write_file(&out.join(MANIFEST_JSON), json.as_bytes())?;
    let failures = (failed > 0).then(|| failures_text(&manifest));
    match &failures {
        Some(text) => write_file(&out.join(FAILURES), text.as_bytes())?,
        None => match std::fs::remove_file(out.join(FAILURES)) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => return Err(e.into()),
            _ => {}
        },
    }
    let zip = match zip {
        Some(mut z) => {
            z.add(MANIFEST_JSON, json.as_bytes())?;
            if let Some(text) = &failures {
                z.add(FAILURES, text.as_bytes())?;
            }
            z.finish()?.into_inner().map_err(|e| e.into_error())?;
            let partial = cleanup.partial_zip.take().expect("set with the writer");
            let done = zip_path(out);
            if done.exists() {
                std::fs::remove_file(&done)?;
            }
            std::fs::rename(&partial, &done)?;
            Some(done)
        }
        None => None,
    };
    cleanup.folder = None;
    Ok(ExportOutcome {
        folder: out.to_path_buf(),
        zip,
        manifest,
        bytes,
        elapsed: started.elapsed(),
    })
}

/// `<folder>.zip`, next to the folder.
pub fn zip_path(folder: &Path) -> PathBuf {
    with_suffix(folder, ".zip")
}

fn with_suffix(folder: &Path, suffix: &str) -> PathBuf {
    let mut name = folder.file_name().unwrap_or_default().to_os_string();
    name.push(suffix);
    folder.with_file_name(name)
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::*;
    use crate::hud::inject;
    use crate::hud::vpk;
    use crate::snapshot::export::tests::fake_install;
    use crate::texture::vtex::tests::COLOR;

    const TEXTURE: &str = "panorama/images/minimap/gold_psd.vtex_c";
    const ICON: &str = "panorama/images/hud/top_bar/icon_ultimate.vsvg_c";
    const BROKEN: &str = "panorama/images/hud/death_icon_png.vtex_c";
    const ITEM: &str = "panorama/images/items/x_png.vtex_c";
    const SVG: &str = "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 24 12\"><circle cx=\"6\" cy=\"6\" r=\"5\"/></svg>";

    fn pak_files() -> BTreeMap<String, Vec<u8>> {
        BTreeMap::from([
            (TEXTURE.to_string(), COLOR.to_vec()),
            (ITEM.to_string(), COLOR.to_vec()),
            (ICON.to_string(), inject::style_resource(SVG)),
            (BROKEN.to_string(), b"not a texture".to_vec()),
            ("panorama/images/readme.txt".to_string(), b"x".to_vec()),
            ("panorama/styles/hud.vcss_c".to_string(), b"x".to_vec()),
        ])
    }

    fn go(_: Progress) -> ControlFlow<()> {
        ControlFlow::Continue(())
    }

    fn date(y: i32, m: u32, d: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, d).unwrap()
    }

    #[test]
    fn names_follow_the_game_path_like_snapshot_text() {
        assert_eq!(
            export_names(TEXTURE),
            ["panorama/images/minimap/gold_psd.png"]
        );
        assert_eq!(
            export_names(ICON),
            [
                "panorama/images/hud/top_bar/icon_ultimate.svg",
                "panorama/images/hud/top_bar/icon_ultimate.png"
            ]
        );
        assert!(export_names("panorama/styles/hud.vcss_c").is_empty());
        for path in [TEXTURE, ICON] {
            let kind = Kind::of(path).unwrap();
            assert_eq!(export_names(path)[0], kind.text_path(path), "{path}");
        }
        assert_eq!(
            css_url(TEXTURE),
            "s2r://panorama/images/minimap/gold_psd.vtex"
        );
        assert_eq!(
            folder_name(Some("123"), date(2026, 10, 5), None),
            "ui-images-123-2026-10-05"
        );
        assert_eq!(
            folder_name(None, date(2026, 1, 2), Some("hud/top_bar/")),
            "ui-images-unknown-2026-01-02-hud-top_bar"
        );
        assert_eq!(
            zip_path(Path::new("/x/ui-images-1-2026-10-05")),
            Path::new("/x/ui-images-1-2026-10-05.zip")
        );
    }

    fn found_file(source: &ImageSource, path: &str) -> PathBuf {
        match source.find(path) {
            Ok(Found::Decoded(file)) => file,
            Ok(Found::Compiled(_)) => panic!("{path} came compiled"),
            Err(e) => panic!("{path}: {e}"),
        }
    }

    #[test]
    fn a_folder_is_read_by_what_it_holds() {
        let tmp = tempfile::tempdir().unwrap();
        let export = tmp.path().join("ui-images-1-2026-10-05");
        std::fs::create_dir_all(export.join("panorama/images")).unwrap();
        std::fs::write(export.join(MANIFEST_JSON), "{}").unwrap();
        assert!(matches!(
            ImageSource::at(&export),
            ImageSource::Decoded { .. }
        ));

        let snapshot = tmp.path().join("2026-10-05_build1");
        std::fs::create_dir_all(snapshot.join(RAW)).unwrap();
        match ImageSource::at(&snapshot) {
            ImageSource::Folder { root, .. } => assert_eq!(root, snapshot.join(RAW)),
            other => panic!("{}", other.describe()),
        }

        let bare = tmp.path().join("bare");
        std::fs::create_dir_all(bare.join("panorama/images")).unwrap();
        match ImageSource::at(&bare) {
            ImageSource::Folder { root, .. } => assert_eq!(root, bare),
            other => panic!("{}", other.describe()),
        }
    }

    #[test]
    fn a_decoded_folder_and_a_chain_find_images_by_game_path() {
        let (tmp, paths) = fake_install(&vpk::write(&pak_files()), "4242");
        let export = tmp.path().join("export");
        let write = |rel: &str| {
            let file = export.join(rel);
            std::fs::create_dir_all(file.parent().unwrap()).unwrap();
            std::fs::write(file, b"x").unwrap();
        };
        write("panorama/images/minimap/gold_psd.png");
        write("panorama/images/hud/top_bar/icon_ultimate.svg");
        write("panorama/images/hud/top_bar/icon_ultimate.png");
        let decoded = ImageSource::decoded(&export);
        assert!(found_file(&decoded, TEXTURE).ends_with("minimap/gold_psd.png"));
        assert!(
            found_file(&decoded, ICON).ends_with("icon_ultimate.svg"),
            "the SVG before its drawn PNG"
        );
        assert!(decoded.find(ITEM).is_err());
        assert!(decoded.read(TEXTURE).is_err(), "no compiled files here");
        let listed: Vec<String> = decoded.images().into_iter().map(|(p, _)| p).collect();
        assert_eq!(listed, [ICON, TEXTURE]);

        let chain = ImageSource::Chain(vec![decoded, ImageSource::game(&paths).unwrap()]);
        assert!(found_file(&chain, TEXTURE).starts_with(&export));
        assert!(matches!(chain.find(ITEM), Ok(Found::Compiled(b)) if b == COLOR));
        assert!(chain.find("panorama/images/nope_psd.vtex_c").is_err());
        assert_eq!(chain.describe(), "Previewing images from export");
        assert_eq!(chain.buildid(&paths).as_deref(), Some("4242"));

        write("renamed/gold.png");
        let manifest = ImagesManifest {
            format: MANIFEST_FORMAT,
            buildid: Some("77".into()),
            deadtune: "0".into(),
            source: "game".into(),
            folder: None,
            exported_at: Utc::now(),
            total: 1,
            exported: 1,
            failed: 0,
            images: vec![ExportedImage {
                path: TEXTURE.into(),
                kind: ImageKind::Texture,
                css_url: css_url(TEXTURE),
                width: None,
                height: None,
                format: None,
                size: None,
                sha256: None,
                files: vec!["renamed/gold.png".into()],
                error: None,
            }],
        };
        let json = serde_json::to_string(&manifest).unwrap();
        std::fs::write(export.join(MANIFEST_JSON), json).unwrap();
        let named = ImageSource::decoded(&export);
        assert!(
            found_file(&named, TEXTURE).ends_with("renamed/gold.png"),
            "the manifest's names win over the naming rule"
        );
        assert_eq!(named.buildid(&paths).as_deref(), Some("77"));
    }

    #[test]
    #[cfg(feature = "svg")]
    fn exports_every_image_with_a_manifest_failures_and_a_zip() {
        let (tmp, paths) = fake_install(&vpk::write(&pak_files()), "4242");
        let source = ImageSource::game(&paths).unwrap();
        assert_eq!(source.buildid(&paths).as_deref(), Some("4242"));
        let out = tmp.path().join("exports/ui-images-4242-2026-10-05");
        let mut seen = Vec::new();
        let options = ExportOptions {
            folder: None,
            zip: true,
        };
        let done = export_all(&source, &out, source.buildid(&paths), &options, &mut |p| {
            seen.push((p.done, p.total, p.path.to_string()));
            ControlFlow::Continue(())
        })
        .unwrap();
        let order: Vec<&str> = seen.iter().map(|s| s.2.as_str()).collect();
        assert_eq!(
            order,
            [BROKEN, ICON, ITEM, TEXTURE],
            "sorted by game path, images only"
        );
        assert_eq!((seen[3].0, seen[3].1), (4, 4));

        let m = ImagesManifest::load(&out).unwrap();
        assert_eq!(m, done.manifest);
        assert_eq!(m.format, MANIFEST_FORMAT);
        assert_eq!(m.buildid.as_deref(), Some("4242"));
        assert_eq!(m.deadtune, env!("CARGO_PKG_VERSION"));
        assert_eq!(m.source, "game");
        assert_eq!(m.folder, None);
        assert_eq!((m.total, m.exported, m.failed), (4, 3, 1));

        let gold = m.image(TEXTURE).unwrap();
        assert_eq!(gold.kind, ImageKind::Texture);
        assert_eq!((gold.width, gold.height), (Some(512), Some(512)));
        assert_eq!(gold.format.as_deref(), Some("BC7"));
        assert_eq!(gold.size, Some(COLOR.len() as u64));
        assert_eq!(gold.sha256.as_deref(), Some(sha256_hex(COLOR).as_str()));
        assert_eq!(gold.css_url, "s2r://panorama/images/minimap/gold_psd.vtex");
        assert_eq!(gold.files, export_names(TEXTURE));
        assert_eq!(gold.error, None);
        let png = std::fs::read(out.join(&gold.files[0])).unwrap();
        assert_eq!(crate::texture::png::read(&png).unwrap().width, 512);

        let icon = m.image(ICON).unwrap();
        assert_eq!(icon.kind, ImageKind::Vector);
        assert_eq!(
            (icon.width, icon.height, icon.format.as_deref()),
            (Some(24), Some(12), Some("SVG"))
        );
        assert_eq!(
            std::fs::read_to_string(out.join(&icon.files[0])).unwrap(),
            SVG
        );
        assert_eq!(icon.files, export_names(ICON));
        let drawn = std::fs::read(out.join(&icon.files[1])).unwrap();
        let drawn = crate::texture::png::read(&drawn).unwrap();
        assert_eq!((drawn.width, drawn.height), (256, 128));

        let broken = m.image(BROKEN).unwrap();
        assert!(broken.files.is_empty());
        assert_eq!(broken.width, None);
        assert_eq!(
            broken.sha256.as_deref(),
            Some(sha256_hex(b"not a texture").as_str())
        );
        assert!(
            broken
                .error
                .as_deref()
                .unwrap()
                .starts_with("couldn't decode the texture")
        );
        let failures = std::fs::read_to_string(out.join(FAILURES)).unwrap();
        assert!(
            failures.starts_with("DeadTune could not export 1 of 4 images (build 4242)."),
            "{failures}"
        );
        assert!(
            failures.contains(&format!("\n{BROKEN}\n    couldn't decode")),
            "{failures}"
        );

        let json = std::fs::read_to_string(out.join(MANIFEST_JSON)).unwrap();
        assert!(json.contains("\"kind\": \"vector\""), "{json}");
        assert!(
            !json.contains("\"error\": null"),
            "no error key when it worked"
        );

        let zip = done.zip.clone().unwrap();
        assert_eq!(
            zip,
            tmp.path().join("exports/ui-images-4242-2026-10-05.zip")
        );
        let bytes = std::fs::read(&zip).unwrap();
        assert_eq!(crate::zip::extract(&bytes, "gold_psd.png").unwrap(), png);
        assert_eq!(
            crate::zip::extract(&bytes, MANIFEST_JSON).unwrap(),
            json.as_bytes()
        );
        assert_eq!(
            crate::zip::extract(&bytes, FAILURES).unwrap(),
            failures.as_bytes()
        );
        assert!(
            !tmp.path()
                .join("exports/ui-images-4242-2026-10-05.zip.partial")
                .exists()
        );
        assert!(done.bytes > png.len() as u64);

        let mut files = pak_files();
        files.insert(BROKEN.to_string(), COLOR.to_vec());
        std::fs::write(paths.citadel_dir.join(GAME_PAK), vpk::write(&files)).unwrap();
        let source = ImageSource::game(&paths).unwrap();
        let again = export_all(&source, &out, None, &ExportOptions::default(), &mut go).unwrap();
        assert_eq!(again.manifest.failed, 0);
        assert!(
            !out.join(FAILURES).exists(),
            "a run without failures removes the old list"
        );
        assert_eq!(again.zip, None);
        assert_eq!(std::fs::read(out.join(&gold.files[0])).unwrap(), png);
    }

    #[test]
    #[cfg(not(feature = "svg"))]
    fn without_the_svg_feature_a_vector_icon_keeps_its_svg_and_says_why() {
        let (entry, files) = export_one(ICON, ImageKind::Vector, Ok(inject::style_resource(SVG)));
        assert_eq!(files.len(), 1);
        assert_eq!(entry.files, export_names(ICON)[..1]);
        assert!(entry.error.unwrap().contains("the SVG is there"));
    }

    #[test]
    fn one_folder_cancel_and_nothing_to_export() {
        let (tmp, paths) = fake_install(&vpk::write(&pak_files()), "7");
        let source = ImageSource::game(&paths).unwrap();
        let out = tmp.path().join("top");
        let top_bar = ExportOptions {
            folder: Some("hud/top_bar".into()),
            zip: false,
        };
        let done = export_all(&source, &out, None, &top_bar, &mut go).unwrap();
        let listed: Vec<&str> = done
            .manifest
            .images
            .iter()
            .map(|i| i.path.as_str())
            .collect();
        assert_eq!(listed, [ICON]);
        assert_eq!(done.manifest.folder.as_deref(), Some("hud/top_bar"));
        assert!(!out.join("panorama/images/minimap").exists());

        let fresh = tmp.path().join("cancelled");
        let zipped = ExportOptions {
            folder: None,
            zip: true,
        };
        let err = export_all(&source, &fresh, None, &zipped, &mut |p| {
            if p.done == 2 {
                ControlFlow::Break(())
            } else {
                ControlFlow::Continue(())
            }
        })
        .unwrap_err();
        assert!(matches!(err, SnapshotError::Cancelled));
        assert!(!fresh.exists(), "a fresh folder is removed");
        assert!(!tmp.path().join("cancelled.zip.partial").exists());
        assert!(!tmp.path().join("cancelled.zip").exists());

        let err = export_all(&source, &out, None, &ExportOptions::default(), &mut |_| {
            ControlFlow::Break(())
        })
        .unwrap_err();
        assert!(matches!(err, SnapshotError::Cancelled));
        assert!(
            out.join(MANIFEST_JSON).is_file(),
            "an existing export is kept"
        );

        let heroes = ExportOptions {
            folder: Some("heroes".into()),
            zip: false,
        };
        let none = tmp.path().join("none");
        let err = export_all(&source, &none, None, &heroes, &mut go).unwrap_err();
        assert_eq!(err.to_string(), "no images under panorama/images/heroes");
        assert!(!none.exists());
    }

    #[test]
    fn a_snapshot_folder_exports_with_its_build() {
        let (tmp, paths) = fake_install(&vpk::write(&pak_files()), "55");
        let data = tmp.path().join("data");
        let selection = crate::snapshot::Selection {
            categories: Default::default(),
            images: crate::snapshot::ImageScope::All,
            ..Default::default()
        };
        let snap = crate::snapshot::take(&paths, &data, &selection, &mut go).unwrap();
        let source = ImageSource::folder(&snap.folder);
        assert!(source.describe().starts_with("Previewing snapshot 55-"));
        assert_eq!(source.buildid(&paths).as_deref(), Some("55"));
        let out = tmp.path().join("from-snapshot");
        let buildid = source.buildid(&paths);
        let done = export_all(&source, &out, buildid, &ExportOptions::default(), &mut go).unwrap();
        assert_eq!(done.manifest.total, 4);
        assert!(done.manifest.source.starts_with("snapshot 55-"));
        let gold = "panorama/images/minimap/gold_psd.png";
        assert_eq!(
            std::fs::read(out.join(gold)).unwrap(),
            std::fs::read(snap.folder.join("text").join(gold)).unwrap(),
            "the same decode as the snapshot's own copy"
        );
    }
}
