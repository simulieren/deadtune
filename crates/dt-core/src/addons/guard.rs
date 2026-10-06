//! Launch guard. The paks the game last started with are the known-good set; any pak
//! newer than that is on trial from the moment the game process appears. The trial
//! passes when the game stays up long enough or prints DeadTune's boot marker, and fails
//! when the game dies early or prints a fatal error. A failure names exactly the paks
//! that changed since the known-good set, so the caller rolls back those and nothing else.
//! Every pak DeadTune owns is guarded the same way: the performance addons and the HUD.
//! Persisted in `state_dir/guard.toml`, so a DeadTune restart mid-trial resumes it.

use std::collections::BTreeMap;
use std::path::Path;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use super::install::{self, read_record};
use super::{AddonError, AddonId};
use crate::backup::{atomic_write, sha256_hex};
use crate::bridge::ack::BOOT;
use crate::hud::HudFeature;
use crate::hud::install::{
    self as hud_install, ADDON_FILE as HUD_FILE, HudAction, HudError, HudPlan, HudRollback,
    addons_dir,
};
use crate::locate::GamePaths;

pub const RECORD_FILE: &str = "guard.toml";
pub const FATAL_MARKER: &str = "FATAL ERROR";
/// A game gone this soon after it appeared did not start.
pub const FAIL_WINDOW: Duration = Duration::from_secs(60);
/// A game still up this long after it appeared has started.
pub const PASS_AFTER: Duration = Duration::from_secs(90);

/// A pak DeadTune owns. Stored as the addon's key or `hud`, so records written before the
/// HUD was guarded still load.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Pak {
    Addon(AddonId),
    Hud,
}

impl Pak {
    pub fn key(self) -> &'static str {
        match self {
            Pak::Addon(id) => id.key(),
            Pak::Hud => "hud",
        }
    }

    pub fn parse(text: &str) -> Option<Pak> {
        match text {
            "hud" => Some(Pak::Hud),
            _ => AddonId::parse(text).map(Pak::Addon),
        }
    }

    /// The player-facing name.
    pub fn name(self) -> &'static str {
        match self {
            Pak::Addon(id) => super::info(id).name,
            Pak::Hud => "DeadTune's HUD changes",
        }
    }
}

impl From<AddonId> for Pak {
    fn from(id: AddonId) -> Pak {
        Pak::Addon(id)
    }
}

impl serde::Serialize for Pak {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(self.key())
    }
}

impl<'de> serde::Deserialize<'de> for Pak {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Pak, D::Error> {
        let text = String::deserialize(d)?;
        Pak::parse(&text).ok_or_else(|| serde::de::Error::custom(format!("unknown pak {text:?}")))
    }
}

/// Pak to the sha256 of its file.
pub type PakSet = BTreeMap<Pak, String>;

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Trial {
    /// Paks the game loaded that the last good set did not have (new or rebuilt).
    pub changed: PakSet,
    /// Everything the game loaded; becomes the last good set when the trial passes.
    pub loaded: PakSet,
    /// Unix seconds; when the game process appeared.
    pub launched_at: u64,
    /// The game build it ran on.
    #[serde(default)]
    pub build: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Verdict {
    Verified {
        at: u64,
    },
    Failed {
        at: u64,
        fatal: Option<String>,
        /// The pak that failed, so the same build is not installed again unasked.
        #[serde(default)]
        sha256: Option<String>,
    },
}

/// What a failed launch did to the HUD.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct HudFailure {
    pub rollback: HudRollback,
    /// The parts of the HUD the failed pak changed.
    pub features: Vec<HudFeature>,
}

/// A failed trial, shown until dismissed.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Failure {
    pub ids: Vec<Pak>,
    pub fatal: Option<String>,
    pub at: u64,
    /// Files DeadTune deleted, for the details view.
    #[serde(default)]
    pub removed: Vec<String>,
    /// Paks that could not be rolled back, with the reason.
    #[serde(default)]
    pub kept: Vec<String>,
    /// Set once the HUD, when it was a suspect, has been rolled back.
    #[serde(default)]
    pub hud: Option<HudFailure>,
}

/// The banner's words for a failure: a headline and what to do next.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FailureMessage {
    pub headline: String,
    pub advice: String,
}

impl Failure {
    pub fn message(&self) -> FailureMessage {
        let restored = self
            .hud
            .as_ref()
            .is_some_and(|h| h.rollback == HudRollback::Restored);
        let addons: Vec<&str> = self
            .ids
            .iter()
            .filter(|p| **p != Pak::Hud)
            .map(|p| p.name())
            .collect();
        let hud = self.ids.contains(&Pak::Hud);
        match (addons.is_empty(), hud) {
            (true, true) => FailureMessage {
                headline: if restored {
                    "DeadTune's HUD changes stopped the game from starting, so DeadTune put back the last HUD that worked."
                } else {
                    "DeadTune's HUD changes stopped the game from starting, so they were turned off."
                }
                .into(),
                advice: "Your settings are kept; try again or turn off the newest HUD change."
                    .into(),
            },
            (false, true) => FailureMessage {
                headline: format!(
                    "Deadlock didn't start with {} and DeadTune's HUD changes.",
                    addons.join(", ")
                ),
                advice: format!(
                    "DeadTune turned them off{}, so the game will start normally now. \
                     Your HUD settings are kept. Turn things back on one at a time to find the culprit.",
                    if restored {
                        " and put back the last HUD that worked"
                    } else {
                        ""
                    }
                ),
            },
            _ => FailureMessage {
                headline: format!("Deadlock didn't start with {}.", addons.join(", ")),
                advice: "DeadTune removed them, so the game will start normally now. \
                         Turn them back on one at a time to find the culprit."
                    .into(),
            },
        }
    }
}

/// Suspects from a failed trial re-enabled one per launch, to find the culprit.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Sequence {
    pub pending: Vec<Pak>,
    pub current: Option<Pak>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct Guard {
    pub last_good: PakSet,
    /// The game build `last_good` started on. A pak proven on one build is untried on the
    /// next: it carries copies of the old build's files.
    pub build: Option<String>,
    pub trial: Option<Trial>,
    pub verdicts: BTreeMap<Pak, Verdict>,
    pub failure: Option<Failure>,
    pub sequence: Option<Sequence>,
    /// The process start we last saw, so one launch starts one trial.
    #[serde(skip)]
    seen_start: Option<u64>,
}

/// What the guard knows about one installed (or rolled back) pak.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PakState {
    /// The running game loaded it and the trial has not ended yet.
    OnTrial,
    /// The game started with this exact pak.
    Verified { at: u64 },
    /// Installed, but no launch has been through with it yet.
    Untried,
    /// A launch with it failed and DeadTune rolled it back.
    Broke { at: u64, fatal: Option<String> },
}

impl PakState {
    /// One plain line for reports and the system check.
    pub fn describe(&self) -> String {
        match self {
            PakState::OnTrial => "on trial: the running game is testing it".into(),
            PakState::Verified { at: 0 } => "verified: the game started with it".into(),
            PakState::Verified { at } => {
                format!("verified: the game started with it ({})", date(*at))
            }
            PakState::Untried => "not tried in game yet; tested on the next launch".into(),
            PakState::Broke { at, .. } => {
                format!(
                    "stopped the game from starting ({}); rolled back",
                    date(*at)
                )
            }
        }
    }
}

fn date(at: u64) -> String {
    chrono::DateTime::<chrono::Local>::from(UNIX_EPOCH + Duration::from_secs(at))
        .format("%Y-%m-%d %H:%M")
        .to_string()
}

/// A pak in the game folder right now.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InstalledPak {
    pub id: Pak,
    pub sha256: String,
    /// When DeadTune wrote it; a pak newer than the running game was not loaded by it.
    pub written_at: Option<SystemTime>,
}

/// One poll's worth of facts.
#[derive(Clone, Debug)]
pub struct Observation<'a> {
    pub running: bool,
    pub started_at: Option<SystemTime>,
    pub now: SystemTime,
    pub installed: &'a [InstalledPak],
    /// Console log lines new since the last poll.
    pub lines: &'a [String],
    /// The installed game's build, when known.
    pub build: Option<&'a str>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Event {
    Started {
        changed: Vec<Pak>,
    },
    Passed {
        ids: Vec<Pak>,
    },
    Failed {
        ids: Vec<Pak>,
        fatal: Option<String>,
    },
}

pub fn unix(t: SystemTime) -> u64 {
    t.duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs())
}

/// Names for a status line: "A, B".
pub fn names(ids: &[Pak]) -> String {
    ids.iter().map(|p| p.name()).collect::<Vec<_>>().join(", ")
}

impl Guard {
    pub fn load(state_dir: &Path) -> Result<Guard, AddonError> {
        match std::fs::read_to_string(state_dir.join(RECORD_FILE)) {
            Ok(text) => toml::from_str(&text).map_err(|e| AddonError::Toml(e.to_string())),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Guard::default()),
            Err(e) => Err(e.into()),
        }
    }

    pub fn save(&self, state_dir: &Path) -> Result<(), AddonError> {
        std::fs::create_dir_all(state_dir)?;
        let text = toml::to_string(self).map_err(|e| AddonError::Toml(e.to_string()))?;
        Ok(atomic_write(&state_dir.join(RECORD_FILE), text.as_bytes())?)
    }

    pub fn is_verified(&self, id: impl Into<Pak>, sha256: &str) -> bool {
        self.last_good.get(&id.into()).is_some_and(|s| s == sha256)
    }

    pub fn verdict(&self, id: impl Into<Pak>) -> Option<&Verdict> {
        self.verdicts.get(&id.into())
    }

    /// The guard's view of `id`, whose file on disk hashes to `installed` (`None`: no file).
    pub fn state_of(&self, id: impl Into<Pak>, installed: Option<&str>) -> Option<PakState> {
        let id = id.into();
        let on_trial = |sha: &str| {
            self.trial
                .as_ref()
                .is_some_and(|t| t.changed.get(&id).is_some_and(|s| s == sha))
        };
        match (installed, self.verdict(id)) {
            (Some(sha), _) if on_trial(sha) => Some(PakState::OnTrial),
            (Some(sha), verdict) if self.is_verified(id, sha) => Some(PakState::Verified {
                at: match verdict {
                    Some(Verdict::Verified { at }) => *at,
                    _ => 0,
                },
            }),
            (Some(_), _) => Some(PakState::Untried),
            (None, Some(Verdict::Failed { at, fatal, .. })) => Some(PakState::Broke {
                at: *at,
                fatal: fatal.clone(),
            }),
            (None, _) => None,
        }
    }

    /// True when `plan` would write the exact HUD pak that broke the last launch with it.
    pub fn holds_back(&self, plan: &HudPlan) -> bool {
        let HudAction::Write(bytes) = &plan.action else {
            return false;
        };
        matches!(
            self.verdict(Pak::Hud),
            Some(Verdict::Failed { sha256: Some(sha), .. }) if *sha == sha256_hex(bytes)
        )
    }

    /// Forgets that `id` broke a launch, so the same build may be installed again.
    pub fn retry(&mut self, id: Pak) {
        if matches!(self.verdicts.get(&id), Some(Verdict::Failed { .. })) {
            self.verdicts.remove(&id);
        }
    }

    /// Advances the machine on one poll. At most one event comes back per poll.
    pub fn step(&mut self, obs: &Observation) -> Option<Event> {
        let now = unix(obs.now);
        let fatal = obs
            .lines
            .iter()
            .find(|l| l.contains(FATAL_MARKER))
            .map(|l| l.trim().to_string());
        let booted = obs.lines.iter().any(|l| l.contains(BOOT));
        let mut started = None;
        if obs.running
            && let Some(start) = obs.started_at.map(unix)
            && self.seen_start != Some(start)
        {
            self.seen_start = Some(start);
            let resumed = self
                .trial
                .as_ref()
                .is_some_and(|t| t.launched_at.abs_diff(start) <= 1);
            if !resumed {
                started = self.begin(obs.installed, start, obs.build);
            }
        }
        let Some(trial) = &self.trial else {
            return started;
        };
        let ids: Vec<Pak> = trial.changed.keys().copied().collect();
        let elapsed = Duration::from_secs(now.saturating_sub(trial.launched_at));
        if let Some(line) = fatal {
            return Some(self.fail(now, Some(line)));
        }
        if !obs.running {
            if elapsed < FAIL_WINDOW {
                return Some(self.fail(now, None));
            }
            // Gone between the fail window and the pass mark: nothing to conclude.
            self.trial = None;
            return started;
        }
        if booted || elapsed >= PASS_AFTER {
            self.last_good = trial.loaded.clone();
            self.build = trial.build.clone();
            self.trial = None;
            for id in &ids {
                self.verdicts.insert(*id, Verdict::Verified { at: now });
            }
            self.advance_sequence();
            return Some(Event::Passed { ids });
        }
        started
    }

    /// The game appeared at `start`: what it loaded, and whether any of it is new.
    fn begin(
        &mut self,
        installed: &[InstalledPak],
        start: u64,
        build: Option<&str>,
    ) -> Option<Event> {
        let same_build = build.is_none_or(|b| self.build.as_deref() == Some(b));
        let build = build.map(str::to_string).or_else(|| self.build.clone());
        let loaded: PakSet = installed
            .iter()
            .filter(|p| p.written_at.is_none_or(|t| unix(t) <= start + 1))
            .map(|p| (p.id, p.sha256.clone()))
            .collect();
        let changed: PakSet = loaded
            .iter()
            .filter(|(id, sha)| !same_build || !self.is_verified(**id, sha))
            .map(|(id, sha)| (*id, sha.clone()))
            .collect();
        if changed.is_empty() {
            // Removals cannot break a start; the smaller set is good too.
            self.last_good = loaded;
            self.build = build;
            self.trial = None;
            return None;
        }
        let ids = changed.keys().copied().collect();
        self.trial = Some(Trial {
            changed,
            loaded,
            launched_at: start,
            build,
        });
        Some(Event::Started { changed: ids })
    }

    fn fail(&mut self, now: u64, fatal: Option<String>) -> Event {
        let trial = self.trial.take().expect("fail only runs inside a trial");
        let ids: Vec<Pak> = trial.changed.keys().copied().collect();
        for (id, sha) in &trial.changed {
            self.verdicts.insert(
                *id,
                Verdict::Failed {
                    at: now,
                    fatal: fatal.clone(),
                    sha256: Some(sha.clone()),
                },
            );
        }
        self.failure = Some(Failure {
            ids: ids.clone(),
            fatal: fatal.clone(),
            at: now,
            removed: Vec::new(),
            kept: Vec::new(),
            hud: None,
        });
        self.advance_sequence();
        Event::Failed { ids, fatal }
    }

    /// Starts testing `ids` one per launch; returns the first candidate to enable.
    pub fn start_sequence(&mut self, ids: Vec<Pak>) -> Option<Pak> {
        self.sequence = Some(Sequence {
            pending: ids,
            current: None,
        });
        self.advance_sequence()
    }

    /// The pak to enable next; `None` once every suspect has a verdict.
    fn advance_sequence(&mut self) -> Option<Pak> {
        let seq = self.sequence.as_mut()?;
        if seq.pending.is_empty() {
            self.sequence = None;
            return None;
        }
        seq.current = Some(seq.pending.remove(0));
        seq.current
    }

    /// The suspect the sequence wants installed for the next launch.
    pub fn candidate(&self) -> Option<Pak> {
        self.sequence.as_ref()?.current
    }

    pub fn stop_sequence(&mut self) {
        self.sequence = None;
    }

    pub fn dismiss(&mut self) {
        self.failure = None;
    }

    /// One line per pak the guard tracks or the folder holds, for reports.
    pub fn report(&self, installed: &[InstalledPak]) -> Vec<String> {
        let mut ids: Vec<Pak> = installed.iter().map(|p| p.id).collect();
        ids.extend(self.verdicts.keys().copied());
        ids.sort();
        ids.dedup();
        ids.into_iter()
            .filter_map(|id| {
                let sha = installed
                    .iter()
                    .find(|p| p.id == id)
                    .map(|p| p.sha256.as_str());
                let state = self.state_of(id, sha)?;
                Some(format!("{}: {}", id.name(), state.describe()))
            })
            .collect()
    }
}

/// Every pak our records name that is on disk, with the time we wrote it.
pub fn installed_paks(paths: &GamePaths, state_dir: &Path) -> Vec<InstalledPak> {
    let dir = addons_dir(paths);
    let mut paks: Vec<InstalledPak> = read_record(state_dir)
        .map(|record| {
            record
                .installed
                .iter()
                .filter(|(_, r)| dir.join(&r.file).exists())
                .filter_map(|(key, r)| {
                    Some(InstalledPak {
                        id: Pak::Addon(AddonId::parse(key)?),
                        sha256: r.sha256.clone(),
                        written_at: r.mtime.map(|n| UNIX_EPOCH + Duration::from_nanos(n)),
                    })
                })
                .collect()
        })
        .unwrap_or_default();
    if let Ok(Some(record)) = hud_install::read_record(state_dir)
        && let Ok(meta) = std::fs::metadata(dir.join(HUD_FILE))
    {
        paks.push(InstalledPak {
            id: Pak::Hud,
            sha256: record.sha256,
            written_at: meta.modified().ok(),
        });
    }
    paks
}

/// Rolls back the paks of `ids`: addons are removed (only files our record names, through
/// `remove_now`); the HUD goes back to the kept copy of the last one that started, or is
/// removed. Fills in the failure's `removed`, `kept` and `hud`.
pub fn rollback(guard: &mut Guard, ids: &[Pak], paths: &GamePaths, state_dir: &Path) {
    let record = read_record(state_dir).unwrap_or_default();
    let mut removed = Vec::new();
    let mut kept = Vec::new();
    let mut hud = None;
    for id in ids {
        match id {
            Pak::Addon(addon) => {
                let file = record
                    .installed
                    .get(addon.key())
                    .map(|r| r.file.clone())
                    .unwrap_or_default();
                match install::remove_now(*addon, paths, state_dir) {
                    Ok(true) => removed.push(file),
                    Ok(false) => {}
                    Err(e) => kept.push(format!("{}: {e}", id.name())),
                }
            }
            Pak::Hud => {
                let features = hud_install::read_record(state_dir)
                    .ok()
                    .flatten()
                    .map(|r| r.features)
                    .unwrap_or_default();
                let on_disk = addons_dir(paths).join(HUD_FILE).exists();
                match hud_install::roll_back(paths, state_dir) {
                    Ok(rollback) => {
                        if rollback == HudRollback::Removed && on_disk {
                            removed.push(HUD_FILE.to_string());
                        }
                        hud = Some(HudFailure { rollback, features });
                    }
                    Err(e) => kept.push(format!("{}: {e}", id.name())),
                }
            }
        }
    }
    if let Some(failure) = &mut guard.failure {
        failure.removed = removed;
        failure.kept = kept;
        failure.hud = hud;
    }
}

/// Keeps a copy of the HUD pak the last good launch started with, so a later failure can
/// put it back.
pub fn keep_verified_hud(
    guard: &Guard,
    paths: &GamePaths,
    state_dir: &Path,
) -> Result<(), HudError> {
    match guard.last_good.get(&Pak::Hud) {
        Some(sha) => hud_install::keep_verified(paths, state_dir, sha),
        None => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::addons::AddonsConfig;
    use crate::addons::install::tests::fake_install;

    const T0: u64 = 1_800_000_000;

    fn at(secs: u64) -> SystemTime {
        UNIX_EPOCH + Duration::from_secs(secs)
    }

    fn pak(id: impl Into<Pak>, sha: &str, written: u64) -> InstalledPak {
        InstalledPak {
            id: id.into(),
            sha256: sha.into(),
            written_at: Some(at(written)),
        }
    }

    fn obs<'a>(
        running: bool,
        now: u64,
        installed: &'a [InstalledPak],
        lines: &'a [String],
    ) -> Observation<'a> {
        Observation {
            running,
            started_at: running.then(|| at(T0)),
            now: at(now),
            installed,
            lines,
            build: None,
        }
    }

    fn lines(items: &[&str]) -> Vec<String> {
        items.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn a_launch_with_nothing_new_is_not_a_trial() {
        let installed = [pak(AddonId::SinnerLightFix, "aaa", T0 - 100)];
        let mut guard = Guard::default();
        guard
            .last_good
            .insert(AddonId::SinnerLightFix.into(), "aaa".into());
        assert_eq!(guard.step(&obs(true, T0 + 1, &installed, &[])), None);
        assert!(guard.trial.is_none());
        assert_eq!(guard.step(&obs(true, T0 + 200, &installed, &[])), None);
    }

    #[test]
    fn a_game_update_puts_every_pak_back_on_trial() {
        let installed = [
            pak(Pak::Hud, "hud", T0 - 100),
            pak(AddonId::BlurDisabler, "bbb", T0 - 100),
        ];
        let mut guard = Guard::default();
        let first = Observation {
            build: Some("1"),
            ..obs(true, T0 + 1, &installed, &[])
        };
        guard.step(&first);
        guard.step(&Observation {
            now: at(T0 + 91),
            ..first
        });
        assert!(guard.is_verified(Pak::Hud, "hud"));
        guard.step(&obs(false, T0 + 200, &installed, &[]));

        let later = T0 + 10_000;
        let updated = Observation {
            started_at: Some(at(later)),
            now: at(later + 1),
            build: Some("2"),
            ..obs(true, later, &installed, &[])
        };
        assert_eq!(
            guard.step(&updated),
            Some(Event::Started {
                changed: vec![Pak::Addon(AddonId::BlurDisabler), Pak::Hud]
            }),
            "the paks were proven on the old build only"
        );
        let fatal = lines(&[
            "FATAL ERROR: Unable to load layout file 'file://{resources}/layout/hud_minimap.xml'.",
        ]);
        assert!(matches!(
            guard.step(&Observation {
                now: at(later + 5),
                lines: &fatal,
                ..updated
            }),
            Some(Event::Failed { ids, .. }) if ids.contains(&Pak::Hud)
        ));
    }

    #[test]
    fn removing_addons_shrinks_the_good_set_without_a_trial() {
        let mut guard = Guard::default();
        guard
            .last_good
            .insert(AddonId::SinnerLightFix.into(), "aaa".into());
        guard
            .last_good
            .insert(AddonId::BlurDisabler.into(), "bbb".into());
        let installed = [pak(AddonId::BlurDisabler, "bbb", T0 - 100)];
        assert_eq!(guard.step(&obs(true, T0 + 1, &installed, &[])), None);
        assert_eq!(guard.last_good.len(), 1);
        assert!(guard.is_verified(AddonId::BlurDisabler, "bbb"));
    }

    #[test]
    fn a_changed_set_starts_a_trial_that_passes_by_time() {
        let mut guard = Guard::default();
        guard
            .last_good
            .insert(AddonId::SinnerLightFix.into(), "aaa".into());
        let installed = [
            pak(AddonId::SinnerLightFix, "aaa", T0 - 100),
            pak(AddonId::BlurDisabler, "bbb", T0 - 50),
            pak(AddonId::VindictaScope, "ccc", T0 + 30),
        ];
        assert_eq!(
            guard.step(&obs(true, T0 + 1, &installed, &[])),
            Some(Event::Started {
                changed: vec![Pak::Addon(AddonId::BlurDisabler)]
            }),
            "the pak written after the game started was not loaded"
        );
        assert_eq!(guard.step(&obs(true, T0 + 60, &installed, &[])), None);
        assert_eq!(
            guard.step(&obs(true, T0 + 91, &installed, &[])),
            Some(Event::Passed {
                ids: vec![Pak::Addon(AddonId::BlurDisabler)]
            })
        );
        assert!(guard.trial.is_none());
        assert!(guard.is_verified(AddonId::BlurDisabler, "bbb"));
        assert!(!guard.is_verified(AddonId::VindictaScope, "ccc"));
        assert!(matches!(
            guard.verdict(AddonId::BlurDisabler),
            Some(Verdict::Verified { at }) if *at == T0 + 91
        ));
        assert_eq!(guard.step(&obs(true, T0 + 500, &installed, &[])), None);
    }

    #[test]
    fn the_boot_marker_passes_a_trial_early() {
        let mut guard = Guard::default();
        let installed = [pak(AddonId::BlurDisabler, "bbb", T0 - 50)];
        guard.step(&obs(true, T0 + 1, &installed, &[]));
        let log = lines(&["[Console] DEADTUNE_BOOT 0.3.0"]);
        assert!(matches!(
            guard.step(&obs(true, T0 + 12, &installed, &log)),
            Some(Event::Passed { .. })
        ));
    }

    #[test]
    fn an_early_exit_fails_the_trial() {
        let mut guard = Guard::default();
        let installed = [
            pak(AddonId::BlurDisabler, "bbb", T0 - 50),
            pak(AddonId::ParticleDisabler, "ppp", T0 - 50),
        ];
        guard.step(&obs(true, T0 + 1, &installed, &[]));
        assert_eq!(
            guard.step(&obs(false, T0 + 20, &installed, &[])),
            Some(Event::Failed {
                ids: vec![
                    Pak::Addon(AddonId::ParticleDisabler),
                    Pak::Addon(AddonId::BlurDisabler)
                ],
                fatal: None
            })
        );
        assert!(guard.last_good.is_empty());
        let failure = guard.failure.clone().unwrap();
        assert_eq!(failure.ids.len(), 2);
        assert_eq!(failure.at, T0 + 20);
        assert!(matches!(
            guard.verdict(AddonId::BlurDisabler),
            Some(Verdict::Failed { fatal: None, .. })
        ));
        guard.dismiss();
        assert!(guard.failure.is_none());
    }

    #[test]
    fn a_fatal_line_fails_the_trial_while_the_game_still_shows_its_dialog() {
        let mut guard = Guard::default();
        let installed = [pak(AddonId::BlurDisabler, "bbb", T0 - 50)];
        guard.step(&obs(true, T0 + 1, &installed, &[]));
        let log = lines(&[
            "Loading...",
            "FATAL ERROR: Unable to read default keybinding configuration user_keys_default",
        ]);
        assert_eq!(
            guard.step(&obs(true, T0 + 9, &installed, &log)),
            Some(Event::Failed {
                ids: vec![Pak::Addon(AddonId::BlurDisabler)],
                fatal: Some(
                    "FATAL ERROR: Unable to read default keybinding configuration user_keys_default"
                        .into()
                )
            })
        );
        assert!(guard.failure.as_ref().unwrap().fatal.is_some());
    }

    #[test]
    fn a_fatal_line_outside_a_trial_and_an_exit_after_the_window_change_nothing() {
        let mut guard = Guard::default();
        let log = lines(&["FATAL ERROR: stale line from last session"]);
        assert_eq!(guard.step(&obs(false, T0, &[], &log)), None);
        assert!(guard.failure.is_none());

        let installed = [pak(AddonId::BlurDisabler, "bbb", T0 - 50)];
        guard.step(&obs(true, T0 + 1, &installed, &[]));
        assert_eq!(guard.step(&obs(false, T0 + 75, &installed, &[])), None);
        assert!(guard.trial.is_none(), "inconclusive: no verdict either way");
        assert!(guard.verdicts.is_empty());
    }

    #[test]
    fn one_at_a_time_marks_the_culprit_and_verifies_the_rest() {
        let mut guard = Guard::default();
        let a = Pak::Addon(AddonId::BlurDisabler);
        let b = Pak::Addon(AddonId::ParticleDisabler);
        let c = Pak::Addon(AddonId::SinnerLightFix);
        assert_eq!(guard.start_sequence(vec![a, b, c]), Some(a));
        assert_eq!(guard.candidate(), Some(a));

        let installed = [pak(a, "aaa", T0 - 50)];
        guard.step(&obs(true, T0 + 1, &installed, &[]));
        assert!(matches!(
            guard.step(&obs(true, T0 + 95, &installed, &[])),
            Some(Event::Passed { .. })
        ));
        assert_eq!(guard.candidate(), Some(b), "next suspect after a pass");

        let t1 = T0 + 300;
        let installed = [pak(a, "aaa", T0 - 50), pak(b, "bbb", T0 + 200)];
        let mut second = obs(true, t1 + 1, &installed, &[]);
        second.started_at = Some(at(t1));
        guard.step(&second);
        let mut exit = obs(false, t1 + 10, &installed, &[]);
        exit.started_at = None;
        assert_eq!(
            guard.step(&exit),
            Some(Event::Failed {
                ids: vec![b],
                fatal: None
            })
        );
        assert_eq!(guard.candidate(), Some(c), "next suspect after a failure");
        assert!(guard.is_verified(a, "aaa"));

        let t2 = T0 + 600;
        let installed = [pak(a, "aaa", T0 - 50), pak(c, "ccc", t2 - 10)];
        let mut third = obs(true, t2 + 1, &installed, &[]);
        third.started_at = Some(at(t2));
        guard.step(&third);
        let mut pass = obs(true, t2 + 100, &installed, &[]);
        pass.started_at = Some(at(t2));
        assert!(matches!(guard.step(&pass), Some(Event::Passed { .. })));
        assert_eq!(guard.candidate(), None);
        assert!(guard.sequence.is_none(), "every suspect has a verdict");
        assert!(matches!(guard.verdict(b), Some(Verdict::Failed { .. })));
        assert!(matches!(guard.verdict(c), Some(Verdict::Verified { .. })));
    }

    #[test]
    fn state_survives_a_restart_mid_trial() {
        let dir = tempfile::tempdir().unwrap();
        let mut guard = Guard::default();
        guard
            .last_good
            .insert(AddonId::SinnerLightFix.into(), "aaa".into());
        let installed = [
            pak(AddonId::SinnerLightFix, "aaa", T0 - 100),
            pak(AddonId::BlurDisabler, "bbb", T0 - 50),
        ];
        guard.step(&obs(true, T0 + 1, &installed, &[]));
        guard.save(dir.path()).unwrap();

        let mut again = Guard::load(dir.path()).unwrap();
        assert_eq!(again.trial, guard.trial);
        assert_eq!(again.last_good, guard.last_good);
        assert_eq!(
            again.step(&obs(true, T0 + 30, &installed, &[])),
            None,
            "the same process start resumes the trial instead of starting another"
        );
        assert!(matches!(
            again.step(&obs(true, T0 + 95, &installed, &[])),
            Some(Event::Passed { .. })
        ));
        again.save(dir.path()).unwrap();
        let third = Guard::load(dir.path()).unwrap();
        assert!(third.is_verified(AddonId::BlurDisabler, "bbb"));
        assert!(third.trial.is_none());
        assert_eq!(
            Guard::load(&dir.path().join("nope")).unwrap(),
            Guard::default()
        );
    }

    #[test]
    fn rollback_removes_only_the_changed_addons_and_only_our_files() {
        let (steam, paths) = fake_install("1");
        let state = steam.path().join("data");
        let config = AddonsConfig {
            enabled: [AddonId::SinnerLightFix, AddonId::BlurDisabler]
                .into_iter()
                .collect(),
            ..Default::default()
        };
        install::execute(
            &install::plan(&paths, &config, &state).unwrap(),
            &paths,
            &state,
        )
        .unwrap();
        let dir = addons_dir(&paths);
        std::fs::write(dir.join("pak01_dir.vpk"), b"someone else's mod").unwrap();

        let installed = installed_paks(&paths, &state);
        assert_eq!(installed.len(), 2);
        let mut guard = Guard::default();
        let sinner = installed
            .iter()
            .find(|p| p.id == Pak::Addon(AddonId::SinnerLightFix))
            .unwrap();
        guard
            .last_good
            .insert(AddonId::SinnerLightFix.into(), sinner.sha256.clone());
        let start =
            installed.iter().filter_map(|p| p.written_at).max().unwrap() + Duration::from_secs(5);
        let mut first = obs(true, unix(start) + 1, &installed, &[]);
        first.started_at = Some(start);
        assert_eq!(
            guard.step(&first),
            Some(Event::Started {
                changed: vec![Pak::Addon(AddonId::BlurDisabler)]
            })
        );
        let mut exit = obs(false, unix(start) + 10, &installed, &[]);
        exit.started_at = None;
        let Some(Event::Failed { ids, .. }) = guard.step(&exit) else {
            panic!("expected a failure");
        };
        rollback(&mut guard, &ids, &paths, &state);

        let mut names: Vec<String> = std::fs::read_dir(&dir)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        assert_eq!(names, ["pak01_dir.vpk", "pak73_dir.vpk"]);
        let failure = guard.failure.as_ref().unwrap();
        assert_eq!(failure.removed, ["pak72_dir.vpk"]);
        assert!(failure.kept.is_empty());
        assert_eq!(installed_paks(&paths, &state).len(), 1);
    }

    fn write_hud(paths: &GamePaths, state: &Path, bytes: &[u8], features: Vec<HudFeature>) {
        let dir = addons_dir(paths);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join(HUD_FILE), bytes).unwrap();
        let record = hud_install::InstallRecord {
            sha256: sha256_hex(bytes),
            build_id: Some("1".into()),
            patched: vec!["panorama/styles/hud.vcss_c".into()],
            features,
            layout: None,
            sources: Default::default(),
        };
        std::fs::create_dir_all(state).unwrap();
        std::fs::write(
            state.join(hud_install::RECORD_FILE),
            toml::to_string(&record).unwrap(),
        )
        .unwrap();
    }

    /// One launch at `start` that ends with `lines` (a boot line or a FATAL).
    fn launch(
        guard: &mut Guard,
        paths: &GamePaths,
        state: &Path,
        start: SystemTime,
        lines: &[String],
    ) -> (Option<Event>, Option<Event>) {
        let installed = installed_paks(paths, state);
        let mut first = obs(true, unix(start) + 1, &installed, &[]);
        first.started_at = Some(start);
        let began = guard.step(&first);
        let mut end = obs(true, unix(start) + 8, &installed, lines);
        end.started_at = Some(start);
        (began, guard.step(&end))
    }

    fn fatal() -> Vec<String> {
        lines(&["FATAL ERROR: Unable to read default keybinding configuration user_keys_default"])
    }

    fn later(secs: u64) -> SystemTime {
        SystemTime::now() + Duration::from_secs(secs)
    }

    #[test]
    fn pak_keys_load_records_written_before_the_hud_was_guarded() {
        let old = "[last_good]\nblur_disabler = \"bbb\"\n\n[failure]\nids = [\"blur_disabler\"]\nat = 5\n";
        let guard: Guard = toml::from_str(old).unwrap();
        assert!(guard.is_verified(AddonId::BlurDisabler, "bbb"));
        assert_eq!(
            guard.failure.unwrap().ids,
            [Pak::Addon(AddonId::BlurDisabler)]
        );

        let mut guard = Guard::default();
        guard.last_good.insert(Pak::Hud, "hhh".into());
        guard.verdicts.insert(
            Pak::Hud,
            Verdict::Failed {
                at: 1,
                fatal: None,
                sha256: Some("bad".into()),
            },
        );
        let again: Guard = toml::from_str(&toml::to_string(&guard).unwrap()).unwrap();
        assert_eq!(again, guard);
        assert_eq!(Pak::parse("nope"), None);
    }

    #[test]
    fn a_changed_hud_pak_goes_on_trial_and_an_unchanged_one_does_not() {
        let mut guard = Guard::default();
        let installed = [pak(Pak::Hud, "h1", T0 - 50)];
        assert_eq!(
            guard.step(&obs(true, T0 + 1, &installed, &[])),
            Some(Event::Started {
                changed: vec![Pak::Hud]
            })
        );
        assert_eq!(
            guard.state_of(Pak::Hud, Some("h1")),
            Some(PakState::OnTrial)
        );
        let log = lines(&["DEADTUNE_BOOT 0.12.0"]);
        assert_eq!(
            guard.step(&obs(true, T0 + 9, &installed, &log)),
            Some(Event::Passed {
                ids: vec![Pak::Hud]
            })
        );
        assert!(guard.is_verified(Pak::Hud, "h1"));
        assert!(matches!(
            guard.state_of(Pak::Hud, Some("h1")),
            Some(PakState::Verified { at }) if at == T0 + 9
        ));

        let t1 = T0 + 1000;
        let mut again = obs(true, t1 + 1, &installed, &[]);
        again.started_at = Some(at(t1));
        assert_eq!(guard.step(&again), None, "same HUD pak, no trial");
        assert!(guard.trial.is_none());

        let changed = [pak(Pak::Hud, "h2", t1 + 100)];
        let t2 = T0 + 2000;
        let mut third = obs(true, t2 + 1, &changed, &[]);
        third.started_at = Some(at(t2));
        assert_eq!(
            guard.step(&third),
            Some(Event::Started {
                changed: vec![Pak::Hud]
            })
        );
    }

    #[test]
    fn a_fatal_with_the_hud_on_trial_restores_the_last_hud_that_started() {
        let (steam, paths) = fake_install("1");
        let state = steam.path().join("data");
        let mut guard = Guard::default();
        write_hud(&paths, &state, b"hud one", vec![HudFeature::Layout]);
        let (began, passed) = launch(
            &mut guard,
            &paths,
            &state,
            later(5),
            &lines(&["DEADTUNE_BOOT"]),
        );
        assert!(matches!(began, Some(Event::Started { .. })));
        assert!(matches!(passed, Some(Event::Passed { .. })));
        keep_verified_hud(&guard, &paths, &state).unwrap();

        write_hud(
            &paths,
            &state,
            b"hud two",
            vec![HudFeature::TopBar, HudFeature::Images],
        );
        let (began, failed) = launch(&mut guard, &paths, &state, later(10), &fatal());
        assert_eq!(
            began,
            Some(Event::Started {
                changed: vec![Pak::Hud]
            })
        );
        let Some(Event::Failed { ids, fatal }) = failed else {
            panic!("expected a failure, got {failed:?}");
        };
        assert_eq!(ids, [Pak::Hud]);
        assert!(fatal.is_some());
        rollback(&mut guard, &ids, &paths, &state);

        let hud_path = addons_dir(&paths).join(HUD_FILE);
        assert_eq!(std::fs::read(&hud_path).unwrap(), b"hud one");
        let failure = guard.failure.clone().unwrap();
        assert_eq!(
            failure.hud,
            Some(HudFailure {
                rollback: HudRollback::Restored,
                features: vec![HudFeature::TopBar, HudFeature::Images],
            })
        );
        assert!(failure.kept.is_empty());
        assert!(
            failure.removed.is_empty(),
            "a restored HUD is not listed as removed"
        );
        let message = failure.message();
        assert_eq!(
            message.headline,
            "DeadTune's HUD changes stopped the game from starting, so DeadTune put back the last HUD that worked."
        );
        assert_eq!(
            message.advice,
            "Your settings are kept; try again or turn off the newest HUD change."
        );
        assert!(
            guard.is_verified(Pak::Hud, &sha256_hex(b"hud one")),
            "the restored pak is the verified one, so the next launch is not a trial"
        );

        let plan = |bytes: &[u8]| HudPlan {
            addon_path: hud_path.clone(),
            action: HudAction::Write(bytes.to_vec()),
            patch: Default::default(),
            conflicts: Vec::new(),
            needs_search_path: false,
            icon_problems: Vec::new(),
            features: Vec::new(),
            layout: None,
            sources: Default::default(),
        };
        assert!(
            guard.holds_back(&plan(b"hud two")),
            "the broken build stays out"
        );
        assert!(
            !guard.holds_back(&plan(b"hud three")),
            "a changed HUD goes in"
        );
        guard.retry(Pak::Hud);
        assert!(
            !guard.holds_back(&plan(b"hud two")),
            "try again lets it back in"
        );
    }

    #[test]
    fn a_fatal_with_no_verified_hud_removes_the_hud_pak() {
        let (steam, paths) = fake_install("1");
        let state = steam.path().join("data");
        let mut guard = Guard::default();
        write_hud(&paths, &state, b"hud one", vec![HudFeature::IngameSettings]);
        let (_, failed) = launch(&mut guard, &paths, &state, later(5), &fatal());
        let Some(Event::Failed { ids, .. }) = failed else {
            panic!("expected a failure, got {failed:?}");
        };
        rollback(&mut guard, &ids, &paths, &state);
        assert!(!addons_dir(&paths).join(HUD_FILE).exists());
        assert!(!state.join(hud_install::RECORD_FILE).exists());
        assert!(installed_paks(&paths, &state).is_empty());
        let failure = guard.failure.clone().unwrap();
        assert_eq!(failure.removed, [HUD_FILE]);
        assert_eq!(
            failure.message().headline,
            "DeadTune's HUD changes stopped the game from starting, so they were turned off."
        );
        assert!(matches!(
            guard.state_of(Pak::Hud, None),
            Some(PakState::Broke { fatal: Some(_), .. })
        ));
        assert_eq!(
            guard.report(&[]),
            [format!(
                "DeadTune's HUD changes: stopped the game from starting ({}); rolled back",
                date(failure.at)
            )]
        );
    }

    #[test]
    fn an_addon_and_the_hud_on_trial_together_roll_back_both_and_bisect_both() {
        let (steam, paths) = fake_install("1");
        let state = steam.path().join("data");
        let config = AddonsConfig {
            enabled: [AddonId::BlurDisabler].into_iter().collect(),
            ..Default::default()
        };
        install::execute(
            &install::plan(&paths, &config, &state).unwrap(),
            &paths,
            &state,
        )
        .unwrap();
        write_hud(&paths, &state, b"hud one", vec![HudFeature::HealthBar]);
        let mut guard = Guard::default();
        let (began, failed) = launch(&mut guard, &paths, &state, later(5), &fatal());
        let both = vec![Pak::Addon(AddonId::BlurDisabler), Pak::Hud];
        assert_eq!(
            began,
            Some(Event::Started {
                changed: both.clone()
            })
        );
        let Some(Event::Failed { ids, .. }) = failed else {
            panic!("expected a failure, got {failed:?}");
        };
        assert_eq!(ids, both);
        rollback(&mut guard, &ids, &paths, &state);
        assert!(installed_paks(&paths, &state).is_empty());
        let failure = guard.failure.clone().unwrap();
        assert_eq!(failure.removed.len(), 2);
        let message = failure.message();
        assert_eq!(
            message.headline,
            "Deadlock didn't start with UI blur disabler and DeadTune's HUD changes."
        );
        assert!(
            message.advice.contains("one at a time"),
            "{}",
            message.advice
        );

        assert_eq!(
            guard.start_sequence(failure.ids.clone()),
            Some(Pak::Addon(AddonId::BlurDisabler))
        );
        let t1 = later(100);
        let installed = [pak(AddonId::BlurDisabler, "bbb", unix(t1) - 10)];
        let mut first = obs(true, unix(t1) + 1, &installed, &[]);
        first.started_at = Some(t1);
        guard.step(&first);
        let mut pass = obs(true, unix(t1) + 95, &installed, &[]);
        pass.started_at = Some(t1);
        assert!(matches!(guard.step(&pass), Some(Event::Passed { .. })));
        assert_eq!(
            guard.candidate(),
            Some(Pak::Hud),
            "the HUD is the next suspect"
        );
    }

    #[test]
    fn with_every_pak_out_a_launch_is_not_a_trial_and_the_kept_hud_survives() {
        let (steam, paths) = fake_install("1");
        let state = steam.path().join("data");
        let mut guard = Guard::default();
        write_hud(&paths, &state, b"hud one", vec![HudFeature::Layout]);
        launch(
            &mut guard,
            &paths,
            &state,
            later(5),
            &lines(&["DEADTUNE_BOOT"]),
        );
        keep_verified_hud(&guard, &paths, &state).unwrap();
        std::fs::remove_file(addons_dir(&paths).join(HUD_FILE)).unwrap();
        std::fs::remove_file(state.join(hud_install::RECORD_FILE)).unwrap();

        let (began, ended) = launch(&mut guard, &paths, &state, later(50), &[]);
        assert_eq!((began, ended), (None, None));
        assert!(guard.trial.is_none());
        assert!(
            state
                .join(hud_install::VERIFIED_DIR)
                .join(format!("{}.vpk", sha256_hex(b"hud one")))
                .is_file()
        );
    }
}
