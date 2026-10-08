//! A one-page HTTP server on 127.0.0.1 that tells whether the game's web panel
//! (`CitadelHTMLPanel`) may load a page from this PC. The live HUD script asks for
//! `/probe?k=<kind>`; each request is recorded, and the page answers through its title,
//! which the script receives as an `HTMLTitle` event.

use std::io::{self, BufRead, BufReader, Write};
use std::net::TcpListener;
use std::sync::{Arc, Mutex};

pub const PORT: u16 = 47613;

/// The probe kind a request line asks for (`GET /probe?k=ip HTTP/1.1` -> `ip`).
pub fn kind(request_line: &str) -> Option<String> {
    let path = request_line.strip_prefix("GET ")?.split(' ').next()?;
    let query = path.strip_prefix("/probe?")?;
    query
        .split('&')
        .find_map(|kv| kv.strip_prefix("k="))
        .filter(|k| !k.is_empty() && k.chars().all(|c| c.is_ascii_alphanumeric()))
        .map(str::to_string)
}

pub fn response(kind: Option<&str>) -> Vec<u8> {
    let (status, body) = match kind {
        Some(k) => (
            "200 OK",
            format!(
                "<!doctype html><html><head><title>DTLIVE http {k}</title></head><body></body></html>"
            ),
        ),
        None => ("404 Not Found", String::new()),
    };
    format!(
        "HTTP/1.1 {status}\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\n\
         Access-Control-Allow-Origin: *\r\nAccess-Control-Allow-Private-Network: true\r\n\
         Cache-Control: no-store\r\nConnection: close\r\n\r\n{body}",
        body.len()
    )
    .into_bytes()
}

/// Starts the server on a background thread; requests are appended to `hits`.
pub fn serve(hits: Arc<Mutex<Vec<String>>>) -> io::Result<()> {
    let listener = TcpListener::bind(("127.0.0.1", PORT))?;
    std::thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            let mut line = String::new();
            if BufReader::new(&stream).read_line(&mut line).is_err() {
                continue;
            }
            let k = kind(line.trim_end());
            if let Some(k) = &k
                && let Ok(mut hits) = hits.lock()
            {
                hits.push(k.clone());
            }
            let _ = (&stream).write_all(&response(k.as_deref()));
        }
    });
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_probe_kind() {
        assert_eq!(kind("GET /probe?k=ip HTTP/1.1").as_deref(), Some("ip"));
        assert_eq!(
            kind("GET /probe?x=1&k=host HTTP/1.1").as_deref(),
            Some("host")
        );
        assert_eq!(kind("GET /favicon.ico HTTP/1.1"), None);
        assert_eq!(kind("GET /probe?k=a/b HTTP/1.1"), None);
        assert_eq!(kind("POST /probe?k=ip HTTP/1.1"), None);
    }

    #[test]
    fn the_page_answers_through_its_title() {
        let text = String::from_utf8(response(Some("ip"))).unwrap();
        assert!(text.starts_with("HTTP/1.1 200 OK\r\n"));
        assert!(text.contains("Access-Control-Allow-Private-Network: true"));
        assert!(text.ends_with("<title>DTLIVE http ip</title></head><body></body></html>"));
        assert!(
            String::from_utf8(response(None))
                .unwrap()
                .starts_with("HTTP/1.1 404")
        );
    }

    #[test]
    fn serves_a_real_request() {
        let hits = Arc::new(Mutex::new(Vec::new()));
        if serve(hits.clone()).is_err() {
            eprintln!("port {PORT} busy; skipping");
            return;
        }
        let mut s = std::net::TcpStream::connect(("127.0.0.1", PORT)).unwrap();
        s.write_all(b"GET /probe?k=ip HTTP/1.1\r\nHost: x\r\n\r\n")
            .unwrap();
        let mut body = String::new();
        io::Read::read_to_string(&mut s, &mut body).unwrap();
        assert!(body.contains("DTLIVE http ip"));
        assert_eq!(*hits.lock().unwrap(), ["ip"]);
    }
}
