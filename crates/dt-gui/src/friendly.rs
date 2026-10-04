//! Plain-language copy for the simple view: setting labels, preset blurbs, human errors.

use dt_core::catalog::{Catalog, Impact};
use dt_core::preset::PresetId;

/// Friendly labels for the curated high/medium-impact convars, in display order.
const LABELS: &[(&str, &str)] = &[
    ("fps_max", "FPS limit"),
    ("r_citadel_upscaling", "Upscaling (DLSS / FSR)"),
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
    (
        "lb_dynamic_shadow_resolution",
        "Automatic shadow resolution",
    ),
    ("lb_shadow_texture_width_override", "Shadow map width"),
    ("lb_shadow_texture_height_override", "Shadow map height"),
    ("r_citadel_distancefield_shadows", "Soft shadows"),
    ("sc_disable_spotlight_shadows", "Turn off spotlight shadows"),
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
    ("panorama_disable_blur", "Turn off menu blur"),
    ("r_citadel_enable_pano_world_blur", "Blur behind the shop"),
    ("thread_pool_option", "CPU thread mode"),
];

pub fn label(name: &str) -> Option<&'static str> {
    LABELS.iter().find(|(n, _)| *n == name).map(|(_, l)| *l)
}

/// Curated settings for the simple view, in display order: labelled, impactful, not denylisted.
pub fn simple_rows(catalog: &Catalog) -> Vec<&'static str> {
    LABELS
        .iter()
        .map(|(n, _)| *n)
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
        for text in LABELS
            .iter()
            .map(|(_, l)| l.to_string())
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
