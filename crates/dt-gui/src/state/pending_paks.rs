//! HUD and addon pak changes that wait for Deadlock to close. The running game holds its
//! addon paks open, so Windows refuses to replace them; Apply writes everything else and
//! records the paks here, and they go in when the game closes or DeadTune next starts.

use dt_core::apply::{
    self, ApplyPlan, ApplyReport, PakKinds, PakTarget, PakTiming, PendingPaks, WaitingPaks,
};
use dt_core::hud::install::HudAction;

use super::{AppState, Status, default_profile};
use crate::profiles;

impl AppState {
    /// What the paks are built from now: nothing of ours in ranked-safe and safe mode.
    pub(crate) fn pak_target(&self) -> PakTarget {
        if self.without_addons() {
            PakTarget::default()
        } else {
            PakTarget {
                hud: self.profile.hud.clone(),
                addons: self.profile.addons.clone(),
            }
        }
    }

    pub(super) fn waiting_paks(&self, target: &PakTarget) -> WaitingPaks {
        match &self.pending_paks {
            None => WaitingPaks::None,
            Some(p) if &p.target == target => WaitingPaks::Same,
            Some(_) => WaitingPaks::Different,
        }
    }

    /// Which pak changes wait for the game to close, while it runs.
    pub fn paks_waiting(&self) -> Option<PakKinds> {
        self.pending_paks
            .as_ref()
            .filter(|_| self.ctx.game_running)
            .map(|p| p.kinds)
    }

    /// Discard also throws away pak changes still waiting for the game.
    pub fn can_discard(&self) -> bool {
        self.is_dirty() || self.pending_paks.is_some()
    }

    /// After an Apply: pak changes it left behind now wait, replacing any that waited before.
    /// Paks written, or nothing to change, clear the wait.
    pub(super) fn note_paks(&mut self, plan: &ApplyPlan, report: &ApplyReport) {
        if !report.paks_deferred && plan.paks == PakTiming::Queued {
            return;
        }
        let next = report.paks_deferred.then(|| {
            let discard_to = match &self.pending_paks {
                Some(was) => was.discard_to.clone(),
                None => {
                    let saved = self.saved.clone().unwrap_or_else(default_profile);
                    PakTarget {
                        hud: saved.hud,
                        addons: saved.addons,
                    }
                }
            };
            PendingPaks {
                target: self.pak_target(),
                discard_to,
                kinds: PakKinds::of(plan).unwrap_or(PakKinds::Both),
            }
        });
        self.set_pending_paks(next);
    }

    fn set_pending_paks(&mut self, next: Option<PendingPaks>) {
        let saved = match &next {
            Some(p) => p.save(&self.store.root),
            None => PendingPaks::clear(&self.store.root),
        };
        if let Err(e) = saved {
            self.status = Some(Status::Error(format!("remembering the HUD changes: {e}")));
        }
        self.pending_paks = next;
        self.live_hud.paks_waiting(self.hud_waits());
    }

    /// A HUD change waits for the game to close.
    pub(crate) fn hud_waits(&self) -> bool {
        self.pending_paks
            .as_ref()
            .is_some_and(|p| matches!(p.kinds, PakKinds::Hud | PakKinds::Both))
    }

    /// Writes the waiting paks. Safe to run any number of times: the plans compare against
    /// what is installed, so paks already in place are left alone.
    pub(super) fn put_pending_paks_in(&mut self) {
        let Some(pending) = self.pending_paks.clone() else {
            return;
        };
        let hud = apply::hud_plan(&self.paths, &pending.target.hud, &self.store).map(|plan| {
            plan.map(|mut plan| {
                if self.guard.holds_back(&plan) {
                    plan.action = HudAction::Nothing;
                }
                plan
            })
        });
        let written = hud.and_then(|hud| {
            let addons = apply::addons_plan(&self.paths, &pending.target.addons, &self.store)?;
            apply::write_paks(&self.paths, hud.as_ref(), addons.as_ref(), &self.store)
        });
        match written {
            Ok(w) if w.locked => {}
            Ok(_) => {
                self.set_pending_paks(None);
                self.status = Some(Status::Info(format!(
                    "Deadlock closed, so DeadTune put your {} changes in.",
                    pending.kinds.label()
                )));
            }
            Err(e) => {
                self.status = Some(Status::Error(format!(
                    "putting in the HUD and addon changes: {e}"
                )));
            }
        }
        self.hud_cache = None;
        self.addons_cache = None;
        self.refresh_preview();
    }

    /// Puts the profile's HUD and addons back to what they were before the waiting change,
    /// in the saved profile too, so the change never goes in.
    pub(super) fn discard_pending_paks(&mut self) {
        let Some(pending) = self.pending_paks.clone() else {
            return;
        };
        let PakTarget { hud, addons } = pending.discard_to;
        self.profile.hud = hud;
        self.profile.addons = addons;
        if self.saved.is_some() {
            self.saved = Some(self.profile.clone());
            if let Err(e) = profiles::save(&self.profiles_dir(), &self.profile) {
                self.status = Some(Status::Error(format!("saving the profile: {e}")));
            }
        }
        self.set_pending_paks(None);
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use dt_core::hud::elements::ElementId;
    use dt_core::hud::install::{ADDON_FILE, addons_dir};
    use dt_core::hud::layout::ElementEdit;

    use super::*;
    use crate::state::Pending;
    use crate::state::testutil::{state, with_game_hud};

    fn hud_pak(state: &AppState) -> PathBuf {
        addons_dir(&state.paths).join(ADDON_FILE)
    }

    fn record(state: &AppState) -> PathBuf {
        state.store.root.join(PendingPaks::FILE)
    }

    fn bigger_minimap(state: &mut AppState) {
        state.set_hud_element(
            ElementId::Minimap,
            ElementEdit {
                scale_pct: 130,
                ..ElementEdit::default()
            },
        );
    }

    fn running_with_hud() -> (tempfile::TempDir, AppState) {
        let (dir, mut state) = state();
        with_game_hud(&state);
        state.observe_game(true, None);
        (dir, state)
    }

    #[test]
    fn a_running_game_defers_the_hud_pak_and_writes_the_rest() {
        let (_dir, mut state) = running_with_hud();
        bigger_minimap(&mut state);
        state.set_convar("fps_max", "240".into()).unwrap();
        let applied = state.apply_and_save().unwrap();

        assert!(applied.report.paks_deferred && applied.report.wrote_gameinfo);
        assert!(!hud_pak(&state).exists(), "the running game holds the paks");
        assert!(state.live.gameinfo.contains("fps_max"));
        assert!(record(&state).is_file());
        assert_eq!(state.paks_waiting(), Some(PakKinds::Hud));
        assert_eq!(
            state.pending(),
            Pending::Nothing,
            "Apply has nothing left to do"
        );
        assert!(state.preview.as_ref().unwrap().is_empty());
        assert!(state.can_discard());
    }

    #[test]
    fn closing_the_game_puts_the_waiting_hud_in_once() {
        let (_dir, mut state) = running_with_hud();
        bigger_minimap(&mut state);
        state.apply_and_save().unwrap();

        state.observe_game(false, None);
        assert!(hud_pak(&state).is_file());
        assert_eq!(state.pending_paks, None);
        assert!(!record(&state).exists());
        assert_eq!(
            state.status,
            Some(Status::Info(
                "Deadlock closed, so DeadTune put your HUD changes in.".into()
            ))
        );
        let pak = std::fs::read(hud_pak(&state)).unwrap();
        state.status = None;
        state.observe_game(false, None);
        assert_eq!(state.status, None);
        assert_eq!(std::fs::read(hud_pak(&state)).unwrap(), pak);
        assert!(state.preview.as_ref().unwrap().is_empty());
    }

    #[test]
    fn waiting_paks_go_in_when_deadtune_next_starts_with_the_game_closed() {
        let (_dir, mut state) = running_with_hud();
        bigger_minimap(&mut state);
        state.apply_and_save().unwrap();
        let (paths, data, settings) = (
            state.paths.clone(),
            state.data_dir.clone(),
            state.settings.clone(),
        );
        drop(state);

        let mut state = AppState::open(paths, data, settings).unwrap();
        assert!(state.pending_paks.is_some());
        assert!(!hud_pak(&state).exists());
        state.observe_game(false, None);
        assert!(hud_pak(&state).is_file());
        assert_eq!(state.pending_paks, None);
    }

    #[test]
    fn waiting_paks_stay_while_deadtune_restarts_with_the_game_running() {
        let (_dir, mut state) = running_with_hud();
        bigger_minimap(&mut state);
        state.apply_and_save().unwrap();
        let mut state = AppState::open(
            state.paths.clone(),
            state.data_dir.clone(),
            state.settings.clone(),
        )
        .unwrap();
        state.observe_game(true, None);
        assert!(!hud_pak(&state).exists());
        assert_eq!(state.paks_waiting(), Some(PakKinds::Hud));
        assert_eq!(state.pending(), Pending::Nothing);
    }

    #[cfg(unix)]
    #[test]
    fn a_pak_held_open_while_the_game_looks_closed_waits_too() {
        use std::os::unix::fs::PermissionsExt;
        let (_dir, mut state) = state();
        with_game_hud(&state);
        let dir = addons_dir(&state.paths);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o555)).unwrap();
        bigger_minimap(&mut state);
        let applied = state.apply_and_save();
        std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o755)).unwrap();

        assert!(applied.unwrap().report.paks_deferred);
        assert!(state.pending_paks.is_some());
        state.launch_game().ok();
        assert!(
            hud_pak(&state).is_file(),
            "Launch puts waiting paks in first"
        );
        assert_eq!(state.pending_paks, None);
    }

    #[test]
    fn nothing_waits_when_no_pak_changes() {
        let (_dir, mut state) = running_with_hud();
        state.set_convar("fps_max", "240".into()).unwrap();
        let applied = state.apply_and_save().unwrap();
        assert!(!applied.report.paks_deferred);
        assert_eq!(state.pending_paks, None);
        assert!(!record(&state).exists());
    }

    #[test]
    fn undoing_the_hud_edit_while_running_clears_the_wait() {
        let (_dir, mut state) = running_with_hud();
        bigger_minimap(&mut state);
        state.apply_and_save().unwrap();
        state.set_hud_element(ElementId::Minimap, ElementEdit::default());
        assert_ne!(state.pending(), Pending::Nothing);
        state.apply_and_save().unwrap();
        assert_eq!(state.pending_paks, None);
    }

    #[test]
    fn discard_throws_the_waiting_hud_change_away() {
        let (_dir, mut state) = running_with_hud();
        bigger_minimap(&mut state);
        state.apply_and_save().unwrap();

        state.revert_all();
        assert_eq!(state.pending_paks, None);
        assert!(!record(&state).exists());
        assert!(state.profile.hud.is_vanilla());
        assert!(!state.is_dirty() && !state.can_discard());
        state.observe_game(false, None);
        assert!(!hud_pak(&state).exists());
        assert!(state.preview.as_ref().unwrap().is_empty());
    }
}
