//! The user's HUD layout and its compilation to CSS.

use std::collections::BTreeMap;

use super::elements::{ElementId, ScaleProp};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Visibility {
    #[default]
    Vanilla,
    Hidden,
    /// Force-show a panel vanilla collapses (e.g. passive item slots).
    Shown,
}

/// One element's edit. `Default` is the identity (no CSS emitted).
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct ElementEdit {
    pub visibility: Visibility,
    /// Reference px at 1080p, x right, y down.
    pub offset_x: i32,
    pub offset_y: i32,
    /// Percent, 100 = vanilla.
    pub scale_pct: u16,
    /// Percent, 100 = vanilla.
    pub opacity_pct: u8,
}

impl Default for ElementEdit {
    fn default() -> Self {
        ElementEdit { visibility: Visibility::Vanilla, offset_x: 0, offset_y: 0, scale_pct: 100, opacity_pct: 100 }
    }
}

pub const OFFSET_LIMIT: i32 = 1920;
pub const SCALE_RANGE: std::ops::RangeInclusive<u16> = 25..=300;

/// Stored next to a profile (`[hud]` in profile TOML, or `hud.toml`).
#[derive(Clone, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct HudLayout {
    pub elements: BTreeMap<ElementId, ElementEdit>,
    /// Advanced: raw CSS appended after the generated rules, keyed by style file path
    /// (`panorama/styles/hud.vcss_c`). Must parse with balanced braces.
    pub extra_css: BTreeMap<String, String>,
}

/// Minified CSS to append, per compiled style file inside the game VPK.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct StylePatch {
    pub files: BTreeMap<String, String>,
}

impl StylePatch {
    pub fn is_empty(&self) -> bool {
        self.files.values().all(|css| css.is_empty())
    }
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum LayoutError {
    #[error("{0:?}: offset {1} outside +-{OFFSET_LIMIT}")]
    Offset(ElementId, i32),
    #[error("{0:?}: scale {1}% outside 25..=300")]
    Scale(ElementId, u16),
    #[error("{0:?}: opacity {1}% above 100")]
    Opacity(ElementId, u8),
    #[error("extra css for {0}: {1}")]
    ExtraCss(String, super::css::CssError),
}

impl HudLayout {
    pub fn is_vanilla(&self) -> bool {
        self.elements.values().all(|e| *e == ElementEdit::default())
            && self.extra_css.values().all(|c| c.trim().is_empty())
    }
}

/// Validates and emits one rule per non-identity element (sorted by `ElementId`),
/// then the minified extra CSS. Deterministic.
pub fn compile(layout: &HudLayout) -> Result<StylePatch, LayoutError> {
    todo!()
}

/// CSS declarations for one edit, given how its element scales. Empty for identity.
pub fn declarations(edit: &ElementEdit, scale: ScaleProp) -> Vec<(&'static str, String)> {
    todo!()
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PreviewRect {
    pub id: ElementId,
    /// x, y, width, height in screen px.
    pub rect: [f32; 4],
    pub visible: bool,
    pub opacity: f32,
}

/// Where each element would land on a `screen` sized canvas (Panorama scales the
/// 1080p reference by screen height). For the GUI's drag-to-place editor.
pub fn preview(layout: &HudLayout, screen: [f32; 2]) -> Vec<PreviewRect> {
    todo!()
}
