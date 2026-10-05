//! Plain-language copy for the simple view: what each curated setting is called, how it is
//! shown (named levels, toggles, or sliders with units), where it lives, preset blurbs, errors.

use dt_core::catalog::ApplyClass;
use dt_core::preset::PresetId;

use crate::state::{Section, parse_bool};

/// Source 2 world units are inches.
const METRES_PER_UNIT: f64 = 0.0254;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Unit {
    Fps,
    /// World units, shown in metres or kilometres.
    Distance,
    Count(&'static str),
    PercentOfScreen,
    Multiplier,
    OutOfTen,
    Scale,
    Degrees,
    /// A forced aspect ratio, shown and slid as the rough field of view it gives.
    WideView,
}

/// How wide the view looks at a forced aspect ratio, as the OptiLock, Boot and Kaiz preset
/// notes estimate it. These degrees are a community scale, not the game's own FOV number:
/// a 16:9 screen on Automatic already sits near 80 here.
const WIDE_VIEW: &[(f64, f64)] = &[
    (1.33, 70.0),
    (1.56, 75.0),
    (1.75, 80.0),
    (2.0, 85.0),
    (2.15, 90.0),
    (2.49, 100.0),
    (3.0, 110.0),
    (3.5, 120.0),
];
/// The Wide view rail starts here; everything left of `WIDE_RAIL_MIN` is Automatic.
const WIDE_RAIL_AUTO: f64 = 85.0;
const WIDE_RAIL_MIN: f64 = 90.0;

fn interpolate(table: impl Iterator<Item = (f64, f64)> + Clone, x: f64) -> f64 {
    let (first, last) = (table.clone().next().unwrap(), table.clone().last().unwrap());
    if x <= first.0 {
        return first.1;
    }
    let mut prev = first;
    for point in table {
        if x <= point.0 {
            return prev.1 + (x - prev.0) / (point.0 - prev.0) * (point.1 - prev.1);
        }
        prev = point;
    }
    last.1
}

pub fn ratio_to_degrees(ratio: f64) -> f64 {
    interpolate(WIDE_VIEW.iter().copied(), ratio)
}

/// Rounded to two decimals so a ratio survives the trip through degrees unchanged.
pub fn degrees_to_ratio(degrees: f64) -> f64 {
    let ratio = interpolate(WIDE_VIEW.iter().map(|(r, d)| (*d, *r)), degrees);
    (ratio * 100.0).round() / 100.0
}

/// Where a raw value sits on its slider. Wide view slides in degrees with Automatic (0) as
/// the leftmost stop; every other unit slides on the raw value.
pub fn to_rail(unit: Unit, raw: f64) -> f64 {
    match unit {
        Unit::WideView if raw <= 0.0 => WIDE_RAIL_AUTO,
        Unit::WideView => ratio_to_degrees(raw).max(WIDE_RAIL_MIN),
        _ => raw,
    }
}

pub fn from_rail(unit: Unit, rail: f64) -> f64 {
    match unit {
        Unit::WideView if rail < WIDE_RAIL_MIN => 0.0,
        Unit::WideView => degrees_to_ratio(rail),
        _ => rail,
    }
}

/// The slider's snap grid, in rail units.
pub fn rail_step(unit: Unit, step: f64) -> f64 {
    match unit {
        Unit::WideView => 1.0,
        _ => step,
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Control {
    /// `invert` shows "on" when the convar is false (e.g. `panorama_disable_blur` as "Menu blur").
    Toggle { invert: bool },
    /// Named values, picked as a segmented control.
    Levels(&'static [(f64, &'static str)]),
    /// A continuous value; `special` names sentinel values such as 0 = Unlimited.
    Slider {
        unit: Unit,
        special: &'static [(f64, &'static str)],
    },
}

pub struct Row {
    pub name: &'static str,
    pub label: &'static str,
    pub help: &'static str,
    pub control: Control,
}

pub struct Group {
    pub title: &'static str,
    pub names: &'static [&'static str],
}

const TOGGLE: Control = Control::Toggle { invert: false };
const INVERTED: Control = Control::Toggle { invert: true };
const OFF_ON: &[(f64, &str)] = &[(0.0, "Off"), (1.0, "On")];
const QUALITY4: &[(f64, &str)] = &[(0.0, "Low"), (1.0, "Medium"), (2.0, "High"), (3.0, "Ultra")];
const QUALITY3: &[(f64, &str)] = &[(0.0, "Low"), (1.0, "Medium"), (2.0, "High")];
const SHADOW_MAP: &[(f64, &str)] = &[
    (-1.0, "Auto"),
    (1.0, "Tiny"),
    (512.0, "Low"),
    (1024.0, "Medium"),
    (2048.0, "High"),
    (4096.0, "Ultra"),
];

const fn slider(unit: Unit, special: &'static [(f64, &'static str)]) -> Control {
    Control::Slider { unit, special }
}

const fn entry(
    name: &'static str,
    label: &'static str,
    help: &'static str,
    control: Control,
) -> Row {
    Row {
        name,
        label,
        help,
        control,
    }
}

/// Every curated high/medium-impact convar, once.
pub const ROWS: &[Row] = &[
    entry(
        "fps_max",
        "FPS limit",
        "Caps your frame rate. Set it near your monitor's refresh rate for less heat and fan noise.",
        slider(Unit::Fps, &[(0.0, "Unlimited")]),
    ),
    entry(
        "r_citadel_upscaling",
        "Upscaling",
        "Renders at a lower resolution and sharpens it back up. Free FPS on most PCs.",
        Control::Levels(&[(0.0, "Off"), (4.0, "DLSS / FSR")]),
    ),
    entry(
        "r_citadel_antialiasing",
        "Anti-aliasing",
        "Smooths jagged edges. Off is a little faster and a little sharper, but shimmers.",
        Control::Levels(OFF_ON),
    ),
    entry(
        "r_texture_stream_mip_bias",
        "Texture sharpness",
        "Lower looks blurrier but saves video memory. Helps on cards with 4 GB or less.",
        Control::Levels(&[
            (0.0, "High"),
            (3.0, "Medium"),
            (5.0, "Low"),
            (8.0, "Lowest"),
        ]),
    ),
    entry(
        "r_texture_lod_scale",
        "Texture size",
        "Loads smaller textures. Saves video memory; surfaces look softer.",
        Control::Levels(&[
            (1.0, "Full"),
            (2.0, "Half"),
            (4.0, "Quarter"),
            (8.0, "Lowest"),
        ]),
    ),
    entry(
        "r_shadows",
        "Shadows",
        "Turns all shadows on or off. Off is a big FPS gain but the world looks flat.",
        TOGGLE,
    ),
    entry(
        "r_citadel_shadow_quality",
        "Shadow quality",
        "Overall shadow detail. Most FPS presets use Low.",
        Control::Levels(QUALITY4),
    ),
    entry(
        "cl_globallight_shadow_mode",
        "Sun shadows",
        "Shadows cast by the sun across the map.",
        Control::Levels(&[(0.0, "Off"), (1.0, "Low"), (2.0, "High")]),
    ),
    entry(
        "csm_max_num_cascades_override",
        "Sun shadow detail",
        "How many detail layers sun shadows use. Fewer means blurrier shadows further away.",
        Control::Levels(&[
            (-1.0, "Auto"),
            (0.0, "Off"),
            (1.0, "Low"),
            (2.0, "Medium"),
            (3.0, "High"),
            (4.0, "Max"),
        ]),
    ),
    entry(
        "csm_max_shadow_dist_override",
        "Sun shadow reach",
        "How far from you sun shadows are drawn.",
        slider(Unit::Distance, &[(-1.0, "Auto"), (0.0, "Off")]),
    ),
    entry(
        "csm_max_visible_dist",
        "Sun shadow distance",
        "Past this distance, sun shadows disappear.",
        slider(Unit::Distance, &[(0.0, "Off")]),
    ),
    entry(
        "csm_sst_max_visible_dist",
        "Static shadow distance",
        "How far shadows from buildings and the map itself are drawn.",
        slider(Unit::Distance, &[(0.0, "Off")]),
    ),
    entry(
        "lb_enable_shadow_casting",
        "Light shadows",
        "Lets lamps and ability lights cast shadows.",
        TOGGLE,
    ),
    entry(
        "lb_dynamic_shadow_resolution",
        "Automatic shadow resolution",
        "Lets the game lower shadow detail when things get busy.",
        TOGGLE,
    ),
    entry(
        "lb_shadow_texture_width_override",
        "Shadow map width",
        "Resolution of light shadows. Tiny makes them blocky but is very cheap.",
        Control::Levels(SHADOW_MAP),
    ),
    entry(
        "lb_shadow_texture_height_override",
        "Shadow map height",
        "Pair this with the width above.",
        Control::Levels(SHADOW_MAP),
    ),
    entry(
        "r_citadel_distancefield_shadows",
        "Soft shadows",
        "Softer, more realistic shadow edges. Costs some GPU time.",
        TOGGLE,
    ),
    entry(
        "sc_disable_spotlight_shadows",
        "Spotlight shadows",
        "Shadows from spotlights. Off is a small, safe FPS gain.",
        INVERTED,
    ),
    entry(
        "lb_enable_dynamic_lights",
        "Ability lighting",
        "Light from abilities and effects. Off also leaves hero portraits grey in the shop.",
        TOGGLE,
    ),
    entry(
        "lb_enable_envmaps",
        "Reflections",
        "Keep this on: off makes heroes render black.",
        TOGGLE,
    ),
    entry(
        "lb_max_visible_barn_lights_override",
        "Spotlight limit",
        "How many spotlights can be visible at once.",
        slider(Unit::Count("lights"), &[(-1.0, "No limit")]),
    ),
    entry(
        "lb_max_visible_envmaps_override",
        "Reflection limit",
        "How many reflection sources can be visible at once.",
        slider(Unit::Count("reflections"), &[(-1.0, "No limit")]),
    ),
    entry(
        "r_ssao",
        "Ambient occlusion",
        "Soft contact shadows in corners and under objects. Off is a solid FPS gain.",
        TOGGLE,
    ),
    entry(
        "r_citadel_ssao_quality",
        "Ambient occlusion quality",
        "Only matters when ambient occlusion is on.",
        Control::Levels(QUALITY4),
    ),
    entry(
        "r_effects_bloom",
        "Glow on effects",
        "The bright glow around abilities and particles.",
        TOGGLE,
    ),
    entry(
        "r_enable_volume_fog",
        "Volumetric fog",
        "Light shafts and thick fog. Off is faster and clearer.",
        TOGGLE,
    ),
    entry(
        "r_citadel_fog_quality",
        "Fog quality",
        "Only matters when volumetric fog is on.",
        Control::Levels(QUALITY3),
    ),
    entry(
        "cl_particle_max_count",
        "Particle limit",
        "Caps particles on screen. Too low makes big fights hard to read.",
        slider(Unit::Count("particles"), &[(0.0, "Unlimited")]),
    ),
    entry(
        "r_particle_max_detail_level",
        "Effect detail",
        "Detail of ability and impact effects.",
        Control::Levels(QUALITY4),
    ),
    entry(
        "r_particle_max_draw_distance",
        "Effect draw distance",
        "Past this distance, effects are not drawn.",
        slider(Unit::Distance, &[]),
    ),
    entry(
        "cl_particle_fallback_base",
        "Cheaper effects under load",
        "Swaps in simpler effects when lots is happening.",
        slider(Unit::OutOfTen, &[(0.0, "Off")]),
    ),
    entry(
        "cl_particle_fallback_multiplier",
        "Cheaper effects strength",
        "How eagerly simpler effects are swapped in.",
        slider(Unit::OutOfTen, &[(0.0, "Off")]),
    ),
    entry(
        "r_threaded_particles",
        "Effects on extra CPU threads",
        "Spreads effect work across CPU cores. Usually best left on.",
        TOGGLE,
    ),
    entry(
        "r_farz",
        "View distance",
        "How far you can see. Too low makes buildings and distant players pop in.",
        slider(Unit::Distance, &[(-1.0, "Auto")]),
    ),
    entry(
        "r_propsmaxdist",
        "Prop draw distance",
        "How far boxes and other props are drawn. Too low hides breakables.",
        slider(Unit::Distance, &[]),
    ),
    entry(
        "r_size_cull_threshold",
        "Hide tiny objects",
        "Skips objects smaller than this. High values can hide trooper health bars.",
        slider(Unit::PercentOfScreen, &[(0.0, "Off")]),
    ),
    entry(
        "sc_fade_distance_scale_override",
        "Fade-in distance",
        "How early objects fade in as you approach. Auto (-1) is the game default.",
        slider(Unit::Scale, &[(-1.0, "Auto")]),
    ),
    entry(
        "sc_screen_size_lod_scale_override",
        "Model detail",
        "Lower switches to simpler models sooner. Heroes look worse up close.",
        slider(Unit::Multiplier, &[(-1.0, "Auto")]),
    ),
    entry(
        "sc_instanced_mesh_lod_bias",
        "Foliage detail",
        "Detail of trees, bushes and clutter.",
        Control::Levels(&[(1.25, "High"), (3.0, "Medium"), (15.0, "Low")]),
    ),
    entry(
        "r_grass_quality",
        "Grass",
        "Off also makes it easier to spot enemies.",
        Control::Levels(&[
            (0.0, "Off"),
            (1.0, "Low"),
            (2.0, "Medium"),
            (3.0, "High"),
            (4.0, "Ultra"),
        ]),
    ),
    entry(
        "sc_clutter_enable",
        "Small clutter",
        "Small props on the ground. Off is faster and easier to read.",
        TOGGLE,
    ),
    entry(
        "panorama_max_fps",
        "Menu FPS limit",
        "Frame cap for the shop and menus. Low values free up your GPU.",
        slider(Unit::Fps, &[]),
    ),
    entry(
        "panorama_disable_blur",
        "Menu blur",
        "Blurred backgrounds in menus.",
        INVERTED,
    ),
    entry(
        "r_citadel_enable_pano_world_blur",
        "Blur behind the shop",
        "Off makes opening the shop much faster.",
        TOGGLE,
    ),
    entry(
        "thread_pool_option",
        "CPU thread mode",
        "Presets disagree here. Kaiz uses A, Sqooky uses C. Try each and compare.",
        Control::Levels(&[(-1.0, "Auto"), (0.0, "A"), (1.0, "B"), (2.0, "C")]),
    ),
    entry(
        "citadel_camera_hero_fov",
        "Field of view",
        "The game's own FOV slider: 75° to 90°, and 90° is the widest. Changes instantly with your key bind.",
        slider(Unit::Degrees, &[]),
    ),
    entry(
        "r_aspectratio",
        "Wide view",
        "A wider view than the game's FOV limit allows; things look a little thinner. Degrees are rough community estimates. Takes effect next time you start Deadlock.",
        slider(Unit::WideView, &[(0.0, "Automatic")]),
    ),
];

/// The Overview's camera card. Comfort settings, not FPS ones, so they sit apart from
/// `KEY_SETTINGS`.
pub const CAMERA: &[&str] = &["citadel_camera_hero_fov", "r_aspectratio"];

/// The handful on the Overview, biggest FPS wins first. Five fit under the goal cards at
/// 1280x800 without scrolling.
pub const KEY_SETTINGS: &[&str] = &[
    "fps_max",
    "r_citadel_upscaling",
    "r_citadel_shadow_quality",
    "r_ssao",
    "r_texture_stream_mip_bias",
];

pub fn groups(section: Section) -> &'static [Group] {
    match section {
        Section::Overview
        | Section::Hud
        | Section::Minimap
        | Section::TopBar
        | Section::Health
        | Section::Addons
        | Section::System
        | Section::GameFiles
        | Section::Safety => &[],
        Section::Display => &[
            Group {
                title: "Resolution",
                names: &["r_citadel_upscaling", "r_citadel_antialiasing"],
            },
            Group {
                title: "Textures",
                names: &["r_texture_stream_mip_bias", "r_texture_lod_scale"],
            },
            Group {
                title: "Camera",
                names: CAMERA,
            },
        ],
        Section::Shadows => &[
            Group {
                title: "Main",
                names: &[
                    "r_shadows",
                    "r_citadel_shadow_quality",
                    "r_citadel_distancefield_shadows",
                ],
            },
            Group {
                title: "Sun",
                names: &[
                    "cl_globallight_shadow_mode",
                    "csm_max_num_cascades_override",
                    "csm_max_shadow_dist_override",
                    "csm_max_visible_dist",
                    "csm_sst_max_visible_dist",
                ],
            },
            Group {
                title: "Lights",
                names: &[
                    "lb_enable_shadow_casting",
                    "sc_disable_spotlight_shadows",
                    "lb_dynamic_shadow_resolution",
                    "lb_shadow_texture_width_override",
                    "lb_shadow_texture_height_override",
                ],
            },
        ],
        Section::Effects => &[
            Group {
                title: "Lighting",
                names: &[
                    "r_ssao",
                    "r_citadel_ssao_quality",
                    "lb_enable_dynamic_lights",
                    "lb_enable_envmaps",
                    "lb_max_visible_barn_lights_override",
                    "lb_max_visible_envmaps_override",
                ],
            },
            Group {
                title: "Atmosphere",
                names: &[
                    "r_effects_bloom",
                    "r_enable_volume_fog",
                    "r_citadel_fog_quality",
                ],
            },
            Group {
                title: "Abilities and particles",
                names: &[
                    "r_particle_max_detail_level",
                    "cl_particle_max_count",
                    "r_particle_max_draw_distance",
                    "cl_particle_fallback_base",
                    "cl_particle_fallback_multiplier",
                ],
            },
        ],
        Section::World => &[
            Group {
                title: "Distance",
                names: &[
                    "r_farz",
                    "r_propsmaxdist",
                    "sc_fade_distance_scale_override",
                    "r_size_cull_threshold",
                ],
            },
            Group {
                title: "Detail",
                names: &[
                    "sc_screen_size_lod_scale_override",
                    "sc_instanced_mesh_lod_bias",
                    "r_grass_quality",
                    "sc_clutter_enable",
                ],
            },
        ],
        Section::Performance => &[
            Group {
                title: "Frame rate",
                names: &["fps_max"],
            },
            Group {
                title: "Menus and shop",
                names: &[
                    "panorama_max_fps",
                    "panorama_disable_blur",
                    "r_citadel_enable_pano_world_blur",
                ],
            },
            Group {
                title: "CPU",
                names: &["thread_pool_option", "r_threaded_particles"],
            },
        ],
    }
}

/// Every convar a section shows (the Overview shows the key settings).
pub fn section_names(section: Section) -> Vec<&'static str> {
    if section == Section::Overview {
        return KEY_SETTINGS.to_vec();
    }
    groups(section)
        .iter()
        .flat_map(|g| g.names.iter().copied())
        .collect()
}

pub fn row(name: &str) -> Option<&'static Row> {
    ROWS.iter().find(|r| r.name == name)
}

fn number(value: &str) -> Option<f64> {
    value.trim().parse().ok()
}

/// Which named level `value` is, comparing numerically so "0" and "0.0" agree.
pub fn level_index(levels: &[(f64, &str)], value: &str) -> Option<usize> {
    let v = number(value)?;
    levels.iter().position(|(l, _)| (l - v).abs() < 1e-9)
}

/// Rounds a dragged slider value to `step` counted from zero, keeping the range's lowest value
/// reachable because it is usually a sentinel (-1 = game default). egui's `step_by` counts
/// from the minimum instead, which turns a preset's 7000 into 6999 on a -1..16000 range.
pub fn snap(v: f64, [lo, hi]: [f64; 2], step: f64) -> f64 {
    if step <= 0.0 {
        return v.clamp(lo, hi);
    }
    let grid = ((v / step).round() * step).clamp(lo, hi);
    if (v - lo).abs() < (v - grid).abs() {
        lo
    } else {
        grid
    }
}

pub fn toggle_on(control: Control, value: &str) -> bool {
    let on = parse_bool(value);
    match control {
        Control::Toggle { invert } => on != invert,
        _ => on,
    }
}

fn trim_float(v: f64, decimals: usize) -> String {
    let text = format!("{v:.decimals$}");
    if text.contains('.') {
        text.trim_end_matches('0').trim_end_matches('.').to_string()
    } else {
        text
    }
}

pub fn format_unit(unit: Unit, v: f64) -> String {
    match unit {
        Unit::Fps => format!("{} FPS", v.round() as i64),
        Unit::Distance => {
            let metres = v * METRES_PER_UNIT;
            if metres >= 1000.0 {
                format!("{} km", trim_float(metres / 1000.0, 1))
            } else {
                format!("{} m", metres.round() as i64)
            }
        }
        Unit::Count(noun) => format!("{} {noun}", v.round() as i64),
        Unit::PercentOfScreen => format!("{}% of screen", trim_float(v, 2)),
        Unit::Multiplier => format!("{}x", trim_float(v, 2)),
        Unit::OutOfTen => format!("{} / 10", v.round() as i64),
        Unit::Scale => format!("{}%", v.round() as i64),
        Unit::Degrees => format!("{}°", trim_float(v, 1)),
        Unit::WideView => format!("~{}° ({})", ratio_to_degrees(v).round(), trim_float(v, 2)),
    }
}

/// What a value reads as in the simple view: a level name, On/Off, or a number with its unit.
pub fn display(control: Control, value: &str) -> String {
    match control {
        Control::Toggle { .. } => if toggle_on(control, value) {
            "On"
        } else {
            "Off"
        }
        .into(),
        Control::Levels(levels) => match level_index(levels, value) {
            Some(i) => levels[i].1.into(),
            None => "Custom".into(),
        },
        Control::Slider { unit, special } => match number(value) {
            Some(v) => special
                .iter()
                .find(|(s, _)| (s - v).abs() < 1e-9)
                .map(|(_, name)| name.to_string())
                .unwrap_or_else(|| format_unit(unit, v)),
            None => "Custom".into(),
        },
    }
}

/// The Overview's goal cards, best looking first. Each is one community preset; the rest sit
/// in the "All presets" dropdown.
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

/// Typed slider text back to a raw value: a special name, or a number in the unit shown.
pub fn parse(control: Control, text: &str) -> Option<f64> {
    let Control::Slider { unit, special } = control else {
        return None;
    };
    let text = text.trim().trim_start_matches('~');
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
        Unit::Distance if text.ends_with("km") => number * 1000.0 / METRES_PER_UNIT,
        Unit::Distance => number / METRES_PER_UNIT,
        // Small numbers are ratios ("2.15"), anything else is degrees ("100").
        Unit::WideView if number > 10.0 => degrees_to_ratio(number),
        _ => number,
    })
}

/// When a saved change reaches the game, in the simple view's words.
pub fn when_it_applies(class: ApplyClass) -> &'static str {
    match class {
        ApplyClass::Live => "Instant with your key bind",
        ApplyClass::LiveCheat => "In sandbox, or next launch",
        ApplyClass::Restart => "Next time you start Deadlock",
    }
}

/// Rows whose label, help or convar name contains every word of `query`, in table order.
pub fn search(query: &str) -> Vec<&'static Row> {
    let words: Vec<String> = query.split_whitespace().map(str::to_lowercase).collect();
    if words.is_empty() {
        return Vec::new();
    }
    ROWS.iter()
        .filter(|r| {
            let text = format!("{} {} {}", r.label, r.help, r.name).to_lowercase();
            words.iter().all(|w| text.contains(w))
        })
        .collect()
}

pub fn section_of(name: &str) -> Option<Section> {
    Section::ALL
        .into_iter()
        .filter(|s| s.has_rows())
        .find(|s| groups(*s).iter().any(|g| g.names.contains(&name)))
}

pub fn group_of(name: &str) -> Option<&'static Group> {
    section_of(name).and_then(|s| groups(s).iter().find(|g| g.names.contains(&name)))
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
        PresetId::SideLock => {
            Some("Similar to OptiLock Potato. Its extra rendering tweaks are in Practice mode.")
        }
        PresetId::SqookyTest => None,
    }
}

/// A raw error in plain words: what happened, and what to do about it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Problem {
    pub title: String,
    pub fix: Option<String>,
}

impl Problem {
    fn new(title: &str, fix: &str) -> Problem {
        Problem {
            title: title.into(),
            fix: Some(fix.into()),
        }
    }

    pub fn line(&self) -> String {
        match &self.fix {
            Some(fix) => format!("{}. {fix}", self.title),
            None => self.title.clone(),
        }
    }
}

/// Context prefixes go first: "launch: ... not found" is about Steam, not a game file.
pub fn explain(raw: &str) -> Problem {
    let lower = raw.to_lowercase();
    let has = |words: &[&str]| words.iter().any(|w| lower.contains(w));
    if raw == crate::state::STALE {
        Problem::new(
            "The game files changed while you were editing",
            "DeadTune reloaded them. Click Apply again.",
        )
    } else if lower.starts_with("launch") {
        Problem::new(
            "Couldn't start Deadlock",
            "Make sure Steam is open and signed in, then try again.",
        )
    } else if lower.starts_with("saving your profile") {
        Problem::new(
            "The game files changed, but DeadTune couldn't save your profile",
            "Your settings are in the game. Check that DeadTune's data folder isn't read-only.",
        )
    } else if lower.contains("boot cfg not updated") {
        Problem::new(
            "Saved, but instant changes may not work",
            "DeadTune couldn't update its file in the game's cfg folder. Close Deadlock and click Apply again.",
        )
    } else if has(&["live push failed", "push failed", "live push skipped"]) {
        Problem::new(
            "Saved, but the instant push didn't reach the game",
            "The changes load the next time you start Deadlock.",
        )
    } else if has(&["isn't downloaded yet"]) {
        Problem::new(
            "This preset isn't downloaded yet",
            "Download it, or import the cfg.zip from its mod page.",
        )
    } else if has(&["braces", "no convars block"]) {
        Problem::new(
            "The game's config file looks damaged",
            "Click Restore original game files on Safety & setup, then try again.",
        )
    } else if has(&["used by another process", "os error 32", "resource busy"]) {
        Problem::new(
            "A game file is in use by another program",
            "Close Deadlock, then click Apply again.",
        )
    } else if has(&[
        "permission",
        "access is denied",
        "denied (os error",
        "read-only",
    ]) {
        Problem::new(
            "DeadTune wasn't allowed to change a game file",
            "Close Deadlock and try again. If it keeps happening, run Steam's Verify integrity of game files.",
        )
    } else if has(&["no space left", "os error 112", "disk full"]) {
        Problem::new(
            "The disk is full",
            "Free some space, then click Apply again.",
        )
    } else if has(&["not found", "no such file", "cannot find"]) {
        Problem::new(
            "A game file is missing",
            "Open System check to see which one.",
        )
    } else if has(&["connection refused", "timed out"]) {
        Problem::new(
            "The game didn't answer",
            "Press your instant-change key in game, or restart Deadlock to load the changes.",
        )
    } else {
        Problem {
            title: raw.trim().trim_end_matches('.').to_string(),
            fix: Some("If it keeps happening, open System check.".into()),
        }
    }
}

/// [`explain`] as one line, for places with room for a single label.
pub fn human_error(raw: &str) -> String {
    explain(raw).line()
}

#[cfg(test)]
mod tests {
    use super::*;
    use dt_core::catalog::{Catalog, Impact, Kind};

    fn label(name: &str) -> Option<&'static str> {
        row(name).map(|r| r.label)
    }

    /// Curated settings for the simple view: labelled, not denylisted, and either impactful
    /// or a camera comfort setting.
    fn simple_rows(catalog: &Catalog) -> Vec<&'static str> {
        ROWS.iter()
            .map(|r| r.name)
            .filter(|n| {
                catalog.get(n).is_some_and(|e| {
                    !e.denylist
                        && (matches!(e.impact, Impact::High | Impact::Medium)
                            || e.category == "Camera & view")
                })
            })
            .collect()
    }

    #[test]
    fn camera_rows_are_curated_and_searchable_as_fov() {
        let catalog = Catalog::embedded();
        for name in CAMERA {
            let e = catalog.get(name).unwrap();
            assert_eq!(e.category, "Camera & view", "{name}");
            assert!(e.range.is_some() && e.step.is_some(), "{name}");
            assert!(!e.denylist && !e.gameinfo_ignored, "{name}");
        }
        assert_eq!(
            catalog.get("citadel_camera_hero_fov").unwrap().range,
            Some([75.0, 90.0]),
            "the engine clamps hero FOV to 75..90"
        );
        let names = |q: &str| search(q).iter().map(|r| r.name).collect::<Vec<_>>();
        assert_eq!(names("fov"), CAMERA);
        assert_eq!(section_of("r_aspectratio"), Some(Section::Display));
        assert_eq!(
            group_of("citadel_camera_hero_fov").map(|g| g.title),
            Some("Camera")
        );
    }

    #[test]
    fn wide_view_maps_ratios_to_degrees_and_back() {
        for (ratio, degrees) in WIDE_VIEW {
            assert_eq!(ratio_to_degrees(*ratio), *degrees);
            assert_eq!(degrees_to_ratio(*degrees), *ratio);
        }
        assert_eq!(
            ratio_to_degrees(2.32),
            95.0,
            "halfway between 2.15 and 2.49"
        );
        assert_eq!(degrees_to_ratio(95.0), 2.32);
        assert_eq!(ratio_to_degrees(9.0), 120.0, "clamped at the widest point");
        assert_eq!(degrees_to_ratio(60.0), 1.33);
    }

    #[test]
    fn wide_view_rail_keeps_automatic_and_preset_ratios() {
        let u = Unit::WideView;
        assert_eq!(to_rail(u, 0.0), WIDE_RAIL_AUTO);
        assert_eq!(from_rail(u, WIDE_RAIL_AUTO), 0.0);
        assert_eq!(from_rail(u, 88.0), 0.0, "left of 90 is Automatic");
        for preset in [2.15, 2.3, 2.4, 2.9, 3.2] {
            assert_eq!(from_rail(u, to_rail(u, preset)), preset, "{preset} drifted");
        }
        let snapped = snap(
            to_rail(u, 2.49) + 0.3,
            [WIDE_RAIL_AUTO, 120.0],
            rail_step(u, 0.01),
        );
        assert_eq!(from_rail(u, snapped), 2.49, "a drag snaps to whole degrees");
        assert_eq!(to_rail(Unit::Degrees, 82.0), 82.0);
        assert_eq!(from_rail(Unit::Degrees, 82.0), 82.0);
    }

    #[test]
    fn camera_values_read_and_type_in_degrees() {
        let fov = row("citadel_camera_hero_fov").unwrap().control;
        assert_eq!(display(fov, "90"), "90°");
        assert_eq!(
            parse(fov, "100"),
            Some(100.0),
            "the control clamps it to 90"
        );
        assert_eq!(parse(fov, "80°"), Some(80.0));
        let wide = row("r_aspectratio").unwrap().control;
        assert_eq!(display(wide, "0"), "Automatic");
        assert_eq!(display(wide, "2.15"), "~90° (2.15)");
        assert_eq!(display(wide, "2.3"), "~94° (2.3)");
        assert_eq!(parse(wide, "100"), Some(2.49));
        assert_eq!(parse(wide, "2.4"), Some(2.4), "a ratio types as itself");
        assert_eq!(parse(wide, "~100° (2.49)"), Some(2.49));
        assert_eq!(parse(wide, "automatic"), Some(0.0));
    }

    #[test]
    fn every_row_names_a_curated_catalog_entry() {
        let catalog = Catalog::embedded();
        let rows = simple_rows(catalog);
        assert_eq!(
            rows.len(),
            ROWS.len(),
            "a row is missing from the catalog or not high/medium impact"
        );
    }

    #[test]
    fn every_high_or_medium_entry_has_a_row() {
        let missing: Vec<&String> = Catalog::embedded()
            .entries
            .iter()
            .filter(|(_, e)| !e.denylist && matches!(e.impact, Impact::High | Impact::Medium))
            .map(|(n, _)| n)
            .filter(|n| label(n).is_none())
            .collect();
        assert!(missing.is_empty(), "add rows for {missing:?}");
    }

    #[test]
    fn every_row_sits_in_exactly_one_section_group() {
        for r in ROWS {
            let homes = Section::ALL
                .iter()
                .filter(|s| **s != Section::Overview)
                .flat_map(|s| section_names(*s))
                .filter(|n| *n == r.name)
                .count();
            assert_eq!(homes, 1, "{} appears in {homes} groups", r.name);
        }
        for s in Section::ALL {
            for name in section_names(s) {
                assert!(row(name).is_some(), "{name} in {s:?} has no row");
            }
        }
    }

    #[test]
    fn controls_match_the_catalog_kind() {
        let catalog = Catalog::embedded();
        for r in ROWS {
            let entry = catalog.get(r.name).unwrap();
            let is_bool = matches!(entry.kind, Kind::Bool);
            assert_eq!(
                matches!(r.control, Control::Toggle { .. }),
                is_bool,
                "{} is {:?} but shown as {:?}",
                r.name,
                entry.kind,
                r.control
            );
            if let Control::Levels(levels) = r.control {
                let [lo, hi] = entry.range.expect("levels need a range");
                for (v, name) in levels {
                    assert!(
                        (lo..=hi).contains(v),
                        "{} level {name} out of range",
                        r.name
                    );
                }
            }
        }
    }

    #[test]
    fn every_preset_and_default_value_has_a_name_not_custom() {
        let catalog = Catalog::embedded();
        for r in ROWS {
            let Control::Levels(_) = r.control else {
                continue;
            };
            let entry = catalog.get(r.name).unwrap();
            let values = entry
                .presets
                .values()
                .filter(|v| !v.starts_with("//"))
                .chain(entry.default.as_ref());
            for v in values {
                assert_ne!(display(r.control, v), "Custom", "{} = {v}", r.name);
            }
        }
    }

    #[test]
    fn values_read_as_words_and_units() {
        let fps = row("fps_max").unwrap().control;
        assert_eq!(display(fps, "0"), "Unlimited");
        assert_eq!(display(fps, "144"), "144 FPS");
        let shadows = row("r_citadel_shadow_quality").unwrap().control;
        assert_eq!(display(shadows, "0"), "Low");
        assert_eq!(display(shadows, "3.0"), "Ultra");
        let upscale = row("r_citadel_upscaling").unwrap().control;
        assert_eq!(display(upscale, "4"), "DLSS / FSR");
        assert_eq!(display(upscale, "2"), "Custom");
        let farz = row("r_farz").unwrap().control;
        assert_eq!(display(farz, "-1"), "Auto");
        assert_eq!(display(farz, "7000"), "178 m");
        let effects = row("r_particle_max_draw_distance").unwrap().control;
        assert_eq!(display(effects, "1e+06"), "25.4 km");
        let blur = row("panorama_disable_blur").unwrap().control;
        assert_eq!(display(blur, "true"), "Off", "inverted toggle");
        assert_eq!(display(blur, "0"), "On");
    }

    #[test]
    fn snapping_keeps_sentinels_and_round_numbers() {
        assert_eq!(snap(7000.0, [-1.0, 16000.0], 500.0), 7000.0);
        assert_eq!(snap(6830.0, [-1.0, 16000.0], 500.0), 7000.0);
        assert_eq!(snap(-1.0, [-1.0, 16000.0], 500.0), -1.0);
        assert_eq!(snap(-0.6, [-1.0, 16000.0], 500.0), -1.0);
        assert_eq!(snap(150.0, [-1.0, 8000.0], 250.0), 250.0);
        assert_eq!(snap(0.2, [-1.0, 8000.0], 250.0), 0.0, "Off stays reachable");
        assert_eq!(snap(0.55, [-1.0, 2.0], 0.0), 0.55);
    }

    #[test]
    fn key_settings_are_rows() {
        assert!(KEY_SETTINGS.len() >= 4);
        for name in KEY_SETTINGS {
            assert!(row(name).is_some(), "{name}");
        }
    }

    #[test]
    fn goals_are_blurbed_presets_from_best_looks_to_max_fps() {
        for (id, _, _) in GOALS {
            assert!(preset_blurb(*id).is_some(), "{id:?}");
        }
        assert_eq!(GOALS.first().unwrap().0, PresetId::Vanilla);
        assert_eq!(GOALS.last().unwrap().0, PresetId::OptilockPotato);
    }

    #[test]
    fn typed_values_parse_back_through_the_unit() {
        let fps = row("fps_max").unwrap().control;
        assert_eq!(parse(fps, "144"), Some(144.0));
        assert_eq!(parse(fps, "144 FPS"), Some(144.0));
        assert_eq!(parse(fps, "unlimited"), Some(0.0));
        assert_eq!(parse(fps, "lots"), None);
        let farz = row("r_farz").unwrap().control;
        assert_eq!(parse(farz, "Auto"), Some(-1.0));
        assert_eq!(parse(farz, "2 km").map(f64::round), Some(78740.0));
        assert_eq!(parse(farz, "178 m").map(f64::round), Some(7008.0));
        let count = row("cl_particle_max_count").unwrap().control;
        assert_eq!(parse(count, "800 particles"), Some(800.0));
        let levels = row("r_citadel_shadow_quality").unwrap().control;
        assert_eq!(parse(levels, "2"), None, "only sliders take typed text");
    }

    #[test]
    fn search_matches_label_help_or_convar_name_with_every_word() {
        let names = |q: &str| search(q).iter().map(|r| r.name).collect::<Vec<_>>();
        assert!(names("shadow").contains(&"r_citadel_shadow_quality"));
        assert!(
            names("shadow").contains(&"r_ssao"),
            "help text mentions shadows"
        );
        assert_eq!(names("farz"), vec!["r_farz"], "convar name");
        assert_eq!(names("SUN reach"), vec!["csm_max_shadow_dist_override"]);
        assert!(names("").is_empty());
        assert!(names("zzz").is_empty());
        for r in search("shadow") {
            assert!(section_of(r.name).is_some(), "{} has a section", r.name);
        }
        assert_eq!(section_of("r_farz"), Some(Section::World));
        assert_eq!(group_of("r_farz").map(|g| g.title), Some("Distance"));
    }

    #[test]
    fn simple_copy_avoids_jargon() {
        let blurbs = dt_core::preset::all()
            .iter()
            .filter_map(|p| preset_blurb(p.id));
        let errors = [
            "unbalanced braces near line 3",
            "Permission denied (os error 13)",
            "SideLock isn't downloaded yet. Download it or import cfg.zip or gameinfo.gi.",
        ]
        .map(human_error);
        for text in ROWS
            .iter()
            .flat_map(|r| [r.label.to_string(), r.help.to_string()])
            .chain(blurbs.map(str::to_string))
            .chain(errors)
        {
            let lower = text.to_lowercase();
            for word in [
                "convar", "gameinfo", "devonly", "cvar", "mip", "lod", "cascade",
            ] {
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
        assert_eq!(
            explain("launch: steam.exe not found").title,
            "Couldn't start Deadlock"
        );
        assert!(
            explain("files saved, but the live push failed: x")
                .title
                .starts_with("Saved")
        );
    }

    #[test]
    fn messages_already_in_plain_words_are_kept() {
        let raw = "Deadlock didn't start with Clutter remover. DeadTune removed them.";
        assert!(
            explain(raw)
                .title
                .starts_with("Deadlock didn't start with Clutter remover")
        );
    }
}
