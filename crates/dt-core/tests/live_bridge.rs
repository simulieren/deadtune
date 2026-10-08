//! The live HUD's web channel end to end: DeadTune's server (`web_bridge`), the real bridge
//! page from `docs/bridge/index.html` and the real script, in Node against stand-ins for
//! Panorama and the game's web panel (`tests/fixtures/live_hud_sim.js`). A thread plays
//! DeadTune's window: it hands the mailbox's posts to the server and feeds back the page's
//! news. Skipped where Node is missing; the Mac that builds releases has it.

use std::collections::BTreeSet;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use dt_core::hud::elements::ELEMENTS;
use dt_core::hud::layout::{ElementEdit, HudLayout};
use dt_core::hud::live::{self, Kind, LiveRule, Mailbox, Message};
use dt_core::hud::web_bridge::{self, Bridge, BridgeStatus};
use serde_json::{Value, json};

const KEY: &str = "deadtune.live.v1";

fn node() -> bool {
    let ok = std::process::Command::new("node")
        .arg("--version")
        .output()
        .is_ok();
    if !ok {
        eprintln!("node not found; skipping the bridge simulation");
    }
    ok
}

fn layout(dx: i32) -> HudLayout {
    let mut l = HudLayout::default();
    for (i, e) in ELEMENTS.iter().enumerate() {
        l.elements.insert(
            e.id,
            ElementEdit {
                offset_x: dx + i as i32,
                offset_y: -3,
                opacity_pct: 70,
                ..ElementEdit::default()
            },
        );
    }
    l
}

/// The baked base, the rules DeadTune sends and older rules the page saved last game.
struct Setup {
    base: String,
    wanted: Vec<LiveRule>,
    saved: Vec<LiveRule>,
    panels: Vec<String>,
}

fn setup() -> Setup {
    let baked = layout(10);
    let wanted = live::rules(&baked).unwrap();
    let saved = live::rules(&layout(200)).unwrap();
    let panels: BTreeSet<String> = wanted
        .iter()
        .filter_map(|r| r.selector.strip_prefix('#'))
        .filter(|id| !id.contains([' ', '.', ':']))
        .map(str::to_string)
        .collect();
    Setup {
        base: live::base_id(&baked).unwrap(),
        wanted,
        saved,
        panels: panels.into_iter().collect(),
    }
}

fn saved_storage(s: &Setup) -> Value {
    let titles = live::titles(&Message {
        seq: 9,
        base: s.base.clone(),
        kind: Kind::Full,
        rules: s.saved.clone(),
    });
    json!({ KEY: json!({ "base": s.base, "titles": titles, "at": 1 }).to_string() })
}

/// What DeadTune's window does during a run: sends `wanted` from `send` on, wakes or
/// sleeps the page at `modes`, and from `applied` on tells the page it applied the last
/// message (as a console `ok` would).
#[derive(Clone, Default)]
struct Window {
    send: Duration,
    modes: Vec<(Duration, bool)>,
    applied: Option<Duration>,
}

fn deadtune(bridge: Bridge, base: &str, wanted: Vec<LiveRule>, w: Window, stop: Arc<AtomicBool>) {
    let mut mb = Mailbox::new(base, 500);
    std::thread::spawn(move || {
        let start = Instant::now();
        let mut sent = None;
        let mut modes = w.modes.into_iter().peekable();
        while !stop.load(Ordering::Relaxed) {
            let now = Instant::now();
            let news = bridge.take();
            if news.hello.is_some() {
                mb.reload();
            }
            for seq in news.acked {
                mb.ack(seq);
            }
            while let Some((_, awake)) = modes.next_if(|(at, _)| now >= start + *at) {
                bridge.set_awake(awake);
            }
            let post = if sent.is_none() && now >= start + w.send {
                mb.send(wanted.clone(), now)
            } else if sent.is_some() {
                mb.poll(now)
            } else {
                None
            };
            if let Some(post) = post {
                sent = Some(post.seq);
                bridge.post(post);
            }
            if let (Some(seq), Some(after)) = (sent, w.applied)
                && now >= start + after
            {
                bridge.set_applied(seq);
            }
            std::thread::sleep(Duration::from_millis(10));
        }
    });
}

struct Run {
    report: Value,
    status: BridgeStatus,
}

impl Run {
    fn lines(&self, kind: &str) -> Vec<String> {
        self.report["events"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|e| e["kind"] == kind)
            .map(|e| e["text"].as_str().unwrap().to_string())
            .collect()
    }

    fn said(&self, text: &str) -> bool {
        self.lines("msg")
            .iter()
            .any(|l| l == &format!("DEADTUNE_LIVE {text}"))
    }

    fn assert_styles(&self, rules: &[LiveRule], panels: &[String]) {
        for r in rules {
            let Some(id) = r
                .selector
                .strip_prefix('#')
                .filter(|id| panels.iter().any(|p| p == id))
            else {
                continue;
            };
            let prop = r
                .prop
                .split('-')
                .enumerate()
                .map(|(i, w)| {
                    if i == 0 {
                        w.to_string()
                    } else {
                        w[..1].to_uppercase() + &w[1..]
                    }
                })
                .collect::<String>();
            assert_eq!(
                self.report["styles"][id][&prop].as_str(),
                Some(r.value.as_str()),
                "{id} {prop}: {:#?}",
                self.lines("msg")
            );
        }
    }
}

/// Runs the scenario: `window` starts DeadTune's server and window, else the page's port
/// has nothing on it. `script` adjusts the script's config.
fn run(s: &Setup, window: Option<Window>, extra: Value) -> Run {
    run_with(s, window, extra, &[("retry: 20,", "retry: 0.3,")])
}

fn run_with(s: &Setup, window: Option<Window>, extra: Value, config: &[(&str, &str)]) -> Run {
    let bridge = Bridge::default();
    let stop = Arc::new(AtomicBool::new(false));
    let port = match window {
        Some(w) => {
            let port = web_bridge::serve(bridge.clone(), 0, || {}).unwrap();
            deadtune(bridge.clone(), &s.base, s.wanted.clone(), w, stop.clone());
            port
        }
        None => std::net::TcpListener::bind(("127.0.0.1", 0))
            .unwrap()
            .local_addr()
            .unwrap()
            .port(),
    };
    let mut script = live::script(&s.base).replacen(
        &format!("port: {}", web_bridge::PORT),
        &format!("port: {port}"),
        1,
    );
    for (from, to) in config {
        assert!(script.contains(from), "{from}");
        script = script.replacen(from, to, 1);
    }
    let page = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../docs/bridge/index.html"
    ))
    .unwrap();
    let mut scenario = json!({
        "script": script,
        "page": page,
        "panels": s.panels,
        "run_ms": 2500,
    });
    for (k, v) in extra.as_object().unwrap() {
        scenario[k] = v.clone();
    }
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("scenario.json");
    std::fs::write(&path, scenario.to_string()).unwrap();
    let sim = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/live_hud_sim.js"
    );
    let out = std::process::Command::new("node")
        .arg(sim)
        .arg(&path)
        .output()
        .unwrap();
    stop.store(true, Ordering::Relaxed);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    Run {
        report: serde_json::from_slice(&out.stdout).unwrap(),
        status: bridge.status(),
    }
}

fn now() -> Option<Window> {
    Some(Window::default())
}

#[test]
fn deadtunes_edits_reach_the_panels_through_the_page_once_each() {
    if !node() {
        return;
    }
    let s = setup();
    let r = run(&s, now(), json!({}));
    r.assert_styles(&s.wanted, &s.panels);
    let base = &s.base;
    assert!(
        r.said(&format!("hello {base} web=panel")),
        "{:#?}",
        r.lines("msg")
    );
    assert!(r.said("web ready storage=ok"));
    assert!(r.said("web fetch ok"));
    assert!(r.said("web restore none"), "nothing saved yet");
    assert_eq!(
        r.report["loads"].as_object().unwrap().len(),
        1,
        "one web panel: {}",
        r.report["loads"]
    );
    let oks: Vec<String> = r
        .lines("msg")
        .into_iter()
        .filter(|l| l.ends_with(&format!(" ok {base}")))
        .collect();
    assert_eq!(
        oks.len(),
        1,
        "every title came twice, applied once: {oks:?}"
    );
    assert!(r.lines("cmd").is_empty(), "no console commands");
    let c = r.status.counters;
    assert_eq!(c.hellos, 1);
    assert_eq!(c.acks, 1, "{c:?}");
    assert_eq!(c.refused, 0);
    let seq: u32 = oks[0]
        .trim_start_matches("DEADTUNE_LIVE ")
        .split(' ')
        .next()
        .unwrap()
        .parse()
        .unwrap();
    assert_eq!(r.status.recent_acks, [seq]);
    let kept: Value = serde_json::from_str(r.report["storage"][KEY].as_str().unwrap()).unwrap();
    assert_eq!(kept["base"], s.base.as_str());
    let titles: Vec<String> = serde_json::from_value(kept["titles"].clone()).unwrap();
    let message = live::parse_titles(&titles).unwrap();
    assert_eq!((message.seq, message.rules), (seq, s.wanted.clone()));
}

#[test]
fn saved_edits_come_back_at_game_start_without_deadtune() {
    if !node() {
        return;
    }
    let s = setup();
    let r = run(&s, None, json!({ "storage": saved_storage(&s) }));
    r.assert_styles(&s.saved, &s.panels);
    assert!(r.said("web restored 9"), "{:#?}", r.lines("msg"));
    assert!(
        r.lines("msg")
            .iter()
            .any(|l| l.starts_with("DEADTUNE_LIVE web fetch blocked TypeError")),
        "{:#?}",
        r.lines("msg")
    );
}

#[test]
fn a_restore_gives_way_to_deadtunes_newer_edits() {
    if !node() {
        return;
    }
    let s = setup();
    let r = run(
        &s,
        Some(Window {
            send: Duration::from_millis(800),
            ..Window::default()
        }),
        json!({ "storage": saved_storage(&s) }),
    );
    assert!(
        r.said("web restored 9"),
        "restored first: {:#?}",
        r.lines("msg")
    );
    r.assert_styles(&s.wanted, &s.panels);

    let late = run(
        &s,
        now(),
        json!({ "storage": saved_storage(&s), "delay_restore_ms": 1200 }),
    );
    late.assert_styles(&s.wanted, &s.panels);
    assert!(
        !late.said("web restored 9"),
        "a restore that comes after DeadTune's message is dropped: {:#?}",
        late.lines("msg")
    );
}

#[test]
fn a_page_reloaded_with_a_fragment_neither_replays_it_nor_says_hello() {
    if !node() {
        return;
    }
    let s = setup();
    let r = run(&s, now(), json!({ "reload_on_hash": true }));
    r.assert_styles(&s.wanted, &s.panels);
    let c = r.status.counters;
    assert_eq!(c.hellos, 1, "{c:?}");
    assert_eq!(c.acks, 0, "the fragment at load is not acted on: {c:?}");
    assert!(r.report["loads"]["DtLiveWeb"].as_u64().unwrap() >= 2);
}

#[test]
fn a_page_that_never_loads_is_retried_a_few_times() {
    if !node() {
        return;
    }
    let s = setup();
    let r = run(&s, now(), json!({ "page_fails": true }));
    assert_eq!(
        r.report["loads"]["DtLiveWeb"].as_u64(),
        Some(u64::from(live::RETRIES) + 1),
        "five tries, then a blank page"
    );
    assert!(
        r.lines("seturl").last().unwrap().ends_with(" about:blank"),
        "{:#?}",
        r.lines("seturl")
    );
    for n in 1..live::RETRIES {
        assert!(r.said(&format!("web retry {n}")), "{:#?}", r.lines("msg"));
    }
    assert!(r.said("web gave up"), "{:#?}", r.lines("msg"));
    assert_eq!(r.status.counters.waits, 0);

    let none = run(&s, now(), json!({ "no_panel": true }));
    assert!(none.said("web nopanel"));
    assert!(none.said(&format!("hello {} web=nopanel", s.base)));
}

impl Run {
    fn at(&self, kind: &str, has: &str) -> Vec<u64> {
        self.report["events"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|e| e["kind"] == kind && e["text"].as_str().unwrap().contains(has))
            .map(|e| e["at"].as_u64().unwrap())
            .collect()
    }

    /// The page's titles with this body, by time.
    fn titled(&self, body: &str) -> Vec<u64> {
        self.report["events"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|e| {
                e["kind"] == "title"
                    && e["text"].as_str().unwrap().splitn(3, ' ').nth(2) == Some(body)
            })
            .map(|e| e["at"].as_u64().unwrap())
            .collect()
    }
}

/// Only the rules on a plain `#id`, which no panel can stop matching.
fn ids_only(mut s: Setup) -> Setup {
    s.wanted
        .retain(|r| s.panels.iter().any(|p| r.selector == format!("#{p}")));
    assert!(!s.wanted.is_empty());
    s
}

fn ms(n: u64) -> Duration {
    Duration::from_millis(n)
}

#[test]
fn the_page_sleeps_and_wakes_with_deadtune_and_works_only_on_messages() {
    if !node() {
        return;
    }
    let s = ids_only(setup());
    let r = run(
        &s,
        Some(Window {
            modes: vec![(ms(0), true), (ms(1200), false), (ms(2600), true)],
            ..Window::default()
        }),
        json!({ "run_ms": 3600 }),
    );
    r.assert_styles(&s.wanted, &s.panels);
    let wakes = r.titled("wake");
    let sleeps = r.titled("sleep");
    assert_eq!(wakes.len(), 2, "{:#?}", r.lines("title"));
    assert_eq!(sleeps.len(), 1, "{:#?}", r.lines("title"));
    let (slept, woke) = (sleeps[0], wakes[1]);
    assert!(
        (1000..2400).contains(&(woke - slept)),
        "asleep and awake again within a second each: {slept} {woke}"
    );
    let fetches = r.at("fetch", "/wait?");
    let asleep: Vec<&u64> = fetches
        .iter()
        .filter(|t| (slept..woke).contains(t))
        .collect();
    assert!(
        asleep.len() <= 1,
        "one held wait while asleep: {asleep:?} of {fetches:?}"
    );
    let work = r.at("work", "");
    assert!(!work.is_empty());
    let messages = r.at("title", " dt1 ");
    for t in &work {
        assert!(
            messages.iter().any(|m| (*m..m + 300).contains(t)),
            "work at {t} ms only right after a message ({messages:?})"
        );
    }
    assert!(
        !work.iter().any(|t| (slept..woke).contains(t)),
        "no work while asleep: {work:?}"
    );
    let c = r.status.counters;
    assert_eq!((c.wakes, c.sleeps), (2, 1), "{c:?}");
    assert!(c.waits <= 8, "waits are held, not polled: {c:?}");
    assert_eq!(c.idle, 0, "none ran out within 20 s: {c:?}");
}

#[test]
fn a_lost_chunk_is_asked_for_again_instead_of_waiting_for_a_resend() {
    if !node() {
        return;
    }
    let mut s = setup();
    s.panels = (0..150).map(|i| format!("LiveSimPanel{i}")).collect();
    s.wanted = s
        .panels
        .iter()
        .map(|p| LiveRule {
            selector: format!("#{p}"),
            prop: "opacity".into(),
            value: "0.5".into(),
        })
        .collect();
    let r = run(&s, now(), json!({ "drop_titles": [" 2/"] }));
    assert!(
        r.lines("seturl").iter().any(|l| l.contains(".need.")),
        "{:#?}",
        r.lines("seturl")
    );
    r.assert_styles(&s.wanted, &s.panels);
    let c = r.status.counters;
    assert_eq!((c.delivered, c.acks), (1, 1), "no resend needed: {c:?}");
}

#[test]
fn the_page_keeps_what_deadtune_says_was_applied_when_its_own_ack_is_lost() {
    if !node() {
        return;
    }
    let s = setup();
    let r = run(
        &s,
        Some(Window {
            modes: vec![(ms(0), true), (ms(1000), false)],
            applied: Some(ms(600)),
            ..Window::default()
        }),
        json!({ "no_ack_fragments": true, "run_ms": 1800 }),
    );
    assert_eq!(r.status.counters.acks, 0);
    let kept: Value = serde_json::from_str(
        r.report["storage"][KEY]
            .as_str()
            .unwrap_or_else(|| panic!("nothing kept: {}", r.report["storage"])),
    )
    .unwrap();
    let titles: Vec<String> = serde_json::from_value(kept["titles"].clone()).unwrap();
    assert_eq!(live::parse_titles(&titles).unwrap().rules, s.wanted);
}

#[test]
fn a_page_that_goes_quiet_while_awake_is_loaded_again() {
    if !node() {
        return;
    }
    let s = setup();
    let r = run_with(
        &s,
        Some(Window {
            modes: vec![(ms(0), true)],
            ..Window::default()
        }),
        json!({ "freeze_page_ms": 600, "run_ms": 2500 }),
        &[("beat: 1,", "beat: 0.2,"), ("quiet: 30,", "quiet: 0.8,")],
    );
    assert!(r.said("web reload 1"), "{:#?}", r.lines("msg"));
    assert!(r.report["loads"]["DtLiveWeb"].as_u64().unwrap() >= 2);
    assert!(r.status.counters.hellos >= 2, "{:?}", r.status.counters);
}
