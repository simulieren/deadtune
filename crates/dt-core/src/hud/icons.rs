//! UI image overrides: the player's own PNG or SVG in place of any Panorama image (map
//! icons, top bar, portraits, item icons). The choice lives in the profile's HUD layout as
//! game path -> [`IconOverride`]; the image itself is copied into `<data>/icons/<sha256>.<ext>`
//! so the player's source file can go. Every HUD build re-encodes each override from the
//! game's current file, so a game update that changes a header is picked up on the next
//! apply, and one icon that no longer works is reported without holding back the rest.

use std::collections::BTreeMap;
use std::fmt;
use std::path::{Path, PathBuf};

use super::vpk::VpkDir;
use crate::backup::{atomic_write, sha256_hex};
use crate::texture::encode::{self, EncodeError, Fit};
use crate::texture::png::{self, PngError};
use crate::texture::svg::{self, SvgError};

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

/// One overridden image, keyed by its game path in [`super::HudLayout::icons`].
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(tag = "input", rename_all = "snake_case")]
pub enum IconOverride {
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

impl IconOverride {
    pub fn image_sha256(&self) -> &str {
        match self {
            IconOverride::Png { image_sha256, .. }
            | IconOverride::Svg { image_sha256 }
            | IconOverride::PngInSvg { image_sha256 } => image_sha256,
        }
    }

    pub fn is_experimental(&self) -> bool {
        matches!(self, IconOverride::PngInSvg { .. })
    }

    fn target(&self) -> Target {
        match self {
            IconOverride::Png { .. } => Target::Raster,
            IconOverride::Svg { .. } | IconOverride::PngInSvg { .. } => Target::Vector,
        }
    }

    fn extension(&self) -> &'static str {
        match self {
            IconOverride::Svg { .. } => "svg",
            IconOverride::Png { .. } | IconOverride::PngInSvg { .. } => "png",
        }
    }

    /// Where the stored copy of the player's image lives.
    pub fn stored_at(&self, data_dir: &Path) -> PathBuf {
        data_dir
            .join(ICONS_DIR)
            .join(format!("{}.{}", self.image_sha256(), self.extension()))
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
/// records it in `icons`. Setting the same image again changes nothing.
pub fn set(
    icons: &mut BTreeMap<String, IconOverride>,
    data_dir: &Path,
    game_path: &str,
    image: &[u8],
    fit: Fit,
) -> Result<IconOverride, IconError> {
    let target = target(game_path)?;
    let image_sha256 = sha256_hex(image);
    let entry = if image.starts_with(PNG_MAGIC) {
        png::read(image)?;
        match target {
            Target::Raster => IconOverride::Png { image_sha256, fit },
            Target::Vector => IconOverride::PngInSvg { image_sha256 },
        }
    } else if let Some(text) = std::str::from_utf8(image)
        .ok()
        .filter(|t| t.contains("<svg"))
    {
        svg::validate(text)?;
        match target {
            Target::Raster => return Err(IconError::SvgForRaster(game_path.to_string())),
            Target::Vector => IconOverride::Svg { image_sha256 },
        }
    } else {
        return Err(IconError::UnknownImage);
    };
    let stored = entry.stored_at(data_dir);
    if !stored.is_file() {
        std::fs::create_dir_all(data_dir.join(ICONS_DIR))?;
        atomic_write(&stored, image)?;
    }
    icons.insert(game_path.to_string(), entry.clone());
    Ok(entry)
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
    if target != entry.target() {
        return Err(format!(
            "a {} override cannot replace this kind of image",
            entry.extension()
        ));
    }
    if !game.contains(path) {
        return Err("the game no longer has this image (renamed or removed by an update)".into());
    }
    let original = game.read(path).map_err(|e| format!("game file: {e}"))?;
    let stored = entry.stored_at(data_dir);
    let image = std::fs::read(&stored).map_err(|e| {
        format!(
            "your image {} could not be read ({e}); set it again",
            stored.display()
        )
    })?;
    if sha256_hex(&image) != entry.image_sha256() {
        return Err(format!(
            "your image {} changed on disk; set it again",
            stored.display()
        ));
    }
    let encoded = match entry {
        IconOverride::Png { fit, .. } => {
            let rgba = png::read(&image).map_err(|e| e.to_string())?;
            encode::replace(&original, &rgba, *fit).map_err(|e: EncodeError| e.to_string())?
        }
        IconOverride::Svg { .. } => {
            let text =
                std::str::from_utf8(&image).map_err(|_| "your SVG is not UTF-8".to_string())?;
            svg::with_svg_text(&original, text).map_err(|e| e.to_string())?
        }
        IconOverride::PngInSvg { .. } => {
            png::read(&image).map_err(|e| e.to_string())?;
            let game_svg = svg::svg_text(&original).map_err(|e| e.to_string())?;
            let view = svg::validate(&game_svg).map_err(|e| e.to_string())?;
            svg::with_svg_text(&original, &svg::png_in_svg(&image, view))
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
    use crate::texture::png::tests::pattern;
    use crate::texture::svg::tests::{GAME_SVG, compiled};
    use crate::texture::vtex::{Flags, Vtex};

    pub const RASTER: &str = "panorama/images/hud/minimap/hero_icon_psd.vtex_c";
    pub const VECTOR: &str = "panorama/images/hud/top_bar/soul_orb.vsvg_c";
    pub const MY_SVG: &str = "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 10 10\"><rect width=\"10\" height=\"10\"/></svg>";

    pub fn game_raster() -> Vec<u8> {
        plain_vtex(
            64,
            32,
            20,
            1,
            Flags::NO_LOD.0,
            &[(1, vec![5; 32])],
            &[0; 64 * 32],
        )
    }

    pub fn game() -> VpkDir {
        VpkDir::in_memory(vpk::write(&BTreeMap::from([
            (RASTER.to_string(), game_raster()),
            (VECTOR.to_string(), compiled(GAME_SVG)),
        ])))
        .unwrap()
    }

    pub fn my_png() -> Vec<u8> {
        png::write(&pattern(16, 8)).unwrap()
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
        assert_eq!(
            got,
            IconOverride::Png {
                image_sha256: sha.clone(),
                fit: Fit::Own
            }
        );
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
            IconOverride::Svg {
                image_sha256: sha256_hex(MY_SVG.as_bytes())
            }
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
        assert_eq!(
            (v.width, v.height, v.format.0, v.mips.len()),
            (64, 32, 28, 1)
        );
        assert_eq!(svg::svg_text(&files[VECTOR]).unwrap(), MY_SVG);

        set(&mut icons, dir.path(), VECTOR, &my_png(), Fit::Original).unwrap();
        let (files, problems) = build(&game(), &icons, dir.path());
        assert_eq!(problems, []);
        let text = svg::svg_text(&files[VECTOR]).unwrap();
        assert!(text.contains("viewBox=\"0 0 24 24\""), "{text}");
        assert!(text.contains("data:image/png;base64,"));
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
            IconOverride::PngInSvg {
                image_sha256: "0".repeat(64),
            },
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
        hud.icons.insert(
            RASTER.to_string(),
            IconOverride::Png {
                image_sha256: "ab".into(),
                fit: Fit::Original,
            },
        );
        assert!(!hud.is_vanilla());
        let text = toml::to_string(&hud).unwrap();
        assert!(text.contains(&format!("[icons.\"{RASTER}\"]")), "{text}");
        assert_eq!(toml::from_str::<HudLayout>(&text).unwrap(), hud);
        let patch = crate::hud::layout::compile(&hud).unwrap();
        assert!(!patch.is_empty());
        assert_eq!(patch.paths().collect::<Vec<_>>(), [RASTER]);
    }

    #[test]
    fn serde_shape() {
        let icons = BTreeMap::from([
            (
                RASTER.to_string(),
                IconOverride::Png {
                    image_sha256: "ab".into(),
                    fit: Fit::Own,
                },
            ),
            (
                VECTOR.to_string(),
                IconOverride::PngInSvg {
                    image_sha256: "cd".into(),
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
        assert_eq!(
            toml::from_str::<BTreeMap<String, IconOverride>>(&text).unwrap(),
            icons
        );
        let defaulted: IconOverride =
            toml::from_str("input = \"png\"\nimage_sha256 = \"ab\"").unwrap();
        assert_eq!(
            defaulted,
            IconOverride::Png {
                image_sha256: "ab".into(),
                fit: Fit::Original
            }
        );
    }
}
