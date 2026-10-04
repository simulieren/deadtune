//! Live console path: which bridge, debounced pushes while dragging, and sending a batch.

use std::collections::BTreeMap;
use std::net::{Ipv4Addr, SocketAddr};
use std::path::Path;
use std::time::{Duration, Instant};

use dt_core::bridge::execfile::ExecFileBridge;
use dt_core::bridge::netcon::NetconBridge;
use dt_core::bridge::{Bridge, BridgeError, ConsoleCmd, clipboard};

pub const DEBOUNCE: Duration = Duration::from_millis(150);
const NETCON_TIMEOUT: Duration = Duration::from_millis(500);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BridgeKind {
    #[default]
    ExecFile,
    Netcon,
    Clipboard,
}

impl BridgeKind {
    pub const ALL: [BridgeKind; 3] = [
        BridgeKind::ExecFile,
        BridgeKind::Netcon,
        BridgeKind::Clipboard,
    ];

    pub fn label(self) -> &'static str {
        match self {
            BridgeKind::ExecFile => "Exec file (bind key)",
            BridgeKind::Netcon => "Netcon (TCP)",
            BridgeKind::Clipboard => "Clipboard",
        }
    }

    /// Clipboard pushes only on an explicit click; overwriting the clipboard while dragging is hostile.
    pub fn pushes_while_dragging(self) -> bool {
        !matches!(self, BridgeKind::Clipboard)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PushOutcome {
    Sent {
        bridge: &'static str,
        count: usize,
    },
    /// The caller copies this to the system clipboard (egui owns clipboard access).
    Copy(String),
}

/// Where the live bridges send to.
#[derive(Clone, Copy, Debug)]
pub struct BridgeTarget<'a> {
    pub kind: BridgeKind,
    pub cfg_dir: &'a Path,
    pub netcon_port: u16,
}

impl BridgeTarget<'_> {
    /// A connected bridge for `apply::execute`, or `None` for the clipboard.
    pub fn open(&self) -> Result<Option<Box<dyn Bridge>>, BridgeError> {
        Ok(match self.kind {
            BridgeKind::ExecFile => Some(Box::new(ExecFileBridge {
                cfg_dir: self.cfg_dir.to_path_buf(),
            })),
            BridgeKind::Netcon => Some(Box::new(NetconBridge::connect(
                SocketAddr::from((Ipv4Addr::LOCALHOST, self.netcon_port)),
                NETCON_TIMEOUT,
            )?)),
            BridgeKind::Clipboard => None,
        })
    }

    pub fn push(&self, cmds: &[ConsoleCmd]) -> Result<PushOutcome, BridgeError> {
        match self.open()? {
            Some(mut bridge) => {
                bridge.push(cmds)?;
                Ok(PushOutcome::Sent {
                    bridge: bridge.name(),
                    count: cmds.len(),
                })
            }
            None => Ok(PushOutcome::Copy(clipboard::batch_string(cmds)?)),
        }
    }
}

/// Collects slider values and releases them once the user pauses for [`DEBOUNCE`].
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LivePush {
    pending: BTreeMap<String, String>,
    last_change: Option<Instant>,
}

impl LivePush {
    pub fn schedule(&mut self, name: &str, value: &str, now: Instant) {
        self.pending.insert(name.to_string(), value.to_string());
        self.last_change = Some(now);
    }

    pub fn is_pending(&self) -> bool {
        !self.pending.is_empty()
    }

    /// The batch to send if the debounce window has passed; clears it.
    pub fn take_due(&mut self, now: Instant) -> Option<Vec<ConsoleCmd>> {
        let last = self.last_change?;
        if now.duration_since(last) < DEBOUNCE {
            return None;
        }
        self.last_change = None;
        let cmds = std::mem::take(&mut self.pending)
            .into_iter()
            .map(|(name, value)| ConsoleCmd { name, value })
            .collect::<Vec<_>>();
        (!cmds.is_empty()).then_some(cmds)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn debounce_waits_for_a_pause_and_keeps_the_last_value() {
        let t0 = Instant::now();
        let mut push = LivePush::default();
        push.schedule("r_farz", "6000", t0);
        push.schedule("r_farz", "6500", t0 + Duration::from_millis(100));
        assert_eq!(
            push.take_due(t0 + Duration::from_millis(200)),
            None,
            "still dragging"
        );
        let due = push
            .take_due(t0 + Duration::from_millis(260))
            .expect("pause passed");
        assert_eq!(
            due,
            vec![ConsoleCmd {
                name: "r_farz".into(),
                value: "6500".into()
            }]
        );
        assert_eq!(
            push.take_due(t0 + Duration::from_secs(5)),
            None,
            "sent once"
        );
        assert!(!push.is_pending());
    }

    #[test]
    fn exec_file_push_writes_the_cfg() {
        let dir = tempfile::tempdir().unwrap();
        let target = BridgeTarget {
            kind: BridgeKind::ExecFile,
            cfg_dir: dir.path(),
            netcon_port: 0,
        };
        let cmds = [ConsoleCmd {
            name: "fps_max".into(),
            value: "120".into(),
        }];
        assert!(matches!(
            target.push(&cmds).unwrap(),
            PushOutcome::Sent { count: 1, .. }
        ));
        let cfg = std::fs::read_to_string(dir.path().join("deadtune_live.cfg")).unwrap();
        assert!(cfg.contains(r#"fps_max "120""#), "{cfg}");
    }

    #[test]
    fn clipboard_push_returns_the_batch() {
        let target = BridgeTarget {
            kind: BridgeKind::Clipboard,
            cfg_dir: Path::new("."),
            netcon_port: 0,
        };
        let cmds = [
            ConsoleCmd {
                name: "a".into(),
                value: "1".into(),
            },
            ConsoleCmd {
                name: "b".into(),
                value: "2".into(),
            },
        ];
        assert_eq!(
            target.push(&cmds).unwrap(),
            PushOutcome::Copy(r#"a "1"; b "2""#.into())
        );
    }
}
