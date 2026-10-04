//! Phone remote (`--features remote`): one embedded page on the LAN IP, guarded by a random
//! token in the URL. Slider changes come back over a channel and go through `set_convar`,
//! the same path as the desktop sliders (so they reach the game through the live bridge).

use std::collections::hash_map::RandomState;
use std::hash::{BuildHasher, Hasher};
use std::io::Read;
use std::net::{IpAddr, Ipv4Addr, SocketAddr, UdpSocket};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Arc, Mutex};

use dt_core::catalog::Kind;
use eframe::egui;

use crate::state::{AppState, Status};

const PAGE: &str = include_str!("remote.html");

pub struct Remote {
    pub url: String,
    rx: Receiver<(String, String)>,
    snapshot: Arc<Mutex<String>>,
    server: Arc<tiny_http::Server>,
}

impl Drop for Remote {
    fn drop(&mut self) {
        self.server.unblock();
    }
}

#[derive(Debug, PartialEq, Eq)]
enum Route {
    Page,
    State,
    Set { name: String, value: String },
    Forbidden,
    NotFound,
}

fn query_param(url: &str, key: &str) -> Option<String> {
    let query = url.split_once('?')?.1;
    query.split('&').find_map(|pair| {
        let (k, v) = pair.split_once('=')?;
        (k == key).then(|| percent_decode(v))
    })
}

fn percent_decode(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'+' => out.push(b' '),
            b'%' => match text
                .get(i + 1..i + 3)
                .and_then(|h| u8::from_str_radix(h, 16).ok())
            {
                Some(b) => {
                    out.push(b);
                    i += 2;
                }
                None => out.push(b'%'),
            },
            b => out.push(b),
        }
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn route(method: &tiny_http::Method, url: &str, token: &str) -> Route {
    if query_param(url, "t").as_deref() != Some(token) {
        return Route::Forbidden;
    }
    let path = url.split('?').next().unwrap_or("");
    match (method, path) {
        (tiny_http::Method::Get, "/") => Route::Page,
        (tiny_http::Method::Get, "/state") => Route::State,
        (tiny_http::Method::Post, "/set") => {
            match (query_param(url, "name"), query_param(url, "value")) {
                (Some(name), Some(value)) => Route::Set { name, value },
                _ => Route::NotFound,
            }
        }
        _ => Route::NotFound,
    }
}

fn token() -> String {
    let part = || {
        let mut h = RandomState::new().build_hasher();
        h.write_u128(
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |d| d.as_nanos()),
        );
        h.finish()
    };
    format!("{:016x}{:016x}", part(), part())
}

/// The address other devices reach us on; no packet is sent by `connect` on UDP.
fn lan_ip() -> IpAddr {
    UdpSocket::bind((Ipv4Addr::UNSPECIFIED, 0))
        .and_then(|s| {
            s.connect((Ipv4Addr::new(192, 168, 255, 255), 9))?;
            s.local_addr()
        })
        .map_or(IpAddr::V4(Ipv4Addr::LOCALHOST), |a| a.ip())
}

fn json_str(text: &str) -> String {
    let mut out = String::from("\"");
    for c in text.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

fn snapshot_json(state: &AppState) -> String {
    let rows: Vec<String> = state
        .settings
        .favourites
        .iter()
        .filter(|n| !state.catalog.is_denied(n))
        .map(|name| {
            let entry = state.catalog.get(name);
            let kind = match entry.map(|e| &e.kind) {
                Some(Kind::Bool) => "bool",
                Some(Kind::Int) => "int",
                Some(Kind::Float) => "float",
                _ => "text",
            };
            let range = entry.and_then(|e| e.range).map_or("null".into(), |[lo, hi]| format!("[{lo},{hi}]"));
            let step = entry.and_then(|e| e.step).map_or("null".into(), |s| s.to_string());
            format!(
                "{{\"name\":{},\"kind\":\"{kind}\",\"value\":{},\"range\":{range},\"step\":{step},\"live\":{}}}",
                json_str(name),
                json_str(&state.current_value(name).unwrap_or_default()),
                state.is_live_now(name)
            )
        })
        .collect();
    format!(
        "{{\"profile\":{},\"running\":{},\"rows\":[{}]}}",
        json_str(&state.profile.name),
        state.ctx.game_running,
        rows.join(",")
    )
}

fn serve(
    server: Arc<tiny_http::Server>,
    token: String,
    snapshot: Arc<Mutex<String>>,
    tx: Sender<(String, String)>,
    ctx: egui::Context,
) {
    for mut request in server.incoming_requests() {
        let header =
            |v: &str| tiny_http::Header::from_bytes("Content-Type", v).expect("static header");
        let response = match route(request.method(), request.url(), &token) {
            Route::Page => tiny_http::Response::from_string(PAGE)
                .with_header(header("text/html; charset=utf-8")),
            Route::State => {
                let body = snapshot.lock().map(|s| s.clone()).unwrap_or_default();
                tiny_http::Response::from_string(body).with_header(header("application/json"))
            }
            Route::Set { name, value } => {
                let mut sink = Vec::new();
                let _ = request.as_reader().take(1024).read_to_end(&mut sink);
                let _ = tx.send((name, value));
                ctx.request_repaint();
                tiny_http::Response::from_string("ok")
            }
            Route::Forbidden => tiny_http::Response::from_string("forbidden").with_status_code(403),
            Route::NotFound => tiny_http::Response::from_string("not found").with_status_code(404),
        };
        let _ = request.respond(response);
    }
}

pub fn start(ctx: egui::Context) -> Result<Remote, String> {
    let ip = lan_ip();
    let server =
        Arc::new(tiny_http::Server::http(SocketAddr::new(ip, 0)).map_err(|e| e.to_string())?);
    let port = server.server_addr().to_ip().map_or(0, |a| a.port());
    let token = token();
    let url = format!("http://{ip}:{port}/?t={token}");
    let snapshot = Arc::new(Mutex::new(String::from("{\"rows\":[]}")));
    let (tx, rx) = channel();
    let thread_server = Arc::clone(&server);
    let thread_snapshot = Arc::clone(&snapshot);
    std::thread::spawn(move || serve(thread_server, token, thread_snapshot, tx, ctx));
    Ok(Remote {
        url,
        rx,
        snapshot,
        server,
    })
}

/// Applies phone edits and refreshes what the phone sees.
pub fn sync(state: &mut AppState) {
    let Some(remote) = &state.remote else { return };
    let edits: Vec<(String, String)> = remote.rx.try_iter().collect();
    if let Ok(mut snap) = remote.snapshot.lock() {
        *snap = snapshot_json(state);
    }
    for (name, value) in edits {
        if !state.settings.favourites.contains(&name) {
            continue;
        }
        if let Err(e) = state.set_convar(&name, value) {
            state.status = Some(Status::Error(e.to_string()));
        }
    }
}

pub fn ui(ui: &mut egui::Ui, state: &mut AppState) {
    ui.heading("Phone remote");
    match &state.remote {
        Some(remote) => {
            ui.label("Open on a phone in the same network:");
            let url = remote.url.clone();
            ui.horizontal(|ui| {
                ui.monospace(&url);
                if ui.small_button("Copy").clicked() {
                    ui.ctx().copy_text(url.clone());
                }
            });
            if ui.button("Stop").clicked() {
                state.remote = None;
            }
        }
        None => {
            ui.weak(
                "Serves your favourites as sliders on the LAN; the URL carries a random token.",
            );
            if ui.button("Start").clicked() {
                match start(ui.ctx().clone()) {
                    Ok(remote) => state.remote = Some(remote),
                    Err(e) => state.status = Some(Status::Error(format!("remote: {e}"))),
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tiny_http::Method;

    #[test]
    fn routes_need_the_token() {
        assert_eq!(route(&Method::Get, "/?t=abc", "abc"), Route::Page);
        assert_eq!(route(&Method::Get, "/state?t=abc", "abc"), Route::State);
        assert_eq!(route(&Method::Get, "/?t=nope", "abc"), Route::Forbidden);
        assert_eq!(route(&Method::Get, "/", "abc"), Route::Forbidden);
        assert_eq!(
            route(&Method::Post, "/set?t=abc&name=fps_max&value=144", "abc"),
            Route::Set {
                name: "fps_max".into(),
                value: "144".into()
            }
        );
        assert_eq!(
            route(&Method::Post, "/set?t=abc&name=fps_max", "abc"),
            Route::NotFound
        );
    }

    #[test]
    fn decodes_query_values() {
        assert_eq!(
            query_param("/set?value=0.5%25+x", "value").as_deref(),
            Some("0.5% x")
        );
        assert_eq!(
            query_param("/set?value=%zz", "value").as_deref(),
            Some("%zz")
        );
    }

    #[test]
    fn tokens_are_long_and_distinct() {
        let (a, b) = (token(), token());
        assert_eq!(a.len(), 32);
        assert_ne!(a, b);
    }

    #[test]
    fn json_escapes_strings() {
        assert_eq!(json_str("a\"b\\c\n"), "\"a\\\"b\\\\c\\u000a\"");
    }

    #[test]
    fn snapshot_lists_favourites_without_denylisted() {
        let (_dir, mut state) = crate::state::testutil::state();
        state.toggle_favourite("fps_max");
        state.toggle_favourite("citadel_player_outline_enemies");
        let json = snapshot_json(&state);
        assert!(json.contains("\"name\":\"fps_max\""), "{json}");
        assert!(json.contains("\"kind\":\"int\""), "{json}");
        assert!(!json.contains("outline"), "{json}");
    }
}
