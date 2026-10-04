//! Plain-language copy for the simple view: setting rows, level names, preset blurbs, human errors.

use dt_core::preset::PresetId;

use crate::state::{SimpleTab, parse_bool};

type Levels = &'static [(&'static str, &'static str)];

/// How a setting is edited. Level values are config text, listed lowest quality first.
pub enum Control {
    Levels(Levels),
    Toggle,
    /// The config value turns the feature off, so the switch shows the opposite.
    ToggleInverted,
    Slider {
        lo: f64,
        hi: f64,
        step: f64,
        unit: &'static str,
        zero: Option<&'static str>,
    },
}

pub struct Row {
    pub name: &'static str,
    pub label: &'static str,
    pub tab: SimpleTab,
    pub group: &'static str,
    pub control: Control,
    pub help: &'static str,
}

const LOW_ULTRA: Levels = &[("0", "Low"), ("1", "Medium"), ("2", "High"), ("3", "Ultra")];
const LOW_HIGH: Levels = &[("0", "Low"), ("1", "Medium"), ("2", "High")];
const OFF_STRONG: Levels = &[
    ("0", "Off"),
    ("1", "Light"),
    ("5", "Medium"),
    ("10", "Strong"),
];
const SHADOW_MAP: Levels = &[
    ("1", "Minimal"),
    ("1024", "Low"),
    ("2048", "Medium"),
    ("4096", "High"),
    ("-1", "Default"),
];

const fn row(
    name: &'static str,
    label: &'static str,
    tab: SimpleTab,
    group: &'static str,
    control: Control,
    help: &'static str,
) -> Row {
    Row {
        name,
        label,
        tab,
        group,
        control,
        help,
    }
}

use SimpleTab::{Advanced as A, Display as D, Effects as E, Quality as Q};

/// Every high or medium impact setting, in display order (tab, then group).
#[rustfmt::skip]
pub const ROWS: &[Row] = &[
    row("fps_max", "FPS limit", D, "Screen", Control::Slider { lo: 0.0, hi: 500.0, step: 1.0, unit: "FPS", zero: Some("Unlimited") },
        "The most frames per second the game will draw. Set it near your monitor's refresh rate to save heat and battery. Unlimited uses all of your graphics card."),
    row("r_citadel_upscaling", "Upscaling", D, "Screen", Control::Levels(&[("0", "Off"), ("4", "DLSS / FSR")]),
        "Draws the game at a lower size and stretches it up with DLSS or FSR. Gives a big FPS boost with a small loss of sharpness. Off draws at full size."),
    row("r_citadel_antialiasing", "Anti-aliasing", D, "Screen", Control::Toggle,
        "Smooths jagged edges. Turning it off is a small FPS gain and a slightly rougher picture."),
    row("r_texture_stream_mip_bias", "Texture sharpness", D, "Screen", Control::Levels(&[("8", "Blurry"), ("3", "Soft"), ("0", "Sharp")]),
        "How crisp surfaces look up close. Softer textures use less video memory, which helps cards with 4 GB or less."),
    row("r_texture_lod_scale", "Texture size", D, "Screen", Control::Levels(&[("4", "Small"), ("2", "Medium"), ("1", "Full")]),
        "The size of textures the game loads. Smaller textures save video memory and look blurrier."),
    row("r_farz", "View distance", D, "Screen", Control::Levels(&[("4500", "Short"), ("6000", "Medium"), ("8000", "Long"), ("-1", "Map default")]),
        "How far you can see. Short distances make far buildings and players pop in late."),
    row("sc_screen_size_lod_scale_override", "Model detail", D, "Screen", Control::Levels(&[("0.001", "Lowest"), ("0.55", "Medium"), ("0.8", "High"), ("-1", "Default")]),
        "How soon the game swaps characters and objects for simpler models. Lower settings gain FPS and make heroes look worse."),

    row("r_shadows", "Shadows", Q, "Shadows", Control::Toggle,
        "The master switch for shadows. Off is a large FPS gain but flattens the whole scene."),
    row("r_citadel_shadow_quality", "Shadow quality", Q, "Shadows", Control::Levels(LOW_ULTRA),
        "How sharp and detailed shadows are. Each step up costs noticeable FPS."),
    row("cl_globallight_shadow_mode", "Sun shadows", Q, "Shadows", Control::Levels(&[("0", "Off"), ("1", "Fast"), ("2", "Full")]),
        "Shadows cast by the sun across the map. Off is a good FPS gain on weaker PCs."),
    row("lb_enable_shadow_casting", "Light shadows", Q, "Shadows", Control::Toggle,
        "Lets lamps and ability lights cast shadows. Off saves a lot of FPS in busy fights."),
    row("r_citadel_distancefield_shadows", "Soft shadows", Q, "Shadows", Control::Toggle,
        "Gives shadows soft, realistic edges. Off is a modest FPS gain."),
    row("sc_disable_spotlight_shadows", "Spotlight shadows", Q, "Shadows", Control::ToggleInverted,
        "Shadows from spotlights in the map. Off is a modest FPS gain."),
    row("r_ssao", "Ambient occlusion", Q, "Lighting", Control::Toggle,
        "Adds dark contact shadows in corners and where objects meet. Off is a large FPS gain."),
    row("r_citadel_ssao_quality", "Ambient occlusion quality", Q, "Lighting", Control::Levels(LOW_ULTRA),
        "How detailed the corner shadows are. Only matters while ambient occlusion is on."),
    row("lb_enable_dynamic_lights", "Ability lighting", Q, "Lighting", Control::Toggle,
        "Light from abilities and effects. Off gains FPS, but hero portraits also lose their colour in the shop and end screen."),
    row("lb_enable_envmaps", "Reflections", Q, "Lighting", Control::Toggle,
        "Reflections on shiny surfaces. Off can make characters look black, so most presets leave it alone."),

    row("r_effects_bloom", "Glow on effects", E, "Effects", Control::Toggle,
        "A soft glow around bright effects such as abilities. Off is a small FPS gain."),
    row("r_enable_volume_fog", "Volumetric fog", E, "Effects", Control::Toggle,
        "Thick fog that light shines through. Off is a small to medium FPS gain."),
    row("r_citadel_fog_quality", "Fog quality", E, "Effects", Control::Levels(LOW_HIGH),
        "How detailed fog looks. Only matters while fog is on."),
    row("r_particle_max_detail_level", "Effect detail", E, "Effects", Control::Levels(LOW_ULTRA),
        "How detailed spells, bullets and impacts look. Lower levels gain a lot of FPS in team fights."),
    row("cl_particle_max_count", "Effect limit", E, "Effects", Control::Slider { lo: 0.0, hi: 5000.0, step: 100.0, unit: "effects", zero: Some("Unlimited") },
        "The most effects the game draws at once. Too low and abilities get hard to read in a fight."),
    row("r_particle_max_draw_distance", "Effect draw distance", E, "Effects", Control::Levels(&[("100000", "Short"), ("300000", "Medium"), ("1000000", "Unlimited")]),
        "How far away effects are still drawn. Shorter saves FPS in big fights."),
    row("cl_particle_fallback_base", "Cheaper effects under load", E, "Effects", Control::Levels(OFF_STRONG),
        "When the game slows down, swaps effects for cheaper ones. Stronger keeps FPS steadier in big fights."),
    row("r_grass_quality", "Grass", E, "World", Control::Levels(&[("0", "Off"), ("1", "Low"), ("2", "Medium"), ("3", "High"), ("4", "Ultra")]),
        "How much grass is drawn. Off also makes it easier to see."),
    row("sc_clutter_enable", "Small clutter", E, "World", Control::Toggle,
        "Little props lying around the map. Off gains FPS and makes the map easier to read."),

    row("r_propsmaxdist", "Prop draw distance", A, "Distance and detail", Control::Levels(&[("600", "Near"), ("900", "Medium"), ("1200", "Far")]),
        "How far boxes and other props are drawn. Too short hides breakable objects."),
    row("r_size_cull_threshold", "Hide tiny objects", A, "Distance and detail", Control::Levels(&[("0.8", "Mild"), ("1.4", "Medium"), ("1.75", "Aggressive")]),
        "Skips drawing very small things. Stronger gains FPS, but trooper health bars and boxes show up later."),
    row("sc_fade_distance_scale_override", "Fade-in distance", A, "Distance and detail", Control::Levels(&[("5", "Very short"), ("100", "Normal"), ("180", "Long"), ("-1", "Default")]),
        "How far away objects fade in and out. Shorter saves FPS and makes things appear closer to you."),
    row("sc_instanced_mesh_lod_bias", "Foliage detail", A, "Distance and detail", Control::Levels(&[("15", "Lowest"), ("3", "Reduced"), ("1.25", "Default")]),
        "How detailed trees, bushes and clutter look. Lower gains a little FPS."),
    row("csm_max_num_cascades_override", "Sun shadow detail", A, "Sun and light shadows", Control::Levels(&[("0", "Minimal"), ("1", "Low"), ("2", "Medium"), ("4", "High"), ("-1", "Default")]),
        "How many layers the sun shadows use. Fewer layers are cheaper and less detailed far away."),
    row("csm_max_shadow_dist_override", "Sun shadow reach", A, "Sun and light shadows", Control::Levels(&[("0", "Minimal"), ("1000", "Short"), ("4000", "Medium"), ("8000", "Long"), ("-1", "Default")]),
        "How far from you sun shadows are calculated."),
    row("csm_max_visible_dist", "Sun shadow distance", A, "Sun and light shadows", Control::Levels(&[("0", "Off"), ("1000", "Short"), ("3500", "Medium"), ("7500", "Far")]),
        "Beyond this distance, sun shadows are not drawn."),
    row("csm_sst_max_visible_dist", "Static shadow distance", A, "Sun and light shadows", Control::Levels(&[("10", "Minimal"), ("500", "Short"), ("1000", "Medium"), ("2000", "Far")]),
        "How far shadows from buildings and other fixed objects are drawn."),
    row("lb_dynamic_shadow_resolution", "Automatic shadow resolution", A, "Sun and light shadows", Control::Toggle,
        "Lets the game lower shadow sharpness on its own when it is busy."),
    row("lb_shadow_texture_width_override", "Shadow map width", A, "Sun and light shadows", Control::Levels(SHADOW_MAP),
        "Sharpness of light shadows, left to right. Very low values look blocky."),
    row("lb_shadow_texture_height_override", "Shadow map height", A, "Sun and light shadows", Control::Levels(SHADOW_MAP),
        "Sharpness of light shadows, top to bottom. Very low values look blocky."),
    row("lb_max_visible_barn_lights_override", "Spotlight limit", A, "Lights and CPU", Control::Levels(&[("1", "Minimal"), ("4", "Few"), ("16", "Many"), ("-1", "No limit")]),
        "The most spotlights drawn at once."),
    row("lb_max_visible_envmaps_override", "Reflection limit", A, "Lights and CPU", Control::Levels(&[("4", "Few"), ("10", "Many"), ("-1", "No limit")]),
        "The most reflection sources drawn at once."),
    row("cl_particle_fallback_multiplier", "Cheaper effects strength", A, "Lights and CPU", Control::Levels(OFF_STRONG),
        "How hard the game leans on cheaper effects once it slows down."),
    row("r_threaded_particles", "Effects on extra CPU threads", A, "Lights and CPU", Control::Toggle,
        "Spreads effect work over more CPU threads. Helps on CPUs with many cores."),
    row("thread_pool_option", "CPU thread mode", A, "Lights and CPU", Control::Levels(&[("-1", "Default"), ("0", "Mode A"), ("1", "Mode B"), ("2", "Mode C")]),
        "How the game shares work between CPU cores. Results differ per CPU, so try each one and watch your FPS."),
    row("panorama_max_fps", "Menu FPS limit", A, "Menus", Control::Slider { lo: 10.0, hi: 240.0, step: 5.0, unit: "FPS", zero: None },
        "The most frames per second menus such as the shop draw. Lower keeps the game smoother while a menu is open."),
    row("panorama_disable_blur", "Menu blur", A, "Menus", Control::ToggleInverted,
        "Frosted blur behind menus. Off is a modest FPS gain."),
    row("r_citadel_enable_pano_world_blur", "Blur behind the shop", A, "Menus", Control::Toggle,
        "Blurs the world behind the shop. Off makes the shop much faster."),
];

pub fn row_for(name: &str) -> Option<&'static Row> {
    ROWS.iter().find(|r| r.name == name)
}

pub fn rows_in(tab: SimpleTab) -> impl Iterator<Item = &'static Row> {
    ROWS.iter().filter(move |r| r.tab == tab)
}

/// Index of the level closest to `value`. Preset values often sit between named levels.
pub fn nearest_level(options: Levels, value: &str) -> Option<usize> {
    if let Some(i) = options.iter().position(|(v, _)| *v == value.trim()) {
        return Some(i);
    }
    let v: f64 = value.trim().parse().ok()?;
    options
        .iter()
        .enumerate()
        .filter_map(|(i, (o, _))| Some((i, (o.parse::<f64>().ok()? - v).abs())))
        .min_by(|a, b| a.1.total_cmp(&b.1))
        .map(|(i, _)| i)
}

/// Switch position for config text; inverted rows flip it.
pub fn switch_on(control: &Control, value: &str) -> bool {
    parse_bool(value) != matches!(control, Control::ToggleInverted)
}

pub fn slider_text(unit: &str, zero: Option<&str>, value: f64) -> String {
    match zero {
        Some(z) if value == 0.0 => z.to_string(),
        _ => format!("{} {unit}", value.round() as i64),
    }
}

impl Row {
    /// What the player reads for a raw config value.
    pub fn describe(&self, value: &str) -> String {
        match &self.control {
            Control::Levels(opts) => nearest_level(opts, value)
                .map_or_else(|| value.to_string(), |i| opts[i].1.to_string()),
            Control::Toggle | Control::ToggleInverted => if switch_on(&self.control, value) {
                "On"
            } else {
                "Off"
            }
            .into(),
            Control::Slider { unit, zero, .. } => {
                slider_text(unit, *zero, value.trim().parse().unwrap_or(0.0))
            }
        }
    }
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
    use dt_core::catalog::{Catalog, Impact, Kind};

    #[test]
    fn every_high_or_medium_entry_has_a_row_and_every_row_a_catalog_entry() {
        let catalog = Catalog::embedded();
        let missing: Vec<&String> = catalog
            .entries
            .iter()
            .filter(|(_, e)| !e.denylist && matches!(e.impact, Impact::High | Impact::Medium))
            .map(|(n, _)| n)
            .filter(|n| row_for(n).is_none())
            .collect();
        assert!(missing.is_empty(), "add rows for {missing:?}");
        for r in ROWS {
            let e = catalog
                .get(r.name)
                .unwrap_or_else(|| panic!("{} not in catalog", r.name));
            assert!(!e.denylist, "{} is denylisted", r.name);
        }
        assert_eq!(ROWS[0].name, "fps_max");
    }

    #[test]
    fn control_kind_matches_the_catalog_type() {
        let catalog = Catalog::embedded();
        for r in ROWS {
            let e = catalog.get(r.name).unwrap();
            match &r.control {
                Control::Toggle | Control::ToggleInverted => {
                    let zero_one = matches!(e.kind, Kind::Int) && e.range == Some([0.0, 1.0]);
                    assert!(
                        matches!(e.kind, Kind::Bool) || zero_one,
                        "{} is not a switch",
                        r.name
                    )
                }
                Control::Levels(opts) => {
                    assert!(!matches!(e.kind, Kind::Bool), "{} is a bool", r.name);
                    for (v, label) in *opts {
                        let n: f64 = v.parse().unwrap_or_else(|_| panic!("{} level {v}", r.name));
                        if let Some([lo, hi]) = e.range {
                            assert!(
                                (lo..=hi).contains(&n),
                                "{} level {v} outside {lo}..{hi}",
                                r.name
                            );
                        }
                        assert!(
                            !label.starts_with(|c: char| c.is_ascii_digit()),
                            "{} level {label}",
                            r.name
                        );
                    }
                }
                Control::Slider { lo, hi, .. } => {
                    assert!(lo < hi);
                    assert!(!matches!(e.kind, Kind::Bool));
                }
            }
        }
    }

    #[test]
    fn every_preset_value_lands_on_a_named_level() {
        let catalog = Catalog::embedded();
        for r in ROWS {
            let Control::Levels(opts) = &r.control else {
                continue;
            };
            let e = catalog.get(r.name).unwrap();
            for v in e.presets.values().chain(e.default.iter()) {
                let v = v.trim_start_matches("//");
                assert!(nearest_level(opts, v).is_some(), "{} value {v}", r.name);
            }
        }
    }

    #[test]
    fn nearest_level_snaps_between_names() {
        let opts: Levels = &[("0.001", "Lowest"), ("0.55", "Medium"), ("0.8", "High")];
        assert_eq!(nearest_level(opts, "0.00001"), Some(0));
        assert_eq!(nearest_level(opts, "0.6"), Some(1));
        assert_eq!(nearest_level(opts, "0.8"), Some(2));
        assert_eq!(nearest_level(opts, "junk"), None);
    }

    #[test]
    fn describe_uses_words_and_units() {
        let fps = row_for("fps_max").unwrap();
        assert_eq!(fps.describe("0"), "Unlimited");
        assert_eq!(fps.describe("144"), "144 FPS");
        assert_eq!(
            row_for("r_citadel_upscaling").unwrap().describe("4"),
            "DLSS / FSR"
        );
        assert_eq!(
            row_for("r_citadel_shadow_quality").unwrap().describe("3"),
            "Ultra"
        );
        let spot = row_for("sc_disable_spotlight_shadows").unwrap();
        assert_eq!(spot.describe("1"), "Off");
        assert_eq!(spot.describe("false"), "On");
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
        let rows = ROWS.iter().flat_map(|r| [r.label, r.help]);
        for text in rows
            .map(str::to_string)
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
