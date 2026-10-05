//! Image collections: a zip or a folder of PNG and SVG files named after the game images
//! they replace, as "Save all images" writes them (`panorama/images/minimap/x_psd.png`) or
//! as a modder lays them out by hand (`my_pack/minimap/x.png`, or just `x_psd.png`). Each
//! file is matched to one game image by its path; what matches nothing is listed back so
//! the player can see why it was left out.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use super::icons::IMAGES_ROOT;

/// Files bigger than this are not images a HUD needs, and are left out unread.
pub const MAX_FILE: u64 = 32 << 20;
const COMPILED: [&str; 2] = [".vtex_c", ".vsvg_c"];
const SOURCE_SUFFIXES: [&str; 2] = ["_psd", "_png"];

/// One file from a collection, named by its path inside it with `/` separators.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PackFile {
    pub name: String,
    pub bytes: Vec<u8>,
}

/// Why a file of a collection replaces nothing.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Skip {
    /// Not a `.png` or `.svg`: a manifest, a readme, a text file.
    NotImage,
    /// No game image has that name.
    NoMatch,
    /// Several game images share the bare file name; the file needs its folder.
    Ambiguous,
    /// Another file in the collection already replaces the same image (a vector icon's
    /// drawn PNG next to its SVG, or a second copy).
    Twin,
}

impl Skip {
    pub fn reason(self) -> &'static str {
        match self {
            Skip::NotImage => "not a PNG or SVG",
            Skip::NoMatch => "no game image has this name",
            Skip::Ambiguous => "several game images have this name; keep its folder",
            Skip::Twin => "another file here already replaces the same image",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Matched {
    pub game_path: String,
    pub file: PackFile,
}

/// What importing a collection would do.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Plan {
    pub matched: Vec<Matched>,
    pub skipped: Vec<(String, Skip)>,
}

impl Plan {
    /// Skipped files other than the ones that were never images.
    pub fn unmatched(&self) -> impl Iterator<Item = &(String, Skip)> {
        self.skipped
            .iter()
            .filter(|(_, why)| *why != Skip::NotImage)
    }
}

enum Ext {
    Png,
    Svg,
}

/// Game paths by every name a file may use for them: each tail of the path below
/// `panorama/images/` without its compiled extension (`minimap/x_psd`, `x_psd`), each also
/// without a `_psd`/`_png` source suffix.
struct Index<'a> {
    by_tail: BTreeMap<String, BTreeSet<&'a str>>,
}

impl<'a> Index<'a> {
    fn new(game_paths: &'a [String]) -> Index<'a> {
        let mut by_tail: BTreeMap<String, BTreeSet<&str>> = BTreeMap::new();
        for path in game_paths {
            let Some(rel) = path.strip_prefix(IMAGES_ROOT) else {
                continue;
            };
            let Some(stem) = COMPILED.iter().find_map(|ext| rel.strip_suffix(ext)) else {
                continue;
            };
            let stem = stem.to_lowercase();
            let mut keys = vec![stem.clone()];
            keys.extend(
                SOURCE_SUFFIXES
                    .iter()
                    .filter_map(|s| stem.strip_suffix(s).map(str::to_string)),
            );
            for key in keys {
                let parts: Vec<&str> = key.split('/').collect();
                for start in 0..parts.len() {
                    by_tail
                        .entry(parts[start..].join("/"))
                        .or_default()
                        .insert(path);
                }
            }
        }
        Index { by_tail }
    }

    /// The game image `stem` names for a file of kind `ext`: the longest tail of its path
    /// that some image has, when that names one image.
    fn find(&self, stem: &str, ext: &Ext) -> Result<&'a str, Skip> {
        let parts: Vec<&str> = stem.split('/').collect();
        for start in 0..parts.len() {
            let Some(paths) = self.by_tail.get(&parts[start..].join("/")) else {
                continue;
            };
            let candidates: BTreeSet<&str> =
                paths.iter().copied().filter(|p| suits(p, ext)).collect();
            if candidates.is_empty() {
                continue;
            }
            if candidates.len() == 1 || one_name(&candidates) {
                return pick(&candidates, ext).ok_or(Skip::NoMatch);
            }
            return Err(Skip::Ambiguous);
        }
        Err(Skip::NoMatch)
    }
}

/// A PNG names a texture first and a vector icon only when no texture has the name; an SVG
/// names only vector icons.
fn suits(path: &str, ext: &Ext) -> bool {
    match ext {
        Ext::Svg => path.ends_with(".vsvg_c"),
        Ext::Png => true,
    }
}

fn pick<'a>(paths: &BTreeSet<&'a str>, ext: &Ext) -> Option<&'a str> {
    let raster = paths.iter().find(|p| p.ends_with(".vtex_c"));
    let vector = paths.iter().find(|p| p.ends_with(".vsvg_c"));
    match ext {
        Ext::Svg => vector.copied(),
        Ext::Png => raster.or(vector).copied(),
    }
}

/// A texture and a vector icon at the same path: one name, which [`pick`] settles by kind.
fn one_name(paths: &BTreeSet<&str>) -> bool {
    let stems: BTreeSet<&str> = paths
        .iter()
        .map(|p| COMPILED.iter().find_map(|e| p.strip_suffix(e)).unwrap_or(p))
        .collect();
    paths.len() == 2 && stems.len() == 1
}

fn hidden(name: &str) -> bool {
    name.split('/')
        .any(|part| part.starts_with('.') || part.eq_ignore_ascii_case("__MACOSX"))
}

/// Matches each file of a collection to one of `game_paths`. An SVG wins over a PNG for
/// the same vector icon; otherwise the first file for an image wins.
pub fn plan(files: Vec<PackFile>, game_paths: &[String]) -> Plan {
    let index = Index::new(game_paths);
    let mut out = Plan::default();
    let mut taken: BTreeMap<&str, usize> = BTreeMap::new();
    let mut files: Vec<PackFile> = files
        .into_iter()
        .map(|f| PackFile {
            name: f
                .name
                .replace('\\', "/")
                .trim_start_matches("./")
                .to_string(),
            bytes: f.bytes,
        })
        .filter(|f| !hidden(&f.name))
        .collect();
    files.sort_by_key(|f| !f.name.to_lowercase().ends_with(".svg"));
    for file in files {
        let lower = file.name.to_lowercase();
        let (stem, ext) = match (lower.strip_suffix(".png"), lower.strip_suffix(".svg")) {
            (Some(stem), _) => (stem, Ext::Png),
            (_, Some(stem)) => (stem, Ext::Svg),
            _ => {
                out.skipped.push((file.name, Skip::NotImage));
                continue;
            }
        };
        let stem = stem
            .rsplit_once(IMAGES_ROOT)
            .map_or(stem, |(_, rest)| rest)
            .to_string();
        match index.find(&stem, &ext) {
            Ok(path) if taken.contains_key(path) => out.skipped.push((file.name, Skip::Twin)),
            Ok(path) => {
                taken.insert(path, out.matched.len());
                out.matched.push(Matched {
                    game_path: path.to_string(),
                    file,
                });
            }
            Err(why) => out.skipped.push((file.name, why)),
        }
    }
    out.matched.sort_by(|a, b| a.game_path.cmp(&b.game_path));
    out.skipped.sort();
    out
}

/// The PNG and SVG files under `dir`, named by their path below it; bigger files and
/// anything unreadable are left out.
pub fn read_folder(dir: &Path) -> Vec<PackFile> {
    let mut out = Vec::new();
    walk(dir, dir, &mut out);
    out.sort_by(|a, b| a.name.cmp(&b.name));
    out
}

fn walk(root: &Path, dir: &Path, out: &mut Vec<PackFile>) {
    let Ok(read) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in read.flatten() {
        let path = entry.path();
        if path.is_dir() {
            walk(root, &path, out);
            continue;
        }
        let image = path
            .extension()
            .and_then(|e| e.to_str())
            .is_some_and(|e| e.eq_ignore_ascii_case("png") || e.eq_ignore_ascii_case("svg"));
        let small = entry.metadata().is_ok_and(|m| m.len() <= MAX_FILE);
        let (Ok(rel), true, true) = (path.strip_prefix(root), image, small) else {
            continue;
        };
        if let Ok(bytes) = std::fs::read(&path) {
            let name: Vec<_> = rel.iter().map(|s| s.to_string_lossy()).collect();
            out.push(PackFile {
                name: name.join("/"),
                bytes,
            });
        }
    }
}

/// The name a collection gives the replacement for `game_path`: its game path with the
/// compiled extension swapped for `ext`, the shape "Save all images" writes and [`plan`]
/// reads back.
pub fn file_name(game_path: &str, ext: &str) -> String {
    let stem = COMPILED
        .iter()
        .find_map(|e| game_path.strip_suffix(e))
        .unwrap_or(game_path);
    format!("{stem}.{ext}")
}

#[cfg(test)]
mod tests {
    use super::*;

    const ALLY: &str = "panorama/images/minimap/hero_ally_psd.vtex_c";
    const ULT: &str = "panorama/images/hud/top_bar/icon_ultimate.vsvg_c";
    const ITEM_A: &str = "panorama/images/items/weapon/shot_psd.vtex_c";
    const ITEM_B: &str = "panorama/images/items/spirit/shot_psd.vtex_c";
    const TWIN_PNG: &str = "panorama/images/hud/glyph_psd.vtex_c";
    const TWIN_SVG: &str = "panorama/images/hud/glyph_psd.vsvg_c";

    fn game() -> Vec<String> {
        [ALLY, ULT, ITEM_A, ITEM_B, TWIN_PNG, TWIN_SVG]
            .map(String::from)
            .to_vec()
    }

    fn file(name: &str) -> PackFile {
        PackFile {
            name: name.into(),
            bytes: name.as_bytes().to_vec(),
        }
    }

    fn matched(plan: &Plan) -> Vec<(&str, &str)> {
        plan.matched
            .iter()
            .map(|m| (m.file.name.as_str(), m.game_path.as_str()))
            .collect()
    }

    #[test]
    fn a_save_all_images_export_maps_back_and_prefers_the_svg() {
        let plan = plan(
            vec![
                file("panorama/images/minimap/hero_ally_psd.png"),
                file("panorama/images/hud/top_bar/icon_ultimate.png"),
                file("panorama/images/hud/top_bar/icon_ultimate.svg"),
                file("manifest.json"),
            ],
            &game(),
        );
        assert_eq!(
            matched(&plan),
            [
                ("panorama/images/hud/top_bar/icon_ultimate.svg", ULT),
                ("panorama/images/minimap/hero_ally_psd.png", ALLY),
            ]
        );
        assert_eq!(
            plan.skipped,
            [
                ("manifest.json".to_string(), Skip::NotImage),
                (
                    "panorama/images/hud/top_bar/icon_ultimate.png".to_string(),
                    Skip::Twin
                ),
            ]
        );
        assert_eq!(plan.unmatched().count(), 1);
    }

    #[test]
    fn hand_made_packs_match_by_folder_tail_or_unique_name() {
        let plan = plan(
            vec![
                file("My Pack\\minimap\\hero_ally.PNG"),
                file("pack/weapon/shot.png"),
                file("shot_psd.png"),
                file("icon_ultimate.svg"),
                file("nothing_like_it.png"),
                file("__MACOSX/minimap/._hero_ally.png"),
                file(".DS_Store"),
            ],
            &game(),
        );
        assert_eq!(
            matched(&plan),
            [
                ("icon_ultimate.svg", ULT),
                ("pack/weapon/shot.png", ITEM_A),
                ("My Pack/minimap/hero_ally.PNG", ALLY),
            ]
        );
        assert_eq!(
            plan.skipped,
            [
                ("nothing_like_it.png".to_string(), Skip::NoMatch),
                ("shot_psd.png".to_string(), Skip::Ambiguous),
            ]
        );
    }

    #[test]
    fn a_png_takes_the_texture_and_an_svg_the_vector_of_one_name() {
        let plan = plan(vec![file("glyph.png"), file("hud/glyph_psd.svg")], &game());
        assert_eq!(
            matched(&plan),
            [("hud/glyph_psd.svg", TWIN_SVG), ("glyph.png", TWIN_PNG)]
        );
    }

    #[test]
    fn file_names_round_trip_through_the_plan() {
        let names: Vec<PackFile> = game()
            .iter()
            .map(|p| {
                file(&file_name(
                    p,
                    if p.ends_with(".vsvg_c") { "svg" } else { "png" },
                ))
            })
            .collect();
        assert_eq!(names[0].name, "panorama/images/minimap/hero_ally_psd.png");
        let plan = plan(names, &game());
        assert_eq!(plan.matched.len(), game().len());
        assert!(plan.skipped.is_empty(), "{:?}", plan.skipped);
    }

    #[test]
    fn a_folder_reads_only_small_images() {
        let dir = tempfile::tempdir().unwrap();
        let sub = dir.path().join("minimap");
        std::fs::create_dir_all(&sub).unwrap();
        std::fs::write(sub.join("hero_ally.png"), b"png").unwrap();
        std::fs::write(dir.path().join("readme.txt"), b"hi").unwrap();
        std::fs::write(dir.path().join("x.SVG"), b"<svg/>").unwrap();
        let files = read_folder(dir.path());
        let names: Vec<&str> = files.iter().map(|f| f.name.as_str()).collect();
        assert_eq!(names, ["minimap/hero_ally.png", "x.SVG"]);
    }
}
