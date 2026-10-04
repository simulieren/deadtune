//! TCP console client for `-netconport <port>`.

use std::net::{SocketAddr, TcpStream};
use std::time::Duration;

use super::{Bridge, BridgeError, ConsoleCmd};

pub const DEFAULT_PORT: u16 = 2121;

pub struct NetconBridge {
    stream: TcpStream,
}

impl NetconBridge {
    pub fn connect(addr: SocketAddr, timeout: Duration) -> Result<NetconBridge, BridgeError> {
        let _ = (addr, timeout);
        todo!()
    }

    /// True if something accepts TCP on `127.0.0.1:port` right now.
    pub fn probe(port: u16) -> bool {
        let _ = port;
        todo!()
    }
}

impl Bridge for NetconBridge {
    fn name(&self) -> &'static str {
        "netcon"
    }

    fn push(&mut self, cmds: &[ConsoleCmd]) -> Result<(), BridgeError> {
        let _ = (cmds, &self.stream);
        todo!()
    }
}
