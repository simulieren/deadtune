//! "Check live preview": everything DeadTune can find out about the live HUD preview,
//! turned into a short checklist and one next step. `diagnose` is pure over `Facts`; the
//! `read_*` functions gather them from the game folder. Design: docs/plans/live-hud/plan.md
//! section 9.

use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use super::install::ADDON_FILE;
use super::live::{self, CTL, DATA, HUD_LAYOUT, LiveLine, OWN_SCRIPT, SEQ_MOD};
use super::{inject, vpk::VpkDir};
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
        /// How many of the hidden sliders the pak's HUD layout holds.
        sliders: usize,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BridgeFacts {
    Netcon,
    /// The exec-file bridge: the player presses this key in game.
    Key(String),
    Clipboard,
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
    pub bridge: BridgeFacts,
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
    /// The test message never reached the script; with the exec-file bridge the key was
    /// probably not pressed.
    TryAgain {
        key: Option<String>,
    },
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
            Step::TryAgain { key: Some(key) } => format!(
                "Press Check live preview again and press {key} in game while the countdown runs."
            ),
            Step::TryAgain { key: None } => "Press Check live preview again.".into(),
            Step::SendReport(why) => format!("{why} Press Copy report and paste it to us."),
        }
    }

    /// The step can be done for the player: close the game and start it through DeadTune.
    pub fn restarts(&self) -> bool {
        matches!(self, Step::CloseAndLaunch)
    }
}

/// The script's hello line, read into plain facts.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Hello {
    pub base: String,
    pub ctl: Option<u32>,
    pub data: Vec<u32>,
    /// Hidden data sliders the script found in the HUD.
    pub found: Option<usize>,
    pub api: Option<String>,
    pub html: Option<String>,
    pub line: String,
}

impl Hello {
    fn parse(line: &str) -> Option<Hello> {
        let Some(LiveLine::Hello { base, probes }) = live::parse_line(line) else {
            return None;
        };
        let field = |key: &str| {
            probes
                .split_whitespace()
                .find_map(|kv| kv.strip_prefix(key)?.strip_prefix('='))
        };
        Some(Hello {
            base,
            ctl: field("ctl").and_then(|v| v.parse().ok()),
            data: field("d")
                .map(|v| v.split(',').filter_map(|w| w.parse().ok()).collect())
                .unwrap_or_default(),
            found: field("n").and_then(|v| v.parse().ok()),
            api: field("api").map(str::to_string),
            html: field("html").map(str::to_string),
            line: line.trim().to_string(),
        })
    }

    fn sliders(&self) -> Sliders {
        let all = self.found == Some(DATA.len()) && self.ctl.is_some();
        let probe: Vec<u32> = (0..DATA.len() as u32).map(|k| 1000 + k).collect();
        match (all, self.found) {
            (true, _) if self.ctl == Some(1 << 10) && self.data == probe => Sliders::ShowProbe,
            (true, _) => Sliders::Load,
            (false, Some(0)) | (false, None) => Sliders::Missing,
            (false, Some(n)) => Sliders::Some(n),
        }
    }

    /// The hello's other probes in words.
    fn extras(&self) -> String {
        let api = match self.api.as_deref() {
            Some("missing") | None => "the game has no settings API for scripts (expected)",
            Some(_) => "the game has a settings API for scripts",
        };
        let html = match self.html.as_deref() {
            Some("1") => "web panels are available",
            _ => "web panels are not available",
        };
        format!("HUD {}; {api}; {html}.", self.base)
    }
}

enum Sliders {
    ShowProbe,
    Load,
    Some(usize),
    Missing,
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

    pub fn hellos(&self) -> Vec<Hello> {
        self.lines.iter().filter_map(|l| Hello::parse(l)).collect()
    }

    /// The script's answer to message `seq`.
    fn answer(&self, seq: u32) -> Option<LiveLine> {
        self.lines
            .iter()
            .filter_map(|l| live::parse_line(l))
            .filter(|line| match line {
                LiveLine::Ok { seq: s, .. }
                | LiveLine::Got { seq: s, .. }
                | LiveLine::WrongBase { seq: s, .. } => *s % SEQ_MOD == seq % SEQ_MOD,
                LiveLine::Hello { .. } => false,
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
    let secs = now.duration_since(then).unwrap_or_default().as_secs();
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

pub fn diagnose(f: &Facts) -> Diagnosis {
    let mut rows = Vec::new();
    let mut steps: Vec<Step> = Vec::new();
    let running = match &f.game {
        GameFacts::Running { started } => Some(*started),
        GameFacts::Closed => None,
    };
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

    let started = running.flatten();
    let session = match &f.log {
        LogFacts::NotFound { looked } => {
            let looked: Vec<String> = looked.iter().map(|p| p.display().to_string()).collect();
            if running.is_some() {
                steps.push(Step::CloseAndLaunch);
            }
            rows.push(with(
                row(
                    Tone::Bad,
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
                steps.push(Step::CloseAndLaunch);
                rows.push(row(
                    Tone::Bad,
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
        PakFacts::Read {
            script_base,
            sliders,
            ..
        } => {
            match (script_base, *sliders) {
                (Some(base), n) if n == DATA.len() + 1 => rows.push(row(
                    Tone::Good,
                    format!("The installed HUD carries the live script and the hidden sliders (HUD {base})"),
                )),
                (Some(base), n) => {
                    steps.push(Step::Apply);
                    rows.push(row(
                        Tone::Bad,
                        format!("The installed HUD carries the live script (HUD {base}) but only {n} of {} hidden sliders", DATA.len() + 1),
                    ))
                }
                (None, _) if f.pending_hud => rows.push(row(
                    Tone::Warn,
                    "The installed HUD has no live script yet; your waiting change adds it",
                )),
                (None, _) => {
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
    let written = match &f.pak {
        PakFacts::Read { written, .. } => *written,
        _ => None,
    };
    let older = matches!((started, written), (Some(s), Some(w)) if s + START_SLACK < w);
    let other_base = match (hellos.last(), &disk_base) {
        (Some(h), Some(base)) if h.base != *base => Some(h.base.clone()),
        _ => None,
    };
    if running.is_some() {
        rows.push(if older || other_base.is_some() || f.pending_hud {
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

    let errors = session.as_ref().map(Session::errors).unwrap_or_default();
    match (&session, hellos.first()) {
        (None, _) => rows.push(row(
            Tone::Skip,
            "Whether the live script started: not checked",
        )),
        (Some(s), None) => {
            let scope = if s.from_boot {
                "since Deadlock started"
            } else {
                "anywhere in the console log"
            };
            rows.push(row(
                Tone::Bad,
                format!("The live script didn't say hello {scope}, so it isn't running"),
            ));
            if disk_base.is_some() && running.is_some() {
                steps.push(Step::SendReport(if errors.is_empty() {
                    "The live script didn't start, and the game logged no error about it.".into()
                } else {
                    "The live script failed to load.".into()
                }));
            }
        }
        (Some(_), Some(first)) => {
            rows.push(with(
                row(Tone::Good, "The live script is running"),
                format!("{}\n{}", first.extras(), first.line),
            ));
            rows.push(match first.sliders() {
                Sliders::ShowProbe => row(
                    Tone::Good,
                    "The hidden sliders work: they show the values DeadTune set at launch",
                ),
                Sliders::Load => row(
                    Tone::Bad,
                    "The hidden sliders load but read 0, not the values DeadTune set at launch, so they don't follow settings in the HUD",
                ),
                Sliders::Some(n) => {
                    steps.push(Step::SendReport(
                        "Some of the hidden sliders don't load in this game version.".into(),
                    ));
                    row(
                        Tone::Bad,
                        format!(
                            "Only {n} of {} hidden sliders load in the game",
                            DATA.len()
                        ),
                    )
                }
                Sliders::Missing => {
                    steps.push(Step::SendReport(
                        "The hidden sliders don't load in this game version.".into(),
                    ));
                    row(Tone::Bad, "The hidden sliders don't load in the game")
                }
            });
        }
    }

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
        TestFacts::NotRun => rows.push(row(
            Tone::Skip,
            "Sliders follow DeadTune's changes: not tested",
        )),
        TestFacts::Sent { seq, saw } => {
            let answer = session.as_ref().and_then(|s| s.answer(*seq));
            let set = format!("DeadTune set {} to message {seq}", CTL.convar);
            match &answer {
                Some(LiveLine::Ok { .. }) | Some(LiveLine::Got { .. }) => rows.push(with(
                    row(Tone::Good, "Sliders follow DeadTune's changes: yes"),
                    set,
                )),
                Some(LiveLine::WrongBase { base, .. }) => {
                    steps.push(Step::CloseAndLaunch);
                    rows.push(with(
                        row(
                            Tone::Bad,
                            format!("The script got the test, but it belongs to HUD {base}, an older one"),
                        ),
                        set,
                    ))
                }
                _ => {
                    let key = match &f.bridge {
                        BridgeFacts::Key(k) => Some(k.clone()),
                        _ => None,
                    };
                    if !hellos.is_empty() {
                        steps.push(match &key {
                            Some(_) => Step::TryAgain { key: key.clone() },
                            None => Step::SendReport(
                                "The live script runs but doesn't follow DeadTune's changes."
                                    .into(),
                            ),
                        });
                    }
                    let why = match &key {
                        Some(k) => format!(
                            " (with the {k} key bridge the game reads it only when you press {k})"
                        ),
                        None => String::new(),
                    };
                    rows.push(with(
                        row(
                            Tone::Bad,
                            format!(
                                "Sliders follow DeadTune's changes: no answer from the script{why}"
                            ),
                        ),
                        set,
                    ))
                }
            }
            match saw {
                Some(true) => rows.push(row(Tone::Good, "You saw the minimap get bigger")),
                Some(false) => {
                    if matches!(answer, Some(LiveLine::Ok { .. })) {
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
                TestFacts::NotRun => Step::TryAgain { key: None },
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
        Step::TryAgain { .. } => 5,
        Step::Works => 6,
    }
}

fn plural(n: usize) -> &'static str {
    if n == 1 { "" } else { "s" }
}

/// Plain text for Copy report: the checklist, the next step, then the console log lines
/// that matter.
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
        "\nBridge: {:?}. Preview on: {}. HUD change waiting: {}. Test: {:?}.\n",
        f.bridge, f.preview_on, f.pending_hud, f.test
    ));
    match &f.pak {
        PakFacts::Read {
            written,
            script_base,
            sliders,
        } => out.push_str(&format!(
            "HUD file: script {}, {sliders} sliders, written {}.\n",
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

/// DeadTune's HUD pak in `addons_dir`: whether it carries the live script, which base the
/// script was built for, and how many hidden sliders its HUD layout holds.
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
    let sliders = pak
        .read(HUD_LAYOUT)
        .ok()
        .and_then(|bytes| inject::layout_text(&bytes).ok())
        .map_or(0, |xml| {
            std::iter::once(CTL.id)
                .chain(DATA.iter().map(|s| s.id))
                .filter(|id| xml.contains(&format!("\"{id}\"")))
                .count()
        });
    PakFacts::Read {
        written,
        script_base,
        sliders,
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

/// Facts for one situation the check tells apart, for the screenshot lever and tests:
/// `works`, `old_pak`, `pending`, `no_condebug`, `script_error`, `no_sliders`, `sliders`,
/// `not_followed`, `not_seen`.
pub fn sample(kind: &str) -> Option<Facts> {
    let at = |secs: u64| SystemTime::UNIX_EPOCH + Duration::from_secs(1_800_000_000 + secs);
    let base = "1a2b3c4d";
    let probe: Vec<String> = (1000..1000 + DATA.len()).map(|k| k.to_string()).collect();
    let hello = |ctl: &str, d: &str, n: usize| {
        format!(
            "[Console] DEADTUNE_LIVE hello {base} ctl={ctl} raw=1024/0.001 col={ctl} d={d} n={n} api=missing kv=1 kvf=0 ld=1 cp=1 html=0"
        )
    };
    let good = hello("1024", &probe.join(","), DATA.len());
    let ok = format!("[Console] DEADTUNE_LIVE 300 ok {base}");
    let key_lines = [
        "[InputService] execing deadtune_live",
        "[Console] \"DeadTune: applied 1\"",
        "[InputService] execing deadtune_hud",
    ];
    let mut lines: Vec<String> = vec![
        "[Console] DEADTUNE_BOOT 0.19.0".into(),
        "[Panorama] Loading layout panorama/layout/hud.xml".into(),
    ];
    let mut test = TestFacts::NotRun;
    let mut written = at(50);
    let mut modified = at(590);
    let mut pending_hud = false;
    let mut script_base = Some(base.to_string());
    let sent = |saw| TestFacts::Sent { seq: 300, saw };
    match kind {
        "works" => {
            lines.extend([good, ok]);
            test = sent(Some(true));
        }
        "old_pak" => {
            lines.extend(key_lines.map(String::from));
            written = at(300);
        }
        "pending" => {
            lines.extend(key_lines.map(String::from));
            pending_hud = true;
            script_base = None;
        }
        "no_condebug" => modified = at(10),
        "script_error" => lines.push(
            "[Panorama] JS error: panorama/scripts/deadtune/live_hud.vjs_c:12: ReferenceError: DT_LIVE is not defined".into(),
        ),
        "no_sliders" => lines.push(hello("null", &["0"; DATA.len()].join(","), 0)),
        "sliders" => lines.push(good),
        "not_followed" => {
            lines.push(good);
            test = sent(None);
        }
        "not_seen" => {
            lines.extend([good, ok]);
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
            sliders: DATA.len() + 1,
        },
        pending_hud,
        bridge: BridgeFacts::Key("F8".into()),
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
        assert!(matches!(step("no_sliders"), Step::SendReport(w) if w.contains("sliders")));
        assert_eq!(step("sliders"), Step::TryAgain { key: None });
        assert_eq!(
            step("not_followed"),
            Step::TryAgain {
                key: Some("F8".into())
            }
        );
        assert!(matches!(step("not_seen"), Step::SendReport(w) if w.contains("didn't change")));
        assert_eq!(sample("nonsense"), None);
    }

    fn t(secs: u64) -> SystemTime {
        SystemTime::UNIX_EPOCH + Duration::from_secs(1_800_000_000 + secs)
    }

    fn hello(base: &str, ctl: &str, d: &str, n: usize) -> String {
        format!(
            "[Console] DEADTUNE_LIVE hello {base} ctl={ctl} raw=1024/0.001 col={ctl} d={d} n={n} api=missing kv=1 kvf=0 ld=1 cp=1 html=0\r"
        )
    }

    fn probe_d() -> String {
        (1000..1010)
            .map(|k| k.to_string())
            .collect::<Vec<_>>()
            .join(",")
    }

    fn log(lines: &[&str]) -> String {
        let mut text = String::from("[Engine] older session\r\n[Console] DEADTUNE_BOOT 0.17.0\r\n");
        text.push_str("[Console] DEADTUNE_LIVE hello deadbeef ctl=1024 d=1 n=10\r\n");
        text.push_str("[Console] DEADTUNE_BOOT 0.18.0\r\n");
        for l in lines {
            text.push_str(l);
            text.push('\n');
        }
        text
    }

    /// A healthy session: game started after the pak, logging, script said hello.
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
                sliders: DATA.len() + 1,
            },
            pending_hud: false,
            bridge: BridgeFacts::Key("F8".into()),
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
    fn hello_with_sliders_and_a_followed_probe_works_once_the_player_saw_it() {
        let h = hello(BASE, "1024", &probe_d(), 10);
        let mut f = facts(&[&h, "[Console] DEADTUNE_LIVE 300 ok 1a2b3c4d"]);
        f.test = TestFacts::Sent {
            seq: 300,
            saw: Some(true),
        };
        let d = diagnose(&f);
        assert_eq!(d.step, Step::Works, "{:#?}", texts(&d));
        assert!(has(&d, Tone::Good, "The live script is running"));
        assert!(has(&d, Tone::Good, "The hidden sliders work"));
        assert!(has(
            &d,
            Tone::Good,
            "Sliders follow DeadTune's changes: yes"
        ));
        assert!(has(
            &d,
            Tone::Good,
            "Deadlock loaded the HUD that is on disk"
        ));
        assert!(
            d.rows.iter().all(|r| r.tone == Tone::Good),
            "{:#?}",
            texts(&d)
        );
    }

    #[test]
    fn simons_log_without_a_hello_on_an_old_pak_says_close_and_launch() {
        let mut f = facts(&[
            "[InputService] execing deadtune_live",
            "[Console] \"DeadTune: applied 1\"",
            "[InputService] execing deadtune_hud",
        ]);
        f.pak = PakFacts::Read {
            written: Some(t(300)),
            script_base: Some(BASE.into()),
            sliders: DATA.len() + 1,
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
        assert_eq!(
            d.step.text(),
            "Close Deadlock completely, then press Launch in DeadTune."
        );
    }

    #[test]
    fn a_pending_hud_change_says_close_and_launch() {
        let mut f = facts(&[]);
        f.pending_hud = true;
        f.pak = PakFacts::Read {
            written: Some(t(50)),
            script_base: None,
            sliders: 0,
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
    fn a_game_without_condebug_says_close_and_launch() {
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

        f.log = LogFacts::NotFound {
            looked: vec![PathBuf::from("console.log")],
        };
        let d = diagnose(&f);
        assert_eq!(d.step, Step::CloseAndLaunch);
        assert!(has(&d, Tone::Bad, "No console log found"));
    }

    #[test]
    fn a_script_error_is_shown_verbatim_and_asks_for_the_report() {
        let err = "[Panorama] JS error: panorama/scripts/deadtune/live_hud.vjs_c:12: ReferenceError: DT_LIVE is not defined";
        let d = diagnose(&facts(&[err, "[Panorama] unrelated error in hud_chat"]));
        assert_eq!(d.errors, [err]);
        assert!(has(&d, Tone::Bad, "1 error line about the HUD"));
        assert!(matches!(&d.step, Step::SendReport(why) if why.contains("failed to load")));
        let row = d
            .rows
            .iter()
            .find(|r| r.text.contains("error line"))
            .unwrap();
        assert_eq!(row.detail.as_deref(), Some(err));
    }

    #[test]
    fn hello_without_sliders_asks_for_the_report() {
        let h = hello(BASE, "null", "0,0,0,0,0,0,0,0,0,0", 0);
        let d = diagnose(&facts(&[&h]));
        assert!(has(&d, Tone::Good, "The live script is running"));
        assert!(has(&d, Tone::Bad, "hidden sliders don't load"));
        assert!(matches!(&d.step, Step::SendReport(why) if why.contains("sliders")));
    }

    #[test]
    fn hello_with_sliders_and_no_test_yet_suggests_running_it() {
        let h = hello(BASE, "1024", &probe_d(), 10);
        let d = diagnose(&facts(&[&h]));
        assert!(has(&d, Tone::Good, "hidden sliders work"));
        assert!(has(&d, Tone::Skip, "not tested"));
        assert_eq!(d.step, Step::TryAgain { key: None });
    }

    #[test]
    fn a_probe_without_an_answer_asks_for_the_key_press() {
        let h = hello(BASE, "1024", &probe_d(), 10);
        let mut f = facts(&[&h]);
        f.test = TestFacts::Sent {
            seq: 300,
            saw: Some(false),
        };
        let d = diagnose(&f);
        assert!(has(
            &d,
            Tone::Bad,
            "no answer from the script (with the F8 key bridge"
        ));
        assert_eq!(
            d.step,
            Step::TryAgain {
                key: Some("F8".into())
            }
        );
        f.bridge = BridgeFacts::Netcon;
        assert!(matches!(diagnose(&f).step, Step::SendReport(_)));
    }

    #[test]
    fn a_followed_probe_the_player_did_not_see_asks_for_the_report() {
        let h = hello(BASE, "1024", &probe_d(), 10);
        let mut f = facts(&[&h, "[Console] DEADTUNE_LIVE 300 ok 1a2b3c4d"]);
        f.test = TestFacts::Sent {
            seq: 300 + SEQ_MOD,
            saw: Some(false),
        };
        let d = diagnose(&f);
        assert!(
            has(&d, Tone::Good, "follow DeadTune's changes: yes"),
            "seq compares modulo the slot's 10 bits"
        );
        assert!(matches!(&d.step, Step::SendReport(why) if why.contains("didn't change")));
    }

    #[test]
    fn a_hello_from_another_hud_is_stale() {
        let h = hello("0badf00d", "1024", &probe_d(), 10);
        let d = diagnose(&facts(&[&h]));
        assert_eq!(d.step, Step::CloseAndLaunch);
        assert!(has(&d, Tone::Bad, "the script in game says HUD 0badf00d"));
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
    fn report_carries_the_checklist_and_the_log_lines() {
        let h = hello(BASE, "1024", &probe_d(), 10);
        let f = facts(&[
            &h,
            "[InputService] execing deadtune_hud",
            "[Panorama] other",
        ]);
        let d = diagnose(&f);
        let text = report(&f, &d, "0.19.0");
        assert!(text.starts_with("DeadTune 0.19.0 live preview check"));
        assert!(text.contains("[ok]   The live script is running"));
        assert!(text.contains("Next step: "));
        assert!(text.contains("DEADTUNE_LIVE hello 1a2b3c4d"));
        assert!(
            !text.contains("hello deadbeef"),
            "an older session stays out"
        );
        assert!(text.contains("--- last 50 lines ---\n[Console] DEADTUNE_BOOT 0.18.0"));
    }

    #[test]
    fn reads_the_installed_pak_back() {
        use super::super::layout::HudLayout;
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(read_pak(dir.path()), PakFacts::Missing);
        let layout = test_layout(&HudLayout::default());
        let base = live::base_id(&layout).unwrap();
        let script = live::script(&base, &live::dictionary(&layout).unwrap());
        let mut hud = live::stand_in_layout();
        hud.children.push(live::slots_panel());
        let mut files = std::collections::BTreeMap::new();
        files.insert(OWN_SCRIPT.to_string(), inject::script_resource(&script));
        files.insert(HUD_LAYOUT.to_string(), inject::compiled_layout(&hud));
        std::fs::write(
            dir.path().join(ADDON_FILE),
            super::super::vpk::write(&files),
        )
        .unwrap();
        match read_pak(dir.path()) {
            PakFacts::Read {
                script_base,
                sliders,
                written,
            } => {
                assert_eq!(script_base, Some(base));
                assert_eq!(sliders, DATA.len() + 1);
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
