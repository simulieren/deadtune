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

/// DeadTune's window: sends `wanted` from `after` on and answers the page's news.
fn deadtune(bridge: Bridge, s: &Setup, after: Duration, stop: Arc<AtomicBool>) {
    let mut mb = Mailbox::new(&s.base, 500);
    let wanted = s.wanted.clone();
    std::thread::spawn(move || {
        let start = Instant::now();
        let mut sent = false;
        while !stop.load(Ordering::Relaxed) {
            let now = Instant::now();
            let news = bridge.take();
            if news.hello.is_some() {
                mb.reload();
            }
            for seq in news.acked {
                mb.ack(seq);
            }
            let post = if !sent && now >= start + after {
                sent = true;
                mb.send(wanted.clone(), now)
            } else if sent {
                mb.poll(now)
            } else {
                None
            };
            if let Some(post) = post {
                bridge.post(post);
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

/// Runs the scenario: `server` starts DeadTune's server and window (after `after`), else
/// the page's port has nothing on it.
fn run(s: &Setup, server: Option<Duration>, extra: Value) -> Run {
    let bridge = Bridge::default();
    let stop = Arc::new(AtomicBool::new(false));
    let port = match server {
        Some(after) => {
            let port = web_bridge::serve(bridge.clone(), 0, || {}).unwrap();
            deadtune(bridge.clone(), s, after, stop.clone());
            port
        }
        None => std::net::TcpListener::bind(("127.0.0.1", 0))
            .unwrap()
            .local_addr()
            .unwrap()
            .port(),
    };
    let script = live::script(&s.base)
        .replacen(
            &format!("port: {}", web_bridge::PORT),
            &format!("port: {port}"),
            1,
        )
        .replacen("retry: 20,", "retry: 0.3,", 1);
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

#[test]
fn deadtunes_edits_reach_the_panels_through_the_page_once_each() {
    if !node() {
        return;
    }
    let s = setup();
    let r = run(&s, Some(Duration::ZERO), json!({}));
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
    assert!(
        r.said("web control title=about:blank"),
        "the plain http:// control stays blank"
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
    assert_eq!(c.controls, 0, "the stand-in refuses http:// like the game");
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
        Some(Duration::from_millis(800)),
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
        Some(Duration::ZERO),
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
    let r = run(&s, Some(Duration::ZERO), json!({ "reload_on_hash": true }));
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
    let r = run(&s, Some(Duration::ZERO), json!({ "page_fails": true }));
    assert_eq!(
        r.report["loads"]["DtLiveWeb"].as_u64(),
        Some(u64::from(live::RETRIES))
    );
    for n in 1..live::RETRIES {
        assert!(r.said(&format!("web retry {n}")), "{:#?}", r.lines("msg"));
    }
    assert_eq!(r.status.counters.polls, 0);

    let none = run(&s, Some(Duration::ZERO), json!({ "no_panel": true }));
    assert!(none.said("web nopanel"));
    assert!(none.said(&format!("hello {} web=nopanel", s.base)));
}
