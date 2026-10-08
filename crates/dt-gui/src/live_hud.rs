//! Live HUD preview in the window: which state the preview is in and what to send to the
//! game's script when. Pure: `AppState` feeds it events and delivers what `tick` returns.
//! Design: docs/plans/live-hud/plan.md section 5.

use std::time::{Duration, Instant, SystemTime};

use dt_core::bridge::ConsoleCmd;
use dt_core::bridge::conlog::{TailReport, TailState};
use dt_core::hud::install::InstallRecord;
use dt_core::hud::layout::{HudFeature, HudLayout};
use dt_core::hud::live::{self, Batch, LiveLine, Mailbox, NotLive};

/// Edits closer together than this go out as one message.
pub const DEBOUNCE: Duration = Duration::from_millis(100);
/// How long the script's hello may take before the page explains what to check.
pub const HELLO_WAIT: Duration = Duration::from_secs(20);

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LiveHud {
    Off,
    /// The preview is on, but the installed HUD pak has no live script yet.
    NotInstalled,
    GameClosed,
    /// The game runs a HUD pak other than the one DeadTune installed: it started before the
    /// pak was written (`base` unknown), or its script announced another base.
    Stale {
        base: Option<String>,
    },
    /// The game runs and the script has not answered yet.
    Waiting {
        since: Instant,
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tone {
    Weak,
    Warn,
    Good,
    Bad,
}

/// The one line every HUD page shows about the preview.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StatusLine {
    pub text: String,
    pub tone: Tone,
    pub hover: Option<String>,
}

impl StatusLine {
    fn new(text: impl Into<String>, tone: Tone) -> StatusLine {
        StatusLine {
            text: text.into(),
            tone,
            hover: None,
        }
    }

    fn hover(self, hover: impl Into<String>) -> StatusLine {
        StatusLine {
            hover: Some(hover.into()),
            ..self
        }
    }
}

/// Where the game's console log stands, for a hello that is slow to come.
fn log_line(logs: &[TailReport], now: Instant) -> String {
    let read = logs
        .iter()
        .filter_map(|r| match &r.state {
            TailState::Read { last_line, .. } => Some((r, *last_line)),
            _ => None,
        })
        .max_by_key(|(_, last)| *last);
    if let Some((report, last)) = read {
        let ago = match last {
            Some(at) => format!(
                "its last line came {} s ago",
                now.saturating_duration_since(at).as_secs()
            ),
            None => "nothing new in it since DeadTune started".to_string(),
        };
        return format!(
            "No word from the live script yet. DeadTune reads {} and {ago}.",
            report.path.display()
        );
    }
    if let Some((report, why)) = logs.iter().find_map(|r| match &r.state {
        TailState::Unreadable(why) => Some((r, why)),
        _ => None,
    }) {
        return format!("DeadTune can't read {}: {why}", report.path.display());
    }
    "DeadTune can't find the game's console log. Launch Deadlock through DeadTune so -condebug \
     is on."
        .to_string()
}

/// What `AppState` writes or sends for the preview.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Delivery {
    /// A chunk for the slots: into `deadtune_hud.cfg`, and over netcon when that is on.
    Chunk(Batch),
    /// The slots back to the game's defaults.
    Restore(Vec<ConsoleCmd>),
    /// An empty `deadtune_hud.cfg`, so `exec deadtune_hud` always finds a file.
    Blank,
}

/// The layout the installed pak baked, when that pak carries the live script.
#[derive(Clone, Debug)]
struct Baked {
    /// The record's layout text, to tell an unchanged record without parsing it.
    source: String,
    layout: HudLayout,
    base: String,
    dict: Vec<String>,
    /// When the pak file was written; a game started earlier runs another one.
    written: Option<SystemTime>,
}

#[derive(Clone, Copy, Debug)]
struct Running {
    since: Instant,
    started_at: Option<SystemTime>,
}

/// What the game poll last said. Until its first answer a log line may well be from the
/// game that is running now (DeadTune started after it), so it is kept, not dropped.
#[derive(Clone, Copy, Debug)]
enum Game {
    Unknown,
    Closed,
    Running(Running),
}

impl Game {
    fn running(self) -> Option<Running> {
        match self {
            Game::Running(r) => Some(r),
            Game::Unknown | Game::Closed => None,
        }
    }
}

/// What a session sends; exists exactly while the state is `Waiting` or `Live`.
#[derive(Clone, Debug)]
struct Session {
    mailbox: Mailbox,
    due: Option<Instant>,
    undo: bool,
}

#[derive(Clone, Debug)]
pub struct LivePreview {
    state: LiveHud,
    on: bool,
    baked: Option<Baked>,
    game: Game,
    /// The base the script last announced (hello, ack or wrongbase), and when.
    hello: Option<(String, Instant)>,
    error: Option<String>,
    desired: HudLayout,
    not_live: Vec<NotLive>,
    session: Option<Session>,
    /// The slots still need setting back to the game's defaults.
    restore: bool,
    /// The cfg still needs writing for this session.
    blank: bool,
    next_seq: u32,
    /// Set by the screenshot lever: the state stays put, with this key wait.
    fake: Option<bool>,
    /// A HUD change waits for the game to close, so the game runs an older HUD.
    paks_waiting: bool,
    /// The check's bigger minimap, sent in place of the profile's layout while it lasts.
    flash: Option<Flash>,
}

/// The visible test of "Check live preview".
#[derive(Clone, Debug)]
struct Flash {
    layout: HudLayout,
    /// The message that carries it, once it went out.
    seq: Option<u32>,
    /// When the script said it applied that message.
    applied: Option<Instant>,
}

/// How far the check's bigger minimap got.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FlashState {
    pub seq: Option<u32>,
    pub applied: Option<Instant>,
}

impl LivePreview {
    /// `seed` should differ between DeadTune runs (see `Mailbox::new`).
    pub fn new(seed: u32) -> LivePreview {
        LivePreview {
            state: LiveHud::Off,
            on: false,
            baked: None,
            game: Game::Unknown,
            hello: None,
            error: None,
            desired: HudLayout::default(),
            not_live: Vec::new(),
            session: None,
            restore: false,
            blank: false,
            next_seq: seed,
            fake: None,
            paks_waiting: false,
            flash: None,
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
            || self.blank
            || self
                .session
                .as_ref()
                .is_some_and(|s| s.due.is_some() || s.undo || s.mailbox.pending())
    }

    /// The page's line for the current state. `key` is the bound key, `logs` the console
    /// log candidates.
    pub fn status(&self, key: &str, logs: &[TailReport], now: Instant) -> Option<StatusLine> {
        let press = format!("Press {key} in game");
        let close = "Close Deadlock, then press Launch.";
        Some(match &self.state {
            LiveHud::Off => return None,
            LiveHud::NotInstalled | LiveHud::Stale { .. } | LiveHud::Waiting { .. }
                if self.paks_waiting && self.game.running().is_some() =>
            {
                StatusLine::new(
                    format!("Your last Apply waits until Deadlock closes. {close}"),
                    Tone::Warn,
                )
                .hover(
                    "Windows locks the HUD file while the game runs, so the new HUD and its \
                     live script go in when Deadlock closes.",
                )
            }
            LiveHud::NotInstalled => StatusLine::new(
                "Apply to add the live script to your HUD; Deadlock loads it when it starts",
                Tone::Weak,
            ),
            LiveHud::GameClosed => {
                StatusLine::new("Live preview starts when Deadlock runs", Tone::Weak)
            }
            LiveHud::Stale { base } => {
                let why = match base {
                    Some(base) => format!(
                        "The game runs HUD {base}; DeadTune installed {}.",
                        self.baked.as_ref().map_or("another", |b| b.base.as_str())
                    ),
                    None => "Deadlock started before DeadTune wrote the HUD with the live script."
                        .to_string(),
                };
                StatusLine::new(
                    format!("Deadlock is running the HUD from before your last Apply. {close}"),
                    Tone::Warn,
                )
                .hover(why)
            }
            LiveHud::Waiting { .. } if self.key_pending() => {
                StatusLine::new(format!("{press} to connect the live preview"), Tone::Weak).hover(
                    format!(
                        "The key runs the file DeadTune just wrote; the script answers through the \
                 console log. {}",
                        log_line(logs, now)
                    ),
                )
            }
            LiveHud::Waiting { since } if now.saturating_duration_since(*since) < HELLO_WAIT => {
                StatusLine::new("Looking for the live script in game\u{2026}", Tone::Weak)
            }
            LiveHud::Waiting { .. } => StatusLine::new(log_line(logs, now), Tone::Weak),
            LiveHud::Live { base, acked, .. } => {
                let answer = match acked {
                    Some(at) => format!(
                        "Last answer from the game {} s ago.",
                        now.saturating_duration_since(*at).as_secs()
                    ),
                    None => "No answer from the game yet.".to_string(),
                };
                let text = if self.key_pending() {
                    format!("Live in game \u{b7} {press} to show your latest edits")
                } else {
                    "Live in game".to_string()
                };
                StatusLine::new(text, Tone::Good).hover(format!("HUD {base} is running. {answer}"))
            }
            LiveHud::Error(message) => StatusLine::new(message.clone(), Tone::Bad),
        })
    }

    /// The installed HUD pak carries the live script and the preview is on.
    pub fn script_installed(&self) -> bool {
        self.on && self.baked.is_some()
    }

    /// A chunk waits in the cfg for the player to press the bound key.
    pub fn key_pending(&self) -> bool {
        self.fake.unwrap_or(false)
            || self
                .session
                .as_ref()
                .is_some_and(|s| s.mailbox.key_pending())
    }

    /// A HUD change waits for the game to close.
    pub fn paks_waiting(&mut self, waiting: bool) {
        self.paks_waiting = waiting;
    }

    /// Sends `layout` in place of the profile's until `end_flash`; only while a session
    /// runs. Returns whether it went.
    pub fn flash(&mut self, layout: HudLayout, now: Instant) -> bool {
        let Some(session) = self.session.as_mut() else {
            return false;
        };
        session.due = Some(now);
        session.undo = false;
        self.flash = Some(Flash {
            layout,
            seq: None,
            applied: None,
        });
        true
    }

    /// Back to the profile's layout.
    pub fn end_flash(&mut self, now: Instant) {
        if self.flash.take().is_some()
            && let Some(session) = self.session.as_mut()
        {
            session.due = Some(now);
        }
    }

    pub fn flash_state(&self) -> Option<FlashState> {
        self.flash.as_ref().map(|f| FlashState {
            seq: f.seq,
            applied: f.applied,
        })
    }

    /// The layout wants the preview: the switch is on and there is a pak to ride in.
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

    /// The installed HUD pak's record and when the pak was written; only a pak built with
    /// the live script counts.
    pub fn record(&mut self, record: Option<&InstallRecord>, written: Option<SystemTime>) {
        let source = record
            .filter(|r| r.features.contains(&HudFeature::LivePreview))
            .and_then(|r| r.layout.as_deref());
        if source == self.baked.as_ref().map(|b| b.source.as_str()) {
            if let Some(baked) = self.baked.as_mut() {
                baked.written = written;
            }
            self.settle();
            return;
        }
        self.baked = source.and_then(|source| {
            let layout = toml::from_str::<HudLayout>(source).ok()?;
            Some(Baked {
                source: source.to_string(),
                base: live::base_id(&layout).ok()?,
                dict: live::dictionary(&layout).ok()?,
                layout,
                written,
            })
        });
        self.settle();
    }

    /// The polled game state. A game that just appeared runs a HUD that has not said hello.
    pub fn game(&mut self, running: bool, started_at: Option<SystemTime>, now: Instant) {
        match (running, &mut self.game) {
            (true, Game::Running(r)) => {
                r.started_at = started_at.or(r.started_at);
            }
            (true, previous) => {
                if matches!(previous, Game::Closed) {
                    self.hello = None;
                }
                self.game = Game::Running(Running {
                    since: now,
                    started_at,
                });
            }
            (false, Game::Closed) => return,
            (false, _) => {
                self.game = Game::Closed;
                self.hello = None;
            }
        }
        self.error = None;
        self.settle();
    }

    /// A console log line; only the script's own lines mean anything.
    pub fn line(&mut self, text: &str, now: Instant) {
        let Some(line) = live::parse_line(text) else {
            return;
        };
        if matches!(self.game, Game::Closed) {
            return;
        }
        match line {
            LiveLine::Hello { base, .. } => {
                let same = self.hello.as_ref().is_some_and(|(b, _)| *b == base);
                self.hello = Some((base, now));
                if let (true, Some(session)) = (same, self.session.as_mut()) {
                    session.mailbox.reload();
                    session.due = Some(now);
                }
                self.settle();
            }
            LiveLine::WrongBase { base, .. } => {
                if let Some(session) = self.session.as_mut() {
                    session.mailbox.heard();
                }
                self.hello = Some((base, now));
                self.settle();
            }
            LiveLine::Ok { seq: done, base } => {
                if let Some(session) = self.session.as_mut()
                    && session.mailbox.base() == base
                {
                    session.mailbox.ack(done);
                    if let Some(flash) = self.flash.as_mut().filter(|f| f.seq == Some(done)) {
                        flash.applied = Some(now);
                    }
                    self.hello = Some((base, now));
                    self.settle();
                    if let LiveHud::Live { seq, acked, .. } = &mut self.state {
                        *seq = Some(done);
                        *acked = Some(now);
                    }
                }
            }
            LiveLine::Got { seq, have, base } => {
                if let Some(session) = self.session.as_mut()
                    && session.mailbox.base() == base
                {
                    session.mailbox.got(seq, have);
                }
            }
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

    /// Delivering failed; `message` is one plain sentence.
    pub fn fail(&mut self, message: String) {
        if self.error.as_ref() == Some(&message) {
            return;
        }
        self.error = Some(message);
        self.settle();
    }

    /// What to deliver now, if anything. `pull`: the game reads chunks through the bound
    /// key and the script's own execs, not over netcon.
    pub fn tick(&mut self, now: Instant, pull: bool) -> Option<Delivery> {
        if std::mem::take(&mut self.restore) {
            return Some(Delivery::Restore(live::reset_cmds()));
        }
        if std::mem::take(&mut self.blank) {
            return Some(Delivery::Blank);
        }
        let baked = self.baked.as_ref()?;
        let session = self.session.as_mut()?;
        if !session.undo && session.due.is_none_or(|due| now < due) {
            let batch = session.mailbox.poll(now, pull);
            if let (Some(flash), Some(b)) = (self.flash.as_mut(), &batch)
                && b.i == 1
            {
                flash.seq = Some(b.seq);
            }
            return batch.map(Delivery::Chunk);
        }
        let undo = std::mem::take(&mut session.undo);
        session.due = None;
        let target = self.flash.as_ref().map_or(&self.desired, |f| &f.layout);
        let rules = if undo {
            live::rules(&baked.layout)
        } else {
            live::overrides(target, &baked.layout).map(|o| o.rules)
        };
        match rules {
            Ok(rules) => {
                let batch = session.mailbox.send(rules, now, pull);
                if let (Some(flash), Some(b)) = (self.flash.as_mut(), &batch) {
                    flash.seq = Some(b.seq);
                }
                batch.map(Delivery::Chunk)
            }
            Err(e) => {
                self.fail(format!("The live preview can't read this layout: {e}"));
                self.tick(now, pull)
            }
        }
    }

    /// What to write when DeadTune closes: the slots back to their defaults, if a session
    /// may have set them.
    pub fn exit_cmds(&self) -> Option<Vec<ConsoleCmd>> {
        (self.restore || self.session.is_some()).then(live::reset_cmds)
    }

    /// The check's bigger minimap as sent, for the screenshot lever.
    pub fn inject_flash(&mut self, applied: Option<Instant>) {
        self.flash = Some(Flash {
            layout: self.desired.clone(),
            seq: Some(1),
            applied,
        });
    }

    /// Puts the preview in `state` without a game, for the screenshot lever.
    pub fn inject(&mut self, state: LiveHud, key_pending: bool) {
        self.on = state != LiveHud::Off;
        self.session = None;
        self.fake = Some(key_pending);
        self.state = state;
    }

    /// The state the inputs call for. A session runs while the state is `Waiting` or
    /// `Live` and ends on leaving them, owing the restore batch.
    fn settle(&mut self) {
        if self.fake.is_some() {
            return;
        }
        let next = match (&self.baked, self.game.running(), &self.hello) {
            _ if !self.on => LiveHud::Off,
            _ if self.error.is_some() => LiveHud::Error(self.error.clone().unwrap_or_default()),
            (None, _, _) => LiveHud::NotInstalled,
            (Some(_), None, _) => LiveHud::GameClosed,
            (Some(baked), Some(_), Some((base, _))) if *base != baked.base => LiveHud::Stale {
                base: Some(base.clone()),
            },
            (Some(baked), Some(game), None)
                if game
                    .started_at
                    .zip(baked.written)
                    .is_some_and(|(started, written)| started < written) =>
            {
                LiveHud::Stale { base: None }
            }
            (Some(_), Some(game), None) => LiveHud::Waiting { since: game.since },
            (Some(baked), Some(_), Some(_)) => match &self.state {
                LiveHud::Live { base, .. } if *base == baked.base => return,
                _ => LiveHud::Live {
                    base: baked.base.clone(),
                    seq: None,
                    acked: None,
                    undone: false,
                },
            },
        };
        let in_session = matches!(next, LiveHud::Waiting { .. } | LiveHud::Live { .. });
        match (in_session, &self.session, &self.baked) {
            (true, None, Some(baked)) => {
                let first_seq = self.next_seq;
                // Sessions of one run never reuse a seq: the script skips the seq it applied last.
                self.next_seq = self.next_seq.wrapping_add(97);
                self.session = Some(Session {
                    mailbox: Mailbox::new(&baked.base, baked.dict.clone(), first_seq),
                    due: self.game.running().map(|game| game.since),
                    undo: false,
                });
                self.blank = true;
            }
            (false, Some(_), _) => {
                self.session = None;
                self.flash = None;
                self.restore = true;
            }
            _ => {}
        }
        self.state = next;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use dt_core::hud::elements::ElementId;
    use dt_core::hud::layout::ElementEdit;
    use dt_core::hud::live::{DATA, Kind};

    fn moved(x: i32) -> HudLayout {
        let mut layout = HudLayout::default();
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
        features.retain(|f| *f != HudFeature::LivePreview);
        if live {
            features.push(HudFeature::LivePreview);
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
            "[Console] DEADTUNE_LIVE hello {} ctl=1024 d=1000,1001 n=11 gi=0",
            live::base_id(layout).unwrap()
        )
    }

    fn ok(seq: u32, layout: &HudLayout) -> String {
        format!("DEADTUNE_LIVE {seq} ok {}", live::base_id(layout).unwrap())
    }

    fn chunk(d: Option<Delivery>) -> Batch {
        match d {
            Some(Delivery::Chunk(b)) => b,
            other => panic!("expected a chunk, got {other:?}"),
        }
    }

    fn message(baked: &HudLayout, batch: &Batch) -> live::Message {
        let words: Vec<u32> = batch.cmds[1..]
            .iter()
            .map(|c| c.value.parse().unwrap())
            .collect();
        live::decode(&[words], &live::dictionary(baked).unwrap()).unwrap()
    }

    /// Every chunk of the message `first` opens, as the script would collect them.
    fn whole(p: &mut LivePreview, first: Batch, baked: &HudLayout, now: Instant) -> live::Message {
        let base = live::base_id(baked).unwrap();
        let mut chunks = vec![first.clone()];
        while chunks.len() < first.n {
            p.line(
                &format!("DEADTUNE_LIVE {} got {} {base}", first.seq, chunks.len()),
                now,
            );
            chunks.push(chunk(p.tick(now, false)));
        }
        let words: Vec<Vec<u32>> = chunks
            .iter()
            .map(|b| {
                b.cmds[1..]
                    .iter()
                    .map(|c| c.value.parse().unwrap())
                    .collect()
            })
            .collect();
        live::decode(&words, &live::dictionary(baked).unwrap()).unwrap()
    }

    /// A preview on a running game whose script said hello for `baked`, with its first
    /// message delivered and acked, so the next tick starts clean.
    fn live_on(baked: &HudLayout, t0: Instant) -> LivePreview {
        let mut p = LivePreview::new(100);
        p.toggle(true);
        p.edit(baked, t0);
        p.record(Some(&record_of(baked, true)), None);
        p.game(true, None, t0);
        assert_eq!(p.tick(t0, false), Some(Delivery::Blank));
        p.line(&hello(baked), t0);
        let first = chunk(p.tick(t0, false));
        assert_eq!(first.cmds.len(), 1 + DATA.len());
        p.line(&ok(100, baked), t0);
        assert_eq!(p.tick(t0 + Duration::from_secs(5), false), None);
        p
    }

    #[test]
    fn toggle_on_without_a_record_is_not_installed() {
        let mut p = LivePreview::new(1);
        assert_eq!(p.state(), &LiveHud::Off);
        p.toggle(true);
        assert_eq!(p.state(), &LiveHud::NotInstalled);
        p.record(Some(&record_of(&moved(10), false)), None);
        assert_eq!(p.state(), &LiveHud::NotInstalled, "pak without the script");
    }

    #[test]
    fn record_with_the_script_waits_for_the_game_then_sends_and_waits_for_an_answer() {
        let t0 = Instant::now();
        let baked = moved(10);
        let mut p = LivePreview::new(1);
        p.toggle(true);
        p.edit(&baked, t0);
        p.record(Some(&record_of(&baked, true)), None);
        assert_eq!(p.state(), &LiveHud::GameClosed);
        p.game(true, None, t0);
        assert_eq!(p.state(), &LiveHud::Waiting { since: t0 });
        assert_eq!(
            p.tick(t0, true),
            Some(Delivery::Blank),
            "the cfg exists from now on"
        );
        let first = chunk(p.tick(t0, true));
        assert!(
            first.needs_key,
            "the first message asks the game, through the key"
        );
        assert!(p.key_pending());
        p.game(true, None, t0 + Duration::from_secs(3));
        assert_eq!(p.state(), &LiveHud::Waiting { since: t0 }, "idempotent");
        p.record(Some(&record_of(&baked, true)), None);
        assert_eq!(p.state(), &LiveHud::Waiting { since: t0 });
        p.line(&ok(1, &baked), t0 + Duration::from_secs(4));
        assert!(
            matches!(p.state(), LiveHud::Live { seq: Some(1), .. }),
            "the ack is the hello: {:?}",
            p.state()
        );
        assert!(!p.key_pending());
        p.game(false, None, t0 + Duration::from_secs(5));
        assert_eq!(p.state(), &LiveHud::GameClosed);
        assert_eq!(
            p.tick(t0, true),
            Some(Delivery::Restore(live::reset_cmds())),
            "the slots go back"
        );
    }

    #[test]
    fn a_game_started_before_the_pak_was_written_needs_a_restart() {
        let t0 = Instant::now();
        let baked = moved(10);
        let written = SystemTime::now();
        let mut p = LivePreview::new(1);
        p.toggle(true);
        p.record(Some(&record_of(&baked, true)), Some(written));
        p.game(true, Some(written - Duration::from_secs(60)), t0);
        assert_eq!(p.state(), &LiveHud::Stale { base: None });
        assert_eq!(p.tick(t0, true), None, "nothing is sent to an older HUD");
        p.game(false, None, t0);
        p.game(true, Some(written + Duration::from_secs(60)), t0);
        assert_eq!(p.state(), &LiveHud::Waiting { since: t0 });
    }

    #[test]
    fn hello_with_the_installed_base_is_live_and_another_is_stale() {
        let t0 = Instant::now();
        let baked = moved(10);
        let mut p = LivePreview::new(1);
        p.toggle(true);
        p.record(Some(&record_of(&baked, true)), None);
        p.game(true, None, t0);
        p.line(&hello(&moved(99)), t0);
        assert_eq!(
            p.state(),
            &LiveHud::Stale {
                base: Some(live::base_id(&moved(99)).unwrap())
            }
        );
        assert_eq!(
            p.tick(t0, false),
            Some(Delivery::Restore(live::reset_cmds())),
            "leaving the session restores the slots"
        );
        p.line(&hello(&baked), t0);
        let live = LiveHud::Live {
            base: live::base_id(&baked).unwrap(),
            seq: None,
            acked: None,
            undone: false,
        };
        assert_eq!(p.state(), &live);
        assert_eq!(p.tick(t0, false), Some(Delivery::Blank));
        let first = chunk(p.tick(t0, false));
        assert_eq!(message(&baked, &first).kind, Kind::Full);
        p.line(
            "[Console] DEADTUNE_LIVE 1 wrongbase cafe0001\r",
            t0 + Duration::from_secs(1),
        );
        assert_eq!(
            p.state(),
            &LiveHud::Stale {
                base: Some("cafe0001".into())
            },
            "the script's own word wins"
        );
    }

    #[test]
    fn simons_hello_line_counts_even_before_the_first_game_poll() {
        let t0 = Instant::now();
        let baked = moved(10);
        let base = live::base_id(&baked).unwrap();
        let mut p = LivePreview::new(1);
        p.toggle(true);
        p.record(Some(&record_of(&baked, true)), None);
        p.line(&format!("[Console] DEADTUNE_LIVE hello {base}\r"), t0);
        assert_eq!(p.state(), &LiveHud::GameClosed);
        p.game(true, None, t0);
        assert!(
            matches!(p.state(), LiveHud::Live { .. }),
            "DeadTune started after the game: {:?}",
            p.state()
        );
    }

    #[test]
    fn a_hello_while_the_game_is_known_closed_is_an_old_line() {
        let t0 = Instant::now();
        let baked = moved(10);
        let mut p = LivePreview::new(1);
        p.toggle(true);
        p.record(Some(&record_of(&baked, true)), None);
        p.game(false, None, t0);
        p.line(&hello(&baked), t0);
        p.game(true, None, t0);
        assert_eq!(p.state(), &LiveHud::Waiting { since: t0 });
        p.line(&hello(&baked), t0);
        assert!(matches!(p.state(), LiveHud::Live { .. }));
        p.game(false, None, t0);
        p.game(true, None, t0);
        assert_eq!(
            p.state(),
            &LiveHud::Waiting { since: t0 },
            "a new game needs its own hello"
        );
    }

    #[test]
    fn a_second_hello_means_the_script_reloaded_and_everything_goes_again() {
        let t0 = Instant::now();
        let baked = moved(10);
        let mut p = live_on(&baked, t0);
        let t1 = t0 + Duration::from_secs(30);
        p.line(&hello(&baked), t1);
        assert!(matches!(p.state(), LiveHud::Live { .. }));
        let again = chunk(p.tick(t1, false));
        assert_eq!(message(&baked, &again).kind, Kind::Full);
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
            p.tick(t1 + Duration::from_millis(120), false),
            None,
            "still editing"
        );
        let batch = chunk(p.tick(t1 + Duration::from_millis(160), false));
        let m = message(&baked, &batch);
        assert_eq!(m.kind, Kind::Patch);
        assert_eq!(m.rules.len(), 1);
        assert_eq!(m.rules[0].value, "translateX(30px) translateY(0px)");
        assert_eq!(
            p.tick(t1 + Duration::from_millis(200), false),
            None,
            "sent once"
        );
        p.edit(&moved(30), t1 + Duration::from_millis(300));
        assert_eq!(
            p.tick(t1 + Duration::from_millis(500), false),
            None,
            "the same layout is not an edit"
        );
    }

    #[test]
    fn the_next_message_waits_for_the_ack_unless_the_last_was_never_read() {
        let t0 = Instant::now();
        let baked = moved(10);
        let mut p = live_on(&baked, t0);
        let t1 = t0 + Duration::from_secs(10);
        p.edit(&moved(20), t1);
        let first = chunk(p.tick(t1 + DEBOUNCE, false));
        p.edit(&moved(30), t1 + DEBOUNCE);
        let replaced = chunk(p.tick(t1 + DEBOUNCE * 3, false));
        assert_ne!(replaced.seq, first.seq, "an unread message is replaced");
        p.line(
            &format!(
                "DEADTUNE_LIVE {} got 1 {}",
                replaced.seq,
                live::base_id(&baked).unwrap()
            ),
            t1 + DEBOUNCE * 3,
        );
        p.edit(&moved(40), t1 + DEBOUNCE * 3);
        assert_eq!(
            p.tick(t1 + DEBOUNCE * 5, false),
            None,
            "the script is reading, so the next waits"
        );
        p.line(&ok(replaced.seq, &baked), t1 + DEBOUNCE * 5);
        assert!(matches!(
            p.state(),
            LiveHud::Live {
                seq: Some(_),
                acked: Some(_),
                ..
            }
        ));
        let next = chunk(p.tick(t1 + DEBOUNCE * 5, false));
        assert_eq!(next.seq, replaced.seq + 1);
    }

    #[test]
    fn undo_sends_the_baked_rules_as_a_full_message() {
        let t0 = Instant::now();
        let baked = moved(10);
        let mut p = live_on(&baked, t0);
        let t1 = t0 + Duration::from_secs(10);
        p.edit(&moved(20), t1);
        let sent = chunk(p.tick(t1 + DEBOUNCE, false));
        p.line(&ok(sent.seq, &baked), t1 + DEBOUNCE);
        p.undo();
        assert!(matches!(p.state(), LiveHud::Live { undone: true, .. }));
        let batch = chunk(p.tick(t1 + DEBOUNCE, false));
        let msg = message(&baked, &batch);
        assert_eq!(msg.kind, Kind::Patch, "the keys stay, only values change");
        assert_eq!(msg.rules, live::rules(&baked).unwrap());
        p.edit(&moved(40), t1 + Duration::from_secs(1));
        assert!(matches!(p.state(), LiveHud::Live { undone: false, .. }));
    }

    #[test]
    fn dropping_a_key_sends_a_full_message() {
        let t0 = Instant::now();
        let baked = HudLayout::default();
        let mut p = live_on(&baked, t0);
        let t1 = t0 + Duration::from_secs(10);
        p.edit(&moved(20), t1);
        let sent = chunk(p.tick(t1 + DEBOUNCE, false));
        p.line(&ok(sent.seq, &baked), t1 + DEBOUNCE);
        p.edit(&baked, t1 + DEBOUNCE);
        let batch = chunk(p.tick(t1 + DEBOUNCE * 2, false));
        assert_eq!(message(&baked, &batch).kind, Kind::Full);
    }

    #[test]
    fn game_stopping_restores_the_slots_once() {
        let t0 = Instant::now();
        let baked = moved(10);
        let mut p = live_on(&baked, t0);
        let t1 = t0 + Duration::from_secs(10);
        p.game(false, None, t1);
        assert_eq!(p.state(), &LiveHud::GameClosed);
        assert_eq!(
            p.tick(t1, false),
            Some(Delivery::Restore(live::reset_cmds()))
        );
        assert_eq!(p.tick(t1, false), None, "once");
        p.game(false, None, t1);
        assert_eq!(p.tick(t1, false), None, "idempotent");
    }

    #[test]
    fn toggling_off_while_live_restores() {
        let t0 = Instant::now();
        let baked = moved(10);
        let mut p = live_on(&baked, t0);
        p.toggle(false);
        assert_eq!(p.state(), &LiveHud::Off);
        assert_eq!(
            p.tick(t0, false),
            Some(Delivery::Restore(live::reset_cmds()))
        );
        p.toggle(false);
        assert_eq!(p.tick(t0, false), None);
        assert_eq!(p.exit_cmds(), None);
    }

    #[test]
    fn a_failed_write_is_an_error_until_the_preview_is_turned_off() {
        let t0 = Instant::now();
        let baked = moved(10);
        let mut p = live_on(&baked, t0);
        p.fail("Couldn't write cfg".into());
        assert_eq!(p.state(), &LiveHud::Error("Couldn't write cfg".into()));
        assert_eq!(
            p.tick(t0, false),
            Some(Delivery::Restore(live::reset_cmds())),
            "leaving live restores"
        );
        p.toggle(false);
        p.toggle(true);
        assert!(matches!(p.state(), LiveHud::Live { .. }));
    }

    #[test]
    fn long_messages_go_in_chunks_as_the_script_reports_them() {
        let t0 = Instant::now();
        let baked = HudLayout::default();
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
        let first = chunk(p.tick(t1 + DEBOUNCE, true));
        assert!(first.n > 1, "{} chunks", first.n);
        assert!(first.needs_key);
        assert_eq!(p.tick(t1 + DEBOUNCE, true), None);
        p.line(
            &format!(
                "DEADTUNE_LIVE {} got 1 {}",
                first.seq,
                live::base_id(&baked).unwrap()
            ),
            t1 + DEBOUNCE,
        );
        assert!(!p.key_pending(), "the game ran the file");
        let second = chunk(p.tick(t1 + DEBOUNCE, true));
        assert_eq!((second.seq, second.i), (first.seq, 2));
        assert!(!second.needs_key, "the script pulls the rest itself");
    }

    #[test]
    fn status_lines_say_what_to_do() {
        use std::path::PathBuf;
        let t0 = Instant::now();
        let baked = moved(10);
        let text =
            |p: &LivePreview, logs: &[TailReport], now| p.status("F8", logs, now).map(|l| l.text);
        let mut p = LivePreview::new(1);
        assert_eq!(text(&p, &[], t0), None);
        p.toggle(true);
        assert!(
            text(&p, &[], t0)
                .unwrap()
                .starts_with("Apply to add the live script")
        );
        let written = SystemTime::now();
        p.record(Some(&record_of(&baked, true)), Some(written));
        assert_eq!(
            text(&p, &[], t0).unwrap(),
            "Live preview starts when Deadlock runs"
        );
        p.game(true, Some(written - Duration::from_secs(5)), t0);
        assert_eq!(
            text(&p, &[], t0).unwrap(),
            "Deadlock is running the HUD from before your last Apply. Close Deadlock, then press Launch.",
            "only a game older than the pak needs the restart"
        );
        p.paks_waiting(true);
        assert_eq!(
            text(&p, &[], t0).unwrap(),
            "Your last Apply waits until Deadlock closes. Close Deadlock, then press Launch."
        );
        p.paks_waiting(false);
        p.game(false, None, t0);
        p.game(true, Some(written + Duration::from_secs(5)), t0);
        assert_eq!(
            text(&p, &[], t0).unwrap(),
            "Looking for the live script in game\u{2026}"
        );
        let later = t0 + HELLO_WAIT + Duration::from_secs(1);
        assert!(
            text(&p, &[], later)
                .unwrap()
                .contains("Launch Deadlock through DeadTune"),
            "no log at all"
        );
        let log = TailReport {
            path: PathBuf::from(r"C:\Deadlock\game\citadel\console.log"),
            state: TailState::Read {
                bytes: 10,
                last_line: Some(later - Duration::from_secs(3)),
            },
        };
        assert_eq!(
            text(&p, std::slice::from_ref(&log), later).unwrap(),
            r"No word from the live script yet. DeadTune reads C:\Deadlock\game\citadel\console.log and its last line came 3 s ago."
        );
        assert_eq!(p.tick(later, true), Some(Delivery::Blank));
        chunk(p.tick(later, true));
        assert_eq!(
            text(&p, &[], later).unwrap(),
            "Press F8 in game to connect the live preview"
        );
        p.line(&hello(&baked), later);
        assert!(text(&p, &[], later).unwrap().starts_with("Live in game"));
        p.line(&ok(1, &baked), later);
        assert_eq!(text(&p, &[], later).unwrap(), "Live in game");
    }

    #[test]
    fn a_flash_sends_its_layout_until_it_ends_and_knows_when_it_showed() {
        let t0 = Instant::now();
        let baked = moved(10);
        let mut p = live_on(&baked, t0);
        let big = dt_core::hud::live_check::test_layout(&baked);
        assert!(p.flash(big.clone(), t0));
        let sent = chunk(p.tick(t0, false));
        let msg = whole(&mut p, sent.clone(), &baked, t0);
        assert!(
            msg.rules
                .iter()
                .any(|r| r.value.contains("translateX(-150px)")),
            "{:?}",
            msg.rules
        );
        assert_eq!(
            p.flash_state(),
            Some(FlashState {
                seq: Some(sent.seq),
                applied: None
            })
        );
        let t1 = t0 + Duration::from_secs(1);
        p.line(&ok(sent.seq, &baked), t1);
        assert_eq!(p.flash_state().unwrap().applied, Some(t1));
        p.end_flash(t1);
        assert_eq!(p.flash_state(), None);
        let back = chunk(p.tick(t1, false));
        let msg = whole(&mut p, back, &baked, t1);
        assert!(
            msg.rules
                .iter()
                .any(|r| r.value == "translateX(10px) translateY(0px)"),
            "the profile's layout again: {:?}",
            msg.rules
        );
    }

    #[test]
    fn no_flash_without_a_session() {
        let mut p = LivePreview::new(1);
        p.toggle(true);
        assert!(!p.flash(HudLayout::default(), Instant::now()));
        assert_eq!(p.flash_state(), None);
    }
}
