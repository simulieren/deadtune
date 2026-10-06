//! Live HUD preview in the window: which state the preview is in and what to send to the
//! game's script when. Pure: `AppState` feeds it events and delivers the batches `tick`
//! returns. Design: docs/plans/live-hud/plan.md section 5.

use std::time::{Duration, Instant};

use dt_core::bridge::ConsoleCmd;
use dt_core::hud::install::InstallRecord;
use dt_core::hud::layout::{HudFeature, HudLayout};
use dt_core::hud::live::{self, Kind, LiveLine, LiveRule, Mailbox, NotLive};

/// Edits closer together than this go out as one message.
pub const DEBOUNCE: Duration = Duration::from_millis(100);
/// How long the script's hello may take before the page suggests `-condebug`.
pub const HELLO_WAIT: Duration = Duration::from_secs(20);

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LiveHud {
    Off,
    /// The preview is on, but the installed HUD pak has no live script yet.
    NotInstalled,
    GameClosed,
    /// The game runs and the script has not said hello yet.
    Waiting {
        since: Instant,
    },
    /// The game runs a HUD pak other than the one DeadTune last installed.
    Stale {
        base: String,
    },
    Live {
        base: String,
        /// The last message the script applied, and when it said so.
        seq: Option<u32>,
        acked: Option<Instant>,
        /// The game shows what Apply baked until the next edit.
        undone: bool,
    },
    Error(String),
}

/// The layout the installed pak baked, when that pak carries the live script.
#[derive(Clone, Debug)]
struct Baked {
    /// The record's layout text, to tell an unchanged record without parsing it.
    source: String,
    layout: HudLayout,
    base: String,
}

/// What a live session has sent; exists exactly while the state is `Live`.
#[derive(Clone, Debug)]
struct Session {
    mailbox: Mailbox,
    /// The override set last handed to the mailbox; `None` before the first message.
    sent: Option<Vec<LiveRule>>,
    due: Option<Instant>,
    undo: bool,
}

#[derive(Clone, Debug)]
pub struct LivePreview {
    state: LiveHud,
    on: bool,
    baked: Option<Baked>,
    running_since: Option<Instant>,
    /// The base the script last announced, and when.
    hello: Option<(String, Instant)>,
    error: Option<String>,
    desired: HudLayout,
    not_live: Vec<NotLive>,
    session: Option<Session>,
    /// The slots still need setting back to the game's defaults.
    restore: bool,
    next_seq: u32,
}

impl LivePreview {
    /// `seed` should differ between DeadTune runs (see `Mailbox::new`).
    pub fn new(seed: u32) -> LivePreview {
        LivePreview {
            state: LiveHud::Off,
            on: false,
            baked: None,
            running_since: None,
            hello: None,
            error: None,
            desired: HudLayout::default(),
            not_live: Vec::new(),
            session: None,
            restore: false,
            next_seq: seed,
        }
    }

    pub fn state(&self) -> &LiveHud {
        &self.state
    }

    /// The parts of the current layout the preview can't show, as page wording, each with
    /// `live::coverage`'s reason.
    pub fn not_live(&self) -> Vec<(&'static str, &str)> {
        self.not_live
            .iter()
            .map(|n| {
                let label = match n.feature {
                    HudFeature::TopBar if self.desired.top_bar.has_extras() => "top bar extras",
                    feature => feature.label(),
                };
                (label, n.why.as_str())
            })
            .collect()
    }

    /// A batch is waiting for its time or for the script's answer.
    pub fn busy(&self) -> bool {
        self.restore
            || self
                .session
                .as_ref()
                .is_some_and(|s| s.due.is_some() || s.undo || s.mailbox.pending())
    }

    pub fn toggle(&mut self, on: bool) {
        if self.on == on {
            return;
        }
        self.on = on;
        self.error = None;
        if on {
            self.not_live = live::coverage(&self.desired);
        }
        self.settle();
    }

    /// The installed HUD pak's record; only a pak built with the live script counts.
    pub fn record(&mut self, record: Option<&InstallRecord>) {
        let source = record
            .filter(|r| r.features.contains(&HudFeature::LivePreview))
            .and_then(|r| r.layout.as_deref());
        if source == self.baked.as_ref().map(|b| b.source.as_str()) {
            return;
        }
        let baked = source.and_then(|source| {
            let layout = toml::from_str::<HudLayout>(source).ok()?;
            let base = live::base_id(&layout).ok()?;
            Some(Baked {
                source: source.to_string(),
                layout,
                base,
            })
        });
        self.baked = baked;
        self.settle();
    }

    pub fn game(&mut self, running: bool, now: Instant) {
        if running == self.running_since.is_some() {
            return;
        }
        self.running_since = running.then_some(now);
        if !running {
            self.hello = None;
        }
        self.error = None;
        self.settle();
    }

    /// A console log line; only the script's own lines mean anything.
    pub fn line(&mut self, text: &str, now: Instant) {
        match live::parse_line(text) {
            Some(LiveLine::Hello { base, .. } | LiveLine::WrongBase { base, .. }) => {
                if self.hello.as_ref().is_some_and(|(b, _)| *b == base) {
                    return;
                }
                self.hello = Some((base, now));
                self.settle();
            }
            Some(LiveLine::Ok { seq: done, base }) => {
                let (
                    Some(session),
                    LiveHud::Live {
                        base: ours,
                        seq,
                        acked,
                        ..
                    },
                ) = (self.session.as_mut(), &mut self.state)
                else {
                    return;
                };
                if *ours == base {
                    session.mailbox.ack(done);
                    *seq = Some(done);
                    *acked = Some(now);
                }
            }
            Some(LiveLine::Got { seq, have, base }) => {
                if let Some(session) = self.session.as_mut()
                    && session.mailbox.base() == base
                {
                    session.mailbox.got(seq, have);
                }
            }
            None => {}
        }
    }

    /// The profile's HUD layout, after any change; the same layout again is no edit.
    pub fn edit(&mut self, layout: &HudLayout, now: Instant) {
        if *layout == self.desired {
            return;
        }
        self.desired = layout.clone();
        if self.on {
            self.not_live = live::coverage(layout);
        }
        if let Some(session) = self.session.as_mut() {
            session.due = Some(now + DEBOUNCE);
        }
        if let LiveHud::Live { undone, .. } = &mut self.state {
            *undone = false;
        }
    }

    /// Shows what Apply baked until the next edit.
    pub fn undo(&mut self) {
        if let (Some(session), LiveHud::Live { undone, .. }) =
            (self.session.as_mut(), &mut self.state)
        {
            session.undo = true;
            session.due = None;
            *undone = true;
        }
    }

    /// Delivering a batch failed; `message` is one plain sentence.
    pub fn fail(&mut self, message: String) {
        if self.error.as_ref() == Some(&message) {
            return;
        }
        self.error = Some(message);
        self.settle();
    }

    /// The batch to deliver now, if any.
    pub fn tick(&mut self, now: Instant) -> Option<Vec<ConsoleCmd>> {
        if std::mem::take(&mut self.restore) {
            return Some(live::reset_cmds());
        }
        let baked = self.baked.as_ref()?;
        let session = self.session.as_mut()?;
        if !session.undo && session.due.is_none_or(|due| now < due) {
            return session.mailbox.poll(now);
        }
        let undo = std::mem::take(&mut session.undo);
        session.due = None;
        let rules = if undo {
            live::rules(&baked.layout)
        } else {
            live::overrides(&self.desired, &baked.layout).map(|o| o.rules)
        };
        let rules = match rules {
            Ok(rules) => rules,
            Err(e) => {
                self.fail(format!("The live preview can't read this layout: {e}"));
                return self.tick(now);
            }
        };
        let kept = |sent: &[LiveRule]| {
            sent.iter().all(|old| {
                rules
                    .iter()
                    .any(|r| r.selector == old.selector && r.prop == old.prop)
            })
        };
        let (kind, changed) = match &session.sent {
            Some(sent) if !undo && kept(sent) => (
                Kind::Patch,
                rules
                    .iter()
                    .filter(|r| !sent.contains(r))
                    .cloned()
                    .collect(),
            ),
            _ => (Kind::Full, rules.clone()),
        };
        session.sent = Some(rules);
        if kind == Kind::Patch && changed.is_empty() {
            return session.mailbox.poll(now);
        }
        session.mailbox.send(changed, kind, now)
    }

    /// What to write when DeadTune closes: the slots back to their defaults, if a session
    /// may have set them.
    pub fn exit_cmds(&self) -> Option<Vec<ConsoleCmd>> {
        (self.restore || self.session.is_some()).then(live::reset_cmds)
    }

    /// Puts the preview in `state` without a game, for the screenshot lever.
    pub fn inject(&mut self, state: LiveHud) {
        self.on = state != LiveHud::Off;
        self.session = None;
        self.state = state;
    }

    /// The state the inputs call for. A session starts on entering `Live` and ends on
    /// leaving it, owing the restore batch.
    fn settle(&mut self) {
        let next = match (&self.baked, self.running_since, &self.hello) {
            _ if !self.on => LiveHud::Off,
            _ if self.error.is_some() => LiveHud::Error(self.error.clone().unwrap_or_default()),
            (None, _, _) => LiveHud::NotInstalled,
            (Some(_), None, _) => LiveHud::GameClosed,
            (Some(_), Some(since), None) => LiveHud::Waiting { since },
            (Some(baked), Some(_), Some((base, _))) if *base != baked.base => {
                LiveHud::Stale { base: base.clone() }
            }
            (Some(baked), Some(_), Some((_, at))) => {
                if matches!(&self.state, LiveHud::Live { base, .. } if *base == baked.base) {
                    return;
                }
                let first_seq = self.next_seq;
                // Sessions of one run never reuse a seq: the script skips the seq it applied last.
                self.next_seq = self.next_seq.wrapping_add(1 << 16);
                self.session = Some(Session {
                    mailbox: Mailbox::new(&baked.base, first_seq),
                    sent: None,
                    due: Some(*at),
                    undo: false,
                });
                LiveHud::Live {
                    base: baked.base.clone(),
                    seq: None,
                    acked: None,
                    undone: false,
                }
            }
        };
        if !matches!(next, LiveHud::Live { .. }) && self.session.take().is_some() {
            self.restore = true;
        }
        self.state = next;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use dt_core::hud::elements::ElementId;
    use dt_core::hud::layout::ElementEdit;
    use dt_core::hud::live::{CHUNK, SLOTS};

    fn moved(x: i32) -> HudLayout {
        let mut layout = HudLayout {
            live: true,
            ..HudLayout::default()
        };
        layout.elements.insert(
            ElementId::Minimap,
            ElementEdit {
                offset_x: x,
                ..ElementEdit::default()
            },
        );
        layout
    }

    fn record_of(layout: &HudLayout, live: bool) -> InstallRecord {
        let mut features = layout.features();
        if !live {
            features.retain(|f| *f != HudFeature::LivePreview);
        }
        InstallRecord {
            sha256: "00".into(),
            build_id: None,
            patched: Vec::new(),
            features,
            layout: Some(toml::to_string(layout).unwrap()),
            sources: Default::default(),
        }
    }

    fn hello(layout: &HudLayout) -> String {
        format!(
            "[Console] DEADTUNE_LIVE hello {}",
            live::base_id(layout).unwrap()
        )
    }

    fn ok(seq: u32, layout: &HudLayout) -> String {
        format!("DEADTUNE_LIVE {seq} ok {}", live::base_id(layout).unwrap())
    }

    /// A preview on a running game whose script said hello for `baked`, with its first
    /// message delivered and acked, so the next tick starts clean.
    fn live_on(baked: &HudLayout, t0: Instant) -> LivePreview {
        let mut p = LivePreview::new(100);
        p.toggle(true);
        p.edit(baked, t0);
        p.record(Some(&record_of(baked, true)));
        p.game(true, t0);
        p.line(&hello(baked), t0);
        let first = p.tick(t0).expect("entering live sends the current layout");
        assert_eq!(first.len(), SLOTS.len());
        p.line(&ok(100, baked), t0);
        assert_eq!(p.tick(t0 + Duration::from_secs(5)), None);
        p
    }

    fn values(cmds: &[ConsoleCmd]) -> Vec<String> {
        cmds.iter().map(|c| c.value.clone()).collect()
    }

    #[test]
    fn toggle_on_without_a_record_is_not_installed() {
        let mut p = LivePreview::new(1);
        assert_eq!(p.state(), &LiveHud::Off);
        p.toggle(true);
        assert_eq!(p.state(), &LiveHud::NotInstalled);
        p.record(Some(&record_of(&moved(10), false)));
        assert_eq!(p.state(), &LiveHud::NotInstalled, "pak without the script");
    }

    #[test]
    fn record_with_the_script_waits_for_the_game_then_for_hello() {
        let t0 = Instant::now();
        let baked = moved(10);
        let mut p = LivePreview::new(1);
        p.toggle(true);
        p.record(Some(&record_of(&baked, true)));
        assert_eq!(p.state(), &LiveHud::GameClosed);
        p.game(true, t0);
        assert_eq!(p.state(), &LiveHud::Waiting { since: t0 });
        p.game(true, t0 + Duration::from_secs(3));
        assert_eq!(p.state(), &LiveHud::Waiting { since: t0 }, "idempotent");
        p.record(Some(&record_of(&baked, true)));
        assert_eq!(p.state(), &LiveHud::Waiting { since: t0 });
        p.game(false, t0 + Duration::from_secs(4));
        assert_eq!(p.state(), &LiveHud::GameClosed);
        assert_eq!(p.tick(t0), None, "nothing was sent, so nothing to restore");
    }

    #[test]
    fn hello_with_the_installed_base_is_live_and_another_is_stale() {
        let t0 = Instant::now();
        let baked = moved(10);
        let mut p = LivePreview::new(1);
        p.toggle(true);
        p.record(Some(&record_of(&baked, true)));
        p.game(true, t0);
        p.line(&hello(&moved(99)), t0);
        assert_eq!(
            p.state(),
            &LiveHud::Stale {
                base: live::base_id(&moved(99)).unwrap()
            }
        );
        p.line(&hello(&baked), t0);
        let live = LiveHud::Live {
            base: live::base_id(&baked).unwrap(),
            seq: None,
            acked: None,
            undone: false,
        };
        assert_eq!(p.state(), &live);
        p.line(&hello(&baked), t0 + Duration::from_secs(10));
        assert_eq!(p.state(), &live, "the periodic hello changes nothing");
    }

    #[test]
    fn edits_go_out_once_after_the_debounce() {
        let t0 = Instant::now();
        let baked = moved(10);
        let mut p = live_on(&baked, t0);
        let t1 = t0 + Duration::from_secs(10);
        p.edit(&moved(20), t1);
        p.edit(&moved(30), t1 + Duration::from_millis(50));
        assert_eq!(
            p.tick(t1 + Duration::from_millis(120)),
            None,
            "still editing"
        );
        let batch = p
            .tick(t1 + Duration::from_millis(160))
            .expect("pause passed");
        assert_eq!(batch.len(), SLOTS.len());
        assert!(batch[0].value.contains(" patch "), "{:?}", batch[0]);
        assert!(
            batch[0].value.contains("translateX(30px)"),
            "{:?}",
            batch[0]
        );
        assert_eq!(p.tick(t1 + Duration::from_millis(200)), None, "sent once");
        p.edit(&moved(30), t1 + Duration::from_millis(300));
        assert_eq!(
            p.tick(t1 + Duration::from_millis(500)),
            None,
            "the same layout is not an edit"
        );
    }

    #[test]
    fn the_next_message_waits_for_the_ack() {
        let t0 = Instant::now();
        let baked = moved(10);
        let mut p = live_on(&baked, t0);
        let t1 = t0 + Duration::from_secs(10);
        p.edit(&moved(20), t1);
        assert!(p.tick(t1 + DEBOUNCE).is_some());
        p.edit(&moved(30), t1 + DEBOUNCE);
        assert_eq!(p.tick(t1 + DEBOUNCE * 3), None, "one message in flight");
        p.line(&ok(101, &baked), t1 + DEBOUNCE * 3);
        assert!(matches!(
            p.state(),
            LiveHud::Live {
                seq: Some(101),
                acked: Some(_),
                ..
            }
        ));
        let next = p.tick(t1 + DEBOUNCE * 3).expect("acked, so the next goes");
        assert!(next[0].value.starts_with("dt1 102 "), "{:?}", next[0]);
    }

    #[test]
    fn undo_sends_the_baked_rules_as_a_full_message() {
        let t0 = Instant::now();
        let baked = moved(10);
        let mut p = live_on(&baked, t0);
        let t1 = t0 + Duration::from_secs(10);
        p.edit(&moved(20), t1);
        p.tick(t1 + DEBOUNCE).unwrap();
        p.line(&ok(101, &baked), t1 + DEBOUNCE);
        p.undo();
        assert!(matches!(p.state(), LiveHud::Live { undone: true, .. }));
        let batch = p.tick(t1 + DEBOUNCE).expect("undo goes at once");
        let msg = live::decode(
            &values(&batch)
                .into_iter()
                .filter(|v| !v.is_empty())
                .collect::<Vec<_>>(),
        )
        .unwrap();
        assert_eq!(msg.kind, Kind::Full);
        assert_eq!(msg.rules, live::rules(&baked).unwrap());
        p.edit(&moved(40), t1 + Duration::from_secs(1));
        assert!(matches!(p.state(), LiveHud::Live { undone: false, .. }));
    }

    #[test]
    fn dropping_a_key_sends_a_full_message() {
        let t0 = Instant::now();
        let baked = HudLayout {
            live: true,
            ..HudLayout::default()
        };
        let mut p = live_on(&baked, t0);
        let t1 = t0 + Duration::from_secs(10);
        p.edit(&moved(20), t1);
        p.tick(t1 + DEBOUNCE).unwrap();
        p.line(&ok(101, &baked), t1 + DEBOUNCE);
        p.edit(&baked, t1 + DEBOUNCE);
        let batch = p.tick(t1 + DEBOUNCE * 2).expect("back to vanilla goes out");
        assert!(batch[0].value.contains(" full"), "{:?}", batch[0]);
    }

    #[test]
    fn game_stopping_restores_the_slots_once() {
        let t0 = Instant::now();
        let baked = moved(10);
        let mut p = live_on(&baked, t0);
        let t1 = t0 + Duration::from_secs(10);
        p.game(false, t1);
        assert_eq!(p.state(), &LiveHud::GameClosed);
        assert_eq!(p.tick(t1), Some(live::reset_cmds()));
        assert_eq!(p.tick(t1), None, "once");
        p.game(false, t1);
        assert_eq!(p.tick(t1), None, "idempotent");
    }

    #[test]
    fn toggling_off_while_live_restores() {
        let t0 = Instant::now();
        let baked = moved(10);
        let mut p = live_on(&baked, t0);
        p.toggle(false);
        assert_eq!(p.state(), &LiveHud::Off);
        assert_eq!(p.tick(t0), Some(live::reset_cmds()));
        p.toggle(false);
        assert_eq!(p.tick(t0), None);
    }

    #[test]
    fn a_failed_write_is_an_error_until_the_preview_is_turned_off() {
        let t0 = Instant::now();
        let baked = moved(10);
        let mut p = live_on(&baked, t0);
        p.fail("Couldn't write cfg".into());
        assert_eq!(p.state(), &LiveHud::Error("Couldn't write cfg".into()));
        assert_eq!(
            p.tick(t0),
            Some(live::reset_cmds()),
            "leaving live restores"
        );
        p.toggle(false);
        p.toggle(true);
        assert!(matches!(p.state(), LiveHud::Live { .. }));
    }

    #[test]
    fn long_messages_go_in_batches_as_the_script_reports_chunks() {
        let t0 = Instant::now();
        let baked = HudLayout {
            live: true,
            ..HudLayout::default()
        };
        let mut p = live_on(&baked, t0);
        let mut big = baked.clone();
        for (i, id) in [
            ElementId::TopBar,
            ElementId::Minimap,
            ElementId::HealthAndAmmo,
            ElementId::AbilitySlots,
            ElementId::ItemSlots,
            ElementId::Chat,
            ElementId::KillFeed,
        ]
        .into_iter()
        .enumerate()
        {
            big.elements.insert(
                id,
                ElementEdit {
                    offset_x: 10 + i as i32,
                    offset_y: -20,
                    scale_pct: 120,
                    opacity_pct: 80,
                    ..ElementEdit::default()
                },
            );
        }
        let t1 = t0 + Duration::from_secs(10);
        p.edit(&big, t1);
        let first = p.tick(t1 + DEBOUNCE).unwrap();
        let n: usize = first[0].value.split(' ').nth(2).unwrap()[2..]
            .parse()
            .unwrap();
        assert!(n > SLOTS.len(), "{n} chunks of {CHUNK}");
        assert_eq!(p.tick(t1 + DEBOUNCE), None);
        p.line(
            &format!(
                "DEADTUNE_LIVE 101 got {} {}",
                SLOTS.len(),
                live::base_id(&baked).unwrap()
            ),
            t1 + DEBOUNCE,
        );
        let second = p.tick(t1 + DEBOUNCE).expect("the rest follows");
        assert!(
            second[0]
                .value
                .starts_with(&format!("dt1 101 {}/", SLOTS.len() + 1))
        );
    }
}
