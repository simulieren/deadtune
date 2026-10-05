//! DeadTune rows inside the game's own Settings menu: a "Wide FOV" slider under Camera
//! Settings bound to `r_aspectratio`, and a "DeadTune" group under Advanced with live
//! controls for performance ConVars that otherwise need a restart. Both go into the
//! game's current `popup_settings.vxml_c` through `inject`, right after the stock FOV row
//! and at the end of the Advanced section, so the menu is rebuilt from the game's file
//! after every update and the stock rows stay byte for byte.
//!
//! Idea by Mixboat (Wide FOV Slider, GameBanana 724244, CC BY-NC-ND); rebuilt by DeadTune
//! from your game files. Only the idea comes from that mod: the stock `CitadelSettingsSlider`
//! writes whatever ConVar its `convar` attribute names, dev-only ones included. Notes:
//! docs/plans/ingame-settings/plan.md.
//!
//! Persistence: `r_aspectratio` is not archived, so our script (`assets/ingame_settings.js`)
//! copies every slider change into `STASH_CONVAR`, an archived ConVar the game saves to
//! `cfg/user_convars_*.vcfg`. `sync_wide_fov` reads it back on DeadTune's next run and
//! writes the value into `gameinfo.gi`, where DeadTune's Wide view already lives. The
//! invisible-menu trick the mod uses at boot is not needed.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use super::inject::{Anchor, Element, LayoutEdit};
use crate::backup::{BackupStore, FileKind, atomic_write};
use crate::catalog::{Catalog, Kind};
use crate::gi::{self, Override};
use crate::locate::GamePaths;
use crate::usercfg;

pub const SETTINGS_LAYOUT: &str = "panorama/layout/popups/popup_settings.vxml_c";
pub const OWN_SCRIPT: &str = "panorama/scripts/deadtune/ingame_settings.vjs_c";
const SCRIPT: &str = include_str!("assets/ingame_settings.js");

/// The stock FOV slider; our Wide FOV row goes right after the row holding it.
pub const FOV_ROW_ANCHOR: &str = "CameraFOV";
/// The Advanced section; our group is its last subsection.
pub const ADVANCED_SECTION: &str = "citadel_settings_advanced";
pub const GROUP_ID: &str = "citadel_settings_deadtune";
pub const GROUP_TITLE: &str = "DeadTune";
pub const WIDE_FOV_ROW_ID: &str = "DtWideFovRow";
pub const WIDE_FOV_ID: &str = "DtWideFov";
pub const WIDE_FOV_CONVAR: &str = "r_aspectratio";
pub const WIDE_FOV_MIN: f64 = 0.010;
pub const WIDE_FOV_MAX: f64 = 3.200;
pub const WIDE_FOV_SNAP: f64 = 0.005;

/// Where the in-game slider value is kept between launches. Archived (`cl, a`), a float the
/// game only reads for a debug drawing nobody turns on, and the one ConVar the Wide FOV
/// Slider mod has used for the same purpose in the field. Stored as `STASH_OFFSET` plus the
/// ratio so its engine default (0.075) can never be mistaken for a saved value.
pub const STASH_CONVAR: &str = "citadel_ability_preview_path_debug_draw_dt";
pub const STASH_OFFSET: f64 = 10.0;
pub const SYNC_RECORD_FILE: &str = "wide_fov_sync.toml";

/// One row of the DeadTune group: a catalog ConVar and the plain label the menu shows.
/// Ranges and steps come from the catalog, so the slider never offers a value the catalog
/// does not.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PerfRow {
    pub convar: &'static str,
    pub label: &'static str,
}

/// Restart-class, dev-only performance ConVars with a known range and a real FPS impact.
/// `rows_match_the_catalog` keeps this in step with the catalog.
pub const PERFORMANCE_ROWS: &[PerfRow] = &[
    PerfRow {
        convar: "r_citadel_shadow_quality",
        label: "Shadow quality",
    },
    PerfRow {
        convar: "r_citadel_ssao_quality",
        label: "Ambient occlusion",
    },
    PerfRow {
        convar: "r_citadel_fog_quality",
        label: "Fog quality",
    },
    PerfRow {
        convar: "r_grass_quality",
        label: "Grass",
    },
    PerfRow {
        convar: "r_propsmaxdist",
        label: "Prop draw distance",
    },
    PerfRow {
        convar: "cl_particle_max_count",
        label: "Particle cap",
    },
    PerfRow {
        convar: "sc_clutter_enable",
        label: "Small clutter props",
    },
    PerfRow {
        convar: "r_effects_bloom",
        label: "Bloom",
    },
];

pub fn perf_row(convar: &str) -> Option<&'static PerfRow> {
    PERFORMANCE_ROWS.iter().find(|r| r.convar == convar)
}

/// `Default` is vanilla: no rows, no addon files.
#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct IngameSettings {
    pub wide_fov: bool,
    /// ConVars from `PERFORMANCE_ROWS` shown in the DeadTune group.
    pub performance: BTreeSet<String>,
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum IngameError {
    #[error("{0} is not one of the DeadTune group's rows")]
    UnknownRow(String),
    #[error("{0} has no range in the catalog")]
    NoRange(String),
}

/// What the rows add to the addon.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct IngamePatch {
    /// The settings layout's additions.
    pub layout: Option<LayoutEdit>,
    pub own_files: BTreeMap<String, String>,
}

impl IngameSettings {
    pub fn is_vanilla(&self) -> bool {
        *self == IngameSettings::default()
    }

    pub fn changed_count(&self) -> usize {
        usize::from(self.wide_fov) + self.performance.len()
    }

    pub fn validate(&self) -> Result<(), IngameError> {
        for name in &self.performance {
            if perf_row(name).is_none() {
                return Err(IngameError::UnknownRow(name.clone()));
            }
        }
        Ok(())
    }

    pub fn compile(&self) -> Result<IngamePatch, IngameError> {
        self.validate()?;
        if self.is_vanilla() {
            return Ok(IngamePatch::default());
        }
        let mut panels = Vec::new();
        if self.wide_fov {
            panels.push((Anchor::AfterParentOf(FOV_ROW_ANCHOR.into()), wide_fov_row()));
        }
        if !self.performance.is_empty() {
            let mut group = Element::new("PopupSettingsSettingsSubsection").attr("id", GROUP_ID);
            for row in PERFORMANCE_ROWS
                .iter()
                .filter(|r| self.performance.contains(r.convar))
            {
                group = group.child(perf_row_element(row)?);
            }
            panels.push((Anchor::AppendTo(ADVANCED_SECTION.into()), group));
        }
        let mut own_files = BTreeMap::new();
        own_files.insert(OWN_SCRIPT.to_string(), self.script());
        Ok(IngamePatch {
            layout: Some(LayoutEdit {
                style_includes: Vec::new(),
                script_includes: vec![format!("s2r://{OWN_SCRIPT}")],
                panels,
            }),
            own_files,
        })
    }

    fn script(&self) -> String {
        let group = if self.performance.is_empty() {
            ""
        } else {
            GROUP_ID
        };
        format!(
            "var DT_INGAME = {{ wideFov: {}, sliderId: \"{WIDE_FOV_ID}\", min: {}, max: {}, \
             stash: \"{STASH_CONVAR}\", stashOffset: {}, groupId: \"{group}\", groupTitle: \"{GROUP_TITLE}\" }};\n{SCRIPT}",
            self.wide_fov,
            number(WIDE_FOV_MIN),
            number(WIDE_FOV_MAX),
            number(STASH_OFFSET),
        )
    }
}

fn wide_fov_row() -> Element {
    Element::new("PopupSettingsSettingsRow")
        .attr("id", WIDE_FOV_ROW_ID)
        .child(
            Element::new("CitadelSettingsSlider")
                .attr("id", WIDE_FOV_ID)
                .attr("class", "VideoPreview")
                .attr("convar", WIDE_FOV_CONVAR)
                .attr("min", &format!("{WIDE_FOV_MIN:.3}"))
                .attr("max", &format!("{WIDE_FOV_MAX:.3}"))
                .attr("snap", &format!("{WIDE_FOV_SNAP:.3}"))
                .attr("percentage", "false")
                .attr("displayprecision", "3")
                .attr("text", "Wide FOV")
                .attr("textentry", "true"),
        )
}

fn perf_row_element(row: &PerfRow) -> Result<Element, IngameError> {
    let entry = Catalog::embedded()
        .get(row.convar)
        .ok_or_else(|| IngameError::UnknownRow(row.convar.to_string()))?;
    let id = format!("Dt_{}", row.convar);
    let control = match entry.kind {
        Kind::Bool => Element::new("CitadelSettingsToggle")
            .attr("id", &id)
            .attr("text", row.label)
            .attr("convar", row.convar),
        _ => {
            let [lo, hi] = entry
                .range
                .ok_or_else(|| IngameError::NoRange(row.convar.to_string()))?;
            let step = entry.step.unwrap_or(1.0);
            let precision = if matches!(entry.kind, Kind::Float) {
                "2"
            } else {
                "0"
            };
            Element::new("CitadelSettingsSlider")
                .attr("id", &id)
                .attr("class", "VideoPreview")
                .attr("convar", row.convar)
                .attr("min", &number(lo))
                .attr("max", &number(hi))
                .attr("snap", &number(step))
                .attr("percentage", "false")
                .attr("displayprecision", precision)
                .attr("text", row.label)
                .attr("textentry", "true")
        }
    };
    Ok(Element::new("PopupSettingsSettingsRow")
        .attr("id", &format!("{id}Row"))
        .child(control))
}

/// Shortest decimal form: 0.5, 2, 22.5.
fn number(value: f64) -> String {
    let text = format!("{value:.4}");
    text.trim_end_matches('0').trim_end_matches('.').to_string()
}

/// A ratio as our script stores it: the offset plus the ratio, three decimals.
pub fn stash_encode(ratio: f64) -> String {
    format!("{:.3}", STASH_OFFSET + ratio)
}

/// The ratio a stash value holds, snapped to the slider's step and clamped to its range.
/// Anything below the offset (the engine default included) was never saved by us.
pub fn stash_decode(text: &str) -> Option<f64> {
    let value: f64 = text.trim().parse().ok()?;
    if value < STASH_OFFSET + WIDE_FOV_MIN - WIDE_FOV_SNAP / 2.0 {
        return None;
    }
    // Whole thousandths, so 431 steps read back as exactly 2.155 and not 2.1550000000000002.
    let thousandths = ((value - STASH_OFFSET) / WIDE_FOV_SNAP).round() * (WIDE_FOV_SNAP * 1000.0);
    Some((thousandths / 1000.0).clamp(WIDE_FOV_MIN, WIDE_FOV_MAX))
}

/// The ratio as `gameinfo.gi` and the Overview's Wide view write it.
pub fn wide_fov_text(ratio: f64) -> String {
    let text = format!("{ratio:.3}");
    text.trim_end_matches('0').trim_end_matches('.').to_string()
}

/// A stand-in for the game's settings layout with the two anchors our rows need, for
/// tests and the fake install.
pub fn stand_in_layout() -> Element {
    let row = |control: Element| Element::new("PopupSettingsSettingsRow").child(control);
    Element::new("root")
        .child(Element::new("styles").child(Element::include(
            "s2r://panorama/styles/popups/popup_settings.vcss",
        )))
        .child(
            Element::new("PopupSettings")
                .child(
                    Element::new("PopupSettingsSettingsSection")
                        .attr("id", "citadel_settings_game")
                        .child(
                            Element::new("PopupSettingsSettingsSubsection")
                                .attr("id", "citadel_settings_camera")
                                .child(row(Element::new("CitadelSettingsSlider")
                                    .attr("id", FOV_ROW_ANCHOR)
                                    .attr("class", "VideoPreview")
                                    .attr("convar", "citadel_camera_hero_fov")
                                    .attr("min", "75")
                                    .attr("max", "90")
                                    .attr("snap", "1")
                                    .attr("text", "#citadel_camera_hero_fov")))
                                .child(row(Element::new("CitadelSettingsToggle")
                                    .attr("text", "#citadel_camera_pitch_inverted")
                                    .attr("convar", "citadel_camera_pitch_inverted"))),
                        ),
                )
                .child(
                    Element::new("PopupSettingsSettingsSection")
                        .attr("id", ADVANCED_SECTION)
                        .child(
                            Element::new("PopupSettingsSettingsSubsection")
                                .attr("id", "citadel_settings_hero_specific")
                                .child(row(Element::new("CitadelSettingsToggle")
                                    .attr("text", "#citadel_settings_per_unit_hotkeys")
                                    .attr("convar", "citadel_per_unit_hotkeys_checked"))),
                        ),
                ),
        )
}

/// What `sync_wide_fov` did.
#[derive(Clone, Debug, PartialEq)]
pub struct WideFovSync {
    pub ratio: f64,
    /// `gameinfo.gi` was rewritten; false when it already held the value.
    pub gameinfo_changed: bool,
}

#[derive(Debug, thiserror::Error)]
pub enum SyncError {
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("gameinfo.gi: {0}")]
    Gi(#[from] gi::GiError),
    #[error("sync record: {0}")]
    Record(String),
}

#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
struct SyncRecord {
    /// The stash value last written into gameinfo.gi, verbatim.
    stash: String,
}

fn read_record(state_dir: &Path) -> Result<SyncRecord, SyncError> {
    match std::fs::read_to_string(state_dir.join(SYNC_RECORD_FILE)) {
        Ok(text) => toml::from_str(&text).map_err(|e| SyncError::Record(e.to_string())),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(SyncRecord::default()),
        Err(e) => Err(e.into()),
    }
}

/// Carries a Wide FOV value set in game into `gameinfo.gi`: the stash the game saved in
/// `cfg/user_convars_*.vcfg` becomes `r_aspectratio`, with the original and a backup kept
/// in `store`. Runs once per stash value (the record in `store.root` remembers the last
/// one), so a Wide view changed in DeadTune afterwards is never undone by an older in-game
/// value. `None` when there is nothing new to carry over.
pub fn sync_wide_fov(
    paths: &GamePaths,
    store: &BackupStore,
) -> Result<Option<WideFovSync>, SyncError> {
    let saved = usercfg::read_convars(&paths.cfg_dir)?;
    let Some(stash) = saved.get(STASH_CONVAR) else {
        return Ok(None);
    };
    let Some(ratio) = stash_decode(stash) else {
        return Ok(None);
    };
    if read_record(&store.root)?.stash == *stash {
        return Ok(None);
    }
    let value = wide_fov_text(ratio);
    let text = std::fs::read_to_string(&paths.gameinfo)?;
    let current = gi::effective_values(&text)?;
    let gameinfo_changed = current.get(WIDE_FOV_CONVAR) != Some(&value);
    if gameinfo_changed {
        let overrides = BTreeMap::from([(WIDE_FOV_CONVAR.to_string(), Override::Set(value))]);
        let outcome = gi::apply_overrides(&text, &overrides)?;
        store.snapshot_original(FileKind::GameInfo, &paths.gameinfo)?;
        store.backup(FileKind::GameInfo, &paths.gameinfo)?;
        atomic_write(&paths.gameinfo, outcome.text.as_bytes())?;
    }
    let record = SyncRecord {
        stash: stash.clone(),
    };
    let text = toml::to_string(&record).map_err(|e| SyncError::Record(e.to_string()))?;
    std::fs::create_dir_all(&store.root)?;
    atomic_write(&store.root.join(SYNC_RECORD_FILE), text.as_bytes())?;
    Ok(Some(WideFovSync {
        ratio,
        gameinfo_changed,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::{ApplyClass, Impact};
    use crate::hud::inject;

    fn all_on() -> IngameSettings {
        IngameSettings {
            wide_fov: true,
            performance: PERFORMANCE_ROWS
                .iter()
                .map(|r| r.convar.to_string())
                .collect(),
        }
    }

    #[test]
    fn rows_match_the_catalog() {
        let catalog = Catalog::embedded();
        let csv = std::fs::read_to_string(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../research/data/convar_catalog.csv"
        ))
        .unwrap();
        assert!((5..=10).contains(&PERFORMANCE_ROWS.len()));
        for row in PERFORMANCE_ROWS {
            let entry = catalog
                .get(row.convar)
                .unwrap_or_else(|| panic!("{}", row.convar));
            assert_eq!(entry.apply, ApplyClass::Restart, "{}", row.convar);
            assert!(
                matches!(entry.impact, Impact::High | Impact::Medium),
                "{}",
                row.convar
            );
            assert!(!entry.denylist && !entry.gameinfo_ignored, "{}", row.convar);
            match entry.kind {
                Kind::Bool => {}
                Kind::Int | Kind::Float => assert!(entry.range.is_some(), "{}", row.convar),
                _ => panic!("{}: sliders and toggles only", row.convar),
            }
            let line = csv
                .lines()
                .find(|l| l.starts_with(&format!("{},", row.convar)))
                .unwrap_or_else(|| panic!("{} not in the csv", row.convar));
            assert!(line.contains("devonly"), "{line}");
            assert!(!line.contains("cheat"), "{line}");
            assert!(
                row.label.is_ascii() && !row.label.contains('_'),
                "{}",
                row.label
            );
        }
        assert!(perf_row("fps_max").is_none());
    }

    #[test]
    fn vanilla_compiles_to_nothing_and_unknown_rows_fail() {
        assert_eq!(
            IngameSettings::default().compile(),
            Ok(IngamePatch::default())
        );
        assert!(IngameSettings::default().is_vanilla());
        let bad = IngameSettings {
            performance: BTreeSet::from(["fps_max".to_string()]),
            ..IngameSettings::default()
        };
        assert_eq!(
            bad.compile(),
            Err(IngameError::UnknownRow("fps_max".into()))
        );
        assert_eq!(all_on().changed_count(), 1 + PERFORMANCE_ROWS.len());
    }

    #[test]
    fn rows_land_after_the_fov_row_and_at_the_end_of_advanced() {
        let patch = all_on().compile().unwrap();
        let edit = patch.layout.unwrap();
        let mut root = stand_in_layout();
        let before = inject::to_xml(&root, "");
        inject::apply(&mut root, &edit).unwrap();
        inject::apply(&mut root, &edit).unwrap();
        let xml = inject::to_xml(&root, "");
        assert!(inject::extends(&xml, &before), "{xml}");
        assert_eq!(
            xml.lines().count(),
            before.lines().count() + 3 + 3 + 2 + 3 * PERFORMANCE_ROWS.len(),
            "{xml}"
        );
        let fov = xml.find("id=\"CameraFOV\"").unwrap();
        let after_fov = &xml[fov..];
        let stock_end = after_fov.find("</PopupSettingsSettingsRow>").unwrap();
        assert!(after_fov[stock_end..].trim_start_matches("</PopupSettingsSettingsRow>\n").starts_with(
            "\t\t\t\t<PopupSettingsSettingsRow id=\"DtWideFovRow\">\n\t\t\t\t\t<CitadelSettingsSlider id=\"DtWideFov\" class=\"VideoPreview\" convar=\"r_aspectratio\" min=\"0.010\" max=\"3.200\" snap=\"0.005\" percentage=\"false\" displayprecision=\"3\" text=\"Wide FOV\" textentry=\"true\" />\n"
        ), "{after_fov}");
        let group = xml
            .find("<PopupSettingsSettingsSubsection id=\"citadel_settings_deadtune\">")
            .unwrap();
        assert!(xml[group..].contains("<CitadelSettingsSlider id=\"Dt_r_citadel_shadow_quality\" class=\"VideoPreview\" convar=\"r_citadel_shadow_quality\" min=\"0\" max=\"3\" snap=\"1\" percentage=\"false\" displayprecision=\"0\" text=\"Shadow quality\" textentry=\"true\" />"), "{xml}");
        assert!(xml[group..].contains("<CitadelSettingsToggle id=\"Dt_sc_clutter_enable\" text=\"Small clutter props\" convar=\"sc_clutter_enable\" />"), "{xml}");
        let advanced_end = xml[group..]
            .find("</PopupSettingsSettingsSection>")
            .unwrap();
        assert!(
            xml[group..group + advanced_end].ends_with("</PopupSettingsSettingsSubsection>\n\t\t"),
            "ours is the last subsection"
        );
        assert!(xml.contains("\t<scripts>\n\t\t<include src=\"s2r://panorama/scripts/deadtune/ingame_settings.vjs_c\" />\n\t</scripts>\n"));
        let script = &patch.own_files[OWN_SCRIPT];
        assert!(script.starts_with("var DT_INGAME = { wideFov: true, sliderId: \"DtWideFov\", min: 0.01, max: 3.2, stash: \"citadel_ability_preview_path_debug_draw_dt\", stashOffset: 10, groupId: \"citadel_settings_deadtune\", groupTitle: \"DeadTune\" };\n(function () {"), "{script}");
        assert!(script.contains("host_writeconfig"));

        let only_wide = IngameSettings {
            wide_fov: true,
            ..IngameSettings::default()
        };
        let patch = only_wide.compile().unwrap();
        assert_eq!(patch.layout.as_ref().unwrap().panels.len(), 1);
        assert!(patch.own_files[OWN_SCRIPT].contains("groupId: \"\""));
    }

    #[test]
    fn the_stand_in_layout_is_a_compiled_layout_the_pipeline_patches() {
        let compiled = inject::compiled_layout(&stand_in_layout());
        let edit = all_on().compile().unwrap().layout.unwrap();
        let built = inject::patched_layout(&compiled, &edit, &[], "test").unwrap();
        let text = inject::layout_text(&built).unwrap();
        assert!(inject::extends(
            &text,
            &inject::layout_text(&compiled).unwrap()
        ));
        assert!(text.contains("DtWideFov"));
    }

    #[test]
    fn stash_round_trips_and_rejects_unsaved_values() {
        assert_eq!(stash_encode(2.15), "12.150");
        assert_eq!(stash_decode("12.150"), Some(2.15));
        assert_eq!(stash_decode(" 12.153 "), Some(2.155));
        assert_eq!(stash_decode("10.010"), Some(0.010));
        assert_eq!(stash_decode("13.5"), Some(WIDE_FOV_MAX));
        assert_eq!(stash_decode("0.075"), None, "the engine default");
        assert_eq!(stash_decode("9.999"), None);
        assert_eq!(stash_decode("10.004"), None);
        assert_eq!(stash_decode("x"), None);
        assert_eq!(wide_fov_text(2.15), "2.15");
        assert_eq!(wide_fov_text(2.155), "2.155");
        assert_eq!(wide_fov_text(3.0), "3");
    }

    fn fake(stash: Option<&str>) -> (tempfile::TempDir, GamePaths, BackupStore) {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("steamapps/common/Deadlock");
        let citadel = root.join("game/citadel");
        std::fs::create_dir_all(citadel.join("cfg")).unwrap();
        std::fs::copy(
            concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/tests/fixtures/gameinfo_live_2026-09-29.gi"
            ),
            citadel.join("gameinfo.gi"),
        )
        .unwrap();
        if let Some(stash) = stash {
            std::fs::write(
                citadel.join("cfg/user_convars_0_slot0.vcfg"),
                format!(
                    "\"config\"\n{{\n\t\"convars\"\n\t{{\n\t\t\"citadel_camera_hero_fov\"\t\t\"75\"\n\t\t\"{STASH_CONVAR}\"\t\t\"{stash}\"\n\t}}\n}}\n"
                ),
            )
            .unwrap();
        }
        let paths = crate::locate::from_game_root(&root).unwrap();
        let store = BackupStore::open(dir.path().join("data")).unwrap();
        (dir, paths, store)
    }

    fn wide_fov_in(paths: &GamePaths) -> Option<String> {
        gi::effective_values(&std::fs::read_to_string(&paths.gameinfo).unwrap())
            .unwrap()
            .remove(WIDE_FOV_CONVAR)
    }

    #[test]
    fn sync_writes_the_saved_value_once_and_keeps_later_edits() {
        let (_dir, paths, store) = fake(Some("12.150"));
        let original = std::fs::read(&paths.gameinfo).unwrap();
        let sync = sync_wide_fov(&paths, &store).unwrap().unwrap();
        assert_eq!(
            sync,
            WideFovSync {
                ratio: 2.15,
                gameinfo_changed: true
            }
        );
        assert_eq!(wide_fov_in(&paths).as_deref(), Some("2.15"));
        assert!(store.original(FileKind::GameInfo).is_some());
        assert_eq!(store.list(FileKind::GameInfo).unwrap().len(), 1);
        let written = std::fs::read_to_string(&paths.gameinfo).unwrap();
        assert_eq!(
            gi::detect_eol(&written),
            gi::detect_eol(std::str::from_utf8(&original).unwrap())
        );
        assert_ne!(written.as_bytes(), original);

        assert_eq!(
            sync_wide_fov(&paths, &store).unwrap(),
            None,
            "same stash: nothing to do"
        );
        let edited = gi::apply_overrides(
            &std::fs::read_to_string(&paths.gameinfo).unwrap(),
            &BTreeMap::from([(WIDE_FOV_CONVAR.to_string(), Override::Set("2.4".into()))]),
        )
        .unwrap();
        std::fs::write(&paths.gameinfo, edited.text).unwrap();
        assert_eq!(sync_wide_fov(&paths, &store).unwrap(), None);
        assert_eq!(
            wide_fov_in(&paths).as_deref(),
            Some("2.4"),
            "a DeadTune edit stands"
        );

        std::fs::write(
            paths.cfg_dir.join("user_convars_0_slot0.vcfg"),
            format!("\"config\" {{ \"convars\" {{ \"{STASH_CONVAR}\" \"12.400\" }} }}"),
        )
        .unwrap();
        let sync = sync_wide_fov(&paths, &store).unwrap().unwrap();
        assert!(
            !sync.gameinfo_changed,
            "already there: no rewrite, no backup"
        );
        assert_eq!(store.list(FileKind::GameInfo).unwrap().len(), 1);
    }

    #[test]
    fn sync_ignores_a_missing_or_unsaved_stash() {
        let (_dir, paths, store) = fake(None);
        assert_eq!(sync_wide_fov(&paths, &store).unwrap(), None);
        let (_dir, paths, store) = fake(Some("0.075"));
        assert_eq!(sync_wide_fov(&paths, &store).unwrap(), None);
        assert_eq!(wide_fov_in(&paths), None);
        assert!(!store.root.join(SYNC_RECORD_FILE).exists());
    }
}
