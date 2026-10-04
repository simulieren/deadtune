//! Confirmation loop for live pushes. Every batch ends with `echo DEADTUNE_ACK <nonce> <n>`, one
//! bare query per convar (a name alone prints its value) and `echo DEADTUNE_END <nonce>`; the
//! [`Tracker`] reads the console log back and tells whether the game ran the batch and what each
//! convar ended up as.

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use super::{BridgeError, ConsoleCmd};

pub const ACK: &str = "DEADTUNE_ACK";
pub const END: &str = "DEADTUNE_END";
pub const BOOT: &str = "DEADTUNE_BOOT";
/// `con_logfile` target, relative to the game's mod dir in Source 1; see `conlog::candidates`.
pub const LOG_NAME: &str = "deadtune_console.log";
pub const TIMEOUT: Duration = Duration::from_secs(10);

/// What one push sent, for matching the console's reply to it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Receipt {
    pub nonce: String,
    pub sent: Vec<ConsoleCmd>,
    /// Convars the trailer queries; the sent names, or one name for a probe.
    pub queries: Vec<String>,
}

static COUNTER: AtomicU32 = AtomicU32::new(0);

fn nonce() -> String {
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    let n = COUNTER.fetch_add(1, Ordering::Relaxed) & 0xfff;
    format!("{millis:x}{n:03x}")
}

impl Receipt {
    pub fn for_batch(cmds: &[ConsoleCmd]) -> Receipt {
        Receipt {
            nonce: nonce(),
            sent: cmds.to_vec(),
            queries: cmds.iter().map(|c| c.name.clone()).collect(),
        }
    }

    pub fn for_probe(name: &str) -> Receipt {
        Receipt {
            nonce: nonce(),
            sent: Vec::new(),
            queries: vec![name.to_string()],
        }
    }
}

fn is_identifier(name: &str) -> bool {
    !name.is_empty() && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// The console lines for a batch: log redirect, the commands, then the ack trailer.
pub fn script(receipt: &Receipt) -> Result<Vec<String>, BridgeError> {
    let mut lines = vec![format!("con_logfile {LOG_NAME}")];
    lines.extend(super::lines(&receipt.sent)?);
    lines.push(format!(
        "echo {ACK} {} {}",
        receipt.nonce,
        receipt.queries.len()
    ));
    for name in &receipt.queries {
        if !is_identifier(name) {
            return Err(BridgeError::Unsafe {
                name: name.clone(),
                reason: "name must be ASCII letters, digits or _",
            });
        }
        lines.push(name.clone());
    }
    lines.push(format!("echo {END} {}", receipt.nonce));
    Ok(lines)
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Outcome {
    /// The console printed this value back.
    Applied(String),
    /// The console refused it; the message, or what the value stayed at.
    Rejected(String),
    /// Nothing about this convar between ACK and END.
    NoEcho,
}

#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub enum PushStatus {
    #[default]
    Idle,
    Waiting {
        since: Instant,
        count: usize,
    },
    Confirmed {
        at: SystemTime,
        results: Vec<(String, Outcome)>,
    },
    TimedOut {
        after: Duration,
        count: usize,
    },
}

impl PushStatus {
    pub fn applied(&self) -> usize {
        match self {
            PushStatus::Confirmed { results, .. } => results
                .iter()
                .filter(|(_, o)| matches!(o, Outcome::Applied(_)))
                .count(),
            _ => 0,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Boot {
    pub version: String,
    pub at: SystemTime,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum Event {
    Boot(String),
    Ack(String),
    End(String),
    Value(String, String),
    Rejected(String, String),
}

/// Tokens after a marker, tolerant of a `[12:34:56]` or `[Console]` prefix before it.
fn after_marker<'a>(line: &'a str, marker: &str) -> Option<&'a str> {
    let at = line.find(marker)?;
    Some(line[at + marker.len()..].trim())
}

const REFUSALS: [&str; 9] = [
    "unknown command",
    "cheat",
    "can't",
    "cannot",
    "not allowed",
    "protected",
    "only server",
    "read-only",
    "development",
];

/// A whole-word occurrence of `name`, so `fps_max` does not match `fps_max_ui`.
fn find_word(line: &str, name: &str) -> Option<usize> {
    let mut from = 0;
    while let Some(rel) = line[from..].find(name) {
        let at = from + rel;
        let before = line[..at].chars().next_back();
        let after = line[at + name.len()..].chars().next();
        let boundary = |c: Option<char>| c.is_none_or(|c| !(c.is_ascii_alphanumeric() || c == '_'));
        if boundary(before) && boundary(after) {
            return Some(at);
        }
        from = at + name.len();
    }
    None
}

/// `"name" = "value" ( def. ... )`, `name = value` or `name : value`; the value may be quoted.
fn value_after(rest: &str) -> Option<String> {
    let rest = rest.trim_start_matches('"').trim_start();
    let rest = rest
        .strip_prefix('=')
        .or_else(|| rest.strip_prefix(':'))?
        .trim_start();
    if let Some(quoted) = rest.strip_prefix('"') {
        return quoted.split('"').next().map(str::to_string);
    }
    let end = rest
        .find(|c: char| c.is_whitespace() || c == '(')
        .unwrap_or(rest.len());
    Some(rest[..end].to_string())
}

fn parse(line: &str, names: &[String]) -> Option<Event> {
    if let Some(rest) = after_marker(line, BOOT) {
        return Some(Event::Boot(rest.to_string()));
    }
    if let Some(rest) = after_marker(line, ACK) {
        return rest.split_whitespace().next().map(|n| Event::Ack(n.into()));
    }
    if let Some(rest) = after_marker(line, END) {
        return rest.split_whitespace().next().map(|n| Event::End(n.into()));
    }
    let lower = line.to_ascii_lowercase();
    for name in names {
        let Some(at) = find_word(line, name) else {
            continue;
        };
        if let Some(value) = value_after(&line[at + name.len()..]) {
            return Some(Event::Value(name.clone(), value));
        }
        if REFUSALS.iter().any(|r| lower.contains(r)) {
            return Some(Event::Rejected(name.clone(), line.trim().to_string()));
        }
    }
    None
}

/// `"1"`/`"true"`, `"240"`/`"240.0"` and case differences all count as the same value.
fn same_value(sent: &str, got: &str) -> bool {
    let (a, b) = (sent.trim().trim_matches('"'), got.trim().trim_matches('"'));
    if a.eq_ignore_ascii_case(b) {
        return true;
    }
    let as_bool = |v: &str| match v.to_ascii_lowercase().as_str() {
        "1" | "true" => Some(true),
        "0" | "false" => Some(false),
        _ => None,
    };
    if let (Some(x), Some(y)) = (as_bool(a), as_bool(b)) {
        return x == y;
    }
    matches!((a.parse::<f64>(), b.parse::<f64>()), (Ok(x), Ok(y)) if (x - y).abs() < 1e-6)
}

struct Expected {
    receipt: Receipt,
    since: Instant,
    acked: bool,
    seen: BTreeMap<String, Outcome>,
}

impl Expected {
    fn results(&self) -> Vec<(String, Outcome)> {
        self.receipt
            .queries
            .iter()
            .map(|name| {
                let outcome = self.seen.get(name).cloned().unwrap_or(Outcome::NoEcho);
                (name.clone(), outcome)
            })
            .collect()
    }
}

/// Follows one push at a time through the console log; a new push replaces the previous one.
#[derive(Default)]
pub struct Tracker {
    status: PushStatus,
    expected: Option<Expected>,
    pub boot: Option<Boot>,
    /// Console lines between ACK and END of the last push, for the details view.
    pub transcript: Vec<String>,
}

const TRANSCRIPT_CAP: usize = 40;

impl Tracker {
    pub fn status(&self) -> &PushStatus {
        &self.status
    }

    pub fn is_waiting(&self) -> bool {
        matches!(self.status, PushStatus::Waiting { .. })
    }

    pub fn start(&mut self, receipt: Receipt, now: Instant) {
        self.status = PushStatus::Waiting {
            since: now,
            count: receipt.queries.len(),
        };
        self.transcript.clear();
        self.expected = Some(Expected {
            receipt,
            since: now,
            acked: false,
            seen: BTreeMap::new(),
        });
    }

    /// Screenshot and test lever: show a status without a push behind it.
    pub fn inject(&mut self, status: PushStatus) {
        self.expected = None;
        self.status = status;
    }

    pub fn observe_line(&mut self, line: &str) {
        let names: Vec<String> = self
            .expected
            .as_ref()
            .map(|e| e.receipt.queries.clone())
            .unwrap_or_default();
        let Some(event) = parse(line, &names) else {
            if self.expected.as_ref().is_some_and(|e| e.acked) {
                self.record(line);
            }
            return;
        };
        if let Event::Boot(version) = event {
            self.boot = Some(Boot {
                version,
                at: SystemTime::now(),
            });
            return;
        }
        let Some(expected) = self.expected.as_mut() else {
            return;
        };
        match event {
            Event::Ack(nonce) if nonce == expected.receipt.nonce => expected.acked = true,
            Event::End(nonce) if nonce == expected.receipt.nonce && expected.acked => self.finish(),
            Event::Value(name, value) if expected.acked => {
                let sent = expected.receipt.sent.iter().find(|c| c.name == name);
                let outcome = match sent {
                    Some(cmd) if !same_value(&cmd.value, &value) => {
                        Outcome::Rejected(format!("stayed at {value}"))
                    }
                    _ => Outcome::Applied(value),
                };
                expected.seen.entry(name).or_insert(outcome);
                self.record(line);
            }
            Event::Rejected(name, reason) if expected.acked => {
                expected.seen.insert(name, Outcome::Rejected(reason));
                self.record(line);
            }
            _ => {}
        }
    }

    fn record(&mut self, line: &str) {
        if self.transcript.len() < TRANSCRIPT_CAP {
            self.transcript.push(line.trim().to_string());
        }
    }

    fn finish(&mut self) {
        if let Some(expected) = self.expected.take() {
            self.status = PushStatus::Confirmed {
                at: SystemTime::now(),
                results: expected.results(),
            };
        }
    }

    /// Times the push out; an acked push with no END still counts as confirmed with what came.
    pub fn tick(&mut self, now: Instant) {
        let Some(expected) = &self.expected else {
            return;
        };
        let after = now.duration_since(expected.since);
        if after < TIMEOUT {
            return;
        }
        if expected.acked {
            self.finish();
        } else {
            self.status = PushStatus::TimedOut {
                after,
                count: expected.receipt.queries.len(),
            };
            self.expected = None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bridge::cmd;

    fn receipt(cmds: &[ConsoleCmd]) -> Receipt {
        Receipt {
            nonce: "abc123".into(),
            sent: cmds.to_vec(),
            queries: cmds.iter().map(|c| c.name.clone()).collect(),
        }
    }

    fn feed(tracker: &mut Tracker, lines: &[&str]) {
        for line in lines {
            tracker.observe_line(line);
        }
    }

    #[test]
    fn script_wraps_commands_in_log_redirect_ack_queries_and_end() {
        let lines = script(&receipt(&[cmd("fps_max", "240"), cmd("r_name", "a b")])).unwrap();
        assert_eq!(
            lines,
            [
                "con_logfile deadtune_console.log",
                "fps_max \"240\"",
                "r_name \"a b\"",
                "echo DEADTUNE_ACK abc123 2",
                "fps_max",
                "r_name",
                "echo DEADTUNE_END abc123",
            ]
        );
    }

    #[test]
    fn probe_script_only_queries() {
        let lines = script(&Receipt {
            nonce: "n1".into(),
            sent: vec![],
            queries: vec!["fps_max".into()],
        })
        .unwrap();
        assert_eq!(
            lines,
            [
                "con_logfile deadtune_console.log",
                "echo DEADTUNE_ACK n1 1",
                "fps_max",
                "echo DEADTUNE_END n1",
            ]
        );
    }

    #[test]
    fn script_refuses_a_query_that_is_not_an_identifier() {
        let bad = Receipt {
            nonce: "n".into(),
            sent: vec![],
            queries: vec!["quit; echo".into()],
        };
        assert!(matches!(script(&bad), Err(BridgeError::Unsafe { .. })));
    }

    #[test]
    fn nonces_differ_between_pushes() {
        let a = Receipt::for_batch(&[]);
        let b = Receipt::for_batch(&[]);
        assert_ne!(a.nonce, b.nonce);
        assert!(a.nonce.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn parses_csgo_style_value_lines() {
        let names = vec!["fps_max".to_string()];
        assert_eq!(
            parse(r#""fps_max" = "240" ( def. "400" ) client archive"#, &names),
            Some(Event::Value("fps_max".into(), "240".into()))
        );
        assert_eq!(
            parse(r#""fps_max" = "240.000000""#, &names),
            Some(Event::Value("fps_max".into(), "240.000000".into()))
        );
    }

    #[test]
    fn parses_source2_style_value_lines_with_prefixes() {
        let names = vec!["fps_max".to_string(), "r_farz".to_string()];
        assert_eq!(
            parse("fps_max = 240", &names),
            Some(Event::Value("fps_max".into(), "240".into()))
        );
        assert_eq!(
            parse("[12:04:33] [Console] r_farz : 6000 (default: -1)", &names),
            Some(Event::Value("r_farz".into(), "6000".into()))
        );
        assert_eq!(parse("fps_max_ui = 60", &names), None, "whole words only");
        assert_eq!(parse(" - Frame rate limiter", &names), None);
    }

    #[test]
    fn parses_refusals_that_name_the_convar() {
        let names = vec!["r_shadows".to_string(), "sv_cheats".to_string()];
        assert!(matches!(
            parse("Unknown command 'r_shadows'", &names),
            Some(Event::Rejected(n, _)) if n == "r_shadows"
        ));
        assert!(matches!(
            parse(r#"Unknown command: "r_shadows""#, &names),
            Some(Event::Rejected(n, _)) if n == "r_shadows"
        ));
        assert!(matches!(
            parse(
                "Can't change replicated ConVar sv_cheats from console of client, only server operator can change its value",
                &names
            ),
            Some(Event::Rejected(n, _)) if n == "sv_cheats"
        ));
        assert!(matches!(
            parse("r_shadows is cheat protected.", &names),
            Some(Event::Rejected(n, _)) if n == "r_shadows"
        ));
        assert_eq!(
            parse("Unknown command 'con_logfile'", &names),
            None,
            "only queried names count"
        );
    }

    #[test]
    fn parses_markers_with_timestamp_prefixes() {
        assert_eq!(
            parse("[21:04:10] DEADTUNE_ACK 18f3 2", &[]),
            Some(Event::Ack("18f3".into()))
        );
        assert_eq!(
            parse("DEADTUNE_END 18f3", &[]),
            Some(Event::End("18f3".into()))
        );
        assert_eq!(
            parse("DEADTUNE_BOOT 0.1.0", &[]),
            Some(Event::Boot("0.1.0".into()))
        );
    }

    #[test]
    fn same_value_tolerates_formatting() {
        assert!(same_value("240", "240.000000"));
        assert!(same_value("true", "1"));
        assert!(same_value("0", "false"));
        assert!(same_value("Low", "low"));
        assert!(!same_value("240", "400"));
        assert!(!same_value("true", "0"));
    }

    #[test]
    fn full_round_trip_confirms_each_convar() {
        let t0 = Instant::now();
        let mut tracker = Tracker::default();
        tracker.start(
            receipt(&[
                cmd("fps_max", "240"),
                cmd("r_farz", "6000"),
                cmd("r_devonly", "1"),
                cmd("r_quiet", "1"),
            ]),
            t0,
        );
        assert_eq!(
            tracker.status(),
            &PushStatus::Waiting {
                since: t0,
                count: 4
            }
        );
        feed(
            &mut tracker,
            &[
                "Unknown command 'r_devonly'",
                "fps_max = 240",
                "DEADTUNE_ACK abc123 4",
                "\"fps_max\" = \"240\" ( def. \"400\" )",
                " - Frame rate limiter",
                "r_farz = 2000",
                "Unknown command 'r_devonly'",
                "DEADTUNE_END abc123",
            ],
        );
        let PushStatus::Confirmed { results, .. } = tracker.status() else {
            panic!("{:?}", tracker.status());
        };
        assert_eq!(
            results,
            &[
                ("fps_max".to_string(), Outcome::Applied("240".into())),
                (
                    "r_farz".to_string(),
                    Outcome::Rejected("stayed at 2000".into())
                ),
                (
                    "r_devonly".to_string(),
                    Outcome::Rejected("Unknown command 'r_devonly'".into())
                ),
                ("r_quiet".to_string(), Outcome::NoEcho),
            ]
        );
        assert_eq!(tracker.status().applied(), 1);
        assert_eq!(tracker.transcript.len(), 4, "{:?}", tracker.transcript);
    }

    #[test]
    fn lines_before_the_ack_and_other_nonces_are_ignored() {
        let t0 = Instant::now();
        let mut tracker = Tracker::default();
        tracker.start(receipt(&[cmd("fps_max", "240")]), t0);
        feed(
            &mut tracker,
            &[
                "fps_max = 60",
                "DEADTUNE_ACK older 1",
                "fps_max = 60",
                "DEADTUNE_END older",
            ],
        );
        assert!(tracker.is_waiting(), "{:?}", tracker.status());
        feed(
            &mut tracker,
            &[
                "DEADTUNE_ACK abc123 1",
                "fps_max = 240",
                "DEADTUNE_END abc123",
            ],
        );
        assert_eq!(tracker.status().applied(), 1);
    }

    #[test]
    fn times_out_without_an_ack_and_confirms_partial_after_an_ack() {
        let t0 = Instant::now();
        let mut tracker = Tracker::default();
        tracker.start(receipt(&[cmd("fps_max", "240")]), t0);
        tracker.tick(t0 + Duration::from_secs(9));
        assert!(tracker.is_waiting());
        tracker.tick(t0 + Duration::from_secs(11));
        assert!(matches!(
            tracker.status(),
            PushStatus::TimedOut { count: 1, .. }
        ));

        tracker.start(receipt(&[cmd("fps_max", "240")]), t0);
        tracker.observe_line("DEADTUNE_ACK abc123 1");
        tracker.tick(t0 + Duration::from_secs(11));
        assert!(matches!(
            tracker.status(),
            PushStatus::Confirmed { results, .. } if results == &[("fps_max".to_string(), Outcome::NoEcho)]
        ));
    }

    #[test]
    fn boot_marker_is_noticed_without_a_push() {
        let mut tracker = Tracker::default();
        tracker.observe_line("[Console] DEADTUNE_BOOT 0.1.0");
        assert_eq!(
            tracker.boot.as_ref().map(|b| b.version.as_str()),
            Some("0.1.0")
        );
        assert_eq!(tracker.status(), &PushStatus::Idle);
    }
}
