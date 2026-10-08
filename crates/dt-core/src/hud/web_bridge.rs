//! DeadTune's end of the live HUD's web channel. Since the 2026-10-01 update the game's web
//! panel loads only HTTPS pages, so the live script opens DeadTune's static page on GitHub
//! Pages (`docs/bridge/`), and that page waits on this server on 127.0.0.1 for the next
//! message and hands it to the script through its title. Each wait is held until there is
//! something new or `WAIT` runs out, so an idle page makes one request every 20 s. DeadTune
//! also says whether the page is awake (someone is editing the HUD) or asleep. Design:
//! docs/plans/live-hud/plan.md section 3.

use std::io::{self, BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::{Arc, Condvar, Mutex, MutexGuard};
use std::time::{Duration, Instant};

use super::live::Post;

pub const PORT: u16 = 47613;
pub const PAGE_ORIGIN: &str = "https://simulieren.github.io";
pub const PAGE_URL: &str = "https://simulieren.github.io/deadtune/bridge/";
/// Every title the page sets starts with this; the script ignores a page of another
/// version (an old copy in the game's browser cache) and loads it again. The page acts
/// only for a script that asks for this version (`v=` in its address).
pub const PROTOCOL: &str = "DTLIVE:v2";
/// How long the server holds a wait with nothing new before answering 204.
pub const WAIT: Duration = Duration::from_secs(20);
/// A page that asked this recently, or is waiting now, is connected.
pub const CONNECTED: Duration = Duration::from_secs(2);
const RECENT_ACKS: usize = 32;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Request {
    /// The page waits for a message other than `since`, the one it has, or for DeadTune to
    /// wake or sleep it; `awake` is the mode it is in. `hello` marks its first wait after
    /// the script opened it; `base` is the HUD the script was built for.
    Wait {
        since: u32,
        base: Option<String>,
        hello: bool,
        awake: bool,
    },
    /// The script applied message `seq`.
    Ack {
        seq: u32,
    },
    /// A browser's CORS and private network preflight.
    Preflight,
    Other,
}

/// The request a request line names (`GET /wait?since=3&base=1a2b3c4d&awake=1 HTTP/1.1`).
pub fn parse_request(line: &str) -> Request {
    let mut parts = line.split(' ');
    let (Some(method), Some(target)) = (parts.next(), parts.next()) else {
        return Request::Other;
    };
    if method == "OPTIONS" {
        return Request::Preflight;
    }
    if method != "GET" {
        return Request::Other;
    }
    let (path, query) = target.split_once('?').unwrap_or((target, ""));
    let param = |key: &str| {
        query
            .split('&')
            .find_map(|kv| kv.strip_prefix(key)?.strip_prefix('='))
    };
    match path {
        "/wait" => Request::Wait {
            since: param("since").and_then(|s| s.parse().ok()).unwrap_or(0),
            base: param("base")
                .filter(|b| {
                    !b.is_empty() && b.len() <= 16 && b.chars().all(|c| c.is_ascii_hexdigit())
                })
                .map(str::to_string),
            hello: param("hello") == Some("1"),
            awake: param("awake") == Some("1"),
        },
        "/ack" => match param("seq").and_then(|s| s.parse().ok()) {
            Some(seq) => Request::Ack { seq },
            None => Request::Other,
        },
        _ => Request::Other,
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Counters {
    /// Waits the page made.
    pub waits: u64,
    /// Waits that opened a page: the script loaded and opened it.
    pub hellos: u64,
    /// Waits answered with a message.
    pub delivered: u64,
    /// Waits that ran out with nothing new.
    pub idle: u64,
    /// Waits answered by waking the page, or by putting it to sleep.
    pub wakes: u64,
    pub sleeps: u64,
    pub acks: u64,
    pub preflights: u64,
    /// Requests from anywhere but the bridge page, refused.
    pub refused: u64,
    pub other: u64,
}

/// What the page did since the last `Bridge::take`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PageNews {
    /// The base of a page that just opened, when one did.
    pub hello: Option<String>,
    /// The base the page last waited with.
    pub base: Option<String>,
    /// When the page last asked or was last answered.
    pub last_poll: Option<Instant>,
    /// The page is waiting now.
    pub open: bool,
    pub acked: Vec<u32>,
}

/// Everything the server knows, for "Check live preview".
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct BridgeStatus {
    /// The port DeadTune listens on, or why it couldn't; `None` before `serve`.
    pub listening: Option<Result<u16, String>>,
    pub counters: Counters,
    pub first_poll: Option<Instant>,
    pub last_poll: Option<Instant>,
    pub base: Option<String>,
    /// The message the page gets next, by seq.
    pub posted: Option<u32>,
    /// What DeadTune wants: the page awake (someone edits the HUD) or asleep.
    pub awake: bool,
    /// Waits held now.
    pub open: usize,
    /// The last message DeadTune knows the script applied, by either path (the page's ack
    /// or the console's `ok`). Every answer carries it, so the page keeps that message for
    /// the next game start even when its own ack was lost.
    pub applied: Option<u32>,
    pub recent_acks: Vec<u32>,
}

#[derive(Default)]
struct Shared {
    posted: Option<Post>,
    news: PageNews,
    status: BridgeStatus,
}

impl Shared {
    /// The answer to a wait from a page that has `since` and is `awake`, when there is
    /// anything new for it.
    fn answer(&mut self, since: u32, awake: bool) -> Option<Response> {
        let post = self.posted.as_ref().filter(|p| p.seq != since);
        let mode = self.status.awake;
        if post.is_none() && mode == awake {
            return None;
        }
        let mut body = serde_json::json!({ "awake": mode });
        if let Some(applied) = self.status.applied {
            body["applied"] = applied.into();
        }
        if let Some(post) = post {
            body["seq"] = post.seq.into();
            body["chunks"] = post.titles.clone().into();
            body["keep"] = post.keep.clone().into();
            self.status.counters.delivered += 1;
        }
        if mode != awake {
            let c = &mut self.status.counters;
            *(if mode { &mut c.wakes } else { &mut c.sleeps }) += 1;
        }
        Some(Response {
            status: 200,
            content_type: Some("application/json"),
            body: body.to_string(),
        })
    }
}

/// The state the GUI and the server threads share: the GUI posts messages, sets the mode
/// and takes the page's news; the server answers the page from it, holding each wait until
/// something changes.
#[derive(Clone, Default)]
pub struct Bridge(Arc<(Mutex<Shared>, Condvar)>);

impl std::fmt::Debug for Bridge {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("Bridge").field(&self.status()).finish()
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Response {
    pub status: u16,
    pub content_type: Option<&'static str>,
    pub body: String,
}

impl Response {
    fn empty(status: u16) -> Response {
        Response {
            status,
            content_type: None,
            body: String::new(),
        }
    }

    /// The response on the wire. Every one carries the CORS and private network headers:
    /// the page is on github.io and DeadTune on this PC. The preflight may be cached for
    /// ten minutes.
    pub fn bytes(&self) -> Vec<u8> {
        let reason = match self.status {
            200 => "OK",
            204 => "No Content",
            403 => "Forbidden",
            _ => "Not Found",
        };
        let kind = self
            .content_type
            .map_or(String::new(), |t| format!("Content-Type: {t}\r\n"));
        format!(
            "HTTP/1.1 {} {reason}\r\nAccess-Control-Allow-Origin: {PAGE_ORIGIN}\r\n\
             Access-Control-Allow-Private-Network: true\r\nAccess-Control-Allow-Methods: GET, OPTIONS\r\n\
             Access-Control-Max-Age: 600\r\nVary: Origin\r\nCache-Control: no-store\r\n\
             {kind}Content-Length: {}\r\nConnection: close\r\n\r\n{}",
            self.status,
            self.body.len(),
            self.body
        )
        .into_bytes()
    }
}

impl Bridge {
    fn lock(&self) -> MutexGuard<'_, Shared> {
        self.0.0.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// The message the page hands the script next; a waiting page gets it now.
    pub fn post(&self, post: Post) {
        let mut s = self.lock();
        s.status.posted = Some(post.seq);
        s.posted = Some(post);
        self.0.1.notify_all();
    }

    /// The message the page gets next.
    pub fn posted(&self) -> Option<Post> {
        self.lock().posted.clone()
    }

    pub fn clear(&self) {
        let mut s = self.lock();
        s.status.posted = None;
        s.posted = None;
    }

    /// Wakes the page (someone edits the HUD) or puts it to sleep; a waiting page hears it
    /// now. The same mode again changes nothing.
    pub fn set_awake(&self, awake: bool) {
        let mut s = self.lock();
        if s.status.awake != awake {
            s.status.awake = awake;
            self.0.1.notify_all();
        }
    }

    /// The script applied message `seq`; the page hears it with its next answer.
    pub fn set_applied(&self, seq: u32) {
        self.lock().status.applied = Some(seq);
    }

    pub fn take(&self) -> PageNews {
        let mut s = self.lock();
        PageNews {
            hello: s.news.hello.take(),
            acked: std::mem::take(&mut s.news.acked),
            open: s.status.open > 0,
            ..s.news.clone()
        }
    }

    pub fn status(&self) -> BridgeStatus {
        self.lock().status.clone()
    }

    fn listening(&self, result: Result<u16, String>) {
        self.lock().status.listening = Some(result);
    }

    /// The answer to `req` from a page at `origin`. A wait with nothing new is held up to
    /// `hold`. `wake` asks the GUI to look at the news now: a page opened, an ack, or a
    /// page that was away is back. Only the bridge page may wait or ack.
    pub fn respond(
        &self,
        req: &Request,
        origin: Option<&str>,
        now: Instant,
        hold: Duration,
        wake: &dyn Fn(),
    ) -> Response {
        let mut s = self.lock();
        let ours = origin == Some(PAGE_ORIGIN);
        match req {
            Request::Wait { .. } | Request::Ack { .. } | Request::Preflight if !ours => {
                s.status.counters.refused += 1;
                Response::empty(403)
            }
            Request::Wait {
                since,
                base,
                hello,
                awake,
            } => {
                let away = s.status.open == 0
                    && s.news
                        .last_poll
                        .is_none_or(|t| now.saturating_duration_since(t) >= CONNECTED);
                s.status.counters.waits += 1;
                s.status.first_poll.get_or_insert(now);
                s.status.last_poll = Some(now);
                s.news.last_poll = Some(now);
                if base.is_some() {
                    s.status.base = base.clone();
                    s.news.base = base.clone();
                }
                if *hello {
                    s.status.counters.hellos += 1;
                    s.news.hello = Some(base.clone().unwrap_or_default());
                    // A new script shows the baked HUD or what the page restored; what was
                    // posted for the last one may be a patch, so it waits for DeadTune's
                    // next full message.
                    s.posted = None;
                    s.status.posted = None;
                }
                if *hello || away {
                    wake();
                }
                s.status.open += 1;
                let deadline = Instant::now() + hold;
                let response = loop {
                    if let Some(r) = s.answer(*since, *awake) {
                        break r;
                    }
                    let left = deadline.saturating_duration_since(Instant::now());
                    if left.is_zero() {
                        if !hold.is_zero() {
                            s.status.counters.idle += 1;
                        }
                        break Response::empty(204);
                    }
                    s = self
                        .0
                        .1
                        .wait_timeout(s, left)
                        .unwrap_or_else(|e| e.into_inner())
                        .0;
                };
                s.status.open -= 1;
                let answered = if hold.is_zero() { now } else { Instant::now() };
                s.status.last_poll = Some(answered);
                s.news.last_poll = Some(answered);
                response
            }
            Request::Ack { seq } => {
                s.status.counters.acks += 1;
                s.news.acked.push(*seq);
                s.status.recent_acks.push(*seq);
                let over = s.status.recent_acks.len().saturating_sub(RECENT_ACKS);
                s.status.recent_acks.drain(..over);
                wake();
                Response::empty(204)
            }
            Request::Preflight => {
                s.status.counters.preflights += 1;
                Response::empty(204)
            }
            Request::Other => {
                s.status.counters.other += 1;
                Response::empty(404)
            }
        }
    }
}

/// The request line and the `Origin` header, after reading the whole head so closing the
/// socket doesn't reset a request the browser is still sending.
fn read_head(stream: &TcpStream) -> io::Result<(String, Option<String>)> {
    stream.set_read_timeout(Some(Duration::from_secs(2)))?;
    let mut reader = BufReader::new(stream);
    let mut first = String::new();
    reader.read_line(&mut first)?;
    let mut origin = None;
    for _ in 0..100 {
        let mut line = String::new();
        if reader.read_line(&mut line)? == 0 || line.trim_end().is_empty() {
            break;
        }
        if let Some((name, value)) = line.split_once(':')
            && name.eq_ignore_ascii_case("origin")
        {
            origin = Some(value.trim().to_string());
        }
    }
    Ok((first.trim_end().to_string(), origin))
}

/// Listens on 127.0.0.1 only (`port` 0 picks a free one) and answers each connection on
/// its own thread, so a held wait or a browser's idle preconnect never holds up another
/// request. Returns the port.
pub fn serve(
    bridge: Bridge,
    port: u16,
    wake: impl Fn() + Send + Sync + 'static,
) -> io::Result<u16> {
    let listener = match TcpListener::bind(("127.0.0.1", port)) {
        Ok(l) => l,
        Err(e) => {
            bridge.listening(Err(e.to_string()));
            return Err(e);
        }
    };
    let port = listener.local_addr()?.port();
    bridge.listening(Ok(port));
    let wake = Arc::new(wake);
    std::thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            let bridge = bridge.clone();
            let wake = wake.clone();
            std::thread::spawn(move || {
                let Ok((line, origin)) = read_head(&stream) else {
                    return;
                };
                let response = bridge.respond(
                    &parse_request(&line),
                    origin.as_deref(),
                    Instant::now(),
                    WAIT,
                    &|| wake(),
                );
                let _ = (&stream).write_all(&response.bytes());
            });
        }
    });
    Ok(port)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    const PAGE: Option<&str> = Some(PAGE_ORIGIN);
    const NOW: Duration = Duration::ZERO;

    #[test]
    fn reads_every_request_the_page_makes() {
        assert_eq!(
            parse_request("GET /wait?since=41&base=1a2b3c4d&awake=1 HTTP/1.1"),
            Request::Wait {
                since: 41,
                base: Some("1a2b3c4d".into()),
                hello: false,
                awake: true,
            }
        );
        assert_eq!(
            parse_request("GET /wait?hello=1&since=0&base=1a2b3c4d&awake=0 HTTP/1.1"),
            Request::Wait {
                since: 0,
                base: Some("1a2b3c4d".into()),
                hello: true,
                awake: false,
            }
        );
        assert_eq!(
            parse_request("GET /wait HTTP/1.1"),
            Request::Wait {
                since: 0,
                base: None,
                hello: false,
                awake: false,
            }
        );
        assert_eq!(
            parse_request("GET /wait?base=<script> HTTP/1.1"),
            Request::Wait {
                since: 0,
                base: None,
                hello: false,
                awake: false,
            },
            "only hex bases"
        );
        assert_eq!(
            parse_request("GET /ack?seq=7 HTTP/1.1"),
            Request::Ack { seq: 7 }
        );
        assert_eq!(parse_request("GET /ack?seq=x HTTP/1.1"), Request::Other);
        assert_eq!(
            parse_request("OPTIONS /wait?since=0 HTTP/1.1"),
            Request::Preflight
        );
        assert_eq!(
            parse_request("GET /live?since=0 HTTP/1.1"),
            Request::Other,
            "the old 150 ms poll is gone"
        );
        assert_eq!(parse_request("POST /wait HTTP/1.1"), Request::Other);
        assert_eq!(parse_request("GET /favicon.ico HTTP/1.1"), Request::Other);
        assert_eq!(parse_request(""), Request::Other);
    }

    fn text(r: &Response) -> String {
        String::from_utf8(r.bytes()).unwrap()
    }

    /// Counts the times the server asked the window to look.
    #[derive(Default)]
    struct Looks(AtomicUsize);

    impl Looks {
        fn wake(&self) -> impl Fn() + '_ {
            || {
                self.0.fetch_add(1, Ordering::Relaxed);
            }
        }
        fn take(&self) -> usize {
            self.0.swap(0, Ordering::Relaxed)
        }
    }

    fn ask(bridge: &Bridge, req: &Request, now: Instant, hold: Duration) -> Response {
        bridge.respond(req, PAGE, now, hold, &|| {})
    }

    #[test]
    fn every_answer_lets_the_github_page_read_it_and_preflights_are_cached() {
        let bridge = Bridge::default();
        let now = Instant::now();
        let looks = Looks::default();
        let preflight = bridge.respond(&Request::Preflight, PAGE, now, NOW, &looks.wake());
        assert_eq!(looks.take(), 0);
        let wire = text(&preflight);
        assert!(wire.starts_with("HTTP/1.1 204 No Content\r\n"), "{wire}");
        for header in [
            "Access-Control-Allow-Origin: https://simulieren.github.io\r\n",
            "Access-Control-Allow-Private-Network: true\r\n",
            "Access-Control-Allow-Methods: GET, OPTIONS\r\n",
            "Access-Control-Max-Age: 600\r\n",
        ] {
            assert!(wire.contains(header), "{header} in {wire}");
        }
        let missing = ask(&bridge, &Request::Other, now, NOW);
        assert!(text(&missing).starts_with("HTTP/1.1 404"));
        assert!(text(&missing).contains("Access-Control-Allow-Private-Network: true"));
        assert_eq!(bridge.status().counters.preflights, 1);
        assert_eq!(bridge.status().counters.other, 1);
    }

    #[test]
    fn only_the_bridge_page_may_wait_or_ack() {
        let bridge = Bridge::default();
        let now = Instant::now();
        bridge.post(post(5));
        let looks = Looks::default();
        for origin in [None, Some("https://evil.example"), Some("null")] {
            for req in [
                wait(0, false, false),
                Request::Ack { seq: 5 },
                Request::Preflight,
            ] {
                let r = bridge.respond(&req, origin, now, NOW, &looks.wake());
                assert_eq!(r.status, 403, "{req:?} from {origin:?}");
            }
        }
        assert_eq!(looks.take(), 0);
        assert_eq!(bridge.status().counters.refused, 9);
        assert_eq!(bridge.status().counters.waits, 0);
        assert!(bridge.take().acked.is_empty());
    }

    fn wait(since: u32, hello: bool, awake: bool) -> Request {
        Request::Wait {
            since,
            base: Some("1a2b3c4d".into()),
            hello,
            awake,
        }
    }

    fn post(seq: u32) -> Post {
        Post {
            seq,
            titles: vec![
                format!("dt1 {seq} 1/2 1a2b3c4d patch a\"b"),
                format!("dt1 {seq} 2/2 1a2b3c4d patch c"),
            ],
            keep: vec![format!("dt1 {seq} 1/1 1a2b3c4d full x")],
        }
    }

    fn json(r: &Response) -> serde_json::Value {
        assert_eq!(r.status, 200, "{}", r.body);
        serde_json::from_str(&r.body).unwrap()
    }

    #[test]
    fn a_wait_gets_a_message_it_lacks_at_once() {
        let bridge = Bridge::default();
        let t0 = Instant::now();
        let looks = Looks::default();
        let r = bridge.respond(&wait(0, false, false), PAGE, t0, NOW, &looks.wake());
        assert_eq!(r.status, 204, "nothing yet, and no time to hold");
        assert_eq!(looks.take(), 1, "a page that was away is back");
        bridge.post(post(5));
        let r = bridge.respond(
            &wait(0, false, false),
            PAGE,
            t0 + Duration::from_millis(150),
            NOW,
            &looks.wake(),
        );
        assert_eq!(looks.take(), 0);
        let m = json(&r);
        assert_eq!(m["seq"], 5);
        assert_eq!(m["awake"], false);
        assert_eq!(m["chunks"][0], "dt1 5 1/2 1a2b3c4d patch a\"b");
        assert_eq!(m["chunks"].as_array().unwrap().len(), 2);
        assert_eq!(m["keep"][0], "dt1 5 1/1 1a2b3c4d full x");
        assert!(text(&r).contains("Content-Type: application/json\r\n"));
        let r = ask(&bridge, &wait(5, false, false), t0, NOW);
        assert_eq!(r.status, 204, "nothing other than what the page has");
        let status = bridge.status();
        assert_eq!((status.counters.waits, status.counters.delivered), (3, 1));
        assert_eq!(status.first_poll, Some(t0));
        assert_eq!(status.base.as_deref(), Some("1a2b3c4d"));
        assert_eq!(status.posted, Some(5));
        bridge.clear();
        assert_eq!(ask(&bridge, &wait(0, false, false), t0, NOW).status, 204);
    }

    #[test]
    fn a_wait_is_held_until_its_time_runs_out() {
        let bridge = Bridge::default();
        let start = Instant::now();
        let r = ask(
            &bridge,
            &wait(0, false, false),
            start,
            Duration::from_millis(150),
        );
        assert_eq!(r.status, 204);
        assert!(start.elapsed() >= Duration::from_millis(150), "held");
        let status = bridge.status();
        assert_eq!(status.counters.idle, 1);
        assert_eq!(status.open, 0);
        assert!(bridge.take().last_poll.unwrap() >= start + Duration::from_millis(150));
    }

    /// Holds a wait on another thread; returns what it answered and how long it took.
    fn held(bridge: &Bridge, req: Request) -> std::thread::JoinHandle<(Response, Duration)> {
        let b = bridge.clone();
        let handle = std::thread::spawn(move || {
            let start = Instant::now();
            let r = ask(&b, &req, start, Duration::from_secs(10));
            (r, start.elapsed())
        });
        let t = Instant::now();
        while bridge.status().open == 0 {
            assert!(
                t.elapsed() < Duration::from_secs(5),
                "the wait never opened"
            );
            std::thread::sleep(Duration::from_millis(2));
        }
        handle
    }

    #[test]
    fn a_held_wait_is_answered_when_a_message_is_posted() {
        let bridge = Bridge::default();
        let waiting = held(&bridge, wait(0, false, false));
        assert!(bridge.take().open, "the window sees the page waiting");
        std::thread::sleep(Duration::from_millis(50));
        bridge.post(post(7));
        let (r, took) = waiting.join().unwrap();
        assert!(took < Duration::from_secs(5), "{took:?}");
        assert_eq!(json(&r)["seq"], 7);
        assert_eq!(bridge.status().counters.delivered, 1);
        assert_eq!(bridge.status().open, 0);
        assert!(!bridge.take().open);
    }

    #[test]
    fn a_held_wait_is_answered_when_deadtune_wakes_or_sleeps_the_page() {
        let bridge = Bridge::default();
        let waiting = held(&bridge, wait(0, false, false));
        bridge.set_awake(true);
        let (r, took) = waiting.join().unwrap();
        assert!(took < Duration::from_secs(5), "{took:?}");
        let m = json(&r);
        assert_eq!(m["awake"], true);
        assert!(m.get("seq").is_none(), "no message: {m}");

        let r = ask(&bridge, &wait(0, false, false), Instant::now(), NOW);
        assert_eq!(
            json(&r)["awake"],
            true,
            "a page that thinks it sleeps is told at once"
        );
        let r = ask(&bridge, &wait(0, false, true), Instant::now(), NOW);
        assert_eq!(r.status, 204, "a page that knows it is awake waits");

        let waiting = held(&bridge, wait(0, false, true));
        bridge.set_awake(false);
        assert_eq!(json(&waiting.join().unwrap().0)["awake"], false);
        bridge.set_applied(9);
        bridge.set_awake(false);
        assert_eq!(
            json(&ask(&bridge, &wait(0, false, true), Instant::now(), NOW))["applied"],
            9,
            "every answer says what the script applied"
        );
        let c = bridge.status().counters;
        assert_eq!((c.wakes, c.sleeps), (2, 2), "counted per answer: {c:?}");
        assert!(!bridge.status().awake);
    }

    #[test]
    fn a_message_and_sleep_go_out_together() {
        let bridge = Bridge::default();
        bridge.set_awake(true);
        let waiting = held(&bridge, wait(4, false, true));
        bridge.post(post(5));
        bridge.set_awake(false);
        let first = json(&waiting.join().unwrap().0);
        let (seq, awake) = (first["seq"].as_u64(), first["awake"] == true);
        let m = if seq == Some(5) && !awake {
            first.clone()
        } else {
            let since = seq.map_or(4, |s| s as u32);
            json(&ask(
                &bridge,
                &wait(since, false, awake),
                Instant::now(),
                NOW,
            ))
        };
        assert_eq!(m["awake"], false, "{m}");
        assert!(
            seq == Some(5) || m["seq"] == 5,
            "the message went out: {first} {m}"
        );
    }

    #[test]
    fn a_new_page_says_hello_at_once_and_waits_for_a_full_message() {
        let bridge = Bridge::default();
        bridge.post(post(5));
        let looks = Looks::default();
        let b = bridge.clone();
        let waiting = std::thread::spawn(move || {
            let w = Looks::default();
            let r = b.respond(
                &wait(0, true, false),
                PAGE,
                Instant::now(),
                Duration::from_secs(10),
                &w.wake(),
            );
            (r, w.take())
        });
        let t = Instant::now();
        while bridge.status().counters.hellos == 0 {
            assert!(t.elapsed() < Duration::from_secs(5));
            std::thread::sleep(Duration::from_millis(2));
        }
        let news = bridge.take();
        assert_eq!(news.hello.as_deref(), Some("1a2b3c4d"), "before the answer");
        assert!(news.open);
        assert_eq!(bridge.take().hello, None, "taken once");
        assert_eq!(bridge.status().counters.hellos, 1);
        assert_eq!(
            bridge.status().posted,
            None,
            "the old patch is not for this script"
        );
        bridge.post(post(6));
        let (r, woke) = waiting.join().unwrap();
        assert_eq!(woke, 1, "the window looked when the page arrived");
        assert_eq!(json(&r)["seq"], 6);
        assert_eq!(looks.take(), 0);
    }

    #[test]
    fn acks_reach_the_gui_once_and_the_check_keeps_the_recent_ones() {
        let bridge = Bridge::default();
        let now = Instant::now();
        let looks = Looks::default();
        for seq in 0..40 {
            let r = bridge.respond(&Request::Ack { seq }, PAGE, now, NOW, &looks.wake());
            assert_eq!(r.status, 204);
        }
        assert_eq!(looks.take(), 40);
        assert_eq!(bridge.take().acked, (0..40).collect::<Vec<_>>());
        assert!(bridge.take().acked.is_empty());
        let status = bridge.status();
        assert_eq!(status.counters.acks, 40);
        assert_eq!(status.recent_acks, (8..40).collect::<Vec<_>>());
    }

    #[test]
    fn the_page_and_its_cache_carry_this_protocol() {
        let version = PROTOCOL.strip_prefix("DTLIVE:v").unwrap();
        let docs = concat!(env!("CARGO_MANIFEST_DIR"), "/../../docs/bridge/");
        let page = std::fs::read_to_string(format!("{docs}index.html")).unwrap();
        assert!(page.contains(&format!("var VERSION = \"{version}\";")));
        let sw = std::fs::read_to_string(format!("{docs}sw.js")).unwrap();
        assert!(
            sw.contains(&format!("var CACHE = \"deadtune-bridge-v{version}\";")),
            "a new protocol needs a new cache name, so old copies are dropped"
        );
    }

    #[test]
    fn serves_real_requests_on_127_0_0_1() {
        use std::io::Read;
        let bridge = Bridge::default();
        let woke = Arc::new(Mutex::new(0));
        let w = woke.clone();
        let port = serve(bridge.clone(), 0, move || *w.lock().unwrap() += 1).unwrap();
        assert_eq!(bridge.status().listening, Some(Ok(port)));
        bridge.post(Post {
            seq: 9,
            titles: vec!["dt1 9 1/1 1a2b3c4d full ".into()],
            keep: vec!["dt1 9 1/1 1a2b3c4d full ".into()],
        });
        let get = |req: &str| {
            let mut s = TcpStream::connect(("127.0.0.1", port)).unwrap();
            s.write_all(req.as_bytes()).unwrap();
            let mut out = String::new();
            s.read_to_string(&mut out).unwrap();
            out
        };
        let idle = TcpStream::connect(("127.0.0.1", port)).unwrap();
        let out = get(
            "GET /wait?since=0&base=1a2b3c4d&awake=0 HTTP/1.1\r\nHost: 127.0.0.1\r\norigin: https://simulieren.github.io\r\n\r\n",
        );
        assert!(out.starts_with("HTTP/1.1 200 OK"), "{out}");
        assert!(
            out.ends_with("{\"awake\":false,\"chunks\":[\"dt1 9 1/1 1a2b3c4d full \"],\"keep\":[\"dt1 9 1/1 1a2b3c4d full \"],\"seq\":9}"),
            "{out}"
        );
        let out = get("GET /ack?seq=9 HTTP/1.1\r\nOrigin: https://simulieren.github.io\r\n\r\n");
        assert!(out.starts_with("HTTP/1.1 204"), "{out}");
        let out = get("GET /ack?seq=10 HTTP/1.1\r\n\r\n");
        assert!(out.starts_with("HTTP/1.1 403"), "no Origin: {out}");
        drop(idle);
        assert_eq!(bridge.take().acked, [9]);
        assert_eq!(*woke.lock().unwrap(), 2);
        let busy = serve(Bridge::default(), port, || {});
        assert!(busy.is_err(), "one server per port");
    }
}
