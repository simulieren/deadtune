//! UI image overrides: any Panorama image (map icons, top bar, portraits, item icons)
//! rebuilt from a source plus a short list of colour adjustments. The source is the game's
//! own image or the player's PNG or SVG; the adjustments (`texture::adjust`) are replayed on
//! it at every HUD build, from the player's current game file, so a game update that changes
//! a header or an icon's pixels is picked up on the next apply. The choice lives in the
//! profile's HUD layout as game path -> [`IconOverride`]; a player's image is copied into
//! `<data>/icons/<sha256>.<ext>` so their source file can go. One icon that no longer works
//! is reported without holding back the rest.

use std::collections::BTreeMap;
use std::fmt;
use std::path::{Path, PathBuf};

use super::vpk::VpkDir;
use crate::backup::{atomic_write, sha256_hex};
use crate::texture::adjust::{self, Adjust, AdjustKind};
use crate::texture::encode::{self, EncodeError, Fit};
use crate::texture::png::{self, PngError};
use crate::texture::svg::{self, SvgError};
use crate::texture::{self, RgbaImage};

pub const ICONS_DIR: &str = "icons";
pub const IMAGES_ROOT: &str = "panorama/images/";
const PNG_MAGIC: &[u8] = b"\x89PNG\r\n\x1a\n";

/// What kind of game image a path names.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Target {
    /// `.vtex_c`: takes a PNG.
    Raster,
    /// `.vsvg_c`: takes an SVG, or experimentally a PNG wrapped in one.
    Vector,
}

/// Where an override's pixels come from before its adjustments.
#[derive(Clone, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(tag = "input", rename_all = "snake_case")]
pub enum Source {
    /// The game's own image, decoded from the player's current file.
    Game,
    /// A PNG encoded into a `.vtex_c`.
    Png {
        image_sha256: String,
        #[serde(default)]
        fit: Fit,
    },
    /// SVG text written into a `.vsvg_c`.
    Svg { image_sha256: String },
    /// Experimental: a PNG embedded in an SVG sized to the game's view box, for a `.vsvg_c`.
    /// Whether Panorama draws embedded rasters is not confirmed.
    PngInSvg { image_sha256: String },
}

/// One overridden image, keyed by its game path in [`super::HudLayout::icons`]: a source
/// and the adjustments applied to it, in order. Profiles from before adjustments existed
/// carry only the source fields and load unchanged.
#[derive(Clone, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct IconOverride {
    #[serde(flatten)]
    pub source: Source,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub adjust: Vec<Adjust>,
}

impl IconOverride {
    pub fn new(source: Source) -> IconOverride {
        IconOverride {
            source,
            adjust: Vec::new(),
        }
    }

    /// The game's image with nothing done to it: no override at all, so the map drops it.
    pub fn is_vanilla(&self) -> bool {
        self.source == Source::Game && self.adjust.is_empty()
    }

    /// The stored copy's hash, when the source is the player's file.
    pub fn image_sha256(&self) -> Option<&str> {
        match &self.source {
            Source::Game => None,
            Source::Png { image_sha256, .. }
            | Source::Svg { image_sha256 }
            | Source::PngInSvg { image_sha256 } => Some(image_sha256),
        }
    }

    pub fn is_experimental(&self) -> bool {
        matches!(self.source, Source::PngInSvg { .. })
    }

    pub fn fit(&self) -> Option<Fit> {
        match self.source {
            Source::Png { fit, .. } => Some(fit),
            _ => None,
        }
    }

    /// Kinds the source may replace; `Game` suits either.
    fn allows(&self, target: Target) -> bool {
        match self.source {
            Source::Game => true,
            Source::Png { .. } => target == Target::Raster,
            Source::Svg { .. } | Source::PngInSvg { .. } => target == Target::Vector,
        }
    }

    fn extension(&self) -> &'static str {
        match self.source {
            Source::Svg { .. } => "svg",
            Source::Game | Source::Png { .. } | Source::PngInSvg { .. } => "png",
        }
    }

    /// Where the stored copy of the player's image lives, when there is one.
    pub fn stored_at(&self, data_dir: &Path) -> Option<PathBuf> {
        let sha = self.image_sha256()?;
        Some(
            data_dir
                .join(ICONS_DIR)
                .join(format!("{sha}.{}", self.extension())),
        )
    }
}

/// Why an override could not be set. Raised at the boundary, before anything is stored.
#[derive(Debug, thiserror::Error)]
pub enum IconError {
    #[error(
        "{0} is not a replaceable image; expected a path under {IMAGES_ROOT} ending in .vtex_c or .vsvg_c"
    )]
    Path(String),
    #[error("the file is neither a PNG nor an SVG")]
    UnknownImage,
    #[error(transparent)]
    Png(#[from] PngError),
    #[error(transparent)]
    Svg(#[from] SvgError),
    #[error("{0} is a bitmap image; use a PNG, not an SVG")]
    SvgForRaster(String),
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
}

/// An override that did not make it into the pak this time, and why. The rest still ship.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IconProblem {
    pub game_path: String,
    pub reason: String,
}

impl fmt::Display for IconProblem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.game_path, self.reason)
    }
}

/// The kind of image at `game_path`, or why it cannot be overridden.
pub fn target(game_path: &str) -> Result<Target, IconError> {
    let refuse = || Err(IconError::Path(game_path.to_string()));
    let Some(rest) = game_path.strip_prefix(IMAGES_ROOT) else {
        return refuse();
    };
    if rest
        .split('/')
        .any(|s| s.is_empty() || s == "." || s == "..")
        || game_path.contains('\\')
    {
        return refuse();
    }
    if rest.ends_with(".vtex_c") {
        Ok(Target::Raster)
    } else if rest.ends_with(".vsvg_c") {
        Ok(Target::Vector)
    } else {
        refuse()
    }
}

/// Validates `image` (PNG or SVG bytes) for `game_path`, stores a copy under `data_dir`, and
/// makes it the source of `game_path`'s override, keeping any adjustments already on it.
/// Setting the same image again changes nothing.
pub fn set(
    icons: &mut BTreeMap<String, IconOverride>,
    data_dir: &Path,
    game_path: &str,
    image: &[u8],
    fit: Fit,
) -> Result<IconOverride, IconError> {
    let target = target(game_path)?;
    let image_sha256 = sha256_hex(image);
    let source = if image.starts_with(PNG_MAGIC) {
        png::read(image)?;
        match target {
            Target::Raster => Source::Png { image_sha256, fit },
            Target::Vector => Source::PngInSvg { image_sha256 },
        }
    } else if let Some(text) = std::str::from_utf8(image)
        .ok()
        .filter(|t| t.contains("<svg"))
    {
        svg::validate(text)?;
        match target {
            Target::Raster => return Err(IconError::SvgForRaster(game_path.to_string())),
            Target::Vector => Source::Svg { image_sha256 },
        }
    } else {
        return Err(IconError::UnknownImage);
    };
    let entry = icons
        .entry(game_path.to_string())
        .or_insert_with(|| IconOverride::new(Source::Game));
    entry.source = source;
    let stored = entry
        .stored_at(data_dir)
        .expect("a player's image is stored");
    if !stored.is_file() {
        std::fs::create_dir_all(data_dir.join(ICONS_DIR))?;
        atomic_write(&stored, image)?;
    }
    Ok(entry.clone())
}

/// Sets the fit of a PNG source; `false` when `game_path` has no PNG source or it already
/// has that fit.
pub fn set_fit(icons: &mut BTreeMap<String, IconOverride>, game_path: &str, fit: Fit) -> bool {
    match icons.get_mut(game_path).map(|o| &mut o.source) {
        Some(Source::Png { fit: current, .. }) if *current != fit => {
            *current = fit;
            true
        }
        _ => false,
    }
}

/// Puts `adjust` on `game_path`'s override (`texture::adjust::set`), starting one from the
/// game's own image when there is none. `Ok(false)` when nothing changed.
pub fn adjust(
    icons: &mut BTreeMap<String, IconOverride>,
    game_path: &str,
    adjust: Adjust,
) -> Result<bool, IconError> {
    target(game_path)?;
    let entry = icons
        .entry(game_path.to_string())
        .or_insert_with(|| IconOverride::new(Source::Game));
    let changed = adjust::set(&mut entry.adjust, adjust);
    if entry.is_vanilla() {
        icons.remove(game_path);
    }
    Ok(changed)
}

/// Replaces `game_path`'s whole adjustment list (each entry clamped, identities dropped),
/// starting an override from the game's image when there is none and dropping one left
/// with nothing. `Ok(false)` when the list already was that.
pub fn set_adjustments(
    icons: &mut BTreeMap<String, IconOverride>,
    game_path: &str,
    list: &[Adjust],
) -> Result<bool, IconError> {
    target(game_path)?;
    let mut clean = Vec::new();
    for item in list {
        adjust::set(&mut clean, *item);
    }
    let entry = icons
        .entry(game_path.to_string())
        .or_insert_with(|| IconOverride::new(Source::Game));
    let changed = entry.adjust != clean;
    entry.adjust = clean;
    if entry.is_vanilla() {
        icons.remove(game_path);
    }
    Ok(changed)
}

/// Takes one kind of adjustment off `game_path`; an override left with nothing goes.
pub fn remove_adjust(
    icons: &mut BTreeMap<String, IconOverride>,
    game_path: &str,
    kind: AdjustKind,
) -> bool {
    let Some(entry) = icons.get_mut(game_path) else {
        return false;
    };
    let changed = adjust::remove(&mut entry.adjust, kind);
    if entry.is_vanilla() {
        icons.remove(game_path);
    }
    changed
}

/// Drops the override for `game_path`; `false` when there was none.
pub fn reset(icons: &mut BTreeMap<String, IconOverride>, game_path: &str) -> bool {
    icons.remove(game_path).is_some()
}

pub fn reset_all(icons: &mut BTreeMap<String, IconOverride>) {
    icons.clear();
}

/// Each override encoded from the game's current file, keyed by game path, plus one problem
/// per override that could not be built.
pub fn build(
    game: &VpkDir,
    icons: &BTreeMap<String, IconOverride>,
    data_dir: &Path,
) -> (BTreeMap<String, Vec<u8>>, Vec<IconProblem>) {
    let mut files = BTreeMap::new();
    let mut problems = Vec::new();
    for (path, entry) in icons {
        match build_one(game, path, entry, data_dir) {
            Ok(bytes) => {
                files.insert(path.clone(), bytes);
            }
            Err(reason) => problems.push(IconProblem {
                game_path: path.clone(),
                reason,
            }),
        }
    }
    (files, problems)
}

fn build_one(
    game: &VpkDir,
    path: &str,
    entry: &IconOverride,
    data_dir: &Path,
) -> Result<Vec<u8>, String> {
    let target = target(path).map_err(|e| e.to_string())?;
    if !entry.allows(target) {
        return Err(format!(
            "a {} override cannot replace this kind of image",
            entry.extension()
        ));
    }
    if !game.contains(path) {
        return Err("the game no longer has this image (renamed or removed by an update)".into());
    }
    let original = game.read(path).map_err(|e| format!("game file: {e}"))?;
    let image = match entry.stored_at(data_dir) {
        None => Vec::new(),
        Some(stored) => {
            let image = std::fs::read(&stored).map_err(|e| {
                format!(
                    "your image {} could not be read ({e}); set it again",
                    stored.display()
                )
            })?;
            if Some(sha256_hex(&image).as_str()) != entry.image_sha256() {
                return Err(format!(
                    "your image {} changed on disk; set it again",
                    stored.display()
                ));
            }
            image
        }
    };
    let adjusted = |mut rgba: RgbaImage| {
        adjust::apply_all(&mut rgba, &entry.adjust);
        rgba
    };
    let encoded = match (&entry.source, target) {
        (Source::Game, Target::Raster) => {
            let rgba = texture::decode(&original).map_err(|e| e.to_string())?;
            encode::replace(&original, &adjusted(rgba), Fit::Original)
                .map_err(|e: EncodeError| e.to_string())?
        }
        (Source::Game, Target::Vector) => {
            let text = svg::svg_text(&original).map_err(|e| e.to_string())?;
            svg::with_svg_text(&original, &svg::adjust(&text, &entry.adjust))
                .map_err(|e| e.to_string())?
        }
        (Source::Png { fit, .. }, _) => {
            let rgba = png::read(&image).map_err(|e| e.to_string())?;
            encode::replace(&original, &adjusted(rgba), *fit)
                .map_err(|e: EncodeError| e.to_string())?
        }
        (Source::Svg { .. }, _) => {
            let text =
                std::str::from_utf8(&image).map_err(|_| "your SVG is not UTF-8".to_string())?;
            svg::with_svg_text(&original, &svg::adjust(text, &entry.adjust))
                .map_err(|e| e.to_string())?
        }
        (Source::PngInSvg { .. }, _) => {
            let rgba = png::read(&image).map_err(|e| e.to_string())?;
            let png = if entry.adjust.is_empty() {
                image
            } else {
                png::write(&adjusted(rgba)).map_err(|e| e.to_string())?
            };
            let game_svg = svg::svg_text(&original).map_err(|e| e.to_string())?;
            let view = svg::validate(&game_svg).map_err(|e| e.to_string())?;
            svg::with_svg_text(&original, &svg::png_in_svg(&png, view))
                .map_err(|e| e.to_string())?
        }
    };
    Ok(encoded)
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::addons::native_scope::tests::plain_vtex;
    use crate::hud::vpk;
    use crate::texture::adjust::Rgb;
    use crate::texture::png::tests::pattern;
    use crate::texture::svg::tests::{GAME_SVG, compiled};
    use crate::texture::vtex::{Flags, Vtex};

    pub const RASTER: &str = "panorama/images/hud/minimap/hero_icon_psd.vtex_c";
    pub const VECTOR: &str = "panorama/images/hud/top_bar/soul_orb.vsvg_c";
    pub const MY_SVG: &str = "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 10 10\"><rect width=\"10\" height=\"10\"/></svg>";
    const RED: Rgb = Rgb([255, 0, 0]);

    fn tint(strength: u8) -> Adjust {
        Adjust::Tint {
            color: RED,
            strength,
        }
    }

    /// A 4x2 BGRA8888 game texture, white on the left half and transparent on the right.
    fn game_bgra() -> Vec<u8> {
        let mut px = Vec::new();
        for _ in 0..2 {
            px.extend_from_slice(&[
                255, 255, 255, 255, 255, 255, 255, 255, 0, 0, 0, 0, 0, 0, 0, 0,
            ]);
        }
        plain_vtex(4, 2, 28, 1, Flags::NO_LOD.0, &[], &px)
    }

    pub fn game() -> VpkDir {
        VpkDir::in_memory(vpk::write(&BTreeMap::from([
            (RASTER.to_string(), game_bgra()),
            (VECTOR.to_string(), compiled(GAME_SVG)),
        ])))
        .unwrap()
    }

    pub fn my_png() -> Vec<u8> {
        png::write(&pattern(16, 8)).unwrap()
    }

    fn png_source(sha: &str, fit: Fit) -> IconOverride {
        IconOverride::new(Source::Png {
            image_sha256: sha.into(),
            fit,
        })
    }

    #[test]
    fn target_accepts_only_panorama_images() {
        assert_eq!(target(RASTER).unwrap(), Target::Raster);
        assert_eq!(target(VECTOR).unwrap(), Target::Vector);
        for bad in [
            "materials/x.vtex_c",
            "panorama/images/x.png",
            "panorama/images/../styles/hud.vcss_c",
            "panorama/images//x.vtex_c",
            "panorama/images/a\\b.vtex_c",
            "panorama/styles/hud.vcss_c",
            "panorama/images/",
        ] {
            assert!(matches!(target(bad), Err(IconError::Path(_))), "{bad}");
        }
    }

    #[test]
    fn set_stores_one_copy_and_is_idempotent() {
        let dir = tempfile::tempdir().unwrap();
        let mut icons = BTreeMap::new();
        let png = my_png();
        let got = set(&mut icons, dir.path(), RASTER, &png, Fit::Own).unwrap();
        let sha = sha256_hex(&png);
        assert_eq!(got, png_source(&sha, Fit::Own));
        let stored = dir.path().join(ICONS_DIR).join(format!("{sha}.png"));
        assert_eq!(std::fs::read(&stored).unwrap(), png);
        let before = icons.clone();
        set(&mut icons, dir.path(), RASTER, &png, Fit::Own).unwrap();
        assert_eq!(icons, before);
        assert_eq!(
            std::fs::read_dir(dir.path().join(ICONS_DIR))
                .unwrap()
                .count(),
            1
        );

        assert_eq!(
            set(
                &mut icons,
                dir.path(),
                VECTOR,
                MY_SVG.as_bytes(),
                Fit::Original
            )
            .unwrap(),
            IconOverride::new(Source::Svg {
                image_sha256: sha256_hex(MY_SVG.as_bytes())
            })
        );
        let wrapped = set(&mut icons, dir.path(), VECTOR, &png, Fit::Original).unwrap();
        assert!(wrapped.is_experimental());
        assert_eq!(icons.len(), 2, "one override per path; the last one wins");

        assert!(reset(&mut icons, VECTOR));
        assert!(!reset(&mut icons, VECTOR));
        reset_all(&mut icons);
        reset_all(&mut icons);
        assert!(icons.is_empty());
        assert!(
            stored.is_file(),
            "stored images outlive a reset; other profiles may use them"
        );
    }

    #[test]
    fn set_refuses_bad_input_before_storing_anything() {
        let dir = tempfile::tempdir().unwrap();
        let mut icons = BTreeMap::new();
        let mut err = |path: &str, bytes: &[u8]| {
            set(&mut icons, dir.path(), path, bytes, Fit::Original).unwrap_err()
        };
        assert!(matches!(
            err("materials/a.vtex_c", &my_png()),
            IconError::Path(_)
        ));
        assert!(matches!(err(RASTER, b"GIF89a"), IconError::UnknownImage));
        assert!(matches!(err(RASTER, &my_png()[..20]), IconError::Png(_)));
        assert!(matches!(
            err(RASTER, MY_SVG.as_bytes()),
            IconError::SvgForRaster(_)
        ));
        assert!(matches!(err(VECTOR, b"<svg><g></svg>"), IconError::Svg(_)));
        assert!(icons.is_empty());
        assert!(!dir.path().join(ICONS_DIR).exists());
    }

    #[test]
    fn adjustments_start_from_the_game_image_and_vanish_when_undone() {
        let mut icons = BTreeMap::new();
        assert!(adjust(&mut icons, RASTER, tint(80)).unwrap());
        assert_eq!(
            icons[RASTER],
            IconOverride {
                source: Source::Game,
                adjust: vec![tint(80)]
            }
        );
        assert!(!icons[RASTER].is_vanilla());
        assert!(icons[RASTER].stored_at(Path::new("/d")).is_none());
        assert!(!adjust(&mut icons, RASTER, tint(80)).unwrap(), "same again");
        assert!(adjust(&mut icons, RASTER, Adjust::Invert).unwrap());
        assert!(remove_adjust(&mut icons, RASTER, AdjustKind::Color));
        assert_eq!(icons[RASTER].adjust, [Adjust::Invert]);
        assert!(adjust(&mut icons, RASTER, Adjust::Hue { degrees: 0 }).is_ok_and(|c| !c));
        assert!(remove_adjust(&mut icons, RASTER, AdjustKind::Invert));
        assert!(icons.is_empty(), "nothing left to override");
        assert!(!remove_adjust(&mut icons, RASTER, AdjustKind::Invert));
        assert!(adjust(&mut icons, RASTER, tint(0)).is_ok_and(|c| !c));
        assert!(icons.is_empty(), "an identity creates nothing");
        assert!(matches!(
            adjust(&mut icons, "materials/x.vtex_c", tint(50)),
            Err(IconError::Path(_))
        ));
    }

    #[test]
    fn a_whole_list_replaces_cleanly() {
        let mut icons = BTreeMap::new();
        let list = [
            tint(255),
            Adjust::Hue { degrees: 0 },
            Adjust::Invert,
            tint(40),
        ];
        assert!(set_adjustments(&mut icons, RASTER, &list).unwrap());
        assert_eq!(
            icons[RASTER].adjust,
            [tint(40), Adjust::Invert],
            "clamped, identities dropped, same kind replaced in place"
        );
        assert!(!set_adjustments(&mut icons, RASTER, &[tint(40), Adjust::Invert]).unwrap());
        assert!(set_adjustments(&mut icons, RASTER, &[]).unwrap());
        assert!(icons.is_empty());
        assert!(!set_adjustments(&mut icons, RASTER, &[]).unwrap());
        assert!(icons.is_empty());
    }

    #[test]
    fn a_dropped_image_keeps_the_adjustments_and_fit_follows_the_png() {
        let dir = tempfile::tempdir().unwrap();
        let mut icons = BTreeMap::new();
        adjust(&mut icons, RASTER, tint(50)).unwrap();
        let got = set(&mut icons, dir.path(), RASTER, &my_png(), Fit::Original).unwrap();
        assert_eq!(got.adjust, [tint(50)]);
        assert_eq!(got.fit(), Some(Fit::Original));
        assert!(set_fit(&mut icons, RASTER, Fit::Own));
        assert!(!set_fit(&mut icons, RASTER, Fit::Own));
        assert_eq!(icons[RASTER].fit(), Some(Fit::Own));
        assert!(remove_adjust(&mut icons, RASTER, AdjustKind::Color));
        assert_eq!(icons[RASTER].adjust, [], "the PNG source stays");
        adjust(&mut icons, VECTOR, Adjust::Invert).unwrap();
        assert!(
            !set_fit(&mut icons, VECTOR, Fit::Own),
            "no fit without a PNG"
        );
    }

    #[test]
    fn build_encodes_each_kind_from_the_games_file() {
        let dir = tempfile::tempdir().unwrap();
        let mut icons = BTreeMap::new();
        set(&mut icons, dir.path(), RASTER, &my_png(), Fit::Original).unwrap();
        set(
            &mut icons,
            dir.path(),
            VECTOR,
            MY_SVG.as_bytes(),
            Fit::Original,
        )
        .unwrap();
        let (files, problems) = build(&game(), &icons, dir.path());
        assert_eq!(problems, []);
        let v = Vtex::parse(&files[RASTER]).unwrap();
        assert_eq!((v.width, v.height, v.format.0, v.mips.len()), (4, 2, 28, 1));
        assert_eq!(svg::svg_text(&files[VECTOR]).unwrap(), MY_SVG);

        set(&mut icons, dir.path(), VECTOR, &my_png(), Fit::Original).unwrap();
        let (files, problems) = build(&game(), &icons, dir.path());
        assert_eq!(problems, []);
        let text = svg::svg_text(&files[VECTOR]).unwrap();
        assert!(text.contains("viewBox=\"0 0 24 24\""), "{text}");
        assert!(text.contains("data:image/png;base64,"));
    }

    fn bgra_pixels(file: &[u8]) -> Vec<[u8; 4]> {
        let v = Vtex::parse(file).unwrap();
        file[v.pixel_start()..].as_chunks::<4>().0.to_vec()
    }

    #[test]
    fn build_replays_adjustments_on_the_games_own_pixels() {
        let dir = tempfile::tempdir().unwrap();
        let mut icons = BTreeMap::new();
        adjust(&mut icons, RASTER, tint(100)).unwrap();
        adjust(&mut icons, RASTER, Adjust::Opacity { percent: 50 }).unwrap();
        adjust(&mut icons, VECTOR, Adjust::Invert).unwrap();
        let (files, problems) = build(&game(), &icons, dir.path());
        assert_eq!(problems, []);
        let v = Vtex::parse(&files[RASTER]).unwrap();
        assert_eq!((v.width, v.height, v.format.0), (4, 2, 28));
        let px = bgra_pixels(&files[RASTER]);
        assert_eq!(px[0], [0, 0, 255, 128], "white tinted red at half alpha");
        assert_eq!(px[2], [0, 0, 0, 0], "transparent stays transparent");
        let text = svg::svg_text(&files[VECTOR]).unwrap();
        assert_ne!(text, GAME_SVG, "the icon's colours were inverted");
        assert!(svg::validate(&text).is_ok());
    }

    #[test]
    fn build_adjusts_the_players_png_too() {
        let dir = tempfile::tempdir().unwrap();
        let mut icons = BTreeMap::new();
        let png =
            png::write(&RgbaImage::new(4, 2, [255, 255, 255, 255].repeat(8)).unwrap()).unwrap();
        set(&mut icons, dir.path(), RASTER, &png, Fit::Original).unwrap();
        adjust(&mut icons, RASTER, Adjust::Invert).unwrap();
        let (files, problems) = build(&game(), &icons, dir.path());
        assert_eq!(problems, []);
        assert!(
            bgra_pixels(&files[RASTER])
                .iter()
                .all(|p| *p == [0, 0, 0, 255])
        );
    }

    #[test]
    fn one_broken_override_does_not_hold_back_the_rest() {
        let dir = tempfile::tempdir().unwrap();
        let mut icons = BTreeMap::new();
        set(&mut icons, dir.path(), RASTER, &my_png(), Fit::Original).unwrap();
        let gone = "panorama/images/hud/removed_in_update_psd.vtex_c";
        set(&mut icons, dir.path(), gone, &my_png(), Fit::Original).unwrap();
        let deleted = VECTOR;
        icons.insert(
            deleted.to_string(),
            IconOverride::new(Source::PngInSvg {
                image_sha256: "0".repeat(64),
            }),
        );
        icons.insert(
            "materials/hand_edited.vtex_c".to_string(),
            icons[RASTER].clone(),
        );
        icons.insert(
            "panorama/images/hud/wrong_kind.vsvg_c".to_string(),
            icons[RASTER].clone(),
        );
        let (files, problems) = build(&game(), &icons, dir.path());
        assert_eq!(files.keys().collect::<Vec<_>>(), [RASTER]);
        let reasons: BTreeMap<&str, &str> = problems
            .iter()
            .map(|p| (p.game_path.as_str(), p.reason.as_str()))
            .collect();
        assert_eq!(reasons.len(), 4);
        assert!(reasons[gone].contains("no longer has this image"));
        assert!(reasons[deleted].contains("set it again"));
        assert!(reasons["materials/hand_edited.vtex_c"].contains("not a replaceable image"));
        assert!(reasons["panorama/images/hud/wrong_kind.vsvg_c"].contains("cannot replace"));
    }

    #[test]
    fn the_hud_layout_carries_icons_only_when_set() {
        use crate::hud::HudLayout;
        let mut hud = HudLayout::default();
        assert!(!toml::to_string(&hud).unwrap().contains("icons"));
        hud.icons
            .insert(RASTER.to_string(), png_source("ab", Fit::Original));
        assert!(!hud.is_vanilla());
        let text = toml::to_string(&hud).unwrap();
        assert!(text.contains(&format!("[icons.\"{RASTER}\"]")), "{text}");
        assert_eq!(toml::from_str::<HudLayout>(&text).unwrap(), hud);
        let patch = crate::hud::layout::compile(&hud).unwrap();
        assert!(!patch.is_empty());
        assert_eq!(patch.paths().collect::<Vec<_>>(), [RASTER]);
    }

    #[test]
    fn serde_shape_and_old_profiles() {
        let icons = BTreeMap::from([
            (RASTER.to_string(), png_source("ab", Fit::Own)),
            (
                VECTOR.to_string(),
                IconOverride {
                    source: Source::PngInSvg {
                        image_sha256: "cd".into(),
                    },
                    adjust: vec![tint(80), Adjust::Invert],
                },
            ),
            (
                "panorama/images/hud/game_psd.vtex_c".to_string(),
                IconOverride {
                    source: Source::Game,
                    adjust: vec![Adjust::Hue { degrees: -30 }],
                },
            ),
        ]);
        let text = toml::to_string(&icons).unwrap();
        assert!(
            text.contains(&format!(
                "[\"{RASTER}\"]\ninput = \"png\"\nimage_sha256 = \"ab\"\nfit = \"own\""
            )),
            "{text}"
        );
        assert!(text.contains("input = \"png_in_svg\""), "{text}");
        assert!(text.contains("input = \"game\""), "{text}");
        assert!(text.contains("kind = \"tint\""), "{text}");
        assert_eq!(
            toml::from_str::<BTreeMap<String, IconOverride>>(&text).unwrap(),
            icons
        );
        let old: IconOverride = toml::from_str("input = \"png\"\nimage_sha256 = \"ab\"").unwrap();
        assert_eq!(old, png_source("ab", Fit::Original));
        let old: IconOverride = toml::from_str("input = \"svg\"\nimage_sha256 = \"ab\"").unwrap();
        assert_eq!(
            old,
            IconOverride::new(Source::Svg {
                image_sha256: "ab".into()
            })
        );
    }
}
