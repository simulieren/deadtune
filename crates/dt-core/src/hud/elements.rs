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

/// Vanilla placement at the 1920x1080 reference resolution, for the preview canvas.
/// Approximate: derived from align + size + margins in vanilla CSS.
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
        vanilla: VanillaBox { h: HAlign::Center, v: VAlign::Top, width: 1260.0, height: 100.0, dx: 0.0, dy: 0.0, collapsed: false },
        notes: "Hero portraits, souls, clock. Layout lives on the CitadelHudTopBar rule in citadel_hud_top_bar.vcss_c (full height, fit-children width); the box is the 1260px #TeamsContainer strip with an estimated 100px height. Hidden during death replay and in the hideout.",
    },
    ElementSpec {
        id: ElementId::Minimap,
        label: "Minimap",
        selector: "#minimap_persp",
        scale: ScaleProp::PreTransform { origin: "100% 100%" },
        vanilla_ui_scale_pct: 100,
        vanilla: VanillaBox { h: HAlign::Right, v: VAlign::Bottom, width: 440.0, height: 520.0, dx: 0.0, dy: -15.0, collapsed: false },
        notes: "Scales from the bottom-right corner. Sits inside .clamp_width, so on 21:9 it hugs the 1920px clamp, not the screen edge. Vanilla transitions pre-transform-scale2d (hideout uses 0.9), so state rules like .InHideout override the scale.",
    },
    ElementSpec {
        id: ElementId::HealthAndAmmo,
        label: "Health and ammo",
        selector: "#health_and_abilities_container",
        scale: ScaleProp::UiScale,
        vanilla_ui_scale_pct: 120,
        vanilla: VanillaBox { h: HAlign::Center, v: VAlign::Bottom, width: 300.0, height: 456.0, dx: -495.0, dy: -20.0, collapsed: false },
        notes: "250x380 at ui-scale 120%, centred between margin-left 300px and margin-right 1290px; whether ui-scale also scales those margins is unverified, so x is approximate. 21:9 and 16:10 rules change the margins.",
    },
    ElementSpec {
        id: ElementId::AbilitySlots,
        label: "Ability slots",
        selector: "#hud_signature",
        scale: ScaleProp::UiScale,
        vanilla_ui_scale_pct: 90,
        vanilla: VanillaBox { h: HAlign::Center, v: VAlign::Bottom, width: 330.0, height: 200.0, dx: 0.0, dy: 0.0, collapsed: true },
        notes: "Inside #AbilitiesContainer (1140x420, bottom centre). Size is fit-children, estimated from four 220px-tall ability columns at ui-scale 90%. The base rule collapses it; .viewing_as_player and .ShowGoldAndAPOnHud show it. Shrinks to 75% while the shop is open.",
    },
    ElementSpec {
        id: ElementId::ItemSlots,
        label: "Item slots",
        selector: "#ActiveAbilitiesMenu",
        scale: ScaleProp::UiScale,
        vanilla_ui_scale_pct: 100,
        vanilla: VanillaBox { h: HAlign::Center, v: VAlign::Bottom, width: 340.0, height: 100.0, dx: 300.0, dy: 0.0, collapsed: false },
        notes: "Active items 1 to 4. Right-aligned with margin-right 100px inside #AbilitiesContainer (1140px wide, centred), so it ends 330px right of screen centre; the 340px width is an estimate (fit-children).",
    },
    ElementSpec {
        id: ElementId::PassiveItems,
        label: "Passive items",
        selector: "#hud_passive_items",
        scale: ScaleProp::UiScale,
        vanilla_ui_scale_pct: 130,
        vanilla: VanillaBox { h: HAlign::Center, v: VAlign::Bottom, width: 780.0, height: 78.0, dx: 0.0, dy: -322.0, collapsed: true },
        notes: "Collapsed in vanilla; Shown reveals it (what QoL Lite does). 600x60 at ui-scale 130%, top-aligned with a 20px margin inside #AbilitiesContainer, whose top is 420px above the screen bottom. Collapsed again while the shop is open.",
    },
    ElementSpec {
        id: ElementId::PlayerStats,
        label: "Player stats",
        selector: "#StatsAndModsContainer",
        scale: ScaleProp::UiScale,
        vanilla_ui_scale_pct: 100,
        vanilla: VanillaBox { h: HAlign::Left, v: VAlign::Bottom, width: 400.0, height: 360.0, dx: 0.0, dy: 0.0, collapsed: false },
        notes: "Stats, souls and purchased mods in the lower left. The container itself is full screen; the box is an estimate of #LowerLeft's content (#hudPlayerStats is 300px wide). Hidden while editing builds and in spectator mode.",
    },
    ElementSpec {
        id: ElementId::AmmoCounter,
        label: "Ammo counter",
        selector: "#ammo_panel",
        scale: ScaleProp::UiScale,
        vanilla_ui_scale_pct: 100,
        vanilla: VanillaBox { h: HAlign::Center, v: VAlign::Center, width: 120.0, height: 40.0, dx: 0.0, dy: 80.0, collapsed: false },
        notes: "Next to the crosshair. Defined in ability_hud_elements/element_gun.xml (y: 80px, centred), not in hud.xml; whether a hud.vcss_c rule reaches it is unverified, it may need element_gun.vcss_c. Size is an estimate.",
    },
    ElementSpec {
        id: ElementId::KillFeed,
        label: "Kill feed",
        selector: "#DataFeed",
        scale: ScaleProp::UiScale,
        vanilla_ui_scale_pct: 100,
        vanilla: VanillaBox { h: HAlign::Left, v: VAlign::Top, width: 1000.0, height: 240.0, dx: 0.0, dy: 200.0, collapsed: false },
        notes: "HudDataFeed root is full screen; the box is #EventFeed (1000px wide, 200px from the top, from hud_data_feed.vcss_c) with an estimated height. Hidden during death replay.",
    },
    ElementSpec {
        id: ElementId::Chat,
        label: "Chat",
        selector: "#Chat",
        scale: ScaleProp::UiScale,
        vanilla_ui_scale_pct: 100,
        vanilla: VanillaBox { h: HAlign::Center, v: VAlign::Bottom, width: 350.0, height: 240.0, dx: 0.0, dy: -200.0, collapsed: false },
        notes: "350px wide (chat.vcss_c), 200px above the bottom; height is the 200px message area plus an estimated input row.",
    },
];

pub fn spec(id: ElementId) -> &'static ElementSpec {
    ELEMENTS.iter().find(|s| s.id == id).expect("every ElementId has a row in ELEMENTS")
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
            assert!(s.selector.starts_with('#'), "{:?} targets a hud.xml id", s.id);
        }
    }
}
