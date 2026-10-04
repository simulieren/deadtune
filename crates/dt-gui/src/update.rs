//! Self-update: one state machine, driven by messages from a worker thread. Checking,
//! downloading, verifying and installing all run off the UI thread; the window only
//! starts jobs, applies [`UpdateState::step`] to what they report, and restarts.

use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::sync::mpsc::{Receiver, Sender, TryRecvError, channel};

use chrono::{DateTime, TimeDelta, Utc};
use dt_core::update::{Asset, Channel, Current, Offer, Version};

/// Off without the `fetch` feature: nothing then touches the network.
pub const AVAILABLE: bool = cfg!(feature = "fetch");
pub const UNAVAILABLE: &str = "Updates aren't available in this build.";
const CHECK_EVERY: TimeDelta = TimeDelta::hours(24);

/// `[update]` in settings.toml.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct UpdateSettings {
    pub enabled: bool,
    pub channel: Channel,
    /// "Skip this version": automatic checks stay quiet about exactly this one.
    pub skipped: Option<Version>,
    pub last_check: Option<DateTime<Utc>>,
}

impl Default for UpdateSettings {
    fn default() -> UpdateSettings {
        UpdateSettings {
            enabled: true,
            channel: Channel::Stable,
            skipped: None,
            last_check: None,
        }
    }
}

/// The startup check: on, a release build (dev builds have no commit), and not checked
/// in the last day. A `last_check` in the future (clock moved back) counts as stale.
pub fn should_check(
    settings: &UpdateSettings,
    now: DateTime<Utc>,
    build: Option<&Current>,
) -> bool {
    settings.enabled
        && build.is_some_and(|b| b.commit.is_some())
        && settings
            .last_check
            .is_none_or(|t| t > now || now - t >= CHECK_EVERY)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    Check,
    Download,
    #[cfg_attr(not(feature = "fetch"), allow(dead_code))]
    Verify,
    Install,
    Restart,
}

impl Phase {
    pub fn verb(self) -> &'static str {
        match self {
            Phase::Check => "checking for updates",
            Phase::Download => "downloading",
            Phase::Verify => "verifying the download",
            Phase::Install => "installing",
            Phase::Restart => "restarting",
        }
    }
}

/// An offered release: everything needed to download, verify and install it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Release {
    pub version: Version,
    pub commit: String,
    pub notes_url: String,
    pub asset: Asset,
}

#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub enum UpdateState {
    #[default]
    Idle,
    Checking,
    UpToDate {
        checked_at: DateTime<Utc>,
    },
    Available(Release),
    /// Covers verify and install too: they follow the last byte without a message.
    Downloading {
        release: Release,
        done: u64,
        total: u64,
    },
    /// Installed over the running executable; the restart is pending.
    Ready {
        version: Version,
    },
    Failed {
        message: String,
        during: Phase,
        /// Kept so Retry can start the install again.
        release: Option<Release>,
    },
}

/// What the worker reports.
#[derive(Debug)]
pub enum Msg {
    Checked {
        at: DateTime<Utc>,
        result: Result<Offer, String>,
    },
    #[cfg_attr(not(feature = "fetch"), allow(dead_code))]
    Progress {
        done: u64,
        total: u64,
    },
    Failed {
        during: Phase,
        message: String,
    },
    Installed,
}

/// Side effects a transition asks the window for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    /// Persist `last_check`.
    Checked(DateTime<Utc>),
    /// Start the new executable and close this window.
    Restart,
}

impl UpdateState {
    pub fn busy(&self) -> bool {
        matches!(
            self,
            UpdateState::Checking | UpdateState::Downloading { .. }
        )
    }

    /// Messages that don't belong to the current state are stale and change nothing.
    pub fn step(self, msg: Msg) -> (UpdateState, Option<Action>) {
        match (self, msg) {
            (UpdateState::Checking, Msg::Checked { at, result }) => match result {
                Ok(Offer::Available {
                    version,
                    commit,
                    notes_url,
                    asset,
                }) => (
                    UpdateState::Available(Release {
                        version,
                        commit,
                        notes_url,
                        asset,
                    }),
                    Some(Action::Checked(at)),
                ),
                Ok(Offer::UpToDate | Offer::Skipped) => (
                    UpdateState::UpToDate { checked_at: at },
                    Some(Action::Checked(at)),
                ),
                Err(message) => (
                    UpdateState::Failed {
                        message,
                        during: Phase::Check,
                        release: None,
                    },
                    None,
                ),
            },
            (UpdateState::Downloading { release, .. }, Msg::Progress { done, total }) => (
                UpdateState::Downloading {
                    release,
                    done,
                    total,
                },
                None,
            ),
            (UpdateState::Downloading { release, .. }, Msg::Failed { during, message }) => (
                UpdateState::Failed {
                    message,
                    during,
                    release: Some(release),
                },
                None,
            ),
            (UpdateState::Downloading { release, .. }, Msg::Installed) => (
                UpdateState::Ready {
                    version: release.version,
                },
                Some(Action::Restart),
            ),
            (state, _) => (state, None),
        }
    }
}

#[derive(Default)]
pub struct Updater {
    pub state: UpdateState,
    /// "Later": the banner stays hidden until the next offer.
    pub later: bool,
    rx: Option<Receiver<Msg>>,
}

impl Updater {
    pub fn check(&mut self, channel: Channel, skipped: Option<Version>) {
        if self.state.busy() {
            return;
        }
        self.state = UpdateState::Checking;
        self.later = false;
        self.rx = Some(spawn(move |tx| {
            let _ = tx.send(check_job(channel, skipped));
        }));
    }

    pub fn install(&mut self, release: Release) {
        if self.state.busy() {
            return;
        }
        let exe = match exe() {
            Ok(exe) => exe.to_owned(),
            Err(e) => {
                self.state = UpdateState::Failed {
                    message: e,
                    during: Phase::Install,
                    release: Some(release),
                };
                return;
            }
        };
        self.state = UpdateState::Downloading {
            done: 0,
            total: release.asset.size,
            release: release.clone(),
        };
        self.rx = Some(spawn(move |tx| {
            let msg = match install_job(&release, &exe, tx) {
                Ok(()) => Msg::Installed,
                Err((during, message)) => Msg::Failed { during, message },
            };
            let _ = tx.send(msg);
        }));
    }

    /// Applies everything the worker sent since the last frame.
    pub fn poll(&mut self) -> Vec<Action> {
        let Some(rx) = &self.rx else {
            return Vec::new();
        };
        let mut msgs = Vec::new();
        let gone = loop {
            match rx.try_recv() {
                Ok(msg) => msgs.push(msg),
                Err(TryRecvError::Empty) => break false,
                Err(TryRecvError::Disconnected) => break true,
            }
        };
        let mut actions = Vec::new();
        for msg in msgs {
            let (next, action) = std::mem::take(&mut self.state).step(msg);
            self.state = next;
            actions.extend(action);
        }
        if gone {
            self.rx = None;
            let stopped = || "the update worker stopped unexpectedly".to_string();
            self.state = match std::mem::take(&mut self.state) {
                UpdateState::Checking => UpdateState::Failed {
                    message: stopped(),
                    during: Phase::Check,
                    release: None,
                },
                UpdateState::Downloading { release, .. } => UpdateState::Failed {
                    message: stopped(),
                    during: Phase::Download,
                    release: Some(release),
                },
                other => other,
            };
        }
        actions
    }
}

fn spawn(job: impl FnOnce(&Sender<Msg>) + Send + 'static) -> Receiver<Msg> {
    let (tx, rx) = channel();
    std::thread::spawn(move || job(&tx));
    rx
}

/// The running executable's path, read once at startup: after an install renames it
/// aside, `current_exe` on Linux reports the renamed `.old` file.
pub fn exe() -> Result<&'static Path, String> {
    static EXE: OnceLock<Result<PathBuf, String>> = OnceLock::new();
    EXE.get_or_init(|| std::env::current_exe().map_err(|e| e.to_string()))
        .as_ref()
        .map(PathBuf::as_path)
        .map_err(Clone::clone)
}

/// Starts the (now replaced) executable with this run's arguments.
pub fn spawn_new_exe() -> Result<(), String> {
    std::process::Command::new(exe()?)
        .args(std::env::args_os().skip(1))
        .spawn()
        .map(drop)
        .map_err(|e| e.to_string())
}

/// `0.2.0 (abc1234)`, or `0.2.0 (dev build)` without an embedded commit.
pub fn this_version() -> String {
    let commit = option_env!("DEADTUNE_COMMIT").map_or("dev build", |c| &c[..c.len().min(7)]);
    format!("{} ({commit})", env!("CARGO_PKG_VERSION"))
}

#[cfg(feature = "fetch")]
fn check_job(channel: Channel, skipped: Option<Version>) -> Msg {
    let at = Utc::now();
    let result = Current::this_build()
        .ok_or_else(|| "no updates are published for this platform".to_string())
        .and_then(|current| {
            let manifest = dt_core::update::fetch_manifest(channel).map_err(|e| e.to_string())?;
            Ok(dt_core::update::decide(&current, &manifest, skipped))
        });
    Msg::Checked { at, result }
}

#[cfg(not(feature = "fetch"))]
fn check_job(_: Channel, _: Option<Version>) -> Msg {
    Msg::Checked {
        at: Utc::now(),
        result: Err(UNAVAILABLE.into()),
    }
}

/// Nothing is written unless `verify` passed; a failed install restores the old file.
#[cfg(feature = "fetch")]
fn install_job(release: &Release, exe: &Path, tx: &Sender<Msg>) -> Result<(), (Phase, String)> {
    use dt_core::update::{self, Target};
    let target = Target::CURRENT.ok_or((
        Phase::Install,
        "no updates are published for this platform".to_string(),
    ))?;
    let bytes = update::download(&release.asset, |done, total| {
        let _ = tx.send(Msg::Progress { done, total });
    })
    .map_err(|e| (Phase::Download, e.to_string()))?;
    update::verify(
        &bytes,
        &release.asset,
        release.version,
        &release.commit,
        target,
        update::RELEASE_PUBKEY,
    )
    .map_err(|e| (Phase::Verify, e.to_string()))?;
    update::install(exe, &bytes).map_err(|e| (Phase::Install, e.to_string()))?;
    Ok(())
}

#[cfg(not(feature = "fetch"))]
fn install_job(_: &Release, _: &Path, _: &Sender<Msg>) -> Result<(), (Phase, String)> {
    Err((Phase::Download, UNAVAILABLE.into()))
}

/// Screenshot lever: `DEADTUNE_FAKE_UPDATE=0.9.0` offers that version without a check.
pub fn fake_release(version: Version) -> Release {
    Release {
        version,
        commit: "0000000".into(),
        notes_url: "https://github.com/simulieren/deadtune/releases".into(),
        asset: Asset {
            url: String::new(),
            size: 0,
            sha256: String::new(),
            minisig: String::new(),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use dt_core::update::Target;

    fn release(version: Version) -> Release {
        fake_release(version)
    }

    fn at(hours: i64) -> DateTime<Utc> {
        DateTime::from_timestamp(1_800_000_000, 0).unwrap() + TimeDelta::hours(hours)
    }

    fn offer(r: &Release) -> Offer {
        Offer::Available {
            version: r.version,
            commit: r.commit.clone(),
            notes_url: r.notes_url.clone(),
            asset: r.asset.clone(),
        }
    }

    #[test]
    fn a_check_lands_on_available_up_to_date_or_failed() {
        let r = release(Version(0, 9, 0));
        let checked = |result| UpdateState::Checking.step(Msg::Checked { at: at(0), result });
        assert_eq!(
            checked(Ok(offer(&r))),
            (UpdateState::Available(r), Some(Action::Checked(at(0))))
        );
        for quiet in [Offer::UpToDate, Offer::Skipped] {
            assert_eq!(
                checked(Ok(quiet)),
                (
                    UpdateState::UpToDate { checked_at: at(0) },
                    Some(Action::Checked(at(0)))
                )
            );
        }
        assert_eq!(
            checked(Err("offline".into())),
            (
                UpdateState::Failed {
                    message: "offline".into(),
                    during: Phase::Check,
                    release: None
                },
                None
            ),
            "a failed check does not count as checked"
        );
    }

    #[test]
    fn download_reports_progress_then_restarts_or_fails_keeping_the_release() {
        let r = release(Version(0, 9, 0));
        let downloading = UpdateState::Downloading {
            release: r.clone(),
            done: 0,
            total: 100,
        };
        let (state, action) = downloading.clone().step(Msg::Progress {
            done: 40,
            total: 100,
        });
        assert_eq!(action, None);
        assert_eq!(
            state,
            UpdateState::Downloading {
                release: r.clone(),
                done: 40,
                total: 100
            }
        );
        assert_eq!(
            state.clone().step(Msg::Installed),
            (
                UpdateState::Ready {
                    version: Version(0, 9, 0)
                },
                Some(Action::Restart)
            )
        );
        assert_eq!(
            state.step(Msg::Failed {
                during: Phase::Verify,
                message: "download checksum mismatch".into()
            }),
            (
                UpdateState::Failed {
                    message: "download checksum mismatch".into(),
                    during: Phase::Verify,
                    release: Some(r)
                },
                None
            )
        );
    }

    #[test]
    fn stale_messages_change_nothing() {
        let r = release(Version(0, 9, 0));
        for state in [
            UpdateState::Idle,
            UpdateState::Available(r.clone()),
            UpdateState::Ready { version: r.version },
        ] {
            for msg in [
                Msg::Progress { done: 1, total: 2 },
                Msg::Installed,
                Msg::Checked {
                    at: at(0),
                    result: Ok(Offer::UpToDate),
                },
            ] {
                assert_eq!(state.clone().step(msg), (state.clone(), None));
            }
        }
    }

    #[test]
    fn a_vanished_worker_fails_the_job() {
        let mut updater = Updater {
            state: UpdateState::Checking,
            later: false,
            rx: Some(spawn(|_| {})),
        };
        while updater.rx.is_some() {
            updater.poll();
        }
        assert!(
            matches!(
                updater.state,
                UpdateState::Failed {
                    during: Phase::Check,
                    ..
                }
            ),
            "{:?}",
            updater.state
        );
    }

    #[test]
    fn startup_check_needs_enabled_a_release_build_and_a_day_since_the_last() {
        let release_build = Current {
            version: Version(0, 2, 0),
            commit: Some("abc".into()),
            target: Target::WindowsX64,
        };
        let dev_build = Current {
            commit: None,
            ..release_build.clone()
        };
        let mut s = UpdateSettings::default();
        let now = at(100);
        assert!(should_check(&s, now, Some(&release_build)), "never checked");
        assert!(!should_check(&s, now, Some(&dev_build)), "dev build");
        assert!(!should_check(&s, now, None), "unsupported target");
        s.last_check = Some(at(90));
        assert!(!should_check(&s, now, Some(&release_build)), "10h ago");
        s.last_check = Some(at(76));
        assert!(should_check(&s, now, Some(&release_build)), "24h ago");
        s.last_check = Some(at(130));
        assert!(
            should_check(&s, now, Some(&release_build)),
            "clock went back"
        );
        s.last_check = None;
        s.enabled = false;
        assert!(!should_check(&s, now, Some(&release_build)), "turned off");
    }
}
