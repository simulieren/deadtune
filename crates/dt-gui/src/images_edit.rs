//! Colour edits on the UI images page without a window: the slider draft, undo and redo,
//! multi-select and bulk edits. Every change lands in `profile.hud.icons` through
//! `dt_core::hud::icons`, so it ships with Apply and reverts with Discard.
//!
//! A slider writes into a [`Draft`] that the preview draws from; the profile only takes it
//! once the slider rests for [`SETTLE`], because `refresh_preview` rebuilds the whole HUD
//! pak and is far too slow to run every frame.

use std::collections::{BTreeMap, BTreeSet};
use std::time::{Duration, Instant};

use dt_core::hud::icons::{self, IconOverride, Source};
use dt_core::texture::adjust::{self, Adjust, AdjustKind, Blend, Rgb};
use dt_core::texture::svg::{self, Palette};

use crate::images::Picture;
use crate::state::AppState;

pub const SETTLE: Duration = Duration::from_millis(250);
const HISTORY_CAP: usize = 100;
pub const AMBER: Rgb = Rgb([0xd4, 0x86, 0x0b]);

type Icons = BTreeMap<String, IconOverride>;

/// Which colour operation the panel's chip row edits.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ColorMode {
    #[default]
    Tint,
    Colorize,
    Overlay,
}

impl ColorMode {
    pub const ALL: [ColorMode; 3] = [ColorMode::Tint, ColorMode::Colorize, ColorMode::Overlay];

    pub fn label(self) -> &'static str {
        match self {
            ColorMode::Tint => "Tint",
            ColorMode::Colorize => "Colorize",
            ColorMode::Overlay => "Overlay",
        }
    }

    /// The strength a colour pick starts at when the slider sits at zero.
    fn default_strength(self) -> u8 {
        match self {
            ColorMode::Tint | ColorMode::Colorize => 100,
            ColorMode::Overlay => 50,
        }
    }
}

/// The list the sliders are editing for one image, ahead of the profile.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Draft {
    pub path: String,
    pub adjust: Vec<Adjust>,
    pub changed: Instant,
    /// Whether this draft already put its undo snapshot on the stack.
    pub pushed: bool,
}

/// Snapshots of `profile.hud.icons` from before each page edit.
#[derive(Clone, Debug, Default)]
pub struct History {
    pub undo: Vec<Icons>,
    pub redo: Vec<Icons>,
}

impl History {
    fn record(&mut self, before: Icons) {
        self.undo.push(before);
        if self.undo.len() > HISTORY_CAP {
            self.undo.remove(0);
        }
        self.redo.clear();
    }
}

pub struct EditState {
    pub marked: BTreeSet<String>,
    /// Where a shift-click range starts.
    pub anchor: Option<String>,
    pub draft: Option<Draft>,
    pub history: History,
    /// The colours of the last vector icon asked about, by game path.
    pub palette: Option<(String, Palette)>,
    pub mode: ColorMode,
    pub color: Rgb,
    pub strength: u8,
    pub blend: Blend,
}

impl Default for EditState {
    fn default() -> EditState {
        EditState {
            marked: BTreeSet::new(),
            anchor: None,
            draft: None,
            history: History::default(),
            palette: None,
            mode: ColorMode::Tint,
            color: AMBER,
            strength: 0,
            blend: Blend::Normal,
        }
    }
}

impl EditState {
    /// Drops the draft and both stacks, for when the profile is replaced wholesale.
    pub fn forget(&mut self) {
        self.draft = None;
        self.history = History::default();
        self.palette = None;
    }
}

/// `tint:#ff0000:80;hue:30` as a list.
pub fn parse_specs(text: &str) -> Result<Vec<Adjust>, String> {
    text.split(';')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(Adjust::parse)
        .collect()
}

impl AppState {
    /// The list shown for `path`: the draft's while the sliders hold one, else the profile's.
    pub fn shown_adjustments(&self, path: &str) -> &[Adjust] {
        match &self.images.edit.draft {
            Some(draft) if draft.path == path => &draft.adjust,
            _ => self.saved_adjustments(path),
        }
    }

    fn saved_adjustments(&self, path: &str) -> &[Adjust] {
        self.image_override(path).map_or(&[], |o| &o.adjust)
    }

    fn draft_waiting(&self) -> bool {
        self.images
            .edit
            .draft
            .as_ref()
            .is_some_and(|d| d.adjust != self.saved_adjustments(&d.path))
    }

    /// One slider move: puts `adjust` in `path`'s draft, starting the draft from the profile.
    pub fn edit_image(&mut self, path: &str, adjust: Adjust, now: Instant) {
        if icons::target(path).is_err() {
            return;
        }
        if self
            .images
            .edit
            .draft
            .as_ref()
            .is_some_and(|d| d.path != path)
        {
            self.commit_edits();
        }
        let start = self.saved_adjustments(path).to_vec();
        let draft = self.images.edit.draft.get_or_insert_with(|| Draft {
            path: path.to_string(),
            adjust: start,
            changed: now,
            pushed: false,
        });
        if adjust::set(&mut draft.adjust, adjust) {
            draft.changed = now;
        }
    }

    /// Commits a draft that has rested for [`SETTLE`]; true while one is still waiting.
    pub fn settle_edits(&mut self, now: Instant) -> bool {
        let Some(changed) = self.images.edit.draft.as_ref().map(|d| d.changed) else {
            return false;
        };
        if !self.draft_waiting() {
            return false;
        }
        if now.saturating_duration_since(changed) < SETTLE {
            return true;
        }
        self.write_draft();
        false
    }

    /// Commits the draft now and ends it. Every other page edit calls this first.
    pub fn commit_edits(&mut self) {
        self.write_draft();
        self.images.edit.draft = None;
    }

    fn write_draft(&mut self) {
        if !self.draft_waiting() {
            return;
        }
        let Some(draft) = self.images.edit.draft.as_mut() else {
            return;
        };
        if !draft.pushed {
            draft.pushed = true;
            self.images
                .edit
                .history
                .record(self.profile.hud.icons.clone());
        }
        let (path, list) = (draft.path.clone(), draft.adjust.clone());
        let _ = icons::set_adjustments(&mut self.profile.hud.icons, &path, &list);
        self.refresh_preview();
    }

    /// Commits any draft and returns the overrides as they stand, for [`Self::end_edit`].
    pub(crate) fn begin_edit(&mut self) -> Icons {
        self.commit_edits();
        self.profile.hud.icons.clone()
    }

    /// Records `before` for undo when the overrides changed since, and rebuilds the preview.
    pub(crate) fn end_edit(&mut self, before: Icons) -> bool {
        if before == self.profile.hud.icons {
            return false;
        }
        self.images.edit.history.record(before);
        self.images.edit.palette = None;
        self.refresh_preview();
        true
    }

    pub fn remove_edit(&mut self, path: &str, kind: AdjustKind) {
        let before = self.begin_edit();
        icons::remove_adjust(&mut self.profile.hud.icons, path, kind);
        self.end_edit(before);
        self.sync_color_panel();
    }

    /// One edit for many images; each keeps its own source and other adjustments.
    pub fn adjust_images(&mut self, paths: &[String], adjust: Adjust) {
        let before = self.begin_edit();
        for path in paths {
            let _ = icons::adjust(&mut self.profile.hud.icons, path, adjust);
        }
        self.end_edit(before);
        self.sync_color_panel();
    }

    pub fn reset_images(&mut self, paths: &[String]) {
        let before = self.begin_edit();
        for path in paths {
            icons::reset(&mut self.profile.hud.icons, path);
        }
        if self.end_edit(before) {
            self.images.notice = None;
        }
        self.sync_color_panel();
    }

    pub fn can_undo(&self) -> bool {
        !self.images.edit.history.undo.is_empty() || self.draft_waiting()
    }

    pub fn can_redo(&self) -> bool {
        !self.images.edit.history.redo.is_empty()
    }

    pub fn undo_images(&mut self) {
        self.commit_edits();
        if let Some(previous) = self.images.edit.history.undo.pop() {
            let current = std::mem::replace(&mut self.profile.hud.icons, previous);
            self.images.edit.history.redo.push(current);
            self.after_history_move();
        }
    }

    pub fn redo_images(&mut self) {
        self.commit_edits();
        if let Some(next) = self.images.edit.history.redo.pop() {
            let current = std::mem::replace(&mut self.profile.hud.icons, next);
            self.images.edit.history.undo.push(current);
            self.after_history_move();
        }
    }

    fn after_history_move(&mut self) {
        self.images.edit.palette = None;
        self.images.notice = None;
        self.refresh_preview();
        self.sync_color_panel();
    }

    /// A tile click: plain selects, ctrl (cmd on a Mac) toggles a mark, shift marks the run
    /// from the anchor to `path` in `visible` order.
    pub fn mark_image(&mut self, path: &str, ctrl: bool, shift: bool, visible: &[String]) {
        let from = self
            .images
            .edit
            .anchor
            .clone()
            .or_else(|| self.images.selected.clone());
        if shift && let Some(from) = from {
            let at = |p: &str| visible.iter().position(|v| v == p);
            if let (Some(a), Some(b)) = (at(&from), at(path)) {
                let (lo, hi) = (a.min(b), a.max(b));
                self.images.edit.marked = visible[lo..=hi].iter().cloned().collect();
                return;
            }
        }
        if ctrl {
            let marked = &mut self.images.edit.marked;
            if marked.is_empty()
                && let Some(selected) = &self.images.selected
            {
                marked.insert(selected.clone());
            }
            if !marked.remove(path) {
                marked.insert(path.to_string());
            }
            self.images.edit.anchor = Some(path.to_string());
            return;
        }
        self.images.edit.marked.clear();
        self.images.edit.anchor = Some(path.to_string());
        self.select_image(Some(path.to_string()));
    }

    /// What a bulk edit on "selected" means: the marked tiles, else the selected one.
    pub fn marked_or_selected(&self) -> Vec<String> {
        if self.images.edit.marked.is_empty() {
            self.images.selected.iter().cloned().collect()
        } else {
            self.images.edit.marked.iter().cloned().collect()
        }
    }

    /// The colour operation the panel's chips and sliders describe.
    pub fn current_color_op(&self) -> Adjust {
        let e = &self.images.edit;
        match e.mode {
            ColorMode::Tint => Adjust::Tint {
                color: e.color,
                strength: e.strength,
            },
            ColorMode::Colorize => Adjust::Colorize {
                color: e.color,
                strength: e.strength,
            },
            ColorMode::Overlay => Adjust::Overlay {
                color: e.color,
                blend: e.blend,
                opacity: e.strength,
            },
        }
    }

    fn edit_selected_color(&mut self, now: Instant) {
        if let Some(path) = self.images.selected.clone() {
            self.edit_image(&path, self.current_color_op(), now);
        }
    }

    pub fn set_color_mode(&mut self, mode: ColorMode, now: Instant) {
        self.images.edit.mode = mode;
        self.edit_selected_color(now);
    }

    pub fn set_color(&mut self, color: Rgb, now: Instant) {
        let e = &mut self.images.edit;
        e.color = color;
        if e.strength == 0 {
            e.strength = e.mode.default_strength();
        }
        self.edit_selected_color(now);
    }

    pub fn set_strength(&mut self, strength: u8, now: Instant) {
        self.images.edit.strength = strength.min(100);
        self.edit_selected_color(now);
    }

    pub fn set_blend(&mut self, blend: Blend, now: Instant) {
        self.images.edit.blend = blend;
        self.edit_selected_color(now);
    }

    /// Points the colour chips and sliders at the selected image's colour operation, or
    /// leaves the colour and shows zero strength when it has none.
    pub fn sync_color_panel(&mut self) {
        let op = self.images.selected.as_deref().and_then(|p| {
            self.shown_adjustments(p)
                .iter()
                .find(|a| a.kind() == AdjustKind::Color)
                .copied()
        });
        let e = &mut self.images.edit;
        match op {
            Some(Adjust::Tint { color, strength }) => {
                (e.mode, e.color, e.strength) = (ColorMode::Tint, color, strength);
            }
            Some(Adjust::Colorize { color, strength }) => {
                (e.mode, e.color, e.strength) = (ColorMode::Colorize, color, strength);
            }
            Some(Adjust::Overlay {
                color,
                blend,
                opacity,
            }) => {
                (e.mode, e.color, e.strength, e.blend) =
                    (ColorMode::Overlay, color, opacity, blend);
            }
            _ => e.strength = 0,
        }
    }

    /// The SVG text an icon's adjustments apply to: the player's SVG, else the game's.
    fn source_svg(&self, path: &str) -> Option<String> {
        if !path.ends_with(".vsvg_c") {
            return None;
        }
        match self.image_override(path).map(|o| &o.source) {
            Some(Source::Svg { .. }) => std::fs::read_to_string(self.stored_image(path)?).ok(),
            Some(Source::PngInSvg { .. } | Source::Png { .. }) => None,
            Some(Source::Game) | None => {
                let bytes = self.image_library()?.source.read(path).ok()?;
                svg::svg_text(&bytes).ok()
            }
        }
    }

    /// The colours a vector icon's source names, read once per path.
    pub fn palette_for(&mut self, path: &str) -> Option<&Palette> {
        if !path.ends_with(".vsvg_c") {
            return None;
        }
        if self
            .images
            .edit
            .palette
            .as_ref()
            .is_none_or(|(p, _)| p != path)
        {
            let palette = self
                .source_svg(path)
                .map(|text| svg::palette(&text))
                .unwrap_or_default();
            self.images.edit.palette = Some((path.to_string(), palette));
        }
        self.images.edit.palette.as_ref().map(|(_, p)| p)
    }

    /// The palette cached for `path`, if [`Self::palette_for`] read it.
    pub fn cached_palette(&self, path: &str) -> Option<&Palette> {
        self.images
            .edit
            .palette
            .as_ref()
            .filter(|(p, _)| p == path)
            .map(|(_, p)| p)
    }

    pub fn swap_color(&mut self, path: &str, from: Rgb, to: Rgb, now: Instant) {
        self.edit_image(path, Adjust::Swap { from, to }, now);
    }

    /// The picture of `path` as the player will see it, or `None` when it is the game's.
    pub fn your_picture(&self, path: &str, side: u32) -> Option<Picture> {
        let drafting = self
            .images
            .edit
            .draft
            .as_ref()
            .is_some_and(|d| d.path == path);
        let file = self.stored_image(path);
        let adjust = self.shown_adjustments(path);
        match (file, adjust.is_empty()) {
            (Some(file), true) => Some(Picture::Mine { file, side }),
            (None, true) => drafting.then(|| Picture::Game {
                path: path.to_string(),
                side,
            }),
            (file, false) => Some(Picture::Edited {
                path: path.to_string(),
                file,
                adjust: adjust.to_vec(),
                side,
            }),
        }
    }

    fn lever_entry(&self, part: &str) -> Option<String> {
        self.image_library()?
            .entries
            .iter()
            .find(|e| e.path.contains(part))
            .map(|e| e.path.clone())
    }

    /// `DEADTUNE_IMAGES_EDIT`: `<part of a path>=<spec>[;<spec>],...`, each undoable.
    pub fn edit_lever(&mut self, value: &str) -> Result<(), String> {
        for item in value.split(',').filter(|s| !s.trim().is_empty()) {
            let (part, specs) = item
                .split_once('=')
                .ok_or_else(|| format!("{item:?} is not <part>=<spec>"))?;
            let path = self
                .lever_entry(part.trim())
                .ok_or_else(|| format!("no image matching {part:?}"))?;
            for adjust in parse_specs(specs)? {
                self.adjust_images(std::slice::from_ref(&path), adjust);
            }
        }
        Ok(())
    }

    /// `DEADTUNE_IMAGES_BULK`: `<spec>[;<spec>]` on every image the filters leave visible.
    pub fn bulk_lever(&mut self, value: &str) -> Result<(), String> {
        let paths = self.visible_paths();
        for adjust in parse_specs(value)? {
            self.adjust_images(&paths, adjust);
        }
        Ok(())
    }

    /// `DEADTUNE_IMAGES_MARK`: `<part>,<part>` marks the first match of each.
    pub fn mark_lever(&mut self, value: &str) {
        for part in value.split(',').map(str::trim).filter(|s| !s.is_empty()) {
            if let Some(path) = self.lever_entry(part) {
                self.images.edit.marked.insert(path);
            }
        }
    }

    /// Game paths of the entries the folder, search and "Changed by me" filters leave.
    pub fn visible_paths(&self) -> Vec<String> {
        self.image_library().map_or_else(Vec::new, |library| {
            self.images
                .visible(library, &self.profile.hud.icons)
                .iter()
                .map(|e| e.path.clone())
                .collect()
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::images::render;
    use crate::images::tests::{ITEM, TEXTURE, TOP_BAR, loaded, my_png};
    use dt_core::hud::install::HudAction;

    const RED: Rgb = Rgb([255, 0, 0]);
    const BLUE_ICON: &str = "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 10 10\"><rect width=\"10\" height=\"5\" fill=\"#ece8e1\"/><rect y=\"5\" width=\"10\" height=\"5\" fill=\"#000000\"/></svg>";

    fn tint(strength: u8) -> Adjust {
        Adjust::Tint {
            color: RED,
            strength,
        }
    }

    fn later(start: Instant, ms: u64) -> Instant {
        start + Duration::from_millis(ms)
    }

    fn list(state: &AppState, path: &str) -> Vec<Adjust> {
        state
            .image_override(path)
            .map(|o| o.adjust.clone())
            .unwrap_or_default()
    }

    fn all() -> Vec<String> {
        [TEXTURE, ITEM].map(String::from).to_vec()
    }

    #[test]
    fn a_slider_move_waits_in_the_draft_until_it_settles() {
        let (_dir, mut state) = loaded();
        let t0 = Instant::now();
        state.edit_image(TEXTURE, tint(40), t0);
        assert_eq!(state.shown_adjustments(TEXTURE), [tint(40)]);
        assert!(state.image_override(TEXTURE).is_none(), "profile untouched");
        assert!(state.settle_edits(later(t0, 100)), "still waiting");
        state.edit_image(TEXTURE, tint(60), later(t0, 200));
        assert!(state.settle_edits(later(t0, 400)), "the clock restarted");
        assert!(state.image_override(TEXTURE).is_none());
        assert!(!state.settle_edits(later(t0, 460)));
        assert_eq!(list(&state, TEXTURE), [tint(60)]);
        assert!(state.is_dirty());
        let hud = state.preview.as_ref().unwrap().hud.as_ref().unwrap();
        assert!(hud.patch.icons.contains_key(TEXTURE), "preview rebuilt");
        assert!(
            !state.settle_edits(later(t0, 900)),
            "nothing left to wait for"
        );
    }

    #[test]
    fn one_draft_is_one_undo_entry() {
        let (_dir, mut state) = loaded();
        let t0 = Instant::now();
        state.edit_image(TEXTURE, tint(40), t0);
        state.settle_edits(later(t0, 300));
        state.edit_image(TEXTURE, tint(70), later(t0, 400));
        state.edit_image(TEXTURE, Adjust::Hue { degrees: 30 }, later(t0, 450));
        state.settle_edits(later(t0, 800));
        state.commit_edits();
        assert_eq!(state.images.edit.history.undo.len(), 1);
        assert_eq!(
            list(&state, TEXTURE),
            [tint(70), Adjust::Hue { degrees: 30 }]
        );
        state.undo_images();
        assert!(state.image_override(TEXTURE).is_none());
        assert!(state.can_redo() && !state.can_undo());
    }

    #[test]
    fn an_identity_move_takes_the_edit_back_out() {
        let (_dir, mut state) = loaded();
        let t0 = Instant::now();
        state.edit_image(TEXTURE, tint(40), t0);
        state.commit_edits();
        state.edit_image(TEXTURE, tint(0), t0);
        state.commit_edits();
        assert!(state.image_override(TEXTURE).is_none(), "vanilla again");
        assert_eq!(state.images.edit.history.undo.len(), 2);
    }

    #[test]
    fn remove_edit_takes_one_kind_off() {
        let (_dir, mut state) = loaded();
        state.edit_image(TEXTURE, tint(40), Instant::now());
        state.edit_image(TEXTURE, Adjust::Invert, Instant::now());
        state.remove_edit(TEXTURE, AdjustKind::Color);
        assert_eq!(
            list(&state, TEXTURE),
            [Adjust::Invert],
            "draft committed first"
        );
        state.remove_edit(TEXTURE, AdjustKind::Invert);
        assert!(state.image_override(TEXTURE).is_none());
        assert_eq!(state.images.edit.history.undo.len(), 3);
    }

    #[test]
    fn bulk_edits_keep_each_images_own_override() {
        let (_dir, mut state) = loaded();
        state.replace_image(ITEM, &my_png(4, 4));
        state.edit_image(TEXTURE, Adjust::Invert, Instant::now());
        state.images.folder = None;
        state.images.search = "stand_in".into();
        assert_eq!(state.visible_paths(), [ITEM]);
        state.images.search.clear();
        state.adjust_images(&all(), tint(50));
        assert_eq!(list(&state, TEXTURE), [Adjust::Invert, tint(50)]);
        assert_eq!(list(&state, ITEM), [tint(50)]);
        assert!(matches!(
            state.image_override(ITEM).unwrap().source,
            Source::Png { .. }
        ));
        assert_eq!(state.image_override(TEXTURE).unwrap().source, Source::Game);

        state.images.edit.marked = BTreeSet::from([ITEM.to_string()]);
        state.adjust_images(&state.marked_or_selected(), Adjust::Hue { degrees: 20 });
        assert_eq!(list(&state, ITEM), [tint(50), Adjust::Hue { degrees: 20 }]);
        assert_eq!(list(&state, TEXTURE), [Adjust::Invert, tint(50)]);
        state.undo_images();
        assert_eq!(list(&state, ITEM), [tint(50)], "one bulk is one undo");
    }

    #[test]
    fn ctrl_toggles_marks_and_shift_marks_a_run() {
        let (_dir, mut state) = loaded();
        let visible = state.visible_paths();
        assert_eq!(visible.len(), 4);
        state.mark_image(&visible[0], false, false, &visible);
        assert_eq!(state.images.selected.as_deref(), Some(visible[0].as_str()));
        assert_eq!(state.marked_or_selected(), [visible[0].clone()]);
        state.mark_image(&visible[2], true, false, &visible);
        assert_eq!(
            state.images.edit.marked,
            BTreeSet::from([visible[0].clone(), visible[2].clone()])
        );
        state.mark_image(&visible[0], true, false, &visible);
        assert_eq!(state.marked_or_selected(), [visible[2].clone()]);
        state.mark_image(&visible[3], false, true, &visible);
        assert_eq!(
            state.images.edit.marked,
            visible.iter().cloned().collect(),
            "from the last ctrl-click"
        );
        assert_eq!(
            state.images.selected.as_deref(),
            Some(visible[0].as_str()),
            "marking leaves the selection"
        );
        state.mark_image(&visible[1], false, false, &visible);
        assert!(state.images.edit.marked.is_empty());
        state.mark_image(&visible[3], false, true, &visible);
        assert_eq!(state.images.edit.marked.len(), 3);
    }

    #[test]
    fn undo_and_redo_walk_through_replace_bulk_and_reset() {
        let (_dir, mut state) = loaded();
        state.replace_image(ITEM, &my_png(4, 4));
        state.adjust_images(&all(), tint(50));
        state.reset_image(ITEM);
        let after_reset = state.profile.hud.icons.clone();
        assert_eq!(state.images.edit.history.undo.len(), 3);
        state.undo_images();
        assert_eq!(list(&state, ITEM), [tint(50)]);
        state.undo_images();
        assert!(state.image_override(TEXTURE).is_none());
        assert!(state.image_override(ITEM).unwrap().adjust.is_empty());
        state.undo_images();
        assert!(state.profile.hud.icons.is_empty());
        assert!(!state.can_undo());
        state.undo_images();
        assert!(state.profile.hud.icons.is_empty(), "nothing more to undo");
        for _ in 0..3 {
            state.redo_images();
        }
        assert_eq!(state.profile.hud.icons, after_reset);
        state.undo_images();
        state.edit_image(TEXTURE, Adjust::Invert, Instant::now());
        state.commit_edits();
        assert!(!state.can_redo(), "a new edit drops the redo stack");
    }

    #[test]
    fn the_stack_keeps_the_last_hundred() {
        let (_dir, mut state) = loaded();
        for degrees in 1..=120 {
            state.adjust_images(&[TEXTURE.to_string()], Adjust::Hue { degrees });
        }
        assert_eq!(state.images.edit.history.undo.len(), 100);
    }

    #[test]
    fn reset_this_folder_leaves_other_folders() {
        let (_dir, mut state) = loaded();
        state.adjust_images(&all(), tint(50));
        state.images.folder = Some("minimap".into());
        state.reset_images(&state.visible_paths());
        assert!(state.image_override(TEXTURE).is_none());
        assert_eq!(list(&state, ITEM), [tint(50)]);
        state.undo_images();
        assert_eq!(list(&state, TEXTURE), [tint(50)]);
    }

    #[test]
    fn resetting_every_image_after_apply_removes_the_pak() {
        let (_dir, mut state) = loaded();
        state.edit_image(TEXTURE, tint(80), Instant::now());
        state.apply_and_save().unwrap();
        assert!(!state.is_dirty(), "apply committed the draft");
        assert_eq!(list(&state, TEXTURE), [tint(80)]);
        let hud = |state: &AppState| state.preview.as_ref().unwrap().hud.clone().unwrap();
        assert!(hud(&state).addon_path.is_file());
        state.reset_all_images();
        assert!(state.profile.hud.icons.is_empty());
        assert_eq!(hud(&state).action, HudAction::Remove);
        assert_eq!(
            state.images.edit.history.undo.len(),
            2,
            "apply keeps history"
        );
    }

    #[test]
    fn discard_drops_the_draft_and_the_history() {
        let (_dir, mut state) = loaded();
        state.adjust_images(&all(), tint(50));
        state.edit_image(TEXTURE, Adjust::Invert, Instant::now());
        state.revert_all();
        assert!(state.profile.hud.icons.is_empty());
        assert!(state.images.edit.draft.is_none());
        assert!(!state.can_undo() && !state.can_redo());
        assert!(state.your_picture(TEXTURE, 64).is_none());
    }

    #[test]
    fn changing_the_selection_commits_the_draft() {
        let (_dir, mut state) = loaded();
        state.select_image(Some(TEXTURE.into()));
        state.set_color(RED, Instant::now());
        assert_eq!(state.images.edit.strength, 100, "a pick starts at full");
        assert!(state.image_override(TEXTURE).is_none());
        state.mark_image(ITEM, false, false, &all());
        assert_eq!(list(&state, TEXTURE), [tint(100)]);
        assert_eq!(state.images.edit.strength, 0, "ITEM has no colour edit");
        state.select_image(Some(TEXTURE.into()));
        assert_eq!(
            (state.images.edit.mode, state.images.edit.strength),
            (ColorMode::Tint, 100)
        );
        state.set_color_mode(ColorMode::Overlay, Instant::now());
        state.set_blend(Blend::Multiply, Instant::now());
        state.set_strength(30, Instant::now());
        assert_eq!(
            state.shown_adjustments(TEXTURE),
            [Adjust::Overlay {
                color: RED,
                blend: Blend::Multiply,
                opacity: 30
            }]
        );
    }

    #[test]
    fn your_picture_follows_the_override_and_the_draft() {
        let (_dir, mut state) = loaded();
        assert_eq!(state.your_picture(ITEM, 64), None);
        state.replace_image(ITEM, &my_png(4, 4));
        let file = state.stored_image(ITEM).unwrap();
        assert_eq!(
            state.your_picture(ITEM, 64),
            Some(Picture::Mine {
                file: file.clone(),
                side: 64
            })
        );
        state.edit_image(ITEM, Adjust::Invert, Instant::now());
        assert_eq!(
            state.your_picture(ITEM, 64),
            Some(Picture::Edited {
                path: ITEM.into(),
                file: Some(file),
                adjust: vec![Adjust::Invert],
                side: 64
            }),
            "the draft shows before it is committed"
        );
        state.edit_image(TEXTURE, tint(100), Instant::now());
        let picture = state.your_picture(TEXTURE, 8).unwrap();
        let source = state.image_library().unwrap().source.clone();
        let shown = render(&source, &picture).unwrap();
        assert_eq!(shown.image.pixel(0, 0), [0, 0, 0, 255], "blue tinted red");
        assert_eq!((shown.facts.width, shown.facts.height), (32, 16));
        let mine = render(&source, &state.your_picture(ITEM, 8).unwrap()).unwrap();
        assert_eq!(mine.image.pixel(0, 0), [0, 255, 255, 255], "red inverted");
    }

    #[test]
    fn a_vector_icon_lists_its_colours_and_swaps_one() {
        let (_dir, mut state) = loaded();
        assert_eq!(state.palette_for(TOP_BAR).map(|p| p.colors.len()), Some(0));
        assert_eq!(state.palette_for(TEXTURE), None);
        state.replace_image(TOP_BAR, BLUE_ICON.as_bytes());
        let from = Rgb([0xec, 0xe8, 0xe1]);
        let colors: Vec<Rgb> = state
            .palette_for(TOP_BAR)
            .unwrap()
            .colors
            .iter()
            .map(|s| s.rgb)
            .collect();
        assert_eq!(colors.len(), 2);
        assert!(colors.contains(&from));
        let blue = Rgb([0x4d, 0x75, 0xc3]);
        state.swap_color(TOP_BAR, from, blue, Instant::now());
        state.commit_edits();
        assert_eq!(list(&state, TOP_BAR), [Adjust::Swap { from, to: blue }]);
        assert!(
            state.cached_palette(TOP_BAR).is_some(),
            "a swap keeps the source, so its colours stay"
        );
        let source = state.image_library().unwrap().source.clone();
        let shown = render(&source, &state.your_picture(TOP_BAR, 10).unwrap()).unwrap();
        assert_eq!(shown.image.pixel(5, 1), [0x4d, 0x75, 0xc3, 255]);
        assert_eq!(shown.image.pixel(5, 8), [0, 0, 0, 255]);
    }

    #[test]
    fn levers_parse_specs_and_stay_undoable() {
        assert_eq!(
            parse_specs("tint:#ff0000:80; hue:30 ;").unwrap(),
            [tint(80), Adjust::Hue { degrees: 30 }]
        );
        assert!(parse_specs("tint:red:80").is_err());
        let (_dir, mut state) = loaded();
        state
            .edit_lever("hero_ally=tint:#ff0000:80;invert")
            .unwrap();
        assert_eq!(list(&state, TEXTURE), [tint(80), Adjust::Invert]);
        assert!(state.edit_lever("nothing_like_this=invert").is_err());
        state.images.folder = Some("items".into());
        state.bulk_lever("hue:30").unwrap();
        assert_eq!(list(&state, ITEM), [Adjust::Hue { degrees: 30 }]);
        state.mark_lever("hero_ally, stand_in");
        assert_eq!(state.images.edit.marked.len(), 2);
        state.undo_images();
        assert!(state.image_override(ITEM).is_none());
    }
}
