//! The user's HUD layout and its compilation to CSS.

use std::collections::BTreeMap;

use super::css::CssError;
use super::elements::{self, ElementId, ElementSpec, HAlign, ScaleProp, VAlign, ELEMENTS, HUD_STYLE};

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
    ExtraCss(String, CssError),
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
    let mut files: BTreeMap<String, String> = BTreeMap::new();
    for (&id, edit) in &layout.elements {
        validate(id, edit)?;
        let decls = declarations(edit, elements::spec(id));
        if decls.is_empty() {
            continue;
        }
        let rule = files.entry(HUD_STYLE.to_string()).or_default();
        rule.push_str(elements::spec(id).selector);
        rule.push('{');
        for (prop, value) in &decls {
            rule.push_str(prop);
            rule.push(':');
            rule.push_str(value);
            rule.push(';');
        }
        rule.push('}');
    }
    for (path, css) in &layout.extra_css {
        check_balanced(css).map_err(|e| LayoutError::ExtraCss(path.clone(), e))?;
        let css = css.trim();
        if !css.is_empty() {
            files.entry(path.clone()).or_default().push_str(css);
        }
    }
    Ok(StylePatch { files })
}

fn validate(id: ElementId, edit: &ElementEdit) -> Result<(), LayoutError> {
    for offset in [edit.offset_x, edit.offset_y] {
        if !(-OFFSET_LIMIT..=OFFSET_LIMIT).contains(&offset) {
            return Err(LayoutError::Offset(id, offset));
        }
    }
    if !SCALE_RANGE.contains(&edit.scale_pct) {
        return Err(LayoutError::Scale(id, edit.scale_pct));
    }
    if edit.opacity_pct > 100 {
        return Err(LayoutError::Opacity(id, edit.opacity_pct));
    }
    Ok(())
}

/// Brace balance outside comments and strings, so a stray `}` cannot close
/// a vanilla rule early or leave ours open.
fn check_balanced(css: &str) -> Result<(), CssError> {
    let bytes = css.as_bytes();
    let mut open: Vec<usize> = Vec::new();
    let mut quote: Option<u8> = None;
    let mut i = 0;
    while i < bytes.len() {
        let b = bytes[i];
        match quote {
            Some(_) if b == b'\\' => i += 1,
            Some(q) if b == q => quote = None,
            Some(_) => {}
            None => match b {
                b'"' | b'\'' => quote = Some(b),
                b'/' if bytes.get(i + 1) == Some(&b'*') => {
                    let close = css[i + 2..].find("*/").ok_or(CssError::OpenComment)?;
                    i += close + 3;
                }
                b'{' => open.push(i),
                b'}' => {
                    open.pop().ok_or(CssError::Unbalanced(i))?;
                }
                _ => {}
            },
        }
        i += 1;
    }
    match open.pop() {
        Some(pos) => Err(CssError::Unbalanced(pos)),
        None => Ok(()),
    }
}

/// CSS declarations for one edit on its element. Empty for identity.
pub fn declarations(edit: &ElementEdit, spec: &ElementSpec) -> Vec<(&'static str, String)> {
    let mut decls = Vec::new();
    match edit.visibility {
        Visibility::Vanilla => {}
        Visibility::Hidden => decls.push(("visibility", "collapse".to_string())),
        Visibility::Shown => decls.push(("visibility", "visible".to_string())),
    }
    if edit.offset_x != 0 || edit.offset_y != 0 {
        decls.push((
            "transform",
            format!("translateX({}px) translateY({}px)", edit.offset_x, edit.offset_y),
        ));
    }
    if edit.scale_pct != 100 {
        match spec.scale {
            ScaleProp::UiScale => {
                let pct = f64::from(spec.vanilla_ui_scale_pct) * f64::from(edit.scale_pct) / 100.0;
                decls.push(("ui-scale", format!("{}%", number(pct))));
            }
            ScaleProp::PreTransform { origin } => {
                decls.push(("pre-transform-scale2d", number(f64::from(edit.scale_pct) / 100.0)));
                decls.push(("transform-origin", origin.to_string()));
            }
        }
    }
    if edit.opacity_pct != 100 {
        decls.push(("opacity", number(f64::from(edit.opacity_pct) / 100.0)));
    }
    decls
}

/// Shortest decimal form: 0.5, 2, 22.5.
fn number(value: f64) -> String {
    let text = format!("{value:.4}");
    text.trim_end_matches('0').trim_end_matches('.').to_string()
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
    let k = screen[1] / 1080.0;
    let ref_width = screen[0] / k;
    ELEMENTS
        .iter()
        .map(|spec| {
            let edit = layout.elements.get(&spec.id).cloned().unwrap_or_default();
            let b = spec.vanilla;
            let x = match b.h {
                HAlign::Left => 0.0,
                HAlign::Center => (ref_width - b.width) / 2.0,
                HAlign::Right => ref_width - b.width,
            } + b.dx
                + edit.offset_x as f32;
            let y = match b.v {
                VAlign::Top => 0.0,
                VAlign::Center => (1080.0 - b.height) / 2.0,
                VAlign::Bottom => 1080.0 - b.height,
            } + b.dy
                + edit.offset_y as f32;
            let (fx, fy) = scale_origin(spec);
            let s = f32::from(edit.scale_pct) / 100.0;
            let (w, h) = (b.width * s, b.height * s);
            let x = x + (b.width - w) * fx;
            let y = y + (b.height - h) * fy;
            PreviewRect {
                id: spec.id,
                rect: [x * k, y * k, w * k, h * k],
                visible: !(edit.visibility == Visibility::Hidden
                    || (b.collapsed && edit.visibility != Visibility::Shown)),
                opacity: f32::from(edit.opacity_pct) / 100.0,
            }
        })
        .collect()
}

/// Fixed point of a scale, as fractions of the box. `ui-scale` re-lays the panel
/// out, so its aligned edge stays put; `pre-transform-scale2d` pivots on `origin`.
fn scale_origin(spec: &ElementSpec) -> (f32, f32) {
    match spec.scale {
        ScaleProp::UiScale => (
            match spec.vanilla.h {
                HAlign::Left => 0.0,
                HAlign::Center => 0.5,
                HAlign::Right => 1.0,
            },
            match spec.vanilla.v {
                VAlign::Top => 0.0,
                VAlign::Center => 0.5,
                VAlign::Bottom => 1.0,
            },
        ),
        ScaleProp::PreTransform { origin } => {
            let mut parts = origin.split_whitespace().map(|p| {
                p.strip_suffix('%').and_then(|n| n.parse::<f32>().ok()).map_or(0.5, |n| n / 100.0)
            });
            (parts.next().unwrap_or(0.5), parts.next().unwrap_or(0.5))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn layout(edits: &[(ElementId, ElementEdit)]) -> HudLayout {
        HudLayout { elements: edits.iter().cloned().collect(), extra_css: BTreeMap::new() }
    }

    fn edit() -> ElementEdit {
        ElementEdit::default()
    }

    fn hud_css(layout: &HudLayout) -> String {
        compile(layout).expect("valid layout").files.get(HUD_STYLE).cloned().unwrap_or_default()
    }

    #[test]
    fn identity_compiles_to_nothing() {
        assert!(compile(&HudLayout::default()).expect("valid").files.is_empty());
        let all_default = layout(&[(ElementId::Minimap, edit()), (ElementId::Chat, edit())]);
        assert!(all_default.is_vanilla());
        assert!(compile(&all_default).expect("valid").files.is_empty());
    }

    #[test]
    fn each_property_emits_its_declaration() {
        let chat = elements::spec(ElementId::Chat);
        let cases: [(ElementEdit, &[(&str, &str)]); 7] = [
            (ElementEdit { visibility: Visibility::Hidden, ..edit() }, &[("visibility", "collapse")]),
            (ElementEdit { visibility: Visibility::Shown, ..edit() }, &[("visibility", "visible")]),
            (ElementEdit { offset_x: 12, ..edit() }, &[("transform", "translateX(12px) translateY(0px)")]),
            (ElementEdit { offset_y: -40, ..edit() }, &[("transform", "translateX(0px) translateY(-40px)")]),
            (ElementEdit { scale_pct: 150, ..edit() }, &[("ui-scale", "150%")]),
            (ElementEdit { opacity_pct: 50, ..edit() }, &[("opacity", "0.5")]),
            (ElementEdit { opacity_pct: 5, ..edit() }, &[("opacity", "0.05")]),
        ];
        for (e, want) in cases {
            let got = declarations(&e, chat);
            let got: Vec<(&str, &str)> = got.iter().map(|(p, v)| (*p, v.as_str())).collect();
            assert_eq!(got, want, "{e:?}");
        }
        assert!(declarations(&edit(), chat).is_empty());
    }

    #[test]
    fn ui_scale_is_relative_to_vanilla() {
        let health = elements::spec(ElementId::HealthAndAmmo);
        let half = ElementEdit { scale_pct: 50, ..edit() };
        assert_eq!(declarations(&half, health), vec![("ui-scale", "60%".to_string())]);
        let sig = elements::spec(ElementId::AbilitySlots);
        let quarter = ElementEdit { scale_pct: 25, ..edit() };
        assert_eq!(declarations(&quarter, sig), vec![("ui-scale", "22.5%".to_string())]);
    }

    #[test]
    fn minimap_rule_text() {
        let l = layout(&[(
            ElementId::Minimap,
            ElementEdit { offset_x: -20, offset_y: 10, scale_pct: 80, opacity_pct: 75, ..edit() },
        )]);
        assert_eq!(
            hud_css(&l),
            "#minimap_persp{transform:translateX(-20px) translateY(10px);pre-transform-scale2d:0.8;transform-origin:100% 100%;opacity:0.75;}"
        );
    }

    #[test]
    fn top_bar_rule_text() {
        let l = layout(&[(ElementId::TopBar, ElementEdit { visibility: Visibility::Hidden, scale_pct: 90, ..edit() })]);
        assert_eq!(hud_css(&l), "#TopBar{visibility:collapse;ui-scale:90%;}");
    }

    #[test]
    fn rules_sorted_by_element_and_deterministic() {
        let l = layout(&[
            (ElementId::Chat, ElementEdit { opacity_pct: 0, ..edit() }),
            (ElementId::TopBar, ElementEdit { offset_y: 30, ..edit() }),
            (ElementId::Minimap, ElementEdit { scale_pct: 120, ..edit() }),
        ]);
        let css = hud_css(&l);
        assert_eq!(
            css,
            "#TopBar{transform:translateX(0px) translateY(30px);}\
             #minimap_persp{pre-transform-scale2d:1.2;transform-origin:100% 100%;}\
             #Chat{opacity:0;}"
        );
        assert_eq!(compile(&l), compile(&l.clone()));
    }

    #[test]
    fn validation_errors() {
        let cases = [
            (ElementEdit { offset_x: OFFSET_LIMIT + 1, ..edit() }, LayoutError::Offset(ElementId::Chat, OFFSET_LIMIT + 1)),
            (ElementEdit { offset_y: -OFFSET_LIMIT - 1, ..edit() }, LayoutError::Offset(ElementId::Chat, -OFFSET_LIMIT - 1)),
            (ElementEdit { scale_pct: 24, ..edit() }, LayoutError::Scale(ElementId::Chat, 24)),
            (ElementEdit { scale_pct: 301, ..edit() }, LayoutError::Scale(ElementId::Chat, 301)),
            (ElementEdit { opacity_pct: 101, ..edit() }, LayoutError::Opacity(ElementId::Chat, 101)),
        ];
        for (e, want) in cases {
            assert_eq!(compile(&layout(&[(ElementId::Chat, e)])), Err(want));
        }
        let edge = ElementEdit { offset_x: OFFSET_LIMIT, offset_y: -OFFSET_LIMIT, scale_pct: 300, opacity_pct: 0, ..edit() };
        assert!(compile(&layout(&[(ElementId::Chat, edge)])).is_ok());
    }

    #[test]
    fn extra_css_appended_per_file() {
        let mut l = layout(&[(ElementId::Chat, ElementEdit { opacity_pct: 50, ..edit() })]);
        l.extra_css.insert(HUD_STYLE.to_string(), "  #hud_signature{wash-color:red;}\n".to_string());
        l.extra_css.insert("panorama/styles/chat.vcss_c".to_string(), "CitadelChat{width:400px;}".to_string());
        l.extra_css.insert("panorama/styles/empty.vcss_c".to_string(), "   ".to_string());
        let patch = compile(&l).expect("valid");
        assert_eq!(patch.files[HUD_STYLE], "#Chat{opacity:0.5;}#hud_signature{wash-color:red;}");
        assert_eq!(patch.files["panorama/styles/chat.vcss_c"], "CitadelChat{width:400px;}");
        assert!(!patch.files.contains_key("panorama/styles/empty.vcss_c"));
    }

    #[test]
    fn extra_css_unbalanced() {
        let cases = [
            ("#a{x:y;", CssError::Unbalanced(2)),
            ("#a{x:y;}}", CssError::Unbalanced(8)),
            ("#a{content:\"}\";}/* { */", CssError::Unbalanced(0)),
            ("#a{x:y;} /* open", CssError::OpenComment),
        ];
        for (i, (css, want)) in cases.into_iter().enumerate() {
            let mut l = HudLayout::default();
            l.extra_css.insert(HUD_STYLE.to_string(), css.to_string());
            let got = compile(&l);
            if i == 2 {
                assert!(got.is_ok(), "braces in strings and comments are ignored");
            } else {
                assert_eq!(got, Err(LayoutError::ExtraCss(HUD_STYLE.to_string(), want)), "{css}");
            }
        }
    }

    #[test]
    fn preview_vanilla_matches_table_at_1080p() {
        let rects = preview(&HudLayout::default(), [1920.0, 1080.0]);
        assert_eq!(rects.len(), ELEMENTS.len());
        for (r, spec) in rects.iter().zip(ELEMENTS) {
            let b = spec.vanilla;
            let x = match b.h {
                HAlign::Left => b.dx,
                HAlign::Center => (1920.0 - b.width) / 2.0 + b.dx,
                HAlign::Right => 1920.0 - b.width + b.dx,
            };
            let y = match b.v {
                VAlign::Top => b.dy,
                VAlign::Center => (1080.0 - b.height) / 2.0 + b.dy,
                VAlign::Bottom => 1080.0 - b.height + b.dy,
            };
            assert_eq!(r.id, spec.id);
            assert_eq!(r.rect, [x, y, b.width, b.height], "{:?}", spec.id);
            assert_eq!(r.visible, !b.collapsed);
            assert_eq!(r.opacity, 1.0);
        }
        let minimap = rects.iter().find(|r| r.id == ElementId::Minimap).expect("row");
        assert_eq!(minimap.rect, [1480.0, 545.0, 440.0, 520.0]);
    }

    #[test]
    fn preview_scales_with_screen_height() {
        let l = layout(&[(ElementId::Chat, ElementEdit { offset_x: 30, offset_y: -60, scale_pct: 150, ..edit() })]);
        let base = preview(&l, [1920.0, 1080.0]);
        let big = preview(&l, [2560.0, 1440.0]);
        for (a, b) in base.iter().zip(&big) {
            for (u, v) in a.rect.iter().zip(b.rect) {
                assert!((u * 4.0 / 3.0 - v).abs() < 1e-3, "{:?}: {:?} vs {:?}", a.id, a.rect, b.rect);
            }
        }
    }

    #[test]
    fn preview_applies_edits() {
        let l = layout(&[
            (ElementId::Minimap, ElementEdit { scale_pct: 50, opacity_pct: 40, ..edit() }),
            (ElementId::Chat, ElementEdit { offset_x: 30, offset_y: -60, scale_pct: 200, ..edit() }),
            (ElementId::TopBar, ElementEdit { visibility: Visibility::Hidden, ..edit() }),
            (ElementId::PassiveItems, ElementEdit { visibility: Visibility::Shown, ..edit() }),
        ]);
        let rects = preview(&l, [1920.0, 1080.0]);
        let get = |id| rects.iter().find(|r| r.id == id).copied().expect("row");
        assert_eq!(get(ElementId::Minimap).rect, [1700.0, 805.0, 220.0, 260.0], "bottom-right corner fixed");
        assert_eq!(get(ElementId::Minimap).opacity, 0.4);
        assert_eq!(get(ElementId::Chat).rect, [640.0, 340.0, 700.0, 480.0], "bottom-centre anchor fixed");
        assert!(!get(ElementId::TopBar).visible);
        assert!(get(ElementId::PassiveItems).visible);
        assert!(!get(ElementId::AbilitySlots).visible);
    }

    #[test]
    fn toml_round_trip_uses_snake_case_tables() {
        let mut l = layout(&[
            (ElementId::Minimap, ElementEdit { scale_pct: 80, ..edit() }),
            (ElementId::HealthAndAmmo, ElementEdit { visibility: Visibility::Hidden, offset_x: -5, ..edit() }),
        ]);
        l.extra_css.insert(HUD_STYLE.to_string(), "#Chat{opacity:0.5;}".to_string());
        let text = toml::to_string(&l).expect("serialize");
        assert!(text.contains("[elements.minimap]"), "{text}");
        assert!(text.contains("[elements.health_and_ammo]"), "{text}");
        assert!(text.contains("visibility = \"hidden\""), "{text}");
        let back: HudLayout = toml::from_str(&text).expect("parse");
        assert_eq!(back, l);

        let partial: HudLayout = toml::from_str("[elements.chat]\nopacity_pct = 30\n").expect("parse partial");
        assert_eq!(partial.elements[&ElementId::Chat], ElementEdit { opacity_pct: 30, ..edit() });
    }
}
