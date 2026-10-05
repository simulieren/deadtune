//! The HUD elements DeadTune can edit: one table, one row per element.
//! Selectors and vanilla geometry come from the game's `hud.xml` / `hud.css`
//! (research/hud/vpk-and-compiled-resources.md section 3.3). All rules target
//! `panorama/styles/hud.vcss_c`, which styles every panel in the `hud.xml` tree.

pub const HUD_STYLE: &str = "panorama/styles/hud.vcss_c";

#[derive(
    Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Serialize, serde::Deserialize,
)]
#[serde(rename_all = "snake_case")]
pub enum ElementId {
    TopBar,
    Minimap,
    HealthAndAmmo,
    AbilitySlots,
    ItemSlots,
    PassiveItems,
    PlayerStats,
    AmmoCounter,
    KillFeed,
    Chat,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HAlign {
    Left,
    Center,
    Right,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VAlign {
    Top,
    Center,
    Bottom,
}

/// How scale is expressed. `UiScale` re-lays out children (crisp text);
/// `PreTransform` scales the rendered panel around `origin` (used for the minimap).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScaleProp {
    UiScale,
    PreTransform { origin: &'static str },
}

/// Where a vanilla box's numbers come from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Measured {
    /// The element's visible pixels in the reference screenshot the GUI ships
    /// (`crates/dt-gui/assets/vanilla_hud.jpg`, a 16:9 game capture at default HUD
    /// settings), so the preview can cut the element out of it.
    Screenshot,
    /// Vanilla CSS, because the element is empty or collapsed in the screenshot.
    Css,
}

/// Vanilla placement at the 1920x1080 reference resolution, for the preview canvas.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct VanillaBox {
    pub h: HAlign,
    pub v: VAlign,
    pub width: f32,
    pub height: f32,
    /// Offset of the box from its aligned edge, in reference px (x grows right, y grows down).
    pub dx: f32,
    pub dy: f32,
    /// Vanilla CSS collapses it until a game state shows it.
    pub collapsed: bool,
    pub from: Measured,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ElementSpec {
    pub id: ElementId,
    pub label: &'static str,
    /// Main selector; every edit targets it.
    pub selector: &'static str,
    pub scale: ScaleProp,
    /// `ui-scale` vanilla already sets on the selector. `scale_pct` is relative to
    /// vanilla, so the emitted `ui-scale` is this times `scale_pct`.
    pub vanilla_ui_scale_pct: u16,
    pub vanilla: VanillaBox,
    /// Shown in the editor (e.g. which game states override the edit).
    pub notes: &'static str,
}

pub static ELEMENTS: &[ElementSpec] = &[
    ElementSpec {
        id: ElementId::TopBar,
        label: "Top bar",
        selector: "#TopBar",
        scale: ScaleProp::UiScale,
        vanilla_ui_scale_pct: 100,
        vanilla: VanillaBox {
            h: HAlign::Center,
            v: VAlign::Top,
            width: 1250.0,
            height: 145.0,
            dx: 0.0,
            dy: 0.0,
            collapsed: false,
            from: Measured::Screenshot,
        },
        notes: "Hero portraits, souls, clock. Layout lives on the CitadelHudTopBar rule in citadel_hud_top_bar.vcss_c (full height, fit-children width); the box is the visible strip in the reference screenshot, portraits down to the spawn and eye icons under them (1250x145). Hidden during death replay and in the hideout.",
    },
    ElementSpec {
        id: ElementId::Minimap,
        label: "Minimap",
        selector: "#minimap_persp",
        scale: ScaleProp::PreTransform {
            origin: "100% 100%",
        },
        vanilla_ui_scale_pct: 100,
        vanilla: VanillaBox {
            h: HAlign::Right,
            v: VAlign::Bottom,
            width: 370.0,
            height: 392.0,
            dx: -38.0,
            dy: -48.0,
            collapsed: false,
            from: Measured::Screenshot,
        },
        notes: "The box is the visible map in the reference screenshot, 370x392 with the base markers that stick out top and bottom, 38px from the right and 48px from the bottom (the #minimap_persp panel itself is 440x520 including perspective room). Scales from the bottom-right corner. Sits inside .clamp_width, so on 21:9 it hugs the 1920px clamp, not the screen edge. Vanilla transitions pre-transform-scale2d (hideout uses 0.9), so state rules like .InHideout override the scale.",
    },
    ElementSpec {
        id: ElementId::HealthAndAmmo,
        label: "Health and ammo",
        selector: "#health_and_abilities_container",
        scale: ScaleProp::UiScale,
        vanilla_ui_scale_pct: 120,
        vanilla: VanillaBox {
            h: HAlign::Center,
            v: VAlign::Bottom,
            width: 170.0,
            height: 265.0,
            dx: -457.0,
            dy: -155.0,
            collapsed: false,
            from: Measured::Screenshot,
        },
        notes: "The box is the tilted health gauge in the reference screenshot (170x265, its centre 457px left of the screen centre). The panel itself is 250x380 at ui-scale 120%, centred between margin-left 300px and margin-right 1290px, so moving it moves the gauge. 21:9 and 16:10 rules change the margins.",
    },
    ElementSpec {
        id: ElementId::AbilitySlots,
        label: "Ability slots",
        selector: "#hud_signature",
        scale: ScaleProp::UiScale,
        vanilla_ui_scale_pct: 90,
        vanilla: VanillaBox {
            h: HAlign::Center,
            v: VAlign::Bottom,
            width: 348.0,
            height: 106.0,
            dx: 2.0,
            dy: -14.0,
            collapsed: false,
            from: Measured::Screenshot,
        },
        notes: "The four ability icons with their key labels and the stamina pips above them, 348x106 in the reference screenshot, 14px above the bottom. The base rule collapses it, but .viewing_as_player shows it, so it is visible in normal play. Shrinks to 75% while the shop is open.",
    },
    ElementSpec {
        id: ElementId::ItemSlots,
        label: "Item slots",
        selector: "#ActiveAbilitiesMenu",
        scale: ScaleProp::UiScale,
        vanilla_ui_scale_pct: 100,
        vanilla: VanillaBox {
            h: HAlign::Center,
            v: VAlign::Bottom,
            width: 340.0,
            height: 100.0,
            dx: 300.0,
            dy: 0.0,
            collapsed: false,
            from: Measured::Css,
        },
        notes: "Active items 1 to 4. Right-aligned with margin-right 100px inside #AbilitiesContainer (1140px wide, centred), so it ends 330px right of screen centre; the 340px width is an estimate (fit-children).",
    },
    ElementSpec {
        id: ElementId::PassiveItems,
        label: "Passive items",
        selector: "#hud_passive_items",
        scale: ScaleProp::UiScale,
        vanilla_ui_scale_pct: 130,
        vanilla: VanillaBox {
            h: HAlign::Center,
            v: VAlign::Bottom,
            width: 780.0,
            height: 78.0,
            dx: 0.0,
            dy: -322.0,
            collapsed: true,
            from: Measured::Css,
        },
        notes: "Collapsed in vanilla; Shown reveals it (what QoL Lite does). 600x60 at ui-scale 130%, top-aligned with a 20px margin inside #AbilitiesContainer, whose top is 420px above the screen bottom. Collapsed again while the shop is open.",
    },
    ElementSpec {
        id: ElementId::PlayerStats,
        label: "Player stats",
        selector: "#StatsAndModsContainer",
        scale: ScaleProp::UiScale,
        vanilla_ui_scale_pct: 100,
        vanilla: VanillaBox {
            h: HAlign::Left,
            v: VAlign::Bottom,
            width: 430.0,
            height: 290.0,
            dx: 22.0,
            dy: -20.0,
            collapsed: false,
            from: Measured::Screenshot,
        },
        notes: "Weapon, vitality and spirit totals, level, souls and the purchased item grid in the lower left, 430x290 and 22px in from the left in the reference screenshot. The container itself is full screen. Hidden while editing builds and in spectator mode.",
    },
    ElementSpec {
        id: ElementId::AmmoCounter,
        label: "Ammo counter",
        selector: "#ammo_panel",
        scale: ScaleProp::UiScale,
        vanilla_ui_scale_pct: 100,
        vanilla: VanillaBox {
            h: HAlign::Center,
            v: VAlign::Center,
            width: 100.0,
            height: 60.0,
            dx: 0.0,
            dy: 62.0,
            collapsed: false,
            from: Measured::Screenshot,
        },
        notes: "The reload arc and clip count under the crosshair, 100x60 and centred 62px below the screen centre in the reference screenshot. Defined in ability_hud_elements/element_gun.xml, not in hud.xml; whether a hud.vcss_c rule reaches it is unverified, it may need element_gun.vcss_c.",
    },
    ElementSpec {
        id: ElementId::KillFeed,
        label: "Kill feed",
        selector: "#DataFeed",
        scale: ScaleProp::UiScale,
        vanilla_ui_scale_pct: 100,
        vanilla: VanillaBox {
            h: HAlign::Left,
            v: VAlign::Top,
            width: 1000.0,
            height: 240.0,
            dx: 0.0,
            dy: 200.0,
            collapsed: false,
            from: Measured::Css,
        },
        notes: "HudDataFeed root is full screen; the box is #EventFeed (1000px wide, 200px from the top, from hud_data_feed.vcss_c) with an estimated height. Hidden during death replay.",
    },
    ElementSpec {
        id: ElementId::Chat,
        label: "Chat",
        selector: "#Chat",
        scale: ScaleProp::UiScale,
        vanilla_ui_scale_pct: 100,
        vanilla: VanillaBox {
            h: HAlign::Center,
            v: VAlign::Bottom,
            width: 350.0,
            height: 200.0,
            dx: 0.0,
            dy: -200.0,
            collapsed: false,
            from: Measured::Css,
        },
        notes: "350px wide (chat.vcss_c), centred, 200px above the bottom per vanilla hud.css. Only visible while typing or for a few seconds after a message; not seen in sandbox screenshots, so position is from CSS only.",
    },
];

pub fn spec(id: ElementId) -> &'static ElementSpec {
    ELEMENTS
        .iter()
        .find(|s| s.id == id)
        .expect("every ElementId has a row in ELEMENTS")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn one_row_per_element_in_id_order() {
        let ids: Vec<ElementId> = ELEMENTS.iter().map(|s| s.id).collect();
        let mut sorted = ids.clone();
        sorted.sort();
        sorted.dedup();
        assert_eq!(ids, sorted, "rows sorted by ElementId, no duplicates");
        assert_eq!(ids.len(), 10);
        for s in ELEMENTS {
            assert_eq!(spec(s.id).selector, s.selector);
            assert!(
                s.selector.starts_with('#'),
                "{:?} targets a hud.xml id",
                s.id
            );
        }
    }
}
