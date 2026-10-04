//! TCP console client for `-netconport <port>`.

use std::io::{ErrorKind, Read, Write};
use std::net::{Ipv4Addr, SocketAddr, TcpStream};
use std::time::Duration;

use super::{Bridge, BridgeError, ConsoleCmd};

pub const DEFAULT_PORT: u16 = 2121;

const PROBE_TIMEOUT: Duration = Duration::from_millis(200);

pub struct NetconBridge {
    stream: TcpStream,
}

impl NetconBridge {
    pub fn connect(addr: SocketAddr, timeout: Duration) -> Result<NetconBridge, BridgeError> {
        let stream = TcpStream::connect_timeout(&addr, timeout)?;
        stream.set_nodelay(true)?;
        stream.set_write_timeout(Some(timeout))?;
        Ok(NetconBridge { stream })
    }

    /// True if something accepts TCP on `127.0.0.1:port` right now.
    pub fn probe(port: u16) -> bool {
        let addr = SocketAddr::from((Ipv4Addr::LOCALHOST, port));
        TcpStream::connect_timeout(&addr, PROBE_TIMEOUT).is_ok()
    }

    /// The console sends a banner and echoes log output back. Nobody reads it, but it
    /// must be consumed or the game's send buffer fills and netcon stalls.
    fn drain(&mut self) -> Result<(), BridgeError> {
        self.stream.set_nonblocking(true)?;
        let mut buf = [0u8; 4096];
        let result = loop {
            match self.stream.read(&mut buf) {
                Ok(0) => break Err(ErrorKind::ConnectionAborted.into()),
                Ok(_) => continue,
                Err(e) if e.kind() == ErrorKind::WouldBlock => break Ok(()),
                Err(e) if e.kind() == ErrorKind::Interrupted => continue,
                Err(e) => break Err(e),
            }
        };
        self.stream.set_nonblocking(false)?;
        Ok(result?)
    }
}

impl Bridge for NetconBridge {
    fn name(&self) -> &'static str {
        "netcon"
    }

    fn push(&mut self, cmds: &[ConsoleCmd]) -> Result<(), BridgeError> {
        let payload: String = super::lines(cmds)?.into_iter().map(|l| l + "\n").collect();
        self.drain()?;
        self.stream.write_all(payload.as_bytes())?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bridge::cmd;
    use std::net::TcpListener;
    use std::thread;

    const T: Duration = Duration::from_secs(2);

    /// Accepts one client, sends a banner like Source 2 does, returns everything received until EOF.
    fn recording_server() -> (SocketAddr, thread::JoinHandle<Vec<u8>>) {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
        let addr = listener.local_addr().unwrap();
        let handle = thread::spawn(move || {
            let (mut sock, _) = listener.accept().unwrap();
            // The client may already be gone (unsafe_batch_sends_nothing drops at once).
            let _ = sock.write_all(b"Deadlock netcon\r\n> ");
            sock.set_read_timeout(Some(T)).unwrap();
            let mut got = Vec::new();
            let mut buf = [0u8; 1024];
            // A client closing with the banner unread sends RST instead of FIN.
            while let Ok(n @ 1..) = sock.read(&mut buf) {
                got.extend_from_slice(&buf[..n]);
            }
            got
        });
        (addr, handle)
    }

    #[test]
    fn push_sends_each_line_newline_terminated_despite_banner() {
        let (addr, server) = recording_server();
        let mut bridge = NetconBridge::connect(addr, T).unwrap();
        assert!(bridge.stream.nodelay().unwrap(), "TCP_NODELAY is set");
        thread::sleep(Duration::from_millis(50));
        bridge
            .push(&[cmd("fps_max", "240"), cmd("r_name", "a b")])
            .unwrap();
        bridge.push(&[cmd("c", "3")]).unwrap();
        drop(bridge);
        let got = String::from_utf8(server.join().unwrap()).unwrap();
        assert_eq!(got, "fps_max \"240\"\nr_name \"a b\"\nc \"3\"\n");
    }

    #[test]
    fn unsafe_batch_sends_nothing() {
        let (addr, server) = recording_server();
        let mut bridge = NetconBridge::connect(addr, T).unwrap();
        assert!(bridge.push(&[cmd("a", "1"), cmd("b", "1;quit")]).is_err());
        drop(bridge);
        assert!(server.join().unwrap().is_empty());
    }

    #[test]
    fn push_after_server_closes_is_an_error() {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
        let addr = listener.local_addr().unwrap();
        let mut bridge = NetconBridge::connect(addr, T).unwrap();
        drop(listener.accept().unwrap());
        thread::sleep(Duration::from_millis(50));
        assert!(bridge.push(&[cmd("a", "1")]).is_err());
    }

    #[test]
    fn probe_sees_listener_and_closed_port() {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
        let port = listener.local_addr().unwrap().port();
        assert!(NetconBridge::probe(port));
        drop(listener);
        assert!(!NetconBridge::probe(port));
    }

    #[test]
    fn connect_to_closed_port_fails() {
        let port = {
            let l = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
            l.local_addr().unwrap().port()
        };
        let addr = SocketAddr::from((Ipv4Addr::LOCALHOST, port));
        assert!(matches!(
            NetconBridge::connect(addr, T),
            Err(BridgeError::Io(_))
        ));
    }
}
