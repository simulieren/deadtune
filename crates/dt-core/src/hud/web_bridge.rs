//! DeadTune's end of the live HUD's web channel. Since the 2026-10-01 update the game's web
//! panel loads only HTTPS pages, so the live script opens DeadTune's static page on GitHub
//! Pages (`docs/bridge/`), and that page polls this server on 127.0.0.1 for the latest
//! message and hands it to the script through its title. Design:
//! docs/plans/live-hud/plan.md section 3.

use std::io::{self, BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

use super::live::Post;

pub const PORT: u16 = 47613;
pub const PAGE_ORIGIN: &str = "https://simulieren.github.io";
pub const PAGE_URL: &str = "https://simulieren.github.io/deadtune/bridge/";
/// Every title the page sets starts with this; the script ignores a page of another
/// version (an old copy in the game's browser cache) and loads it again.
pub const PROTOCOL: &str = "DTLIVE:v1";
/// The page polls every 150 ms while it reaches DeadTune; one that polled this recently is
/// connected.
pub const CONNECTED: Duration = Duration::from_secs(2);
const RECENT_ACKS: usize = 32;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Request {
    /// The page asks for a message other than `since`, the one it has. `hello` marks its
    /// first poll after the script opened it; `base` is the HUD the script was built for.
    Live {
        since: u32,
        base: Option<String>,
        hello: bool,
    },
    /// The script applied message `seq`.
    Ack {
        seq: u32,
    },
    /// A browser's CORS and private network preflight.
    Preflight,
    /// The script's plain `http://` load, the control that shows whether the game still
    /// refuses non-HTTPS pages.
    Control,
    Other,
}

/// The request a request line names (`GET /live?since=3&base=1a2b3c4d HTTP/1.1`).
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
        "/live" => Request::Live {
            since: param("since").and_then(|s| s.parse().ok()).unwrap_or(0),
            base: param("base")
                .filter(|b| {
                    !b.is_empty() && b.len() <= 16 && b.chars().all(|c| c.is_ascii_hexdigit())
                })
                .map(str::to_string),
            hello: param("hello") == Some("1"),
        },
        "/ack" => match param("seq").and_then(|s| s.parse().ok()) {
            Some(seq) => Request::Ack { seq },
            None => Request::Other,
        },
        "/control" => Request::Control,
        _ => Request::Other,
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Counters {
    pub polls: u64,
    /// Polls that opened a page: the script loaded and opened it.
    pub hellos: u64,
    /// Polls answered with a message.
    pub delivered: u64,
    pub acks: u64,
    pub preflights: u64,
    /// Plain `http://` loads by the game's web panel.
    pub controls: u64,
    /// Requests from anywhere but the bridge page, refused.
    pub refused: u64,
    pub other: u64,
}

/// What the page did since the last `Bridge::take`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PageNews {
    /// The base of a page that just opened, when one did.
    pub hello: Option<String>,
    /// The base the page last polled with.
    pub base: Option<String>,
    pub last_poll: Option<Instant>,
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
    pub recent_acks: Vec<u32>,
}

#[derive(Default)]
struct Shared {
    posted: Option<Post>,
    news: PageNews,
    status: BridgeStatus,
}

/// The state the GUI and the server thread share: the GUI posts messages and takes the
/// page's news, the server answers the page from it.
#[derive(Clone, Default)]
pub struct Bridge(Arc<Mutex<Shared>>);

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
    /// the page is on github.io and DeadTune on this PC.
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
             Access-Control-Allow-Headers: Content-Type\r\nAccess-Control-Max-Age: 600\r\n\
             Vary: Origin\r\nCache-Control: no-store\r\n{kind}Content-Length: {}\r\n\
             Connection: close\r\n\r\n{}",
            self.status,
            self.body.len(),
            self.body
        )
        .into_bytes()
    }
}

impl Bridge {
    fn lock(&self) -> MutexGuard<'_, Shared> {
        self.0.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// The message the page hands the script next.
    pub fn post(&self, post: Post) {
        let mut s = self.lock();
        s.status.posted = Some(post.seq);
        s.posted = Some(post);
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

    pub fn take(&self) -> PageNews {
        let mut s = self.lock();
        PageNews {
            hello: s.news.hello.take(),
            acked: std::mem::take(&mut s.news.acked),
            ..s.news.clone()
        }
    }

    pub fn status(&self) -> BridgeStatus {
        self.lock().status.clone()
    }

    fn listening(&self, result: Result<u16, String>) {
        self.lock().status.listening = Some(result);
    }

    /// The answer to `req` from a page at `origin`, and whether the GUI should look at the
    /// news now (a page opened, an ack, or a page that was away polls again). Only the
    /// bridge page may poll or ack; the control load is a plain navigation without one.
    pub fn respond(&self, req: &Request, origin: Option<&str>, now: Instant) -> (Response, bool) {
        let mut s = self.lock();
        let s = &mut *s;
        let ours = origin == Some(PAGE_ORIGIN);
        match req {
            Request::Live { .. } | Request::Ack { .. } | Request::Preflight if !ours => {
                s.status.counters.refused += 1;
                (Response::empty(403), false)
            }
            Request::Live { since, base, hello } => {
                let away = s
                    .news
                    .last_poll
                    .is_none_or(|t| now.saturating_duration_since(t) >= CONNECTED);
                s.status.counters.polls += 1;
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
                let response = match &s.posted {
                    Some(post) if post.seq != *since => {
                        s.status.counters.delivered += 1;
                        Response {
                            status: 200,
                            content_type: Some("application/json"),
                            body: serde_json::json!({
                                "seq": post.seq,
                                "chunks": post.titles,
                                "keep": post.keep,
                            })
                            .to_string(),
                        }
                    }
                    _ => Response::empty(204),
                };
                (response, *hello || away)
            }
            Request::Ack { seq } => {
                s.status.counters.acks += 1;
                s.news.acked.push(*seq);
                s.status.recent_acks.push(*seq);
                let over = s.status.recent_acks.len().saturating_sub(RECENT_ACKS);
                s.status.recent_acks.drain(..over);
                (Response::empty(204), true)
            }
            Request::Preflight => {
                s.status.counters.preflights += 1;
                (Response::empty(204), false)
            }
            Request::Control => {
                s.status.counters.controls += 1;
                (
                    Response {
                        status: 200,
                        content_type: Some("text/html; charset=utf-8"),
                        body: format!("<!doctype html><title>{PROTOCOL} 1 control ok</title>"),
                    },
                    false,
                )
            }
            Request::Other => {
                s.status.counters.other += 1;
                (Response::empty(404), false)
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
/// its own thread, so a browser's idle preconnect never holds up a poll. Returns the port.
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
                let (response, look) =
                    bridge.respond(&parse_request(&line), origin.as_deref(), Instant::now());
                if look {
                    wake();
                }
                let _ = (&stream).write_all(&response.bytes());
            });
        }
    });
    Ok(port)
}

#[cfg(test)]
mod tests {
    use super::*;

    const PAGE: Option<&str> = Some(PAGE_ORIGIN);

    #[test]
    fn reads_every_request_the_page_makes() {
        assert_eq!(
            parse_request("GET /live?since=41&base=1a2b3c4d HTTP/1.1"),
            Request::Live {
                since: 41,
                base: Some("1a2b3c4d".into()),
                hello: false
            }
        );
        assert_eq!(
            parse_request("GET /live?hello=1&since=0&base=1a2b3c4d HTTP/1.1"),
            Request::Live {
                since: 0,
                base: Some("1a2b3c4d".into()),
                hello: true
            }
        );
        assert_eq!(
            parse_request("GET /live HTTP/1.1"),
            Request::Live {
                since: 0,
                base: None,
                hello: false
            }
        );
        assert_eq!(
            parse_request("GET /live?base=<script> HTTP/1.1"),
            Request::Live {
                since: 0,
                base: None,
                hello: false
            },
            "only hex bases"
        );
        assert_eq!(
            parse_request("GET /ack?seq=7 HTTP/1.1"),
            Request::Ack { seq: 7 }
        );
        assert_eq!(parse_request("GET /ack?seq=x HTTP/1.1"), Request::Other);
        assert_eq!(
            parse_request("OPTIONS /live?since=0 HTTP/1.1"),
            Request::Preflight
        );
        assert_eq!(parse_request("GET /control?r=1 HTTP/1.1"), Request::Control);
        assert_eq!(parse_request("POST /live HTTP/1.1"), Request::Other);
        assert_eq!(parse_request("GET /favicon.ico HTTP/1.1"), Request::Other);
        assert_eq!(parse_request(""), Request::Other);
    }

    fn text(r: &Response) -> String {
        String::from_utf8(r.bytes()).unwrap()
    }

    #[test]
    fn every_answer_lets_the_github_page_read_it() {
        let bridge = Bridge::default();
        let now = Instant::now();
        let (preflight, look) = bridge.respond(&Request::Preflight, PAGE, now);
        assert!(!look);
        let wire = text(&preflight);
        assert!(wire.starts_with("HTTP/1.1 204 No Content\r\n"), "{wire}");
        for header in [
            "Access-Control-Allow-Origin: https://simulieren.github.io\r\n",
            "Access-Control-Allow-Private-Network: true\r\n",
            "Access-Control-Allow-Methods: GET, OPTIONS\r\n",
            "Access-Control-Allow-Headers: Content-Type\r\n",
        ] {
            assert!(wire.contains(header), "{header} in {wire}");
        }
        let (missing, _) = bridge.respond(&Request::Other, PAGE, now);
        assert!(text(&missing).starts_with("HTTP/1.1 404"));
        assert!(text(&missing).contains("Access-Control-Allow-Private-Network: true"));
        assert_eq!(bridge.status().counters.preflights, 1);
        assert_eq!(bridge.status().counters.other, 1);
    }

    #[test]
    fn only_the_bridge_page_may_poll_or_ack() {
        let bridge = Bridge::default();
        let now = Instant::now();
        bridge.post(post(5));
        for origin in [None, Some("https://evil.example"), Some("null")] {
            for req in [live(0, false), Request::Ack { seq: 5 }, Request::Preflight] {
                let (r, look) = bridge.respond(&req, origin, now);
                assert_eq!((r.status, look), (403, false), "{req:?} from {origin:?}");
            }
        }
        assert_eq!(bridge.status().counters.refused, 9);
        assert_eq!(bridge.status().counters.polls, 0);
        assert!(bridge.take().acked.is_empty());
        let (control, _) = bridge.respond(&Request::Control, None, now);
        assert_eq!(control.status, 200);
        assert!(
            control
                .body
                .contains("<title>DTLIVE:v1 1 control ok</title>")
        );
        assert_eq!(bridge.status().counters.controls, 1);
    }

    fn live(since: u32, hello: bool) -> Request {
        Request::Live {
            since,
            base: Some("1a2b3c4d".into()),
            hello,
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

    #[test]
    fn a_poll_gets_the_latest_message_once() {
        let bridge = Bridge::default();
        let t0 = Instant::now();
        let (r, look) = bridge.respond(&live(0, false), PAGE, t0);
        assert_eq!(r.status, 204);
        assert!(look, "a page that was away is back");
        bridge.post(post(5));
        let (r, look) = bridge.respond(&live(0, false), PAGE, t0 + Duration::from_millis(150));
        assert!(!look);
        assert_eq!(r.status, 200);
        let json: serde_json::Value = serde_json::from_str(&r.body).unwrap();
        assert_eq!(json["seq"], 5);
        assert_eq!(json["chunks"][0], "dt1 5 1/2 1a2b3c4d patch a\"b");
        assert_eq!(json["chunks"].as_array().unwrap().len(), 2);
        assert_eq!(json["keep"][0], "dt1 5 1/1 1a2b3c4d full x");
        assert!(text(&r).contains("Content-Type: application/json\r\n"));
        let (r, _) = bridge.respond(&live(5, false), PAGE, t0 + Duration::from_millis(300));
        assert_eq!(r.status, 204, "nothing other than what the page has");
        let status = bridge.status();
        assert_eq!((status.counters.polls, status.counters.delivered), (3, 1));
        assert_eq!(status.first_poll, Some(t0));
        assert_eq!(status.base.as_deref(), Some("1a2b3c4d"));
        assert_eq!(status.posted, Some(5));
        bridge.clear();
        assert_eq!(bridge.respond(&live(0, false), PAGE, t0).0.status, 204);
    }

    #[test]
    fn a_new_page_says_hello_and_waits_for_a_full_message() {
        let bridge = Bridge::default();
        let t0 = Instant::now();
        bridge.post(post(5));
        let (r, look) = bridge.respond(&live(0, true), PAGE, t0);
        assert_eq!(r.status, 204, "the old patch is not for this script");
        assert!(look);
        let news = bridge.take();
        assert_eq!(news.hello.as_deref(), Some("1a2b3c4d"));
        assert_eq!(news.last_poll, Some(t0));
        assert_eq!(bridge.take().hello, None, "taken once");
        assert_eq!(bridge.take().last_poll, Some(t0), "the last poll stays");
        assert_eq!(bridge.status().counters.hellos, 1);
        assert_eq!(bridge.status().posted, None);
    }

    #[test]
    fn acks_reach_the_gui_once_and_the_check_keeps_the_recent_ones() {
        let bridge = Bridge::default();
        let now = Instant::now();
        for seq in 0..40 {
            let (r, look) = bridge.respond(&Request::Ack { seq }, PAGE, now);
            assert_eq!(r.status, 204);
            assert!(look);
        }
        assert_eq!(bridge.take().acked, (0..40).collect::<Vec<_>>());
        assert!(bridge.take().acked.is_empty());
        let status = bridge.status();
        assert_eq!(status.counters.acks, 40);
        assert_eq!(status.recent_acks, (8..40).collect::<Vec<_>>());
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
            "GET /live?since=0&base=1a2b3c4d HTTP/1.1\r\nHost: 127.0.0.1\r\norigin: https://simulieren.github.io\r\n\r\n",
        );
        assert!(out.starts_with("HTTP/1.1 200 OK"), "{out}");
        assert!(
            out.ends_with("{\"chunks\":[\"dt1 9 1/1 1a2b3c4d full \"],\"keep\":[\"dt1 9 1/1 1a2b3c4d full \"],\"seq\":9}"),
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
