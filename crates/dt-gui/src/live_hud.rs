//! Live HUD preview in the window: which state the preview is in and what to hand the
//! bridge page when. Pure: `AppState` feeds it events and the page's news, and posts what
//! `tick` returns to `web_bridge`. Design: docs/plans/live-hud/plan.md section 5.

use std::time::{Duration, Instant, SystemTime};

use dt_core::bridge::conlog::{TailReport, TailState};
use dt_core::hud::install::InstallRecord;
use dt_core::hud::layout::{HudFeature, HudLayout};
use dt_core::hud::live::{self, LiveLine, Mailbox, NotLive, Post};
use dt_core::hud::web_bridge::{CONNECTED, PageNews};

/// Edits closer together than this go out as one message.
pub const DEBOUNCE: Duration = Duration::from_millis(100);
/// How long the script and its page may take before the page explains what to check.
pub const HELLO_WAIT: Duration = Duration::from_secs(20);
/// The bridge page sleeps this long after the last edit, or after a HUD page opened.
pub const IDLE: Duration = Duration::from_secs(60);

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
    /// The game runs and the script's page hasn't reached DeadTune yet.
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

/// Where the game's console log stands, for a script that is slow to show up.
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
    "No word from the live script yet, and DeadTune can't find the game's console log. Press \
     Check live preview."
        .to_string()
}

/// What `AppState` hands the bridge page.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Delivery {
    Post(Post),
    /// The session ended: nothing waits for the page.
    Clear,
}

/// The layout the installed pak baked, when that pak carries the live script.
#[derive(Clone, Debug)]
struct Baked {
    /// The record's layout text, to tell an unchanged record without parsing it.
    source: String,
    layout: HudLayout,
    base: String,
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
    /// The base the script last announced (hello, ack, wrongbase or the page), and when.
    hello: Option<(String, Instant)>,
    /// The bridge page polled DeadTune within `CONNECTED`.
    connected: bool,
    last_poll: Option<Instant>,
    error: Option<String>,
    desired: HudLayout,
    not_live: Vec<NotLive>,
    session: Option<Session>,
    /// The bridge still holds a message of an ended session.
    clear: bool,
    next_seq: u32,
    /// Set by the screenshot lever: the state stays put.
    fake: bool,
    /// A HUD change waits for the game to close, so the game runs an older HUD.
    paks_waiting: bool,
    /// The check's bigger minimap, sent in place of the profile's layout while it lasts.
    flash: Option<Flash>,
    /// The player's "Live editing" switch: off, the game shows what Apply baked.
    editing: bool,
    /// A HUD page was on screen in the last frame.
    viewing: bool,
    /// The page stays awake until then: a minute after the last edit or the HUD page
    /// opening.
    active_until: Option<Instant>,
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
            connected: false,
            last_poll: None,
            error: None,
            desired: HudLayout::default(),
            not_live: Vec::new(),
            session: None,
            clear: false,
            next_seq: seed,
            fake: false,
            paks_waiting: false,
            flash: None,
            editing: true,
            viewing: false,
            active_until: None,
        }
    }

    /// Whether the bridge page should be awake: a session runs and either the check's
    /// test is on, or live editing is on and someone edited on a HUD page within `IDLE`.
    /// Asleep, the page holds one request and the script restyles nothing on ids.
    pub fn awake(&self, now: Instant) -> bool {
        (self.session.is_some() || self.fake)
            && (self.flash.is_some()
                || (self.editing
                    && self.viewing
                    && self.active_until.is_some_and(|until| now < until)))
    }

    /// Whether a HUD page was on screen this frame; opening one counts as activity.
    pub fn viewing(&mut self, on_page: bool, now: Instant) {
        if on_page && !self.viewing {
            self.active_until = Some(now + IDLE);
        }
        self.viewing = on_page;
    }

    pub fn editing(&self) -> bool {
        self.editing
    }

    /// The "Live editing" switch. Off sends an empty override set, so the game shows the
    /// HUD as Apply baked it, and puts the page to sleep; on wakes it and sends the edits.
    pub fn set_editing(&mut self, on: bool, now: Instant) {
        if self.editing == on {
            return;
        }
        self.editing = on;
        if on {
            self.active_until = Some(now + IDLE);
        }
        if let Some(session) = self.session.as_mut() {
            session.due = Some(now);
            session.undo = false;
        }
        if let LiveHud::Live { undone, .. } = &mut self.state {
            *undone = false;
        }
    }

    /// The last message the script applied, by either path.
    pub fn applied_seq(&self) -> Option<u32> {
        match &self.state {
            LiveHud::Live { seq, .. } => *seq,
            _ => None,
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

    /// A message is waiting for its time or for the script's answer.
    pub fn busy(&self) -> bool {
        self.clear
            || self
                .session
                .as_ref()
                .is_some_and(|s| s.due.is_some() || s.undo || s.mailbox.pending())
    }

    /// The bridge page polled DeadTune lately; it may drop away.
    pub fn connected(&self) -> bool {
        self.connected
    }

    /// The page's line for the current state. `logs` are the console log candidates.
    pub fn status(&self, logs: &[TailReport], now: Instant) -> Option<StatusLine> {
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
            LiveHud::Waiting { since } if now.saturating_duration_since(*since) < HELLO_WAIT => {
                StatusLine::new("Looking for the live script in game\u{2026}", Tone::Weak)
            }
            LiveHud::Waiting { .. } if self.hello.is_some() => StatusLine::new(
                "The live script runs, but its page hasn't reached DeadTune. Press Check live preview.",
                Tone::Warn,
            )
            .hover(
                "The script opens DeadTune's bridge page from simulieren.github.io; that page \
                 talks to DeadTune on this PC.",
            ),
            LiveHud::Waiting { .. } => StatusLine::new(log_line(logs, now), Tone::Weak),
            LiveHud::Live { .. } if !self.editing => StatusLine::new(
                "Live editing is off: the game shows the HUD from your last Apply",
                Tone::Weak,
            )
            .hover("Turn Live editing on to see your edits in game without a restart."),
            LiveHud::Live { base, acked, .. } => {
                let answer = match acked {
                    Some(at) => format!(
                        "Last answer from the game {} s ago.",
                        now.saturating_duration_since(*at).as_secs()
                    ),
                    None => "No answer from the game yet.".to_string(),
                };
                if self.awake(now) {
                    StatusLine::new("Live in game", Tone::Good)
                        .hover(format!("HUD {base} is running. {answer}"))
                } else {
                    StatusLine::new("Live in game, paused until your next HUD change", Tone::Weak)
                        .hover(format!(
                            "The game keeps your edits. DeadTune pauses the live connection a \
                             minute after your last change and when you leave the HUD pages, so \
                             it costs the game nothing. HUD {base} is running. {answer}"
                        ))
                }
            }
            LiveHud::Error(message) => StatusLine::new(message.clone(), Tone::Bad),
        })
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

    /// The script said which HUD it runs. `fresh`: it just loaded, so it shows the baked
    /// HUD or what its page restored, and everything goes again.
    fn heard(&mut self, base: String, now: Instant, fresh: bool) {
        self.hello = Some((base, now));
        if fresh && let Some(session) = self.session.as_mut() {
            session.mailbox.reload();
            session.due = Some(now);
        }
        self.settle();
    }

    /// The script applied message `done`.
    fn applied(&mut self, done: u32, now: Instant) {
        let Some(session) = self.session.as_mut() else {
            return;
        };
        session.mailbox.ack(done);
        if let Some(flash) = self.flash.as_mut().filter(|f| f.seq == Some(done)) {
            flash.applied = Some(now);
        }
        if let LiveHud::Live { seq, acked, .. } = &mut self.state {
            *seq = Some(done);
            *acked = Some(now);
        }
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
            LiveLine::Hello { base, .. } => self.heard(base, now, true),
            LiveLine::WrongBase { base, .. } => self.heard(base, now, false),
            LiveLine::Ok { seq, base } => {
                if self
                    .session
                    .as_ref()
                    .is_some_and(|s| s.mailbox.base() == base)
                {
                    self.heard(base, now, false);
                    self.applied(seq, now);
                }
            }
            LiveLine::Web(_) => {}
        }
    }

    /// What the bridge page did since the last call: a page that opened is a fresh script,
    /// its polls say it is connected, and its acks count like the console's.
    pub fn page(&mut self, news: PageNews, now: Instant) {
        if matches!(self.game, Game::Closed) {
            return;
        }
        self.last_poll = news.last_poll;
        if let Some(base) = news.hello {
            self.heard(base, now, true);
        } else if let Some(base) = news.base
            && self.hello.as_ref().is_none_or(|(b, _)| *b != base)
        {
            self.heard(base, now, false);
        }
        for seq in news.acked {
            self.applied(seq, now);
        }
        self.reconnect(now);
    }

    fn reconnect(&mut self, now: Instant) {
        let connected = self
            .last_poll
            .is_some_and(|t| now.saturating_duration_since(t) < CONNECTED);
        if connected != self.connected {
            self.connected = connected;
            self.settle();
        }
    }

    /// The profile's HUD layout, after any change; the same layout again is no edit.
    pub fn edit(&mut self, layout: &HudLayout, now: Instant) {
        if *layout == self.desired {
            return;
        }
        self.desired = layout.clone();
        self.active_until = Some(now + IDLE);
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

    /// Something went wrong; `message` is one plain sentence.
    pub fn fail(&mut self, message: String) {
        if self.error.as_ref() == Some(&message) {
            return;
        }
        self.error = Some(message);
        self.settle();
    }

    /// What to hand the bridge page now, if anything.
    pub fn tick(&mut self, now: Instant) -> Option<Delivery> {
        self.reconnect(now);
        if std::mem::take(&mut self.clear) {
            return Some(Delivery::Clear);
        }
        let baked = self.baked.as_ref()?;
        let session = self.session.as_mut()?;
        if !session.undo && session.due.is_none_or(|due| now < due) {
            let post = session.mailbox.poll(now);
            if let (Some(flash), Some(p)) = (self.flash.as_mut(), &post) {
                flash.seq = Some(p.seq);
            }
            return post.map(Delivery::Post);
        }
        let undo = std::mem::take(&mut session.undo);
        session.due = None;
        let rules = match &self.flash {
            Some(flash) => live::overrides(&flash.layout, &baked.layout).map(|o| o.rules),
            None if !self.editing => Ok(Vec::new()),
            None if undo => live::rules(&baked.layout),
            None => live::overrides(&self.desired, &baked.layout).map(|o| o.rules),
        };
        match rules {
            Ok(rules) => {
                let post = session.mailbox.send(rules, now);
                if let (Some(flash), Some(p)) = (self.flash.as_mut(), &post) {
                    flash.seq = Some(p.seq);
                }
                post.map(Delivery::Post)
            }
            Err(e) => {
                self.fail(format!("The live preview can't read this layout: {e}"));
                self.tick(now)
            }
        }
    }

    /// The check's bigger minimap as sent, for the screenshot lever.
    pub fn inject_flash(&mut self, applied: Option<Instant>) {
        self.flash = Some(Flash {
            layout: self.desired.clone(),
            seq: Some(1),
            applied,
        });
    }

    /// Puts the preview in `state` without a game, for the screenshot lever; `heard` is
    /// the base the script announced, if it did.
    pub fn inject(&mut self, state: LiveHud, heard: Option<&str>) {
        self.on = state != LiveHud::Off;
        self.session = None;
        self.fake = true;
        self.hello = heard.map(|base| (base.to_string(), Instant::now()));
        self.state = state;
    }

    /// The state the inputs call for. A session runs while the state is `Waiting` or
    /// `Live`; leaving them clears what the bridge holds.
    fn settle(&mut self) {
        if self.fake {
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
            (Some(baked), Some(_), Some(_)) if self.connected => match &self.state {
                LiveHud::Live { base, .. } if *base == baked.base => return,
                _ => LiveHud::Live {
                    base: baked.base.clone(),
                    seq: None,
                    acked: None,
                    undone: false,
                },
            },
            (Some(_), Some(game), _) => match &self.state {
                LiveHud::Waiting { since } => LiveHud::Waiting { since: *since },
                _ => LiveHud::Waiting { since: game.since },
            },
        };
        let in_session = matches!(next, LiveHud::Waiting { .. } | LiveHud::Live { .. });
        match (in_session, &self.session, &self.baked) {
            (true, None, Some(baked)) => {
                let first_seq = self.next_seq;
                // Sessions of one run never reuse a seq: the script skips the seq it applied last.
                self.next_seq = self.next_seq.wrapping_add(97);
                self.session = Some(Session {
                    mailbox: Mailbox::new(&baked.base, first_seq),
                    due: self.game.running().map(|game| game.since),
                    undo: false,
                });
            }
            (false, Some(_), _) => {
                self.session = None;
                self.flash = None;
                self.clear = true;
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
    use dt_core::hud::live::Kind;

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

    fn base(layout: &HudLayout) -> String {
        live::base_id(layout).unwrap()
    }

    fn hello(layout: &HudLayout) -> String {
        format!(
            "[PanoramaScript] DEADTUNE_LIVE hello {} web=panel",
            base(layout)
        )
    }

    fn ok(seq: u32, layout: &HudLayout) -> String {
        format!("DEADTUNE_LIVE {seq} ok {}", base(layout))
    }

    /// The page polled at `now`; `hello` when it just opened, with `acked` acks.
    fn page(p: &mut LivePreview, layout: &HudLayout, now: Instant, hello: bool, acked: &[u32]) {
        p.page(
            PageNews {
                hello: hello.then(|| base(layout)),
                base: Some(base(layout)),
                last_poll: Some(now),
                open: false,
                acked: acked.to_vec(),
            },
            now,
        );
    }

    fn post(d: Option<Delivery>) -> Post {
        match d {
            Some(Delivery::Post(p)) => p,
            other => panic!("expected a post, got {other:?}"),
        }
    }

    fn message(p: &Post) -> live::Message {
        live::parse_titles(&p.titles).unwrap()
    }

    /// A preview on a running game whose page opened for `baked`, with its first message
    /// applied, so the next tick starts clean.
    fn live_on(baked: &HudLayout, t0: Instant) -> LivePreview {
        let mut p = LivePreview::new(100);
        p.toggle(true);
        p.edit(baked, t0);
        p.record(Some(&record_of(baked, true)), None);
        p.game(true, None, t0);
        page(&mut p, baked, t0, true, &[]);
        let first = post(p.tick(t0));
        assert_eq!(message(&first).kind, Kind::Full);
        page(&mut p, baked, t0, false, &[first.seq]);
        assert_eq!(p.tick(t0 + Duration::from_secs(1)), None);
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
    fn the_page_makes_the_preview_live_with_no_key_and_no_log() {
        let t0 = Instant::now();
        let baked = moved(10);
        let mut p = LivePreview::new(1);
        p.toggle(true);
        p.edit(&baked, t0);
        p.record(Some(&record_of(&baked, true)), None);
        assert_eq!(p.state(), &LiveHud::GameClosed);
        p.game(true, None, t0);
        assert_eq!(p.state(), &LiveHud::Waiting { since: t0 });
        let early = post(p.tick(t0));
        assert_eq!(message(&early).kind, Kind::Full);
        let t1 = t0 + Duration::from_secs(2);
        page(&mut p, &baked, t1, true, &[]);
        assert!(
            matches!(p.state(), LiveHud::Live { seq: None, .. }),
            "{:?}",
            p.state()
        );
        let again = post(p.tick(t1));
        assert!(again.seq > early.seq, "a new page gets everything again");
        assert_eq!(message(&again).kind, Kind::Full);
        page(&mut p, &baked, t1, false, &[again.seq]);
        assert!(
            matches!(p.state(), LiveHud::Live { seq: Some(s), acked: Some(_), .. } if *s == again.seq)
        );
        assert!(!p.busy());
        p.game(false, None, t1);
        assert_eq!(p.state(), &LiveHud::GameClosed);
        assert_eq!(p.tick(t1), Some(Delivery::Clear), "the bridge lets go");
        assert_eq!(p.tick(t1), None, "once");
    }

    #[test]
    fn a_console_hello_alone_waits_for_the_page_and_a_silent_page_drops_back() {
        let t0 = Instant::now();
        let baked = moved(10);
        let mut p = LivePreview::new(1);
        p.toggle(true);
        p.record(Some(&record_of(&baked, true)), None);
        p.game(true, None, t0);
        p.line(&hello(&baked), t0);
        assert_eq!(p.state(), &LiveHud::Waiting { since: t0 });
        let later = t0 + HELLO_WAIT;
        assert_eq!(
            p.status(&[], later).unwrap().text,
            "The live script runs, but its page hasn't reached DeadTune. Press Check live preview."
        );
        page(&mut p, &baked, later, false, &[]);
        assert!(matches!(p.state(), LiveHud::Live { .. }));
        assert!(p.connected());
        p.tick(later + CONNECTED);
        assert_eq!(
            p.state(),
            &LiveHud::Waiting { since: t0 },
            "the page went quiet"
        );
        assert!(!p.connected());
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
        assert_eq!(p.tick(t0), None, "nothing is sent to an older HUD");
        p.game(false, None, t0);
        p.game(true, Some(written + Duration::from_secs(60)), t0);
        assert_eq!(p.state(), &LiveHud::Waiting { since: t0 });
    }

    #[test]
    fn another_base_from_the_page_or_the_log_is_stale() {
        let t0 = Instant::now();
        let baked = moved(10);
        let mut p = LivePreview::new(1);
        p.toggle(true);
        p.record(Some(&record_of(&baked, true)), None);
        p.game(true, None, t0);
        page(&mut p, &moved(99), t0, true, &[]);
        assert_eq!(
            p.state(),
            &LiveHud::Stale {
                base: Some(base(&moved(99)))
            }
        );
        assert_eq!(p.tick(t0), Some(Delivery::Clear), "leaving the session");
        page(&mut p, &baked, t0, true, &[]);
        assert!(matches!(p.state(), LiveHud::Live { .. }));
        assert_eq!(message(&post(p.tick(t0))).kind, Kind::Full);
        p.line(
            "[PanoramaScript] DEADTUNE_LIVE 1 wrongbase cafe0001\r",
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
    fn a_page_that_was_open_before_deadtune_started_counts_without_a_hello() {
        let t0 = Instant::now();
        let baked = moved(10);
        let mut p = LivePreview::new(1);
        p.toggle(true);
        p.record(Some(&record_of(&baked, true)), None);
        page(&mut p, &baked, t0, false, &[]);
        assert_eq!(p.state(), &LiveHud::GameClosed, "no game poll yet");
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
        page(&mut p, &baked, t0, true, &[]);
        p.game(true, None, t0);
        assert_eq!(p.state(), &LiveHud::Waiting { since: t0 });
        page(&mut p, &baked, t0, true, &[]);
        assert!(matches!(p.state(), LiveHud::Live { .. }));
        p.game(false, None, t0);
        p.game(true, None, t0);
        assert_eq!(
            p.state(),
            &LiveHud::Waiting { since: t0 },
            "a new game needs its own page"
        );
    }

    #[test]
    fn edits_go_out_once_after_the_debounce() {
        let t0 = Instant::now();
        let baked = moved(10);
        let mut p = live_on(&baked, t0);
        let t1 = t0 + Duration::from_secs(1);
        p.edit(&moved(20), t1);
        p.edit(&moved(30), t1 + Duration::from_millis(50));
        assert_eq!(
            p.tick(t1 + Duration::from_millis(120)),
            None,
            "still editing"
        );
        let sent = post(p.tick(t1 + Duration::from_millis(160)));
        let m = message(&sent);
        assert_eq!(m.kind, Kind::Patch);
        assert_eq!(m.rules.len(), 1);
        assert_eq!(m.rules[0].value, "translateX(30px) translateY(0px)");
        assert_eq!(p.tick(t1 + Duration::from_millis(200)), None, "sent once");
        p.edit(&moved(30), t1 + Duration::from_millis(300));
        assert_eq!(
            p.tick(t1 + Duration::from_millis(500)),
            None,
            "the same layout is not an edit"
        );
    }

    #[test]
    fn a_newer_edit_replaces_the_message_in_flight_and_either_ack_counts() {
        let t0 = Instant::now();
        let baked = moved(10);
        let mut p = live_on(&baked, t0);
        let t1 = t0 + Duration::from_secs(1);
        p.edit(&moved(20), t1);
        let first = post(p.tick(t1 + DEBOUNCE));
        p.edit(&moved(30), t1 + DEBOUNCE);
        let second = post(p.tick(t1 + DEBOUNCE * 3));
        assert_eq!(second.seq, first.seq + 1);
        assert_eq!(
            message(&second).rules[0].value,
            "translateX(30px) translateY(0px)"
        );
        p.line(&ok(second.seq, &baked), t1 + DEBOUNCE * 3);
        assert!(matches!(p.state(), LiveHud::Live { seq: Some(s), .. } if *s == second.seq));
        assert!(!p.busy(), "the console's ok counts like the page's");
    }

    #[test]
    fn undo_sends_the_baked_rules() {
        let t0 = Instant::now();
        let baked = moved(10);
        let mut p = live_on(&baked, t0);
        let t1 = t0 + Duration::from_secs(1);
        p.edit(&moved(20), t1);
        let sent = post(p.tick(t1 + DEBOUNCE));
        page(&mut p, &baked, t1 + DEBOUNCE, false, &[sent.seq]);
        p.undo();
        assert!(matches!(p.state(), LiveHud::Live { undone: true, .. }));
        let m = message(&post(p.tick(t1 + DEBOUNCE)));
        assert_eq!(m.kind, Kind::Patch, "the keys stay, only values change");
        assert_eq!(m.rules, live::rules(&baked).unwrap());
        p.edit(&moved(40), t1 + Duration::from_secs(1));
        assert!(matches!(p.state(), LiveHud::Live { undone: false, .. }));
    }

    #[test]
    fn dropping_a_key_sends_a_full_message() {
        let t0 = Instant::now();
        let baked = HudLayout::default();
        let mut p = live_on(&baked, t0);
        let t1 = t0 + Duration::from_secs(1);
        p.edit(&moved(20), t1);
        let sent = post(p.tick(t1 + DEBOUNCE));
        page(&mut p, &baked, t1 + DEBOUNCE, false, &[sent.seq]);
        p.edit(&baked, t1 + DEBOUNCE);
        assert_eq!(message(&post(p.tick(t1 + DEBOUNCE * 2))).kind, Kind::Full);
    }

    fn secs(n: u64) -> Duration {
        Duration::from_secs(n)
    }

    #[test]
    fn the_page_is_awake_only_while_someone_edits_on_a_hud_page() {
        let t0 = Instant::now();
        let baked = moved(10);
        let mut p = LivePreview::new(1);
        p.toggle(true);
        p.record(Some(&record_of(&baked, true)), None);
        p.viewing(true, t0);
        assert!(!p.awake(t0), "no game, nothing to wake");
        let mut p = live_on(&baked, t0);
        assert!(!p.awake(t0), "not on a HUD page");
        p.viewing(true, t0);
        assert!(p.awake(t0), "a HUD page opened");
        p.viewing(true, t0 + secs(30));
        assert!(
            p.awake(
                IDLE.checked_sub(Duration::from_millis(1))
                    .map(|d| t0 + d)
                    .unwrap()
            )
        );
        assert!(!p.awake(t0 + IDLE), "a minute without edits");
        let t1 = t0 + IDLE + secs(5);
        p.edit(&moved(20), t1);
        assert!(p.awake(t1), "an edit wakes it");
        assert!(!p.awake(t1 + IDLE));
        p.viewing(false, t1 + secs(1));
        assert!(!p.awake(t1 + secs(1)), "left the HUD pages");
        p.edit(&moved(30), t1 + secs(2));
        assert!(!p.awake(t1 + secs(2)), "an edit elsewhere doesn't wake it");
        p.viewing(true, t1 + secs(3));
        assert!(p.awake(t1 + secs(3)), "back on a HUD page");
        p.game(false, None, t1 + secs(4));
        assert!(!p.awake(t1 + secs(4)), "the game closed");
    }

    #[test]
    fn the_check_wakes_the_page_from_any_page() {
        let t0 = Instant::now();
        let baked = moved(10);
        let mut p = live_on(&baked, t0);
        assert!(p.flash(moved(99), t0));
        assert!(p.awake(t0 + IDLE * 2), "the test runs");
        p.end_flash(t0);
        assert!(!p.awake(t0));
        p.set_editing(false, t0);
        assert!(p.flash(moved(99), t0));
        assert!(p.awake(t0), "the check runs with live editing off too");
    }

    #[test]
    fn live_editing_off_shows_the_baked_hud_at_once_and_on_brings_the_edits_back() {
        let t0 = Instant::now();
        let baked = moved(10);
        let mut p = live_on(&baked, t0);
        p.viewing(true, t0);
        p.edit(&moved(30), t0);
        let edit = post(p.tick(t0 + DEBOUNCE));
        page(&mut p, &baked, t0, false, &[edit.seq]);
        assert_eq!(p.applied_seq(), Some(edit.seq));

        let t1 = t0 + secs(1);
        p.set_editing(false, t1);
        assert!(!p.editing());
        assert!(!p.awake(t1), "asleep at once");
        let off = post(p.tick(t1));
        let m = message(&off);
        assert_eq!(
            (m.kind, m.rules.len()),
            (Kind::Full, 0),
            "nothing over the baked HUD"
        );
        page(&mut p, &baked, t1, false, &[off.seq]);
        let line = p.status(&[], t1).unwrap();
        assert!(line.text.starts_with("Live editing is off"), "{line:?}");

        let t2 = t1 + secs(1);
        p.edit(&moved(40), t2);
        assert_eq!(p.tick(t2 + DEBOUNCE), None, "edits stay in DeadTune");
        assert!(!p.awake(t2));

        let t3 = t2 + secs(1);
        p.set_editing(true, t3);
        assert!(p.awake(t3));
        let on = post(p.tick(t3));
        assert!(
            message(&on)
                .rules
                .iter()
                .any(|r| r.value == "translateX(40px) translateY(0px)"),
            "{:?}",
            message(&on)
        );
        p.set_editing(true, t3);
        assert_eq!(p.tick(t3), None, "on again is no change");
    }

    #[test]
    fn status_says_when_the_preview_rests() {
        let t0 = Instant::now();
        let baked = moved(10);
        let mut p = live_on(&baked, t0);
        p.viewing(true, t0);
        assert_eq!(p.status(&[], t0).unwrap().text, "Live in game");
        let rest = p.status(&[], t0 + IDLE).unwrap();
        assert!(rest.text.contains("paused"), "{rest:?}");
        assert_eq!(rest.tone, Tone::Weak);
    }

    #[test]
    fn toggling_off_while_live_clears_the_bridge() {
        let t0 = Instant::now();
        let baked = moved(10);
        let mut p = live_on(&baked, t0);
        p.toggle(false);
        assert_eq!(p.state(), &LiveHud::Off);
        assert_eq!(p.tick(t0), Some(Delivery::Clear));
        p.toggle(false);
        assert_eq!(p.tick(t0), None);
    }

    #[test]
    fn an_error_holds_until_the_preview_is_turned_off() {
        let t0 = Instant::now();
        let baked = moved(10);
        let mut p = live_on(&baked, t0);
        p.fail("The live preview can't read this layout".into());
        assert_eq!(
            p.state(),
            &LiveHud::Error("The live preview can't read this layout".into())
        );
        assert_eq!(p.tick(t0), Some(Delivery::Clear));
        p.toggle(false);
        p.toggle(true);
        assert!(matches!(p.state(), LiveHud::Live { .. }));
    }

    #[test]
    fn status_lines_say_what_to_do() {
        use std::path::PathBuf;
        let t0 = Instant::now();
        let baked = moved(10);
        let text = |p: &LivePreview, logs: &[TailReport], now| p.status(logs, now).map(|l| l.text);
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
                .contains("can't find the game's console log. Press Check live preview."),
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
        page(&mut p, &baked, later, true, &[]);
        p.viewing(true, later);
        assert_eq!(text(&p, &[], later).unwrap(), "Live in game");
    }

    #[test]
    fn a_flash_sends_its_layout_until_it_ends_and_knows_when_it_showed() {
        let t0 = Instant::now();
        let baked = moved(10);
        let mut p = live_on(&baked, t0);
        let big = dt_core::hud::live_check::test_layout(&baked);
        assert!(p.flash(big, t0));
        let sent = post(p.tick(t0));
        assert!(
            message(&sent)
                .rules
                .iter()
                .any(|r| r.value.contains("translateX(-150px)")),
            "{:?}",
            message(&sent).rules
        );
        assert_eq!(
            p.flash_state(),
            Some(FlashState {
                seq: Some(sent.seq),
                applied: None
            })
        );
        let t1 = t0 + Duration::from_secs(1);
        page(&mut p, &baked, t1, false, &[sent.seq]);
        assert_eq!(p.flash_state().unwrap().applied, Some(t1));
        p.end_flash(t1);
        assert_eq!(p.flash_state(), None);
        let back = message(&post(p.tick(t1)));
        assert!(
            back.rules
                .iter()
                .any(|r| r.value == "translateX(10px) translateY(0px)"),
            "the profile's layout again: {:?}",
            back.rules
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
