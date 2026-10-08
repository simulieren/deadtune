//! "Check live preview": the visible test (a bigger minimap for a few seconds, then "did
//! it get bigger?") and the diagnosis `dt_core::hud::live_check` makes when it didn't.

use std::path::PathBuf;
use std::time::{Duration, Instant, SystemTime};

use dt_core::hud::install;
use dt_core::hud::layout::HudFeature;
use dt_core::hud::live_check::{self, Diagnosis, Facts, GameFacts, TestFacts, WebFacts, diagnose};

use crate::live_hud::FlashState;
use crate::state::AppState;

/// How long the bigger minimap stays once the game shows it.
pub const SHOW: Duration = Duration::from_secs(5);
/// How long the check waits for the script to show it: long enough to switch to the
/// game, which may hold its web panel's timers while it is in the background.
pub const WAIT: Duration = Duration::from_secs(20);

#[derive(Clone, Debug, Default)]
pub enum LiveCheck {
    #[default]
    Idle,
    /// The bigger minimap is on its way to the game, or showing.
    Showing {
        since: Instant,
    },
    Asking {
        seq: u32,
    },
    Done(Box<Done>),
}

#[derive(Clone, Debug)]
pub struct Done {
    pub facts: Facts,
    pub diagnosis: Diagnosis,
    pub report: String,
    /// Where the report was saved, or why it wasn't.
    pub saved: Result<PathBuf, String>,
}

/// What the check does next, given how far the bigger minimap got.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Next {
    Stay,
    Ask {
        seq: u32,
    },
    /// The script never showed it; diagnose with what was sent.
    Diagnose(TestFacts),
}

impl LiveCheck {
    pub fn active(&self) -> bool {
        matches!(self, LiveCheck::Showing { .. } | LiveCheck::Asking { .. })
    }

    /// `flash` is `None` once the preview's session ended (game closed, older HUD).
    pub fn next(&self, flash: Option<FlashState>, now: Instant) -> Next {
        let LiveCheck::Showing { since } = self else {
            return Next::Stay;
        };
        let sent = |seq: Option<u32>| match seq {
            Some(seq) => TestFacts::Sent { seq, saw: None },
            None => TestFacts::NotRun,
        };
        match flash {
            None => Next::Diagnose(TestFacts::NotRun),
            Some(FlashState {
                seq: Some(seq),
                applied: Some(at),
            }) if now >= at + SHOW => Next::Ask { seq },
            Some(FlashState {
                applied: Some(_), ..
            }) => Next::Stay,
            Some(FlashState { seq, .. }) if now >= *since + WAIT => Next::Diagnose(sent(seq)),
            Some(_) => Next::Stay,
        }
    }

    /// Seconds left on the countdown the page shows, and whether the game shows the
    /// bigger minimap yet.
    pub fn countdown(&self, flash: Option<FlashState>, now: Instant) -> Option<(u64, bool)> {
        let LiveCheck::Showing { since } = self else {
            return None;
        };
        let (end, shown) = match flash.and_then(|f| f.applied) {
            Some(at) => (at + SHOW, true),
            None => (*since + WAIT, false),
        };
        Some((end.saturating_duration_since(now).as_secs() + 1, shown))
    }
}

impl AppState {
    /// Starts the check: the visible test when a session runs, else the diagnosis now.
    pub fn start_live_check(&mut self, now: Instant) {
        let big = live_check::test_layout(&self.profile.hud);
        if self.live_hud.flash(big, now) {
            self.live_check = LiveCheck::Showing { since: now };
        } else {
            self.finish_live_check(TestFacts::NotRun);
        }
    }

    pub fn tick_live_check(&mut self, now: Instant) {
        match self.live_check.next(self.live_hud.flash_state(), now) {
            Next::Stay => {}
            Next::Ask { seq } => {
                self.live_hud.end_flash(now);
                self.live_check = LiveCheck::Asking { seq };
            }
            Next::Diagnose(test) => {
                self.live_hud.end_flash(now);
                self.finish_live_check(test);
            }
        }
    }

    /// The player's answer to "Did the minimap get bigger?".
    pub fn answer_live_check(&mut self, saw: bool) {
        if let LiveCheck::Asking { seq } = self.live_check {
            self.finish_live_check(TestFacts::Sent {
                seq,
                saw: Some(saw),
            });
        }
    }

    pub fn close_live_check(&mut self) {
        self.live_hud.end_flash(Instant::now());
        self.live_check = LiveCheck::Idle;
    }

    fn live_check_facts(&self, test: TestFacts) -> Facts {
        let logs: Vec<PathBuf> = self.conlog_paths();
        Facts {
            now: SystemTime::now(),
            preview_on: self
                .pak_target()
                .hud
                .features()
                .contains(&HudFeature::LivePreview),
            game: if self.ctx.game_running {
                GameFacts::Running {
                    started: self.game_started,
                }
            } else {
                GameFacts::Closed
            },
            log: live_check::read_log(&logs),
            pak: live_check::read_pak(&install::addons_dir(&self.paths)),
            pending_hud: self.hud_waits(),
            web: self.web_facts(),
            test,
        }
    }

    fn web_facts(&self) -> WebFacts {
        let status = self.web.status();
        WebFacts {
            listening: status.listening,
            counters: status.counters,
            last_poll_ago: status
                .last_poll
                .map(|t| Instant::now().saturating_duration_since(t)),
            base: status.base,
            recent_acks: status.recent_acks,
            awake: status.awake,
        }
    }

    fn finish_live_check(&mut self, test: TestFacts) {
        let facts = self.live_check_facts(test);
        self.show_live_check(facts);
    }

    /// The checklist for `facts`, saved as a report.
    pub fn show_live_check(&mut self, facts: Facts) {
        let diagnosis = diagnose(&facts);
        let report = live_check::report(&facts, &diagnosis, env!("CARGO_PKG_VERSION"));
        let saved = self.save_live_report(&report);
        self.live_check = LiveCheck::Done(Box::new(Done {
            facts,
            diagnosis,
            report,
            saved,
        }));
    }

    pub fn reports_dir(&self) -> PathBuf {
        self.data_dir.join("reports")
    }

    fn save_live_report(&self, report: &str) -> Result<PathBuf, String> {
        let dir = self.reports_dir();
        let name = format!(
            "live-hud-{}.txt",
            chrono::Local::now().format("%Y-%m-%d-%H%M%S")
        );
        std::fs::create_dir_all(&dir)
            .and_then(|_| std::fs::write(dir.join(&name), report))
            .map(|_| dir.join(name))
            .map_err(|e| e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn flash(seq: Option<u32>, applied: Option<Instant>) -> Option<FlashState> {
        Some(FlashState { seq, applied })
    }

    #[test]
    fn shows_for_a_while_after_the_game_applied_it_then_asks() {
        let t0 = Instant::now();
        let check = LiveCheck::Showing { since: t0 };
        let t1 = t0 + Duration::from_secs(3);
        assert_eq!(check.next(flash(Some(7), None), t1), Next::Stay);
        assert_eq!(check.countdown(flash(Some(7), None), t1), Some((18, false)));
        assert_eq!(check.next(flash(Some(7), Some(t1)), t1), Next::Stay);
        assert_eq!(
            check.countdown(flash(Some(7), Some(t1)), t1),
            Some((6, true))
        );
        assert_eq!(
            check.next(flash(Some(7), Some(t1)), t1 + SHOW),
            Next::Ask { seq: 7 }
        );
        assert_eq!(
            check.next(flash(Some(7), Some(t1)), t0 + WAIT * 2),
            Next::Ask { seq: 7 },
            "once shown, the wait no longer matters"
        );
    }

    #[test]
    fn no_answer_in_time_goes_straight_to_the_diagnosis() {
        let t0 = Instant::now();
        let check = LiveCheck::Showing { since: t0 };
        assert_eq!(
            check.next(flash(Some(7), None), t0 + WAIT),
            Next::Diagnose(TestFacts::Sent { seq: 7, saw: None })
        );
        assert_eq!(
            check.next(flash(None, None), t0 + WAIT),
            Next::Diagnose(TestFacts::NotRun),
            "nothing went out"
        );
        assert_eq!(
            check.next(None, t0),
            Next::Diagnose(TestFacts::NotRun),
            "the session ended"
        );
        assert_eq!(LiveCheck::Idle.next(None, t0), Next::Stay);
    }
}
