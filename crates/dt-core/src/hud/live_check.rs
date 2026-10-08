//! "Check live preview": everything DeadTune can find out about the live HUD preview,
//! turned into a short checklist and one next step. `diagnose` is pure over `Facts`; the
//! `read_*` functions gather them from the game folder. Design: docs/plans/live-hud/plan.md
//! section 9.

use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use super::install::ADDON_FILE;
use super::live::{self, LiveLine, OWN_SCRIPT};
use super::vpk::VpkDir;
use super::web_bridge::{Counters, PORT};
use crate::bridge::ack::BOOT;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum GameFacts {
    Closed,
    Running { started: Option<SystemTime> },
}

/// The console log the game writes most recently, and what is in it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LogFacts {
    NotFound {
        looked: Vec<PathBuf>,
    },
    Found {
        path: PathBuf,
        modified: Option<SystemTime>,
        text: String,
    },
}

/// DeadTune's installed HUD pak, read back from disk.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PakFacts {
    Missing,
    Unreadable(String),
    Read {
        written: Option<SystemTime>,
        /// The base baked into the live script, when the pak carries it.
        script_base: Option<String>,
    },
}

/// What DeadTune's server on 127.0.0.1 saw of the bridge page (`web_bridge`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct WebFacts {
    /// The port DeadTune listens on, or why it couldn't.
    pub listening: Option<Result<u16, String>>,
    pub counters: Counters,
    pub last_poll_ago: Option<Duration>,
    /// The HUD the page's script was built for.
    pub base: Option<String>,
    pub recent_acks: Vec<u32>,
}

/// The visible test: DeadTune sent a bigger minimap through the live channel.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TestFacts {
    NotRun,
    Sent {
        seq: u32,
        /// What the player said: did the minimap get bigger?
        saw: Option<bool>,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Facts {
    pub now: SystemTime,
    pub preview_on: bool,
    pub game: GameFacts,
    pub log: LogFacts,
    pub pak: PakFacts,
    /// A HUD change waits for the game to close.
    pub pending_hud: bool,
    pub web: WebFacts,
    pub test: TestFacts,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tone {
    Good,
    Warn,
    Bad,
    /// Not checked, because an earlier row already explains the problem.
    Skip,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Row {
    pub tone: Tone,
    pub text: String,
    pub detail: Option<String>,
}

/// The one thing to do next.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Step {
    Works,
    TurnOn,
    StartGame,
    Apply,
    /// The game runs an older HUD, or was started without its console log.
    CloseAndLaunch,
    TryAgain,
    SendReport(String),
}

impl Step {
    pub fn text(&self) -> String {
        match self {
            Step::Works => "Edits on the HUD pages show in game as you make them.".into(),
            Step::TurnOn => "Turn on Live preview on the HUD page, then press Apply.".into(),
            Step::StartGame => {
                "Start Deadlock with Launch in DeadTune, then press Check live preview again."
                    .into()
            }
            Step::Apply => {
                "Press Apply, close Deadlock completely, then press Launch in DeadTune.".into()
            }
            Step::CloseAndLaunch => {
                "Close Deadlock completely, then press Launch in DeadTune.".into()
            }
            Step::TryAgain => {
                "Wait until you are in a match or the hideout, then press Check live preview again."
                    .into()
            }
            Step::SendReport(why) => format!("{why} Press Copy report and paste it to us."),
        }
    }

    /// The step can be done for the player: close the game and start it through DeadTune.
    pub fn restarts(&self) -> bool {
        matches!(self, Step::CloseAndLaunch)
    }
}

/// What the game said in the console log since it started.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Session {
    /// This session's lines: from the last boot marker on, or the whole log without one.
    pub lines: Vec<String>,
    pub from_boot: bool,
}

impl Session {
    pub fn of(text: &str) -> Session {
        let all: Vec<&str> = text.lines().map(|l| l.trim_end_matches('\r')).collect();
        let boot = all.iter().rposition(|l| l.contains(BOOT));
        Session {
            lines: all[boot.unwrap_or(0)..]
                .iter()
                .map(|l| l.to_string())
                .collect(),
            from_boot: boot.is_some(),
        }
    }

    fn live_lines(&self) -> impl Iterator<Item = (LiveLine, &String)> {
        self.lines
            .iter()
            .filter_map(|l| live::parse_line(l).map(|line| (line, l)))
    }

    /// The script's hello lines: its base and the line itself.
    pub fn hellos(&self) -> Vec<(String, String)> {
        self.live_lines()
            .filter_map(|(line, raw)| match line {
                LiveLine::Hello { base, .. } => Some((base, raw.trim().to_string())),
                _ => None,
            })
            .collect()
    }

    /// What the web panel reported, each report once, in order.
    pub fn web(&self) -> Vec<String> {
        let mut out: Vec<String> = Vec::new();
        for (line, _) in self.live_lines() {
            if let LiveLine::Web(text) = line
                && !out.contains(&text)
            {
                out.push(text);
            }
        }
        out
    }

    /// The script's answer to message `seq`.
    fn answer(&self, seq: u32) -> Option<LiveLine> {
        self.live_lines()
            .map(|(line, _)| line)
            .filter(|line| match line {
                LiveLine::Ok { seq: s, .. } | LiveLine::WrongBase { seq: s, .. } => *s == seq,
                LiveLine::Hello { .. } | LiveLine::Web(_) => false,
            })
            .max_by_key(|line| matches!(line, LiveLine::Ok { .. }))
    }

    /// Panorama and script errors that name our script, the HUD layout or our panels.
    pub fn errors(&self) -> Vec<String> {
        self.lines.iter().filter(|l| is_error(l)).cloned().collect()
    }
}

const OURS: &[&str] = &[
    "deadtune/live_hud",
    "live_hud.vjs",
    "dtlive",
    "panorama/layout/hud.",
    "layout file hud.",
    "hud.xml",
];
const TROUBLE: &[&str] = &[
    "error",
    "fail",
    "unable",
    "exception",
    "uncaught",
    "not defined",
    "cannot",
    "couldn't",
];

fn is_error(line: &str) -> bool {
    let lower = line.to_ascii_lowercase();
    !line.contains("DEADTUNE_")
        && OURS.iter().any(|o| lower.contains(o))
        && TROUBLE.iter().any(|t| lower.contains(t))
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Diagnosis {
    pub rows: Vec<Row>,
    pub step: Step,
    /// Error lines from the console log, verbatim.
    pub errors: Vec<String>,
}

impl Diagnosis {
    pub fn works(&self) -> bool {
        self.step == Step::Works
    }
}

fn ago(now: SystemTime, then: SystemTime) -> String {
    secs_ago(now.duration_since(then).unwrap_or_default())
}

fn secs_ago(d: Duration) -> String {
    let secs = d.as_secs();
    match secs {
        0..=89 => format!("{secs} s ago"),
        90..=5399 => format!("{} min ago", (secs + 30) / 60),
        _ => format!("{} h ago", (secs + 1800) / 3600),
    }
}

fn row(tone: Tone, text: impl Into<String>) -> Row {
    Row {
        tone,
        text: text.into(),
        detail: None,
    }
}

fn with(mut r: Row, detail: impl Into<String>) -> Row {
    r.detail = Some(detail.into());
    r
}

/// Process start times have one-second resolution, and the pak is written moments
/// before a launch through DeadTune.
const START_SLACK: Duration = Duration::from_secs(1);

/// The web panel's own reports, read from the script's lines.
struct PageReports {
    ready: Option<String>,
    blocked: Option<String>,
    no_panel: bool,
    all: Vec<String>,
}

impl PageReports {
    fn of(web: Vec<String>) -> PageReports {
        PageReports {
            ready: web
                .iter()
                .find_map(|w| w.strip_prefix("ready").map(|r| r.trim().to_string())),
            blocked: web.iter().rev().find_map(|w| {
                w.strip_prefix("fetch blocked")
                    .map(|r| r.trim().to_string())
            }),
            no_panel: web.iter().any(|w| w.starts_with("nopanel")),
            all: web,
        }
    }
}

pub fn diagnose(f: &Facts) -> Diagnosis {
    let mut rows = Vec::new();
    let mut steps: Vec<Step> = Vec::new();
    let running = match &f.game {
        GameFacts::Running { started } => Some(*started),
        GameFacts::Closed => None,
    };
    let polls = f.web.counters.polls;
    if !f.preview_on {
        steps.push(Step::TurnOn);
    }

    rows.push(match running {
        Some(Some(at)) => row(
            Tone::Good,
            format!("Deadlock is running (started {})", ago(f.now, at)),
        ),
        Some(None) => row(Tone::Good, "Deadlock is running"),
        None => {
            steps.push(Step::StartGame);
            row(Tone::Bad, "Deadlock is not running")
        }
    });

    // Without the console log the preview still works through the page; only the script's
    // own report is missing then.
    let no_log_tone = if polls > 0 { Tone::Warn } else { Tone::Bad };
    let started = running.flatten();
    let session = match &f.log {
        LogFacts::NotFound { looked } => {
            let looked: Vec<String> = looked.iter().map(|p| p.display().to_string()).collect();
            if running.is_some() && polls == 0 {
                steps.push(Step::CloseAndLaunch);
            }
            rows.push(with(
                row(
                    no_log_tone,
                    "No console log found. Deadlock writes one only when started through DeadTune's Launch (it adds -condebug).",
                ),
                format!("Looked for:\n{}", looked.join("\n")),
            ));
            None
        }
        LogFacts::Found {
            path,
            modified,
            text,
        } => {
            let when = modified.map_or("unknown".to_string(), |m| ago(f.now, m));
            let stale_log =
                matches!((started, modified), (Some(s), Some(m)) if *m + START_SLACK < s);
            if stale_log {
                if polls == 0 {
                    steps.push(Step::CloseAndLaunch);
                }
                rows.push(row(
                    no_log_tone,
                    format!(
                        "This game isn't writing a console log: {} last changed {when}, before Deadlock started",
                        path.display()
                    ),
                ));
                None
            } else {
                rows.push(row(
                    Tone::Good,
                    format!("Console log: {} (last changed {when})", path.display()),
                ));
                Some(Session::of(text))
            }
        }
    };

    let disk_base = match &f.pak {
        PakFacts::Missing => {
            if !f.pending_hud {
                steps.push(Step::Apply);
            }
            rows.push(row(Tone::Bad, "No DeadTune HUD is installed"));
            None
        }
        PakFacts::Unreadable(why) => {
            rows.push(row(
                Tone::Bad,
                format!("DeadTune can't read its installed HUD: {why}"),
            ));
            steps.push(Step::SendReport(
                "DeadTune can't read its own HUD file.".into(),
            ));
            None
        }
        PakFacts::Read { script_base, .. } => {
            match script_base {
                Some(base) => rows.push(row(
                    Tone::Good,
                    format!("The installed HUD carries the live script (HUD {base})"),
                )),
                None if f.pending_hud => rows.push(row(
                    Tone::Warn,
                    "The installed HUD has no live script yet; your waiting change adds it",
                )),
                None => {
                    steps.push(Step::Apply);
                    rows.push(row(Tone::Bad, "The installed HUD has no live script"))
                }
            }
            script_base.clone()
        }
    };

    if f.pending_hud {
        rows.push(row(
            Tone::Warn,
            "Your last HUD change is waiting until Deadlock closes (Windows locks the HUD file while the game runs)",
        ));
        if running.is_some() {
            steps.push(Step::CloseAndLaunch);
        }
    }

    let hellos = session.as_ref().map(Session::hellos).unwrap_or_default();
    let game_base = hellos
        .last()
        .map(|(base, _)| base.clone())
        .or_else(|| f.web.base.clone());
    let written = match &f.pak {
        PakFacts::Read { written, .. } => *written,
        _ => None,
    };
    let older = matches!((started, written), (Some(s), Some(w)) if s + START_SLACK < w);
    let other_base = match (&game_base, &disk_base) {
        (Some(game), Some(disk)) if game != disk => Some(game.clone()),
        _ => None,
    };
    let stale = older || other_base.is_some() || f.pending_hud;
    if running.is_some() {
        rows.push(if stale {
            steps.push(Step::CloseAndLaunch);
            let why = match (&other_base, older, started, written) {
                (Some(other), _, _, _) => format!(
                    "the script in game says HUD {other}, the file on disk is HUD {}",
                    disk_base.clone().unwrap_or_default()
                ),
                (None, true, Some(s), Some(w)) => format!(
                    "Deadlock started {}, the HUD file was written {}",
                    ago(f.now, s),
                    ago(f.now, w)
                ),
                _ => "the new HUD goes in when Deadlock closes".to_string(),
            };
            row(
                Tone::Bad,
                format!("Deadlock is running the HUD from before your last Apply: {why}"),
            )
        } else if disk_base.is_some() {
            row(Tone::Good, "Deadlock loaded the HUD that is on disk")
        } else {
            row(Tone::Skip, "Which HUD Deadlock loaded: not checked")
        });
    }

    let can_run = disk_base.is_some() && running.is_some() && !stale;
    let errors = session.as_ref().map(Session::errors).unwrap_or_default();
    let script_runs = !hellos.is_empty() || f.web.counters.hellos > 0 || polls > 0;
    match (script_runs, &session) {
        (true, _) => {
            let detail = hellos.first().map_or(
                "Seen through the bridge page; no console log line.".to_string(),
                |(_, line)| line.clone(),
            );
            rows.push(with(row(Tone::Good, "The live script is running"), detail));
        }
        (false, None) => rows.push(row(
            Tone::Skip,
            "Whether the live script started: not checked",
        )),
        (false, Some(s)) => {
            let scope = if s.from_boot {
                "since Deadlock started"
            } else {
                "anywhere in the console log"
            };
            rows.push(row(
                Tone::Bad,
                format!("The live script didn't say hello {scope}, so it isn't running"),
            ));
            if can_run {
                steps.push(Step::SendReport(if errors.is_empty() {
                    "The live script didn't start, and the game logged no error about it.".into()
                } else {
                    "The live script failed to load.".into()
                }));
            }
        }
    }

    let page = PageReports::of(session.as_ref().map(Session::web).unwrap_or_default());
    let page_detail = (!page.all.is_empty()).then(|| page.all.join("\n"));
    let attach = |r: Row| match &page_detail {
        Some(d) => with(r, d.clone()),
        None => r,
    };
    let page_loaded = page.ready.is_some() || polls > 0;
    rows.push(attach(match (&page.ready, page.no_panel) {
        (Some(ready), _) => {
            let storage = if ready.contains("storage=ok") {
                "it can keep your live edits for the next game start"
            } else {
                "it can't keep your live edits for the next game start"
            };
            row(
                Tone::Good,
                format!("DeadTune's bridge page loaded in game; {storage}"),
            )
        }
        (None, _) if polls > 0 => row(
            Tone::Good,
            "DeadTune's bridge page loaded in game (it reached DeadTune)",
        ),
        (None, true) => {
            if can_run {
                steps.push(Step::SendReport(
                    "The game wouldn't make a web panel for the live script.".into(),
                ));
            }
            row(
                Tone::Bad,
                "The game wouldn't make a web panel, so the bridge page can't load",
            )
        }
        (None, false) if script_runs => {
            if can_run {
                steps.push(Step::SendReport(
                    "The game didn't load DeadTune's bridge page from GitHub.".into(),
                ));
            }
            row(
                Tone::Bad,
                "DeadTune's bridge page didn't load in game. Check that this PC can open simulieren.github.io in a browser.",
            )
        }
        (None, false) => row(Tone::Skip, "Whether the bridge page loaded: not checked"),
    }));

    let server = match &f.web.listening {
        Some(Ok(port)) => format!("DeadTune listens on 127.0.0.1:{port}"),
        Some(Err(why)) => format!("DeadTune couldn't listen on 127.0.0.1:{PORT}: {why}"),
        None => "DeadTune's server didn't start".to_string(),
    };
    rows.push(match (&f.web.listening, polls) {
        (_, n) if n > 0 => with(
            row(
                Tone::Good,
                format!(
                    "The bridge page reaches DeadTune ({n} request{}, last {})",
                    plural(n as usize),
                    f.web.last_poll_ago.map_or("unknown".into(), secs_ago)
                ),
            ),
            server,
        ),
        (Some(Err(_)), _) => {
            steps.push(Step::SendReport(format!(
                "Another program holds port {PORT}. Close other DeadTune windows, then start DeadTune again."
            )));
            row(Tone::Bad, server)
        }
        _ if page.blocked.is_some() => {
            if can_run {
                steps.push(Step::SendReport(
                    "The bridge page loaded but the game's browser won't let it reach DeadTune."
                        .into(),
                ));
            }
            with(
                row(
                    Tone::Bad,
                    format!(
                        "The bridge page can't reach DeadTune: {}",
                        page.blocked.clone().unwrap_or_default()
                    ),
                ),
                server,
            )
        }
        _ if page_loaded => with(
            row(Tone::Bad, "The bridge page hasn't reached DeadTune yet"),
            server,
        ),
        _ => with(
            row(Tone::Skip, "Whether the bridge page reaches DeadTune: not checked"),
            server,
        ),
    });

    rows.push(match (errors.len(), &session) {
        (_, None) => row(Tone::Skip, "Script errors in the console log: not checked"),
        (0, _) => row(
            Tone::Good,
            "No script errors about the HUD in the console log",
        ),
        (n, _) => with(
            row(
                Tone::Bad,
                format!(
                    "The console log has {n} error line{} about the HUD",
                    plural(n)
                ),
            ),
            errors
                .iter()
                .take(5)
                .cloned()
                .collect::<Vec<_>>()
                .join("\n"),
        ),
    });

    match &f.test {
        TestFacts::NotRun => rows.push(row(Tone::Skip, "Minimap test: not run")),
        TestFacts::Sent { seq, saw } => {
            let answer = session.as_ref().and_then(|s| s.answer(*seq));
            let applied =
                f.web.recent_acks.contains(seq) || matches!(answer, Some(LiveLine::Ok { .. }));
            let sent = format!("DeadTune sent it as message {seq}");
            match (&answer, applied) {
                (_, true) => rows.push(with(
                    row(Tone::Good, "The script applied the bigger minimap"),
                    sent,
                )),
                (Some(LiveLine::WrongBase { base, .. }), false) => {
                    steps.push(Step::CloseAndLaunch);
                    rows.push(with(
                        row(
                            Tone::Bad,
                            format!("The script got the bigger minimap, but it belongs to HUD {base}, an older one"),
                        ),
                        sent,
                    ))
                }
                _ => {
                    if polls > 0 {
                        steps.push(Step::TryAgain);
                    }
                    rows.push(with(
                        row(Tone::Bad, "The bigger minimap never reached the script"),
                        sent,
                    ))
                }
            }
            match saw {
                Some(true) => rows.push(row(Tone::Good, "You saw the minimap get bigger")),
                Some(false) => {
                    if applied {
                        steps.push(Step::SendReport(
                            "The script got the bigger minimap but the HUD didn't change.".into(),
                        ));
                    }
                    rows.push(row(Tone::Bad, "You didn't see the minimap get bigger"))
                }
                None => {}
            }
        }
    }

    let saw_it = matches!(
        f.test,
        TestFacts::Sent {
            saw: Some(true),
            ..
        }
    );
    let step = if saw_it && f.preview_on {
        Step::Works
    } else {
        steps
            .into_iter()
            .min_by_key(priority)
            .unwrap_or(match &f.test {
                TestFacts::NotRun => Step::TryAgain,
                TestFacts::Sent { .. } => {
                    Step::SendReport("Everything DeadTune can see looks right.".into())
                }
            })
    };
    Diagnosis { rows, step, errors }
}

fn priority(step: &Step) -> u8 {
    match step {
        Step::TurnOn => 0,
        Step::StartGame => 1,
        Step::Apply => 2,
        Step::CloseAndLaunch => 3,
        Step::SendReport(_) => 4,
        Step::TryAgain => 5,
        Step::Works => 6,
    }
}

fn plural(n: usize) -> &'static str {
    if n == 1 { "" } else { "s" }
}

/// Plain text for Copy report: the checklist, the next step, the server's counters, then
/// the console log lines that matter.
pub fn report(f: &Facts, d: &Diagnosis, version: &str) -> String {
    let mut out = format!("DeadTune {version} live preview check\n\n");
    for r in &d.rows {
        let mark = match r.tone {
            Tone::Good => "[ok]  ",
            Tone::Warn => "[warn]",
            Tone::Bad => "[FAIL]",
            Tone::Skip => "[skip]",
        };
        out.push_str(&format!("{mark} {}\n", r.text));
        for line in r.detail.iter().flat_map(|d| d.lines()) {
            out.push_str(&format!("         {line}\n"));
        }
    }
    out.push_str(&format!("\nNext step: {}\n", d.step.text()));
    out.push_str(&format!(
        "\nPreview on: {}. HUD change waiting: {}. Test: {:?}.\n",
        f.preview_on, f.pending_hud, f.test
    ));
    let c = &f.web.counters;
    out.push_str(&format!(
        "Bridge server: {:?}; polls {}, page starts {}, messages delivered {}, acks {}, preflights {}, plain http loads {}, refused {}, other {}; last poll {}; page HUD {}; recent acks {:?}.\n",
        f.web.listening,
        c.polls,
        c.hellos,
        c.delivered,
        c.acks,
        c.preflights,
        c.controls,
        c.refused,
        c.other,
        f.web.last_poll_ago.map_or("never".into(), secs_ago),
        f.web.base.as_deref().unwrap_or("unknown"),
        f.web.recent_acks
    ));
    match &f.pak {
        PakFacts::Read {
            written,
            script_base,
        } => out.push_str(&format!(
            "HUD file: script {}, written {}.\n",
            script_base.as_deref().unwrap_or("none"),
            written.map_or("unknown".into(), |w| ago(f.now, w))
        )),
        other => out.push_str(&format!("HUD file: {other:?}\n")),
    }
    if let LogFacts::Found { path, text, .. } = &f.log {
        let session = Session::of(text);
        let ours: Vec<&String> = session
            .lines
            .iter()
            .filter(|l| l.contains("DEADTUNE") || l.contains("DeadTune") || l.contains("deadtune"))
            .collect();
        let skip = ours.len().saturating_sub(200);
        out.push_str(&format!(
            "\n--- {} lines about DeadTune in {} (this session: {}) ---\n",
            ours.len(),
            path.display(),
            if session.from_boot {
                "from the boot marker"
            } else {
                "whole file"
            }
        ));
        for l in &ours[skip..] {
            out.push_str(l);
            out.push('\n');
        }
        out.push_str(&format!("\n--- {} error lines ---\n", d.errors.len()));
        for l in d.errors.iter().take(50) {
            out.push_str(l);
            out.push('\n');
        }
        let tail = session.lines.len().saturating_sub(50);
        out.push_str("\n--- last 50 lines ---\n");
        for l in &session.lines[tail..] {
            out.push_str(l);
            out.push('\n');
        }
    }
    out
}

/// The newest of `candidates` that exists, read whole (the last 8 MB of a bigger one).
pub fn read_log(candidates: &[PathBuf]) -> LogFacts {
    const MAX: u64 = 8 << 20;
    let newest = candidates
        .iter()
        .filter_map(|p| {
            let meta = std::fs::metadata(p).ok()?;
            Some((p, meta.modified().ok(), meta.len()))
        })
        .max_by_key(|(_, modified, _)| *modified);
    let Some((path, modified, len)) = newest else {
        return LogFacts::NotFound {
            looked: candidates.to_vec(),
        };
    };
    let text = read_tail(path, len.saturating_sub(MAX)).unwrap_or_default();
    LogFacts::Found {
        path: path.clone(),
        modified,
        text,
    }
}

fn read_tail(path: &Path, from: u64) -> std::io::Result<String> {
    use std::io::{Read, Seek, SeekFrom};
    let mut file = std::fs::File::open(path)?;
    file.seek(SeekFrom::Start(from))?;
    let mut buf = Vec::new();
    file.read_to_end(&mut buf)?;
    Ok(String::from_utf8_lossy(&buf).into_owned())
}

/// DeadTune's HUD pak in `addons_dir`: whether it carries the live script and which base
/// the script was built for.
pub fn read_pak(addons_dir: &Path) -> PakFacts {
    let path = addons_dir.join(ADDON_FILE);
    let written = match std::fs::metadata(&path) {
        Ok(meta) => meta.modified().ok(),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return PakFacts::Missing,
        Err(e) => return PakFacts::Unreadable(e.to_string()),
    };
    let pak = match VpkDir::open(&path) {
        Ok(pak) => pak,
        Err(e) => return PakFacts::Unreadable(e.to_string()),
    };
    let script_base = pak
        .read(OWN_SCRIPT)
        .ok()
        .and_then(|bytes| script_base(&String::from_utf8_lossy(&bytes)));
    PakFacts::Read {
        written,
        script_base,
    }
}

fn script_base(text: &str) -> Option<String> {
    let rest = &text[text.find("var DT_LIVE")?..];
    let rest = &rest[rest.find("base: \"")? + 7..];
    Some(rest[..rest.find('"')?].to_string())
}

/// The bigger minimap the visible test shows: half again as big, its scale growing from
/// the bottom-right corner, nudged toward the screen's centre.
pub fn test_layout(layout: &super::HudLayout) -> super::HudLayout {
    use super::elements::ElementId;
    use super::layout::{ElementEdit, Visibility};
    let mut out = layout.clone();
    let was = layout
        .elements
        .get(&ElementId::Minimap)
        .cloned()
        .unwrap_or_default();
    out.elements.insert(
        ElementId::Minimap,
        ElementEdit {
            visibility: Visibility::Vanilla,
            offset_x: was.offset_x - 160,
            offset_y: was.offset_y - 80,
            scale_pct: (u32::from(was.scale_pct) * 3 / 2).clamp(150, 250) as u16,
            opacity_pct: 100,
        },
    );
    out
}

/// The names `sample` knows.
pub const SAMPLES: &[&str] = &[
    "works",
    "old_pak",
    "pending",
    "no_condebug",
    "script_error",
    "no_page",
    "fetch_blocked",
    "port_busy",
    "not_followed",
    "not_seen",
];

/// Facts for one situation the check tells apart, for the screenshot lever and tests; the
/// names are `SAMPLES`.
pub fn sample(kind: &str) -> Option<Facts> {
    let at = |secs: u64| SystemTime::UNIX_EPOCH + Duration::from_secs(1_800_000_000 + secs);
    let base = "1a2b3c4d";
    let say = |text: &str| format!("[PanoramaScript] DEADTUNE_LIVE {text}");
    let hello = say(&format!("hello {base} web=panel"));
    let ready = say("web ready storage=ok");
    let fetch_ok = say("web fetch ok");
    let control = say("web control title=about:blank");
    let ok = say(&format!("300 ok {base}"));
    let mut lines: Vec<String> = vec![
        "[Console] DEADTUNE_BOOT 0.27.0".into(),
        "[Panorama] Loading layout panorama/layout/hud.xml".into(),
    ];
    let reached = WebFacts {
        listening: Some(Ok(PORT)),
        counters: Counters {
            polls: 412,
            hellos: 1,
            delivered: 3,
            acks: 3,
            preflights: 1,
            ..Counters::default()
        },
        last_poll_ago: Some(Duration::from_millis(200)),
        base: Some(base.into()),
        recent_acks: vec![298, 299, 300],
    };
    let unreached = WebFacts {
        listening: Some(Ok(PORT)),
        ..WebFacts::default()
    };
    let mut web = reached.clone();
    let mut test = TestFacts::NotRun;
    let mut written = at(50);
    let mut modified = at(590);
    let mut pending_hud = false;
    let mut script_base = Some(base.to_string());
    let sent = |saw| TestFacts::Sent { seq: 300, saw };
    let healthy = [hello.clone(), ready.clone(), fetch_ok, control.clone()];
    match kind {
        "works" => {
            lines.extend(healthy);
            lines.push(ok);
            test = sent(Some(true));
        }
        "old_pak" => {
            written = at(300);
            web = unreached;
        }
        "pending" => {
            pending_hud = true;
            script_base = None;
            web = unreached;
        }
        "no_condebug" => {
            modified = at(10);
            web = unreached;
        }
        "script_error" => {
            lines.push(
                "[Panorama] JS error: panorama/scripts/deadtune/live_hud.vjs_c:12: ReferenceError: DT_LIVE is not defined".into(),
            );
            web = unreached;
        }
        "no_page" => {
            lines.extend([hello, control, say("web retry 1")]);
            web = unreached;
        }
        "fetch_blocked" => {
            lines.extend([
                hello,
                ready,
                say("web fetch blocked TypeError: Failed to fetch"),
                control,
            ]);
            web = unreached;
        }
        "port_busy" => {
            lines.extend([
                hello,
                ready,
                say("web fetch blocked TypeError: Failed to fetch"),
            ]);
            web = WebFacts {
                listening: Some(Err(
                    "Only one usage of each socket address is normally permitted. (os error 10048)"
                        .into(),
                )),
                ..WebFacts::default()
            };
        }
        "not_followed" => {
            lines.extend(healthy);
            web.recent_acks = vec![298, 299];
            test = sent(None);
        }
        "not_seen" => {
            lines.extend(healthy);
            test = sent(Some(false));
        }
        _ => return None,
    }
    Some(Facts {
        now: at(600),
        preview_on: true,
        game: GameFacts::Running {
            started: Some(at(100)),
        },
        log: LogFacts::Found {
            path: PathBuf::from(
                r"C:\Program Files (x86)\Steam\steamapps\common\Deadlock\game\citadel\console.log",
            ),
            modified: Some(modified),
            text: lines.join("\r\n") + "\r\n",
        },
        pak: PakFacts::Read {
            written: Some(written),
            script_base,
        },
        pending_hud,
        web,
        test,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const BASE: &str = "1a2b3c4d";

    #[test]
    fn every_sample_gets_its_own_next_step() {
        let step = |kind: &str| diagnose(&sample(kind).unwrap()).step;
        assert_eq!(step("works"), Step::Works);
        assert_eq!(step("old_pak"), Step::CloseAndLaunch);
        assert_eq!(step("pending"), Step::CloseAndLaunch);
        assert_eq!(step("no_condebug"), Step::CloseAndLaunch);
        assert!(
            matches!(step("script_error"), Step::SendReport(w) if w.contains("failed to load"))
        );
        assert!(matches!(step("no_page"), Step::SendReport(w) if w.contains("bridge page")));
        assert!(
            matches!(step("fetch_blocked"), Step::SendReport(w) if w.contains("won't let it reach"))
        );
        assert!(matches!(step("port_busy"), Step::SendReport(w) if w.contains("port 47613")));
        assert_eq!(step("not_followed"), Step::TryAgain);
        assert!(matches!(step("not_seen"), Step::SendReport(w) if w.contains("didn't change")));
        assert_eq!(sample("nonsense"), None);
        for name in SAMPLES {
            assert!(sample(name).is_some(), "{name}");
        }
    }

    fn t(secs: u64) -> SystemTime {
        SystemTime::UNIX_EPOCH + Duration::from_secs(1_800_000_000 + secs)
    }

    fn say(text: &str) -> String {
        format!("[PanoramaScript] DEADTUNE_LIVE {text}\r")
    }

    fn hello(base: &str) -> String {
        say(&format!("hello {base} web=panel"))
    }

    fn log(lines: &[&str]) -> String {
        let mut text = String::from("[Engine] older session\r\n[Console] DEADTUNE_BOOT 0.25.0\r\n");
        text.push_str("[PanoramaScript] DEADTUNE_LIVE hello deadbeef web=panel\r\n");
        text.push_str("[PanoramaScript] DEADTUNE_LIVE web ready storage=no\r\n");
        text.push_str("[Console] DEADTUNE_BOOT 0.27.0\r\n");
        for l in lines {
            text.push_str(l);
            text.push('\n');
        }
        text
    }

    fn reached(polls: u64, acks: &[u32]) -> WebFacts {
        WebFacts {
            listening: Some(Ok(PORT)),
            counters: Counters {
                polls,
                hellos: 1,
                acks: acks.len() as u64,
                ..Counters::default()
            },
            last_poll_ago: Some(Duration::from_millis(300)),
            base: Some(BASE.into()),
            recent_acks: acks.to_vec(),
        }
    }

    /// A healthy session: game started after the pak, logging, nothing reached DeadTune.
    fn facts(lines: &[&str]) -> Facts {
        Facts {
            now: t(600),
            preview_on: true,
            game: GameFacts::Running {
                started: Some(t(100)),
            },
            log: LogFacts::Found {
                path: PathBuf::from(r"C:\Deadlock\game\citadel\console.log"),
                modified: Some(t(590)),
                text: log(lines),
            },
            pak: PakFacts::Read {
                written: Some(t(50)),
                script_base: Some(BASE.into()),
            },
            pending_hud: false,
            web: WebFacts {
                listening: Some(Ok(PORT)),
                ..WebFacts::default()
            },
            test: TestFacts::NotRun,
        }
    }

    fn texts(d: &Diagnosis) -> Vec<(Tone, String)> {
        d.rows.iter().map(|r| (r.tone, r.text.clone())).collect()
    }

    fn has(d: &Diagnosis, tone: Tone, part: &str) -> bool {
        d.rows
            .iter()
            .any(|r| r.tone == tone && r.text.contains(part))
    }

    #[test]
    fn a_page_that_reaches_deadtune_and_a_seen_test_works() {
        let mut f = facts(&[
            &hello(BASE),
            &say("web ready storage=ok"),
            &say("web fetch ok"),
        ]);
        f.web = reached(40, &[300]);
        f.test = TestFacts::Sent {
            seq: 300,
            saw: Some(true),
        };
        let d = diagnose(&f);
        assert_eq!(d.step, Step::Works, "{:#?}", texts(&d));
        assert!(has(&d, Tone::Good, "The live script is running"));
        assert!(has(
            &d,
            Tone::Good,
            "bridge page loaded in game; it can keep your live edits"
        ));
        assert!(has(
            &d,
            Tone::Good,
            "The bridge page reaches DeadTune (40 requests"
        ));
        assert!(has(&d, Tone::Good, "The script applied the bigger minimap"));
        assert!(
            d.rows.iter().all(|r| r.tone == Tone::Good),
            "{:#?}",
            texts(&d)
        );
    }

    #[test]
    fn the_page_works_without_a_console_log() {
        let mut f = facts(&[]);
        f.log = LogFacts::NotFound {
            looked: vec![PathBuf::from("console.log")],
        };
        f.web = reached(40, &[300]);
        f.test = TestFacts::Sent {
            seq: 300,
            saw: Some(true),
        };
        let d = diagnose(&f);
        assert_eq!(d.step, Step::Works, "{:#?}", texts(&d));
        assert!(has(&d, Tone::Warn, "No console log found"));
        assert!(has(&d, Tone::Good, "The live script is running"));
        assert!(has(&d, Tone::Good, "it reached DeadTune"));
    }

    #[test]
    fn simons_old_pak_says_close_and_launch() {
        let mut f = facts(&[]);
        f.pak = PakFacts::Read {
            written: Some(t(300)),
            script_base: Some(BASE.into()),
        };
        let d = diagnose(&f);
        assert_eq!(d.step, Step::CloseAndLaunch);
        assert!(d.step.restarts());
        assert!(has(
            &d,
            Tone::Bad,
            "running the HUD from before your last Apply"
        ));
        assert!(has(
            &d,
            Tone::Bad,
            "didn't say hello since Deadlock started"
        ));
    }

    #[test]
    fn a_pending_hud_change_says_close_and_launch() {
        let mut f = facts(&[]);
        f.pending_hud = true;
        f.pak = PakFacts::Read {
            written: Some(t(50)),
            script_base: None,
        };
        let d = diagnose(&f);
        assert_eq!(
            d.step,
            Step::CloseAndLaunch,
            "not Apply: it's already applied"
        );
        assert!(has(&d, Tone::Warn, "waiting until Deadlock closes"));
        assert!(has(&d, Tone::Warn, "no live script yet"));
    }

    #[test]
    fn a_game_without_condebug_or_page_says_close_and_launch() {
        let mut f = facts(&[]);
        f.log = LogFacts::Found {
            path: PathBuf::from("console.log"),
            modified: Some(t(10)),
            text: log(&[]),
        };
        let d = diagnose(&f);
        assert_eq!(d.step, Step::CloseAndLaunch);
        assert!(has(&d, Tone::Bad, "isn't writing a console log"));
        assert!(has(&d, Tone::Skip, "started: not checked"));
    }

    #[test]
    fn a_script_error_is_shown_verbatim_and_asks_for_the_report() {
        let err = "[Panorama] JS error: panorama/scripts/deadtune/live_hud.vjs_c:12: ReferenceError: DT_LIVE is not defined";
        let d = diagnose(&facts(&[err, "[Panorama] unrelated error in hud_chat"]));
        assert_eq!(d.errors, [err]);
        assert!(has(&d, Tone::Bad, "1 error line about the HUD"));
        assert!(matches!(&d.step, Step::SendReport(why) if why.contains("failed to load")));
    }

    #[test]
    fn a_script_without_its_page_asks_for_the_report() {
        let d = diagnose(&facts(&[
            &hello(BASE),
            &say("web control title=about:blank"),
        ]));
        assert!(has(&d, Tone::Good, "The live script is running"));
        assert!(has(&d, Tone::Bad, "bridge page didn't load in game"));
        let page = d
            .rows
            .iter()
            .find(|r| r.text.contains("bridge page didn't"))
            .unwrap();
        assert_eq!(page.detail.as_deref(), Some("control title=about:blank"));
        assert!(matches!(&d.step, Step::SendReport(why) if why.contains("bridge page")));
        let d = diagnose(&facts(&[&hello(BASE), &say("web nopanel")]));
        assert!(has(&d, Tone::Bad, "wouldn't make a web panel"));
    }

    #[test]
    fn a_blocked_fetch_names_the_browsers_reason() {
        let d = diagnose(&facts(&[
            &hello(BASE),
            &say("web ready storage=ok"),
            &say("web fetch blocked TypeError: Failed to fetch"),
        ]));
        assert!(has(&d, Tone::Good, "bridge page loaded in game"));
        assert!(has(
            &d,
            Tone::Bad,
            "can't reach DeadTune: TypeError: Failed to fetch"
        ));
        assert!(matches!(&d.step, Step::SendReport(why) if why.contains("won't let it reach")));
    }

    #[test]
    fn a_busy_port_says_so() {
        let mut f = facts(&[&hello(BASE), &say("web ready storage=ok")]);
        f.web.listening = Some(Err("address in use".into()));
        let d = diagnose(&f);
        assert!(has(
            &d,
            Tone::Bad,
            "couldn't listen on 127.0.0.1:47613: address in use"
        ));
        assert!(
            matches!(&d.step, Step::SendReport(why) if why.contains("Close other DeadTune windows"))
        );
    }

    #[test]
    fn a_test_the_script_never_got_says_try_again() {
        let mut f = facts(&[&hello(BASE), &say("web ready storage=ok")]);
        f.web = reached(40, &[299]);
        f.test = TestFacts::Sent {
            seq: 300,
            saw: None,
        };
        let d = diagnose(&f);
        assert!(has(&d, Tone::Bad, "never reached the script"));
        assert_eq!(d.step, Step::TryAgain);
    }

    #[test]
    fn a_console_ok_counts_as_applied_and_unseen_asks_for_the_report() {
        let mut f = facts(&[
            &hello(BASE),
            &say("web ready storage=ok"),
            &say(&format!("300 ok {BASE}")),
        ]);
        f.web = reached(40, &[]);
        f.test = TestFacts::Sent {
            seq: 300,
            saw: Some(false),
        };
        let d = diagnose(&f);
        assert!(has(&d, Tone::Good, "The script applied the bigger minimap"));
        assert!(matches!(&d.step, Step::SendReport(why) if why.contains("didn't change")));
    }

    #[test]
    fn a_hello_from_another_hud_is_stale() {
        let d = diagnose(&facts(&[&hello("0badf00d")]));
        assert_eq!(d.step, Step::CloseAndLaunch);
        assert!(has(&d, Tone::Bad, "the script in game says HUD 0badf00d"));
        let mut f = facts(&[]);
        f.web = reached(3, &[]);
        f.web.base = Some("0badf00d".into());
        assert_eq!(
            diagnose(&f).step,
            Step::CloseAndLaunch,
            "the page names the HUD too"
        );
    }

    #[test]
    fn closed_game_and_missing_pak() {
        let mut f = facts(&[]);
        f.game = GameFacts::Closed;
        assert_eq!(diagnose(&f).step, Step::StartGame);
        f.game = GameFacts::Running { started: None };
        f.pak = PakFacts::Missing;
        assert_eq!(diagnose(&f).step, Step::Apply);
        f.preview_on = false;
        assert_eq!(diagnose(&f).step, Step::TurnOn);
    }

    #[test]
    fn report_carries_the_checklist_the_counters_and_the_log_lines() {
        let f = sample("fetch_blocked").unwrap();
        let d = diagnose(&f);
        let text = report(&f, &d, "0.27.0");
        assert!(text.starts_with("DeadTune 0.27.0 live preview check"));
        assert!(text.contains("[ok]   The live script is running"));
        assert!(text.contains("Next step: "));
        assert!(
            text.contains("Bridge server: Some(Ok(47613)); polls 0, page starts 0"),
            "{text}"
        );
        assert!(text.contains("DEADTUNE_LIVE web fetch blocked TypeError: Failed to fetch"));
        assert!(text.contains("--- last 50 lines ---\n[Console] DEADTUNE_BOOT 0.27.0"));
        let old = report(&facts(&[]), &diagnose(&facts(&[])), "x");
        assert!(
            !old.contains("hello deadbeef"),
            "an older session stays out"
        );
    }

    #[test]
    fn reads_the_installed_pak_back() {
        use super::super::layout::HudLayout;
        use super::super::{inject, live::HUD_LAYOUT};
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(read_pak(dir.path()), PakFacts::Missing);
        let layout = test_layout(&HudLayout::default());
        let base = live::base_id(&layout).unwrap();
        let mut files = std::collections::BTreeMap::new();
        files.insert(
            OWN_SCRIPT.to_string(),
            inject::script_resource(&live::script(&base)),
        );
        files.insert(
            HUD_LAYOUT.to_string(),
            inject::compiled_layout(&live::stand_in_layout()),
        );
        std::fs::write(
            dir.path().join(ADDON_FILE),
            super::super::vpk::write(&files),
        )
        .unwrap();
        match read_pak(dir.path()) {
            PakFacts::Read {
                script_base,
                written,
            } => {
                assert_eq!(script_base, Some(base));
                assert!(written.is_some());
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn reads_the_newest_log_whole() {
        let dir = tempfile::tempdir().unwrap();
        let old = dir.path().join("deadtune_console.log");
        let new = dir.path().join("console.log");
        std::fs::write(&old, "old\n").unwrap();
        std::thread::sleep(Duration::from_millis(20));
        std::fs::write(&new, "a\nb\n").unwrap();
        match read_log(&[old, new.clone(), dir.path().join("missing.log")]) {
            LogFacts::Found { path, text, .. } => {
                assert_eq!(path, new);
                assert_eq!(text, "a\nb\n");
            }
            other => panic!("{other:?}"),
        }
        assert!(matches!(read_log(&[]), LogFacts::NotFound { .. }));
    }

    #[test]
    fn the_test_layout_makes_the_minimap_bigger_and_goes_live() {
        use super::super::elements::ElementId;
        use super::super::layout::HudLayout;
        let base = HudLayout::default();
        let big = test_layout(&base);
        let edit = &big.elements[&ElementId::Minimap];
        assert_eq!(edit.scale_pct, 150);
        assert!(edit.offset_x < 0 && edit.offset_y < 0, "toward the centre");
        let overrides = live::overrides(&big, &base).unwrap();
        assert!(
            overrides
                .rules
                .iter()
                .any(|r| r.selector.contains("minimap_persp")),
            "{:?}",
            overrides.rules
        );
    }
}
