//! Launch guard. The paks the game last started with are the known-good set; any pak
//! newer than that is on trial from the moment the game process appears. The trial
//! passes when the game stays up long enough or prints DeadTune's boot marker, and fails
//! when the game dies early or prints a fatal error. A failure names exactly the addons
//! that changed since the known-good set, so the caller removes those and nothing else.
//! Persisted in `state_dir/guard.toml`, so a DeadTune restart mid-trial resumes it.

use std::collections::BTreeMap;
use std::path::Path;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use super::install::{self, read_record};
use super::{AddonError, AddonId};
use crate::backup::atomic_write;
use crate::bridge::ack::BOOT;
use crate::hud::install::addons_dir;
use crate::locate::GamePaths;

pub const RECORD_FILE: &str = "guard.toml";
pub const FATAL_MARKER: &str = "FATAL ERROR";
/// A game gone this soon after it appeared did not start.
pub const FAIL_WINDOW: Duration = Duration::from_secs(60);
/// A game still up this long after it appeared has started.
pub const PASS_AFTER: Duration = Duration::from_secs(90);

/// Addon id to the sha256 of its pak.
pub type AddonSet = BTreeMap<AddonId, String>;

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Trial {
    /// Paks the game loaded that the last good set did not have (new or rebuilt).
    pub changed: AddonSet,
    /// Everything the game loaded; becomes the last good set when the trial passes.
    pub loaded: AddonSet,
    /// Unix seconds; when the game process appeared.
    pub launched_at: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Verdict {
    Verified { at: u64 },
    Failed { at: u64, fatal: Option<String> },
}

/// A failed trial, shown until dismissed.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Failure {
    pub ids: Vec<AddonId>,
    pub fatal: Option<String>,
    pub at: u64,
    /// Files DeadTune deleted, for the details view.
    #[serde(default)]
    pub removed: Vec<String>,
    /// Addons whose pak could not be removed, with the reason.
    #[serde(default)]
    pub kept: Vec<String>,
}

/// Suspects from a failed trial re-enabled one per launch, to find the culprit.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Sequence {
    pub pending: Vec<AddonId>,
    pub current: Option<AddonId>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct Guard {
    pub last_good: AddonSet,
    pub trial: Option<Trial>,
    pub verdicts: BTreeMap<AddonId, Verdict>,
    pub failure: Option<Failure>,
    pub sequence: Option<Sequence>,
    /// The process start we last saw, so one launch starts one trial.
    #[serde(skip)]
    seen_start: Option<u64>,
}

/// A pak in the game folder right now.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InstalledPak {
    pub id: AddonId,
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
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Event {
    Started {
        changed: Vec<AddonId>,
    },
    Passed {
        ids: Vec<AddonId>,
    },
    Failed {
        ids: Vec<AddonId>,
        fatal: Option<String>,
    },
}

pub fn unix(t: SystemTime) -> u64 {
    t.duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs())
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

    pub fn is_verified(&self, id: AddonId, sha256: &str) -> bool {
        self.last_good.get(&id).is_some_and(|s| s == sha256)
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
                started = self.begin(obs.installed, start);
            }
        }
        let Some(trial) = &self.trial else {
            return started;
        };
        let ids: Vec<AddonId> = trial.changed.keys().copied().collect();
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
            let loaded = trial.loaded.clone();
            self.last_good = loaded;
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
    fn begin(&mut self, installed: &[InstalledPak], start: u64) -> Option<Event> {
        let loaded: AddonSet = installed
            .iter()
            .filter(|p| p.written_at.is_none_or(|t| unix(t) <= start + 1))
            .map(|p| (p.id, p.sha256.clone()))
            .collect();
        let changed: AddonSet = loaded
            .iter()
            .filter(|(id, sha)| !self.is_verified(**id, sha))
            .map(|(id, sha)| (*id, sha.clone()))
            .collect();
        if changed.is_empty() {
            // Removals cannot break a start; the smaller set is good too.
            self.last_good = loaded;
            self.trial = None;
            return None;
        }
        let ids = changed.keys().copied().collect();
        self.trial = Some(Trial {
            changed,
            loaded,
            launched_at: start,
        });
        Some(Event::Started { changed: ids })
    }

    fn fail(&mut self, now: u64, fatal: Option<String>) -> Event {
        let trial = self.trial.take().expect("fail only runs inside a trial");
        let ids: Vec<AddonId> = trial.changed.keys().copied().collect();
        for id in &ids {
            self.verdicts.insert(
                *id,
                Verdict::Failed {
                    at: now,
                    fatal: fatal.clone(),
                },
            );
        }
        self.failure = Some(Failure {
            ids: ids.clone(),
            fatal: fatal.clone(),
            at: now,
            removed: Vec::new(),
            kept: Vec::new(),
        });
        self.advance_sequence();
        Event::Failed { ids, fatal }
    }

    /// Starts testing `ids` one per launch; returns the first candidate to enable.
    pub fn start_sequence(&mut self, ids: Vec<AddonId>) -> Option<AddonId> {
        self.sequence = Some(Sequence {
            pending: ids,
            current: None,
        });
        self.advance_sequence()
    }

    /// The addon to enable next; `None` once every suspect has a verdict.
    fn advance_sequence(&mut self) -> Option<AddonId> {
        let seq = self.sequence.as_mut()?;
        if seq.pending.is_empty() {
            self.sequence = None;
            return None;
        }
        seq.current = Some(seq.pending.remove(0));
        seq.current
    }

    /// The suspect the sequence wants installed for the next launch.
    pub fn candidate(&self) -> Option<AddonId> {
        self.sequence.as_ref()?.current
    }

    pub fn stop_sequence(&mut self) {
        self.sequence = None;
    }

    pub fn dismiss(&mut self) {
        self.failure = None;
    }

    pub fn verdict(&self, id: AddonId) -> Option<&Verdict> {
        self.verdicts.get(&id)
    }
}

/// Every pak our record names that is on disk, with the time we wrote it.
pub fn installed_paks(paths: &GamePaths, state_dir: &Path) -> Vec<InstalledPak> {
    let dir = addons_dir(paths);
    let Ok(record) = read_record(state_dir) else {
        return Vec::new();
    };
    record
        .installed
        .iter()
        .filter(|(_, r)| dir.join(&r.file).exists())
        .filter_map(|(key, r)| {
            Some(InstalledPak {
                id: AddonId::parse(key)?,
                sha256: r.sha256.clone(),
                written_at: r.mtime.map(|n| UNIX_EPOCH + Duration::from_nanos(n)),
            })
        })
        .collect()
}

/// Removes the paks of `ids` (only files our record names, through `remove_now`), and
/// fills in the failure's `removed` and `kept` lists.
pub fn rollback(guard: &mut Guard, ids: &[AddonId], paths: &GamePaths, state_dir: &Path) {
    let record = read_record(state_dir).unwrap_or_default();
    let mut removed = Vec::new();
    let mut kept = Vec::new();
    for id in ids {
        let file = record
            .installed
            .get(id.key())
            .map(|r| r.file.clone())
            .unwrap_or_default();
        match install::remove_now(*id, paths, state_dir) {
            Ok(true) => removed.push(file),
            Ok(false) => {}
            Err(e) => kept.push(format!("{}: {e}", super::info(*id).name)),
        }
    }
    if let Some(failure) = &mut guard.failure {
        failure.removed = removed;
        failure.kept = kept;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::addons::install::tests::fake_install;
    use crate::addons::{AddonsConfig, sources};

    const T0: u64 = 1_800_000_000;

    fn at(secs: u64) -> SystemTime {
        UNIX_EPOCH + Duration::from_secs(secs)
    }

    fn pak(id: AddonId, sha: &str, written: u64) -> InstalledPak {
        InstalledPak {
            id,
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
            .insert(AddonId::SinnerLightFix, "aaa".into());
        assert_eq!(guard.step(&obs(true, T0 + 1, &installed, &[])), None);
        assert!(guard.trial.is_none());
        assert_eq!(guard.step(&obs(true, T0 + 200, &installed, &[])), None);
    }

    #[test]
    fn removing_addons_shrinks_the_good_set_without_a_trial() {
        let mut guard = Guard::default();
        guard
            .last_good
            .insert(AddonId::SinnerLightFix, "aaa".into());
        guard.last_good.insert(AddonId::BlurDisabler, "bbb".into());
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
            .insert(AddonId::SinnerLightFix, "aaa".into());
        let installed = [
            pak(AddonId::SinnerLightFix, "aaa", T0 - 100),
            pak(AddonId::BlurDisabler, "bbb", T0 - 50),
            pak(AddonId::VindictaScope, "ccc", T0 + 30),
        ];
        assert_eq!(
            guard.step(&obs(true, T0 + 1, &installed, &[])),
            Some(Event::Started {
                changed: vec![AddonId::BlurDisabler]
            }),
            "the pak written after the game started was not loaded"
        );
        assert_eq!(guard.step(&obs(true, T0 + 60, &installed, &[])), None);
        assert_eq!(
            guard.step(&obs(true, T0 + 91, &installed, &[])),
            Some(Event::Passed {
                ids: vec![AddonId::BlurDisabler]
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
                ids: vec![AddonId::ParticleDisabler, AddonId::BlurDisabler],
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
                ids: vec![AddonId::BlurDisabler],
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
        let a = AddonId::BlurDisabler;
        let b = AddonId::ParticleDisabler;
        let c = AddonId::SinnerLightFix;
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
            .insert(AddonId::SinnerLightFix, "aaa".into());
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
        sources::import(
            &sources::cache_dir(&state),
            &sources::tests::research("Sinner Light Fix Mod", "pak26_dir.vpk"),
        )
        .unwrap();
        sources::import(
            &sources::cache_dir(&state),
            &sources::tests::research("Blur Disabler", "pak97_dir.vpk"),
        )
        .unwrap();
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
            .find(|p| p.id == AddonId::SinnerLightFix)
            .unwrap();
        guard
            .last_good
            .insert(AddonId::SinnerLightFix, sinner.sha256.clone());
        let start =
            installed.iter().filter_map(|p| p.written_at).max().unwrap() + Duration::from_secs(5);
        let mut first = obs(true, unix(start) + 1, &installed, &[]);
        first.started_at = Some(start);
        assert_eq!(
            guard.step(&first),
            Some(Event::Started {
                changed: vec![AddonId::BlurDisabler]
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
}
