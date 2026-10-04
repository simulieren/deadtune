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
    pub vanilla: VanillaBox,
    /// Shown in the editor (e.g. which game states override the edit).
    pub notes: &'static str,
}

pub static ELEMENTS: &[ElementSpec] = &[];

pub fn spec(id: ElementId) -> &'static ElementSpec {
    ELEMENTS.iter().find(|s| s.id == id).expect("every ElementId has a row in ELEMENTS")
}
