//! A profile = base preset + personal overrides, stored as TOML.
//! Also imports/exports Sqooky's `overrides.gi` format.

use std::collections::BTreeMap;
use std::path::PathBuf;

use serde::de::{Error as _, IntoDeserializer};

use crate::addons::AddonsConfig;
use crate::gi::{Override, Overrides};
use crate::hud::HudLayout;
use crate::preset::PresetId;

/// Serialized as `"kaiz_minspec"` or `"file:<path>"`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BaseRef {
    Preset(PresetId),
    File(PathBuf),
}

#[derive(Clone, Debug, PartialEq, Default, serde::Serialize, serde::Deserialize)]
pub struct ConVarEdits {
    #[serde(default)]
    pub set: BTreeMap<String, String>,
    /// Stored as `[convars.comment] names = [...]`.
    #[serde(default, with = "comment_names")]
    pub comment: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Profile {
    pub name: String,
    pub base: BaseRef,
    pub base_rev: Option<String>,
    #[serde(default)]
    pub convars: ConVarEdits,
    #[serde(default)]
    pub video: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "HudLayout::is_vanilla")]
    pub hud: HudLayout,
    #[serde(default, skip_serializing_if = "AddonsConfig::is_default")]
    pub addons: AddonsConfig,
}

#[derive(Debug, thiserror::Error)]
pub enum ProfileError {
    #[error("toml: {0}")]
    TomlDe(#[from] toml::de::Error),
    #[error("toml: {0}")]
    TomlSer(#[from] toml::ser::Error),
    #[error("overrides.gi line {line}: {msg}")]
    OverridesSyntax { line: usize, msg: String },
}

const FILE_PREFIX: &str = "file:";

impl serde::Serialize for BaseRef {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        match self {
            BaseRef::Preset(id) => id.serialize(s),
            BaseRef::File(path) => {
                let path = path
                    .to_str()
                    .ok_or_else(|| serde::ser::Error::custom("base path is not UTF-8"))?;
                s.serialize_str(&format!("{FILE_PREFIX}{path}"))
            }
        }
    }
}

impl<'de> serde::Deserialize<'de> for BaseRef {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let text = String::deserialize(d)?;
        if let Some(path) = text.strip_prefix(FILE_PREFIX) {
            return Ok(BaseRef::File(PathBuf::from(path)));
        }
        PresetId::deserialize(text.as_str().into_deserializer())
            .map(BaseRef::Preset)
            .map_err(|e: D::Error| {
                D::Error::custom(format!(
                    "base {text:?}: {e}; expected a preset id or \"{FILE_PREFIX}<path>\""
                ))
            })
    }
}

mod comment_names {
    #[derive(serde::Serialize, serde::Deserialize)]
    struct Names<T> {
        #[serde(default)]
        names: T,
    }

    pub fn serialize<S: serde::Serializer>(names: &[String], s: S) -> Result<S::Ok, S::Error> {
        serde::Serialize::serialize(&Names { names }, s)
    }

    pub fn deserialize<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Vec<String>, D::Error> {
        <Names<Vec<String>> as serde::Deserialize>::deserialize(d).map(|n| n.names)
    }
}

impl Profile {
    pub fn from_toml(text: &str) -> Result<Profile, ProfileError> {
        Ok(toml::from_str(text)?)
    }

    pub fn to_toml(&self) -> Result<String, ProfileError> {
        Ok(toml::to_string(self)?)
    }

    /// A name both set and commented is set, matching `gameinfo_updater.py`.
    pub fn overrides(&self) -> Overrides {
        let mut out: Overrides = self
            .convars
            .comment
            .iter()
            .map(|n| (n.clone(), Override::Comment))
            .collect();
        out.extend(
            self.convars
                .set
                .iter()
                .map(|(n, v)| (n.clone(), Override::Set(v.clone()))),
        );
        out
    }
}

/// Mirrors `parse_overrides` in `gameinfo_updater.py`, except that an unrecognized
/// line is an error here instead of a printed warning.
pub fn parse_overrides_gi(text: &str) -> Result<Overrides, ProfileError> {
    let mut out = Overrides::new();
    for (index, raw) in text
        .strip_prefix('\u{feff}')
        .unwrap_or(text)
        .lines()
        .enumerate()
    {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some(name) = line.strip_prefix("//").and_then(comment_lock) {
            out.entry(name.to_string()).or_insert(Override::Comment);
            continue;
        }
        let Some((name, value)) = value_lock(line) else {
            return Err(ProfileError::OverridesSyntax {
                line: index + 1,
                msg: format!("expected `// name` or `name value`, got {line:?}"),
            });
        };
        out.insert(name.to_string(), Override::Set(value.to_string()));
    }
    Ok(out)
}

/// `^//\s*(\w+)(?:\s+#.*)?$` applied to the text after `//`.
fn comment_lock(rest: &str) -> Option<&str> {
    let (name, after) = split_word(rest.trim_start())?;
    let ok = after.is_empty()
        || (after.starts_with(char::is_whitespace) && after.trim_start().starts_with('#'));
    ok.then_some(name)
}

/// `^(\w+)\s+(.+)$`, then the updater's `\s+#.*$` strip. Surrounding quotes are dropped
/// because the updater re-adds them only when missing, so `"0.7"` and `0.7` are the same lock.
fn value_lock(line: &str) -> Option<(&str, &str)> {
    let (name, after) = split_word(line)?;
    if !after.starts_with(char::is_whitespace) {
        return None;
    }
    let value = after.trim_start();
    let comment_at = value
        .char_indices()
        .find(|&(i, c)| c.is_whitespace() && value[i..].trim_start().starts_with('#'))
        .map_or(value.len(), |(i, _)| i);
    let value = value[..comment_at].trim();
    let unquoted = value
        .strip_prefix('"')
        .and_then(|v| v.strip_suffix('"'))
        .unwrap_or(value);
    Some((name, unquoted))
}

/// Splits a leading Python `\w+` run off `text`.
fn split_word(text: &str) -> Option<(&str, &str)> {
    let end = text
        .find(|c: char| !(c.is_alphanumeric() || c == '_'))
        .unwrap_or(text.len());
    (end > 0).then(|| text.split_at(end))
}

pub fn write_overrides_gi(overrides: &Overrides) -> String {
    let mut comments = String::new();
    let mut sets = String::new();
    for (name, o) in overrides {
        match o {
            Override::Comment => comments.push_str(&format!("// {name}\n")),
            Override::Set(value) => sets.push_str(&format!("{name} \"{value}\"\n")),
        }
    }
    format!(
        "# Lines starting with # are ignored\n\n# force commented out\n{comments}\n# force lock value\n{sets}"
    )
}

/// "Plugged in" and "Battery".
pub fn builtin_suggestions() -> Vec<Profile> {
    let pairs = |items: &[(&str, &str)]| {
        items
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    };
    vec![
        Profile {
            name: "Plugged in".into(),
            base: BaseRef::Preset(PresetId::Sqooky),
            base_rev: None,
            convars: ConVarEdits::default(),
            video: BTreeMap::new(),
            hud: HudLayout::default(),
            addons: AddonsConfig::default(),
        },
        Profile {
            name: "Battery".into(),
            base: BaseRef::Preset(PresetId::KaizMinspec),
            base_rev: None,
            convars: ConVarEdits {
                set: pairs(&[
                    ("fps_max", "60"),
                    ("r_particle_max_detail_level", "0"),
                    ("cl_particle_max_count", "800"),
                    ("panorama_max_fps", "15"),
                    ("panorama_max_overlay_fps", "15"),
                ]),
                comment: Vec::new(),
            },
            video: pairs(&[
                ("setting.fps_max", "60"),
                ("setting.r_particle_max_detail_level", "0"),
            ]),
            hud: HudLayout::default(),
            addons: AddonsConfig::default(),
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gi::Override;

    const PLAN_EXAMPLE: &str = r#"name = "Laptop / battery"
base = "kaiz_minspec"            # or sqooky, optilock_potato, vanilla, file:path
base_rev = "a1b2c3"              # git sha / hash of the base when profile was saved

[convars.set]
r_farz = "6000"
fps_max = "60"

[convars.comment]
names = ["citadel_camera_hero_fov"]

[video]
"setting.r_texture_stream_mip_bias" = "4"
"setting.fps_max" = "60"
"#;

    const SQOOKY_EXAMPLE: &str = include_str!(
        "../../../research/configs/OptimizationLock/auto updater/overrides.example.gi"
    );

    fn s(v: &str) -> String {
        v.to_string()
    }

    fn plan_profile() -> Profile {
        Profile {
            name: s("Laptop / battery"),
            base: BaseRef::Preset(PresetId::KaizMinspec),
            base_rev: Some(s("a1b2c3")),
            convars: ConVarEdits {
                set: [(s("r_farz"), s("6000")), (s("fps_max"), s("60"))].into(),
                comment: vec![s("citadel_camera_hero_fov")],
            },
            video: [
                (s("setting.r_texture_stream_mip_bias"), s("4")),
                (s("setting.fps_max"), s("60")),
            ]
            .into(),
            hud: HudLayout::default(),
            addons: AddonsConfig::default(),
        }
    }

    #[test]
    fn plan_example_parses() {
        assert_eq!(Profile::from_toml(PLAN_EXAMPLE).unwrap(), plan_profile());
    }

    #[test]
    fn toml_round_trip() {
        let mut profiles = vec![plan_profile()];
        profiles.extend(builtin_suggestions());
        profiles.push(Profile {
            name: s("file base, nothing else"),
            base: BaseRef::File(PathBuf::from(r"C:\Users\me\gameinfo.gi")),
            base_rev: None,
            convars: ConVarEdits::default(),
            video: BTreeMap::new(),
            hud: HudLayout::default(),
            addons: AddonsConfig::default(),
        });
        for p in profiles {
            let text = p.to_toml().unwrap();
            assert_eq!(Profile::from_toml(&text).unwrap(), p, "{text}");
        }
    }

    #[test]
    fn hud_layout_round_trips_and_vanilla_hud_is_omitted() {
        use crate::hud::{ElementEdit, ElementId};
        let mut p = plan_profile();
        assert!(
            !p.to_toml().unwrap().contains("[hud"),
            "vanilla hud adds no table"
        );
        p.hud.elements.insert(
            ElementId::Minimap,
            ElementEdit {
                scale_pct: 120,
                ..ElementEdit::default()
            },
        );
        let text = p.to_toml().unwrap();
        assert!(text.contains("[hud.elements.minimap]"), "{text}");
        assert_eq!(Profile::from_toml(&text).unwrap(), p, "{text}");
    }

    #[test]
    fn to_toml_keeps_plan_layout() {
        let text = plan_profile().to_toml().unwrap();
        assert!(text.contains("base = \"kaiz_minspec\"\n"), "{text}");
        assert!(
            text.contains("[convars.comment]\nnames = [\"citadel_camera_hero_fov\"]"),
            "{text}"
        );
    }

    #[test]
    fn base_ref_strings() {
        let toml_base = |b: BaseRef| {
            let p = Profile {
                base: b,
                ..plan_profile()
            };
            p.to_toml()
                .unwrap()
                .lines()
                .find(|l| l.starts_with("base ="))
                .unwrap()
                .to_string()
        };
        assert_eq!(
            toml_base(BaseRef::Preset(PresetId::OptilockPotato)),
            r#"base = "optilock_potato""#
        );
        assert_eq!(
            toml_base(BaseRef::Preset(PresetId::Vanilla)),
            r#"base = "vanilla""#
        );
        assert_eq!(
            toml_base(BaseRef::File(PathBuf::from("/tmp/gi.gi"))),
            r#"base = "file:/tmp/gi.gi""#
        );

        let parse_base = |v: &str| {
            Profile::from_toml(&format!("name = \"x\"\nbase = \"{v}\"\n")).map(|p| p.base)
        };
        assert_eq!(
            parse_base("sqooky_test").unwrap(),
            BaseRef::Preset(PresetId::SqookyTest)
        );
        assert_eq!(
            parse_base("file:my presets/a.gi").unwrap(),
            BaseRef::File(PathBuf::from("my presets/a.gi"))
        );
        assert!(parse_base("not_a_preset").is_err());
    }

    #[test]
    fn missing_tables_default_to_empty() {
        let p = Profile::from_toml("name = \"bare\"\nbase = \"vanilla\"\n").unwrap();
        assert_eq!(p.base_rev, None);
        assert_eq!(p.convars, ConVarEdits::default());
        assert!(p.video.is_empty());
    }

    #[test]
    fn overrides_maps_set_and_comment() {
        let expected: Overrides = [
            (s("citadel_camera_hero_fov"), Override::Comment),
            (s("fps_max"), Override::Set(s("60"))),
            (s("r_farz"), Override::Set(s("6000"))),
        ]
        .into();
        assert_eq!(plan_profile().overrides(), expected);
    }

    #[test]
    fn overrides_value_lock_beats_comment_lock() {
        let mut p = plan_profile();
        p.convars.comment.push(s("r_farz"));
        assert_eq!(p.overrides()["r_farz"], Override::Set(s("6000")));
    }

    #[test]
    fn parses_sqooky_example() {
        let expected: Overrides = [
            (s("citadel_camera_hero_fov"), Override::Comment),
            (s("r_farz"), Override::Comment),
            (s("sc_screen_size_lod_scale_override"), Override::Comment),
            (s("sc_fade_distance_scale_override"), Override::Comment),
            (s("citadel_camera_pitch_max"), Override::Comment),
            (s("r_size_cull_threshold"), Override::Set(s("0.7"))),
            (
                s("citadel_damage_offscreen_indicator_disabled"),
                Override::Set(s("false")),
            ),
            (s("citadel_unit_status_dpi"), Override::Set(s("6"))),
            (s("r_aspectratio"), Override::Set(s("2.4"))),
        ]
        .into();
        assert_eq!(parse_overrides_gi(SQOOKY_EXAMPLE).unwrap(), expected);
    }

    #[test]
    fn overrides_gi_line_rules_match_updater() {
        let text = "\u{feff}  # comment\r\n\
                    //r_a   # note\r\n\
                    \t// r_b\r\n\
                    r_c   \"1 2\"   # trailing note\r\n\
                    r_d #notacomment\r\n\
                    r_e \"x\" #a #b\r\n\
                    r_f \"\"\r\n\
                    \r\n";
        let expected: Overrides = [
            (s("r_a"), Override::Comment),
            (s("r_b"), Override::Comment),
            (s("r_c"), Override::Set(s("1 2"))),
            (s("r_d"), Override::Set(s("#notacomment"))),
            (s("r_e"), Override::Set(s("x"))),
            (s("r_f"), Override::Set(s(""))),
        ]
        .into();
        assert_eq!(parse_overrides_gi(text).unwrap(), expected);
    }

    #[test]
    fn overrides_gi_value_lock_wins_and_last_value_wins() {
        let text = "r_a 1\n// r_a\n// r_b\nr_b 2\nr_b 3\n";
        let expected: Overrides = [
            (s("r_a"), Override::Set(s("1"))),
            (s("r_b"), Override::Set(s("3"))),
        ]
        .into();
        assert_eq!(parse_overrides_gi(text).unwrap(), expected);
    }

    #[test]
    fn overrides_gi_rejects_unrecognized_lines() {
        for (text, line) in [
            ("r_a\n", 1),
            ("# ok\n// r_a extra\n", 2),
            ("r_a=1\n", 1),
            ("// \n", 1),
        ] {
            match parse_overrides_gi(text) {
                Err(ProfileError::OverridesSyntax { line: got, .. }) => {
                    assert_eq!(got, line, "{text:?}")
                }
                other => panic!("{text:?} should be rejected, got {other:?}"),
            }
        }
    }

    #[test]
    fn overrides_gi_round_trip() {
        let mut cases = vec![
            parse_overrides_gi(SQOOKY_EXAMPLE).unwrap(),
            Overrides::new(),
        ];
        cases.push(
            [
                (s("r_a"), Override::Set(s(""))),
                (s("r_b"), Override::Set(s("has spaces"))),
                (s("r_c"), Override::Set(s("a\"b"))),
                (s("r_d"), Override::Comment),
                (s("r_e"), Override::Set(s("#hash"))),
            ]
            .into(),
        );
        cases.extend(builtin_suggestions().iter().map(Profile::overrides));
        for x in cases {
            let text = write_overrides_gi(&x);
            assert_eq!(parse_overrides_gi(&text).unwrap(), x, "{text}");
        }
    }

    #[test]
    fn builtin_suggestions_are_plugged_in_and_battery() {
        let profiles = builtin_suggestions();
        let names: Vec<&str> = profiles.iter().map(|p| p.name.as_str()).collect();
        assert_eq!(names, ["Plugged in", "Battery"]);
        let battery = &profiles[1];
        assert_eq!(battery.overrides()["fps_max"], Override::Set(s("60")));
        assert_eq!(battery.video["setting.fps_max"], "60");
    }

    #[test]
    fn builtin_convars_exist_in_catalog() {
        let catalog = include_str!("../../../research/data/convar_catalog.csv");
        let known: std::collections::BTreeSet<&str> = catalog
            .lines()
            .skip(1)
            .filter_map(|l| l.split(',').next())
            .collect();
        for p in builtin_suggestions() {
            for name in p.overrides().keys() {
                assert!(
                    known.contains(name.as_str()),
                    "{} uses {name}, not in convar_catalog.csv",
                    p.name
                );
            }
        }
    }
}
