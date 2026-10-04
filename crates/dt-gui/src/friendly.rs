//! Plain-language copy for the simple view: setting labels, named levels, groups, goals,
//! preset blurbs and human errors.

use dt_core::catalog::{Catalog, Impact, Kind};
use dt_core::preset::PresetId;

/// Friendly labels for the curated high/medium-impact convars.
const LABELS: &[(&str, &str)] = &[
    ("fps_max", "FPS limit"),
    ("r_citadel_upscaling", "Upscaling"),
    ("r_citadel_antialiasing", "Anti-aliasing"),
    ("r_texture_stream_mip_bias", "Texture sharpness"),
    ("r_texture_lod_scale", "Texture size"),
    ("r_shadows", "Shadows"),
    ("r_citadel_shadow_quality", "Shadow quality"),
    ("cl_globallight_shadow_mode", "Sun shadows"),
    ("csm_max_num_cascades_override", "Sun shadow detail"),
    ("csm_max_shadow_dist_override", "Sun shadow reach"),
    ("csm_max_visible_dist", "Sun shadow distance"),
    ("csm_sst_max_visible_dist", "Static shadow distance"),
    ("lb_enable_shadow_casting", "Light shadows"),
    ("lb_dynamic_shadow_resolution", "Adaptive shadow resolution"),
    ("lb_shadow_texture_width_override", "Shadow map width"),
    ("lb_shadow_texture_height_override", "Shadow map height"),
    ("r_citadel_distancefield_shadows", "Soft shadows"),
    ("sc_disable_spotlight_shadows", "Skip spotlight shadows"),
    ("lb_enable_dynamic_lights", "Ability lighting"),
    ("lb_enable_envmaps", "Reflections"),
    ("lb_max_visible_barn_lights_override", "Spotlight limit"),
    ("lb_max_visible_envmaps_override", "Reflection limit"),
    ("r_ssao", "Ambient occlusion"),
    ("r_citadel_ssao_quality", "Ambient occlusion quality"),
    ("r_effects_bloom", "Glow on effects"),
    ("r_enable_volume_fog", "Volumetric fog"),
    ("r_citadel_fog_quality", "Fog quality"),
    ("cl_particle_max_count", "Particle limit"),
    ("r_particle_max_detail_level", "Effect detail"),
    ("r_particle_max_draw_distance", "Effect draw distance"),
    ("cl_particle_fallback_base", "Cheaper effects under load"),
    (
        "cl_particle_fallback_multiplier",
        "Cheaper effects strength",
    ),
    ("r_threaded_particles", "Effects on extra CPU threads"),
    ("r_farz", "View distance"),
    ("r_propsmaxdist", "Prop draw distance"),
    ("r_size_cull_threshold", "Hide tiny objects"),
    ("sc_fade_distance_scale_override", "Fade-in distance"),
    ("sc_screen_size_lod_scale_override", "Model detail"),
    ("sc_instanced_mesh_lod_bias", "Foliage detail"),
    ("r_grass_quality", "Grass"),
    ("sc_clutter_enable", "Small clutter"),
    ("panorama_max_fps", "Menu FPS limit"),
    ("panorama_disable_blur", "Skip menu blur"),
    ("r_citadel_enable_pano_world_blur", "Blur behind the shop"),
    ("thread_pool_option", "CPU thread mode"),
];

/// Short help per setting, written for players; falls back to the catalog notes.
const HELP: &[(&str, &str)] = &[
    (
        "fps_max",
        "Caps your frame rate. Set it near your monitor's refresh rate to save heat and battery.",
    ),
    (
        "r_citadel_upscaling",
        "Renders at a lower resolution and upscales with DLSS or FSR. Big FPS win on most GPUs.",
    ),
    (
        "r_citadel_antialiasing",
        "Smooths jagged edges. Off is a little faster and a little rougher.",
    ),
    (
        "r_texture_stream_mip_bias",
        "Full keeps textures crisp. Lower levels blur them and use less video memory.",
    ),
    (
        "r_texture_lod_scale",
        "Loads smaller textures. Half is hard to notice in a fight; Minimum looks muddy.",
    ),
    (
        "r_shadows",
        "Turning shadows off is one of the biggest FPS wins, but the map looks flat.",
    ),
    (
        "r_citadel_shadow_quality",
        "How sharp and far shadows are drawn.",
    ),
    (
        "cl_globallight_shadow_mode",
        "Shadows cast by the sun. Off removes most shadow cost.",
    ),
    (
        "lb_enable_dynamic_lights",
        "Lights from abilities and effects. Off also leaves hero portraits uncoloured in the shop.",
    ),
    (
        "lb_enable_envmaps",
        "Reflections on wet and shiny surfaces. Off can make characters render black.",
    ),
    (
        "r_ssao",
        "Soft contact shadows in corners. Off is a solid FPS win.",
    ),
    (
        "cl_particle_max_count",
        "Caps how many particles are alive. Too low makes fights hard to read.",
    ),
    (
        "r_particle_max_detail_level",
        "How detailed ability and hit effects are.",
    ),
    (
        "r_farz",
        "How far the world is drawn. Shorter makes distant buildings pop in.",
    ),
    (
        "sc_screen_size_lod_scale_override",
        "How soon models switch to cheaper versions. Lower makes heroes look worse.",
    ),
    ("r_grass_quality", "Grass density."),
    (
        "sc_clutter_enable",
        "Small decorative props. Off also makes enemies easier to spot.",
    ),
    ("panorama_max_fps", "Frame cap for menus and the shop."),
    (
        "r_citadel_enable_pano_world_blur",
        "Blurs the world behind the shop. Off makes the shop much faster.",
    ),
    (
        "thread_pool_option",
        "How work is spread over CPU cores. Presets disagree, so try both ways.",
    ),
    (
        "csm_max_num_cascades_override",
        "How much detail sun shadows keep at range. Auto lets the game decide.",
    ),
    (
        "csm_max_shadow_dist_override",
        "How far from you the sun still casts shadows. Auto lets the game decide.",
    ),
    (
        "csm_max_visible_dist",
        "Beyond this distance sun shadows are not drawn at all.",
    ),
    (
        "csm_sst_max_visible_dist",
        "How far away static shadows (buildings, props) are still drawn.",
    ),
    (
        "lb_enable_shadow_casting",
        "Lets lamps and ability lights cast shadows. Off is a solid FPS win.",
    ),
    (
        "lb_dynamic_shadow_resolution",
        "Lets the game lower shadow sharpness when it gets busy.",
    ),
    (
        "lb_shadow_texture_width_override",
        "Shadow sharpness. Auto picks a sensible size; tiny values look blocky.",
    ),
    (
        "lb_shadow_texture_height_override",
        "Shadow sharpness. Auto picks a sensible size; tiny values look blocky.",
    ),
    (
        "r_citadel_distancefield_shadows",
        "Soft, blurry-edged shadows. Off makes edges harder and saves GPU time.",
    ),
    (
        "sc_disable_spotlight_shadows",
        "Skips shadows from spotlights. On is a small FPS win.",
    ),
    (
        "lb_max_visible_barn_lights_override",
        "How many spotlights can be on screen at once.",
    ),
    (
        "lb_max_visible_envmaps_override",
        "How many reflective surfaces can be on screen at once.",
    ),
    (
        "r_citadel_ssao_quality",
        "How carefully contact shadows are drawn.",
    ),
    (
        "r_effects_bloom",
        "Glow around bright abilities and effects.",
    ),
    (
        "r_enable_volume_fog",
        "Light rays and hazy air. Off is a small FPS win.",
    ),
    ("r_citadel_fog_quality", "How carefully fog is drawn."),
    (
        "r_particle_max_draw_distance",
        "Effects further away than this are skipped.",
    ),
    (
        "cl_particle_fallback_base",
        "How readily effects switch to cheaper versions when a fight gets busy.",
    ),
    (
        "cl_particle_fallback_multiplier",
        "How much cheaper effects get when a fight gets busy.",
    ),
    (
        "r_threaded_particles",
        "Simulates effects on spare CPU cores. Keep on unless you see stutter.",
    ),
    (
        "r_propsmaxdist",
        "How far away boxes and props are drawn. Too short hides breakables.",
    ),
    (
        "r_size_cull_threshold",
        "Hides objects smaller than this share of the screen. Higher also hides health bars sooner.",
    ),
    (
        "sc_fade_distance_scale_override",
        "How far away objects fade in. Auto lets the game decide.",
    ),
    (
        "sc_instanced_mesh_lod_bias",
        "Detail of grass, foliage and clutter in the distance.",
    ),
    (
        "panorama_disable_blur",
        "Skips the blur on menus. On is a small FPS win in the shop.",
    ),
];

/// Groups shown in the simple view, in display order. Each name is in `LABELS`.
pub const GROUPS: &[(&str, &[&str])] = &[
    ("Frame rate", &["fps_max", "panorama_max_fps"]),
    (
        "Image quality",
        &[
            "r_citadel_upscaling",
            "r_citadel_antialiasing",
            "r_texture_stream_mip_bias",
            "r_texture_lod_scale",
        ],
    ),
    (
        "Shadows",
        &[
            "r_shadows",
            "r_citadel_shadow_quality",
            "cl_globallight_shadow_mode",
            "csm_max_num_cascades_override",
            "csm_max_shadow_dist_override",
            "csm_max_visible_dist",
            "csm_sst_max_visible_dist",
            "lb_enable_shadow_casting",
            "lb_dynamic_shadow_resolution",
            "lb_shadow_texture_width_override",
            "lb_shadow_texture_height_override",
            "r_citadel_distancefield_shadows",
            "sc_disable_spotlight_shadows",
        ],
    ),
    (
        "Lighting & effects",
        &[
            "lb_enable_dynamic_lights",
            "lb_enable_envmaps",
            "r_ssao",
            "r_citadel_ssao_quality",
            "r_effects_bloom",
            "r_enable_volume_fog",
            "r_citadel_fog_quality",
            "lb_max_visible_barn_lights_override",
            "lb_max_visible_envmaps_override",
            "cl_particle_max_count",
            "r_particle_max_detail_level",
            "r_particle_max_draw_distance",
            "cl_particle_fallback_base",
            "cl_particle_fallback_multiplier",
        ],
    ),
    (
        "World detail",
        &[
            "r_farz",
            "r_propsmaxdist",
            "sc_screen_size_lod_scale_override",
            "sc_instanced_mesh_lod_bias",
            "r_grass_quality",
            "sc_clutter_enable",
            "r_size_cull_threshold",
            "sc_fade_distance_scale_override",
        ],
    ),
    (
        "Menus & shop",
        &["panorama_disable_blur", "r_citadel_enable_pano_world_blur"],
    ),
    ("CPU", &["thread_pool_option", "r_threaded_particles"]),
];

/// How many groups start expanded.
pub const OPEN_GROUPS: usize = 3;

/// The hero choices, from best looks to most FPS. Each maps to a community preset.
pub const GOALS: &[(PresetId, &str, &str)] = &[
    (
        PresetId::Vanilla,
        "Best looks",
        "The game as Valve ships it",
    ),
    (PresetId::Sqooky, "Balanced", "More FPS, small visual cost"),
    (
        PresetId::KaizMinspec,
        "More FPS",
        "Big FPS gain, looks worse",
    ),
    (
        PresetId::OptilockPotato,
        "Max FPS",
        "Everything off, for old PCs",
    ),
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Unit {
    Fps,
    Count,
    /// Source units, shown as metres (1 unit = 1 inch).
    Metres,
    Pixels,
    /// A 0..1 scale shown as a percentage.
    Scale,
    /// Already a percentage.
    Percent,
}

/// How the simple view edits a setting. Raw numbers never reach the screen.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Control {
    Toggle,
    /// `(raw value, label)` from lowest to highest quality. Labels may repeat for aliases;
    /// the first entry with a label is what clicking it sets.
    Levels(&'static [(&'static str, &'static str)]),
    Slider {
        unit: Unit,
        special: Specials,
    },
}

const LEVELS: &[(&str, &[(&str, &str)])] = &[
    ("r_citadel_upscaling", &[("0", "Off"), ("4", "DLSS / FSR")]),
    ("r_citadel_antialiasing", &[("0", "Off"), ("1", "On")]),
    (
        "r_texture_stream_mip_bias",
        &[("8", "Lowest"), ("3", "Reduced"), ("0", "Full")],
    ),
    (
        "r_texture_lod_scale",
        &[
            ("8", "Minimum"),
            ("4", "Quarter"),
            ("2", "Half"),
            ("1", "Full"),
        ],
    ),
    (
        "r_citadel_shadow_quality",
        &[("0", "Low"), ("1", "Medium"), ("2", "High"), ("3", "Ultra")],
    ),
    (
        "cl_globallight_shadow_mode",
        &[("0", "Off"), ("1", "Basic"), ("2", "Full")],
    ),
    (
        "csm_max_num_cascades_override",
        &[
            ("0", "Lowest"),
            ("1", "Low"),
            ("2", "Medium"),
            ("3", "High"),
            ("4", "Max"),
            ("-1", "Auto"),
        ],
    ),
    (
        "r_citadel_ssao_quality",
        &[("0", "Low"), ("1", "Medium"), ("2", "High"), ("3", "Ultra")],
    ),
    (
        "r_citadel_fog_quality",
        &[("0", "Low"), ("1", "Medium"), ("2", "High")],
    ),
    (
        "r_particle_max_detail_level",
        &[("0", "Low"), ("1", "Medium"), ("2", "High"), ("3", "Ultra")],
    ),
    (
        "cl_particle_fallback_base",
        &[
            ("0", "Off"),
            ("1", "Light"),
            ("5", "Medium"),
            ("10", "Strong"),
        ],
    ),
    (
        "cl_particle_fallback_multiplier",
        &[
            ("0", "Off"),
            ("2", "Light"),
            ("5", "Medium"),
            ("10", "Strong"),
        ],
    ),
    (
        "sc_instanced_mesh_lod_bias",
        &[("15", "Low"), ("3", "Medium"), ("1.25", "High")],
    ),
    (
        "r_grass_quality",
        &[
            ("0", "Off"),
            ("1", "Low"),
            ("2", "Medium"),
            ("3", "High"),
            ("4", "Ultra"),
        ],
    ),
    (
        "thread_pool_option",
        &[
            ("-1", "Auto"),
            ("0", "Mode 0"),
            ("1", "Mode 1"),
            ("2", "Mode 2"),
        ],
    ),
];

/// Raw values with a name instead of a number, such as `0` for "Unlimited".
pub type Specials = &'static [(f64, &'static str)];

const UNLIMITED_AT_ZERO: Specials = &[(0.0, "Unlimited")];
const UNLIMITED_AT_MINUS_ONE: Specials = &[(-1.0, "Unlimited")];
const AUTO_AT_MINUS_ONE: Specials = &[(-1.0, "Auto")];
const OFF_AT_ZERO: Specials = &[(0.0, "Off")];
const AUTO_OR_OFF: Specials = &[(-1.0, "Auto"), (0.0, "Off")];

const SLIDERS: &[(&str, Unit, Specials)] = &[
    ("fps_max", Unit::Fps, UNLIMITED_AT_ZERO),
    ("panorama_max_fps", Unit::Fps, &[]),
    ("cl_particle_max_count", Unit::Count, UNLIMITED_AT_ZERO),
    (
        "lb_max_visible_barn_lights_override",
        Unit::Count,
        UNLIMITED_AT_MINUS_ONE,
    ),
    (
        "lb_max_visible_envmaps_override",
        Unit::Count,
        UNLIMITED_AT_MINUS_ONE,
    ),
    (
        "lb_shadow_texture_width_override",
        Unit::Pixels,
        AUTO_AT_MINUS_ONE,
    ),
    (
        "lb_shadow_texture_height_override",
        Unit::Pixels,
        AUTO_AT_MINUS_ONE,
    ),
    ("csm_max_shadow_dist_override", Unit::Metres, AUTO_OR_OFF),
    ("csm_max_visible_dist", Unit::Metres, OFF_AT_ZERO),
    ("csm_sst_max_visible_dist", Unit::Metres, OFF_AT_ZERO),
    ("r_farz", Unit::Metres, AUTO_AT_MINUS_ONE),
    ("r_propsmaxdist", Unit::Metres, &[]),
    ("r_particle_max_draw_distance", Unit::Metres, &[]),
    (
        "sc_screen_size_lod_scale_override",
        Unit::Scale,
        AUTO_AT_MINUS_ONE,
    ),
    (
        "sc_fade_distance_scale_override",
        Unit::Percent,
        AUTO_AT_MINUS_ONE,
    ),
    ("r_size_cull_threshold", Unit::Percent, &[]),
];

pub fn label(name: &str) -> Option<&'static str> {
    LABELS.iter().find(|(n, _)| *n == name).map(|(_, l)| *l)
}

pub fn help<'a>(name: &str, catalog: &'a Catalog) -> &'a str {
    HELP.iter()
        .find(|(n, _)| *n == name)
        .map(|(_, h)| *h)
        .or_else(|| catalog.get(name).map(|e| e.notes.as_str()))
        .unwrap_or("")
}

/// The control for a curated setting; `None` for anything the simple view must not show raw.
pub fn control(name: &str, kind: &Kind) -> Option<Control> {
    if let Some((_, levels)) = LEVELS.iter().find(|(n, _)| *n == name) {
        return Some(Control::Levels(levels));
    }
    if let Some((_, unit, special)) = SLIDERS.iter().find(|(n, _, _)| *n == name) {
        return Some(Control::Slider {
            unit: *unit,
            special,
        });
    }
    matches!(kind, Kind::Bool).then_some(Control::Toggle)
}

/// Whether two raw texts name the same number (`1.25` vs `1.250`, `1e+06` vs `1000000`).
fn same_number(a: &str, b: &str) -> bool {
    match (a.trim().parse::<f64>(), b.trim().parse::<f64>()) {
        (Ok(x), Ok(y)) => (x - y).abs() < 1e-9,
        _ => a.trim() == b.trim(),
    }
}

/// The level label for a raw value, if the table names it.
pub fn level_label(levels: &[(&str, &'static str)], raw: &str) -> Option<&'static str> {
    levels
        .iter()
        .find(|(v, _)| same_number(v, raw))
        .map(|(_, l)| *l)
}

pub fn slider_text(unit: Unit, special: &[(f64, &str)], value: f64) -> String {
    if let Some((_, name)) = special.iter().find(|(v, _)| (v - value).abs() < 1e-9) {
        return (*name).to_string();
    }
    match unit {
        Unit::Fps => format!("{} FPS", value.round()),
        Unit::Count => format!("{}", value.round()),
        Unit::Metres => {
            let metres = value * 0.0254;
            if metres >= 1000.0 {
                format!("{:.1} km", metres / 1000.0)
            } else {
                format!("{} m", metres.round())
            }
        }
        Unit::Pixels => format!("{} px", value.round()),
        Unit::Scale => format!("{}%", (value * 100.0).round()),
        Unit::Percent => format!("{value}%"),
    }
}

/// Typed slider text back to a raw value: a special name, or a number in the unit shown.
pub fn slider_parse(unit: Unit, special: &[(f64, &str)], text: &str) -> Option<f64> {
    let text = text.trim();
    if let Some((v, _)) = special
        .iter()
        .find(|(_, name)| name.eq_ignore_ascii_case(text))
    {
        return Some(*v);
    }
    let digits: String = text
        .chars()
        .take_while(|c| c.is_ascii_digit() || matches!(c, '.' | '-' | '+' | 'e' | 'E'))
        .collect();
    let number: f64 = digits.parse().ok()?;
    Some(match unit {
        Unit::Metres if text.ends_with("km") => number * 1000.0 / 0.0254,
        Unit::Metres => number / 0.0254,
        Unit::Scale => number / 100.0,
        Unit::Fps | Unit::Count | Unit::Pixels | Unit::Percent => number,
    })
}

/// Curated settings for the simple view, in display order: labelled, impactful, not denylisted.
pub fn simple_rows(catalog: &Catalog) -> Vec<&'static str> {
    GROUPS
        .iter()
        .flat_map(|(_, names)| names.iter().copied())
        .filter(|n| {
            catalog
                .get(n)
                .is_some_and(|e| !e.denylist && matches!(e.impact, Impact::High | Impact::Medium))
        })
        .collect()
}

/// One line per preset for the welcome cards; `None` hides it from the simple picker.
pub fn preset_blurb(id: PresetId) -> Option<&'static str> {
    match id {
        PresetId::Vanilla => Some("The game's own settings. Best looking, no tweaks."),
        PresetId::Sqooky => Some("Balanced: more FPS for a small visual cost."),
        PresetId::OptilockRecommended => Some("Balanced, and also tunes your video settings."),
        PresetId::KaizMinspec => Some("For laptops and weak PCs: big FPS gain, looks worse."),
        PresetId::BootMaxfps => Some("Max FPS, looks worse."),
        PresetId::OptilockPotato => Some("Potato mode: max FPS for very old hardware."),
        PresetId::KaizExtremelow => Some("Absolute minimum: max FPS, looks rough."),
        PresetId::SqookyTest => None,
    }
}

/// One human sentence plus what to do, from a raw error string.
pub fn human_error(raw: &str) -> String {
    if raw == crate::state::STALE {
        return raw.into();
    }
    let lower = raw.to_lowercase();
    if lower.contains("braces") || lower.contains("no convars block") {
        "The game's config file looks damaged. Use \"Restore original game files\", then try again."
            .into()
    } else if lower.contains("permission")
        || lower.contains("access is denied")
        || lower.contains("denied (os error")
    {
        "Windows blocked the change. Close Deadlock and try again; if it keeps happening, run Steam's \"Verify integrity of game files\".".into()
    } else if lower.contains("not found")
        || lower.contains("no such file")
        || lower.contains("cannot find")
    {
        "A game file is missing. Open \"Check setup\" to see which one.".into()
    } else if lower.contains("connection refused") || lower.contains("timed out") {
        "The game didn't answer. Use the key bind instead, or press Apply and restart the game."
            .into()
    } else {
        "Something went wrong. Open \"Check setup\"; the details are in the tooltip.".into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_label_names_a_curated_catalog_entry() {
        let catalog = Catalog::embedded();
        let rows = simple_rows(catalog);
        assert_eq!(
            rows.len(),
            LABELS.len(),
            "a labelled name is missing or not high/medium impact"
        );
        assert_eq!(rows[0], "fps_max");
    }

    #[test]
    fn every_high_or_medium_entry_has_a_label() {
        let missing: Vec<&String> = Catalog::embedded()
            .entries
            .iter()
            .filter(|(_, e)| !e.denylist && matches!(e.impact, Impact::High | Impact::Medium))
            .map(|(n, _)| n)
            .filter(|n| label(n).is_none())
            .collect();
        assert!(missing.is_empty(), "add friendly labels for {missing:?}");
    }

    #[test]
    fn every_label_is_in_exactly_one_group() {
        for (name, _) in LABELS {
            let count = GROUPS
                .iter()
                .flat_map(|(_, names)| names.iter())
                .filter(|n| *n == name)
                .count();
            assert_eq!(count, 1, "{name} appears in {count} groups");
        }
        for name in GROUPS.iter().flat_map(|(_, names)| names.iter()) {
            assert!(label(name).is_some(), "{name} is grouped but has no label");
        }
    }

    #[test]
    fn every_row_has_a_control_without_raw_numbers() {
        let catalog = Catalog::embedded();
        for name in simple_rows(catalog) {
            let entry = catalog.get(name).unwrap();
            let control = control(name, &entry.kind)
                .unwrap_or_else(|| panic!("{name} would show a raw {:?}", entry.kind));
            match control {
                Control::Toggle => assert_eq!(entry.kind, Kind::Bool, "{name}"),
                Control::Levels(levels) => {
                    let [lo, hi] = entry.range.unwrap_or_else(|| panic!("{name} has no range"));
                    for (raw, _) in levels {
                        let v: f64 = raw.parse().unwrap();
                        assert!((lo..=hi).contains(&v), "{name} level {raw} outside range");
                    }
                    let mut named: Vec<&str> = entry.presets.values().map(String::as_str).collect();
                    named.extend(entry.default.as_deref());
                    for raw in named {
                        if raw.starts_with("//") || raw.is_empty() {
                            continue;
                        }
                        assert!(
                            level_label(levels, raw).is_some(),
                            "{name}: preset value {raw} has no named level"
                        );
                    }
                }
                Control::Slider { .. } => {
                    assert!(
                        matches!(entry.kind, Kind::Int | Kind::Float) && entry.range.is_some(),
                        "{name} is not a ranged number"
                    );
                }
            }
        }
    }

    #[test]
    fn slider_text_names_units_and_specials() {
        assert_eq!(slider_text(Unit::Fps, UNLIMITED_AT_ZERO, 0.0), "Unlimited");
        assert_eq!(slider_text(Unit::Fps, UNLIMITED_AT_ZERO, 144.0), "144 FPS");
        assert_eq!(slider_text(Unit::Metres, AUTO_AT_MINUS_ONE, -1.0), "Auto");
        assert_eq!(slider_text(Unit::Metres, &[], 8000.0), "203 m");
        assert_eq!(slider_text(Unit::Metres, &[], 1e6), "25.4 km");
        assert_eq!(slider_text(Unit::Scale, &[], 0.8), "80%");
        assert_eq!(slider_text(Unit::Pixels, &[], 2048.0), "2048 px");
        assert_eq!(
            level_label(&[("1.25", "High"), ("4", "Off")], "1.250"),
            Some("High")
        );
        assert_eq!(level_label(&[("4", "Off")], "2"), None);
        assert_eq!(
            slider_parse(Unit::Fps, UNLIMITED_AT_ZERO, "unlimited"),
            Some(0.0)
        );
        assert_eq!(slider_parse(Unit::Fps, &[], "144 FPS"), Some(144.0));
        assert_eq!(
            slider_parse(Unit::Metres, &[], "2 km").map(f64::round),
            Some(78740.0)
        );
        assert_eq!(slider_parse(Unit::Scale, &[], "80%"), Some(0.8));
        assert_eq!(slider_parse(Unit::Count, &[], "lots"), None);
    }

    #[test]
    fn goals_are_blurbed_presets_ordered_by_fps() {
        for (id, _, _) in GOALS {
            assert!(preset_blurb(*id).is_some(), "{id:?}");
        }
        assert_eq!(GOALS.first().unwrap().0, PresetId::Vanilla);
        assert_eq!(GOALS.last().unwrap().0, PresetId::OptilockPotato);
    }

    #[test]
    fn simple_copy_avoids_jargon() {
        let blurbs = dt_core::preset::all()
            .iter()
            .filter_map(|p| preset_blurb(p.id));
        let errors = [
            "unbalanced braces near line 3",
            "Permission denied (os error 13)",
            "x",
        ]
        .map(human_error);
        let levels = LEVELS
            .iter()
            .flat_map(|(_, l)| l.iter().map(|(_, label)| label.to_string()));
        for text in LABELS
            .iter()
            .map(|(_, l)| l.to_string())
            .chain(HELP.iter().map(|(_, h)| h.to_string()))
            .chain(GOALS.iter().map(|(_, g, s)| format!("{g} {s}")))
            .chain(levels)
            .chain(blurbs.map(str::to_string))
            .chain(errors)
        {
            let lower = text.to_lowercase();
            for word in ["convar", "gameinfo", "devonly", "cvar"] {
                assert!(!lower.contains(word), "{text:?} says {word}");
            }
        }
    }

    #[test]
    fn errors_map_to_actions() {
        assert!(
            human_error("gameinfo.gi: unbalanced braces near line 3").contains("Restore original")
        );
        assert!(human_error("io: Access is denied. (os error 5)").contains("Close Deadlock"));
    }
}
