//! Game file snapshots from the window: a job thread that takes a snapshot and compares
//! it with the previous one (or only compares two), its progress and cancel flag, and the
//! page's cached listing. The work itself is `dt_core::snapshot`.

use std::ops::ControlFlow;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, channel};
use std::time::Duration;

use dt_core::locate::{self, GamePaths};
use dt_core::snapshot::{self, Inventory, Selection, SnapshotDiff, diff, human_bytes, store};

use crate::state::{AppState, Status};

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SnapshotProgress {
    pub done: usize,
    pub total: usize,
    pub path: String,
    pub bytes: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum JobKind {
    /// `auto` when the game update watcher started it.
    Take {
        auto: bool,
    },
    Compare,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Taken {
    pub folder: PathBuf,
    pub buildid: Option<String>,
    pub files: usize,
    pub written: usize,
    pub reused: usize,
    pub bytes: u64,
    pub elapsed: Duration,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SnapshotDone {
    pub taken: Option<Taken>,
    pub diff: Option<SnapshotDiff>,
}

impl SnapshotDone {
    /// One sentence for the status line.
    pub fn summary(&self) -> String {
        let mut out = match &self.taken {
            Some(t) => format!(
                "Snapshot saved: {} files, {} in {:.1} s.",
                t.files,
                human_bytes(t.bytes),
                t.elapsed.as_secs_f64()
            ),
            None => String::new(),
        };
        match &self.diff {
            Some(d) => {
                if !out.is_empty() {
                    out.push(' ');
                }
                out.push_str(&format!(
                    "{}: {} in the files DeadTune watches, {} DeadTune feature{} affected.",
                    d.title(),
                    d.total().phrase(),
                    d.impact().count(),
                    if d.impact().count() == 1 { "" } else { "s" }
                ));
            }
            None if self.taken.is_some() => {
                out.push_str(" Nothing to compare with yet; the next one will be.");
            }
            None => {}
        }
        out
    }
}

enum Msg {
    Progress(SnapshotProgress),
    Done(Box<Result<SnapshotDone, String>>),
}

pub struct SnapshotJob {
    rx: Receiver<Msg>,
    cancel: Arc<AtomicBool>,
    pub kind: JobKind,
    pub progress: SnapshotProgress,
}

impl SnapshotJob {
    /// The thread stops after the file it is on; a fresh folder is removed.
    pub fn cancel(&self) {
        self.cancel.store(true, Ordering::Relaxed);
    }

    fn spawn(
        kind: JobKind,
        work: impl FnOnce(
            &mut dyn FnMut(SnapshotProgress) -> ControlFlow<()>,
        ) -> Result<SnapshotDone, String>
        + Send
        + 'static,
    ) -> SnapshotJob {
        let (tx, rx) = channel();
        let cancel = Arc::new(AtomicBool::new(false));
        let stop = cancel.clone();
        std::thread::spawn(move || {
            let progress_tx = tx.clone();
            let result = work(&mut |p| {
                let _ = progress_tx.send(Msg::Progress(p));
                if stop.load(Ordering::Relaxed) {
                    ControlFlow::Break(())
                } else {
                    ControlFlow::Continue(())
                }
            });
            let _ = tx.send(Msg::Done(Box::new(result)));
        });
        SnapshotJob {
            rx,
            cancel,
            kind,
            progress: SnapshotProgress::default(),
        }
    }
}

/// Takes a snapshot, then compares it with the newest other complete snapshot and writes
/// the report into it.
fn take_and_compare(
    paths: &GamePaths,
    data_dir: &Path,
    selection: &Selection,
    progress: &mut dyn FnMut(SnapshotProgress) -> ControlFlow<()>,
) -> Result<SnapshotDone, String> {
    let out = snapshot::take(paths, data_dir, selection, &mut |p| {
        progress(SnapshotProgress {
            done: p.done,
            total: p.total,
            path: p.path.to_string(),
            bytes: p.bytes,
        })
    })
    .map_err(|e| e.to_string())?;
    let previous = store::list(&store::dir(data_dir))
        .unwrap_or_default()
        .into_iter()
        .find(|s| s.complete && s.folder != out.folder);
    let diff = match previous {
        Some(p) => Some(compare_and_write(&p.folder, &out.folder)?),
        None => None,
    };
    Ok(SnapshotDone {
        taken: Some(Taken {
            folder: out.folder,
            buildid: out.manifest.buildid,
            files: out.manifest.files.len(),
            written: out.written,
            reused: out.reused,
            bytes: out.bytes,
            elapsed: out.elapsed,
        }),
        diff,
    })
}

fn compare_and_write(old: &Path, new: &Path) -> Result<SnapshotDiff, String> {
    let report = snapshot::compare(old, new).map_err(|e| e.to_string())?;
    diff::write(&report).map_err(|e| e.to_string())?;
    Ok(report)
}

impl AppState {
    pub fn snapshot_dir(&self) -> PathBuf {
        store::dir(&self.data_dir)
    }

    /// Takes a snapshot with the page's selection on a thread; `poll_snapshot` finishes it.
    pub fn start_snapshot(&mut self, auto: bool) {
        if self.snapshot_job.is_some() {
            return;
        }
        let paths = self.paths.clone();
        let data_dir = self.data_dir.clone();
        let selection = self.settings.snapshots.selection.clone();
        self.last_snapshot = None;
        self.snapshot_job = Some(SnapshotJob::spawn(
            JobKind::Take { auto },
            move |progress| take_and_compare(&paths, &data_dir, &selection, progress),
        ));
    }

    pub fn start_compare(&mut self, old: PathBuf, new: PathBuf) {
        if self.snapshot_job.is_some() {
            return;
        }
        self.last_snapshot = None;
        self.snapshot_job = Some(SnapshotJob::spawn(JobKind::Compare, move |_| {
            Ok(SnapshotDone {
                taken: None,
                diff: Some(compare_and_write(&old, &new)?),
            })
        }));
    }

    /// Drains the job's messages; when it is done, the listing and the status line follow.
    pub fn poll_snapshot(&mut self) {
        let Some(job) = &mut self.snapshot_job else {
            return;
        };
        let mut done = None;
        for msg in job.rx.try_iter() {
            match msg {
                Msg::Progress(p) => job.progress = p,
                Msg::Done(result) => done = Some(*result),
            }
        }
        let Some(result) = done else {
            return;
        };
        let kind = job.kind;
        self.snapshot_job = None;
        self.refresh_snapshots();
        self.status = Some(match &result {
            Ok(done) => {
                let prefix = match kind {
                    JobKind::Take { auto: true } => "Deadlock updated. ",
                    _ => "",
                };
                Status::Info(format!("{prefix}{}", done.summary()))
            }
            Err(e) if e == "cancelled" => Status::Info("Snapshot cancelled.".into()),
            Err(e) => Status::Error(format!("snapshot: {e}")),
        });
        if let Ok(done) = &result
            && done.diff.is_some()
        {
            self.latest_diff = done.diff.clone();
        }
        self.last_snapshot = Some(result);
    }

    /// Re-reads the snapshot folders, the newest report and the game's build id.
    pub fn refresh_snapshots(&mut self) {
        self.snapshots = store::list(&self.snapshot_dir()).unwrap_or_default();
        self.latest_diff = self
            .snapshots
            .iter()
            .find_map(|s| diff::load_latest(&s.folder));
        self.game_build = locate::buildid(&self.paths);
        let complete = self.snapshots.iter().filter(|s| s.complete).count();
        let (old, new) = self.ui.snapshot_compare;
        if old >= self.snapshots.len() || new >= self.snapshots.len() || old == new {
            self.ui.snapshot_compare = (usize::from(complete > 1), 0);
        }
    }

    /// The pak tree classified, scanned once per game build.
    pub fn snapshot_inventory(&mut self) -> Result<&Inventory, String> {
        if self.snapshot_inventory.is_none() {
            self.snapshot_inventory = Some(
                Inventory::scan(&self.paths)
                    .map_err(|e| crate::friendly::human_error(&e.to_string())),
            );
        }
        self.snapshot_inventory
            .as_ref()
            .expect("set above")
            .as_ref()
            .map_err(Clone::clone)
    }

    /// Files a DeadTune feature reads that the latest report says changed.
    pub fn snapshot_impact_count(&self) -> usize {
        self.latest_diff.as_ref().map_or(0, |d| d.impact().count())
    }

    pub fn delete_snapshot(&mut self, folder: &Path) -> Result<(), String> {
        store::delete(folder).map_err(|e| e.to_string())?;
        self.refresh_snapshots();
        Ok(())
    }

    /// Screenshot lever: a job that reports progress and only ends when cancelled.
    pub fn inject_snapshot_running(&mut self) {
        let fake = SnapshotProgress {
            done: 437,
            total: 1203,
            path: "panorama/layout/popups/popup_settings.vxml_c".into(),
            bytes: 14 << 20,
        };
        let tick = fake.clone();
        let mut job = SnapshotJob::spawn(JobKind::Take { auto: false }, move |progress| {
            loop {
                std::thread::sleep(Duration::from_millis(100));
                if progress(tick.clone()).is_break() {
                    return Err("cancelled".into());
                }
            }
        });
        job.progress = fake;
        self.snapshot_job = Some(job);
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::time::Instant;

    use dt_core::hud::vpk;
    use dt_core::watch::Change;

    use super::*;
    use crate::state::testutil::state;

    const STYLE: &[u8] =
        include_bytes!("../../dt-core/tests/fixtures/hud/hud_abilities_small.vcss_c");
    const TOP_BAR: &[u8] =
        include_bytes!("../../dt-core/tests/fixtures/hud/top_bar_vanilla.vxml_c");

    fn write_pak(state: &AppState, top_bar: &[u8]) {
        let files = BTreeMap::from([
            ("panorama/styles/hud.vcss_c".to_string(), STYLE.to_vec()),
            (
                "panorama/layout/citadel_hud_top_bar.vxml_c".to_string(),
                top_bar.to_vec(),
            ),
        ]);
        std::fs::write(
            state.paths.citadel_dir.join("pak01_dir.vpk"),
            vpk::write(&files),
        )
        .unwrap();
    }

    fn write_build(state: &AppState, buildid: &str) {
        let acf = format!("\"AppState\"\n{{\n\t\"buildid\"\t\t\"{buildid}\"\n}}\n");
        let steamapps = state.paths.game_root.parent().unwrap().parent().unwrap();
        std::fs::write(steamapps.join("appmanifest_1422450.acf"), acf).unwrap();
    }

    fn finish(state: &mut AppState) {
        let deadline = Instant::now() + Duration::from_secs(20);
        while state.snapshot_job.is_some() {
            assert!(Instant::now() < deadline, "snapshot job never finished");
            state.poll_snapshot();
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    #[test]
    fn take_then_take_again_lists_both_and_compares_them() {
        let (_dir, mut state) = state();
        write_pak(&state, TOP_BAR);
        write_build(&state, "100");
        state.paths = dt_core::locate::from_game_root(&state.paths.game_root).unwrap();
        assert!(state.snapshots.is_empty());
        assert_eq!(state.snapshot_inventory().unwrap().files.len(), 5);

        state.start_snapshot(false);
        assert!(state.snapshot_job.is_some());
        finish(&mut state);
        let done = state.last_snapshot.clone().unwrap().unwrap();
        let taken = done.taken.as_ref().unwrap();
        assert_eq!(
            (taken.files, taken.written, taken.buildid.as_deref()),
            (5, 5, Some("100"))
        );
        assert!(done.diff.is_none());
        assert_eq!(state.snapshots.len(), 1);
        assert_eq!(state.game_build.as_deref(), Some("100"));
        assert!(
            matches!(&state.status, Some(Status::Info(s)) if s.starts_with("Snapshot saved: 5 files") && s.contains("Nothing to compare")),
            "{:?}",
            state.status
        );
        assert_eq!(state.snapshot_impact_count(), 0);

        let changed = dt_core::hud::inject::patched_layout(
            TOP_BAR,
            &dt_core::hud::inject::LayoutEdit {
                script_includes: vec!["s2r://x.vjs_c".into()],
                ..Default::default()
            },
            &[],
            "",
        )
        .unwrap();
        write_pak(&state, &changed);
        write_build(&state, "101");
        state.on_changes(&[Change::GameUpdated {
            from: Some("100".into()),
            to: Some("101".into()),
        }]);
        assert!(
            state.snapshot_job.is_some(),
            "auto snapshot after an update"
        );
        assert!(
            state.snapshot_inventory.is_none(),
            "inventory dropped on update"
        );
        finish(&mut state);
        let done = state.last_snapshot.clone().unwrap().unwrap();
        let diff = done.diff.as_ref().unwrap();
        assert_eq!(diff.title(), "Build 100 to 101");
        assert_eq!(state.snapshots.len(), 2);
        assert_eq!(state.snapshots[0].reports, ["diff-100-to-101.md"]);
        assert_eq!(state.latest_diff.as_ref(), Some(diff));
        assert_eq!(state.snapshot_impact_count(), 1);
        assert_eq!(state.ui.snapshot_compare, (1, 0));
        assert!(
            matches!(&state.status, Some(Status::Info(s)) if s.starts_with("Deadlock updated. Snapshot saved") && s.contains("1 DeadTune feature affected")),
            "{:?}",
            state.status
        );

        state.start_compare(
            state.snapshots[1].folder.clone(),
            state.snapshots[0].folder.clone(),
        );
        finish(&mut state);
        let done = state.last_snapshot.clone().unwrap().unwrap();
        assert!(done.taken.is_none());
        assert_eq!(done.diff.as_ref().map(|d| d.files.len()), Some(1));

        state
            .delete_snapshot(&state.snapshots[0].folder.clone())
            .unwrap();
        assert_eq!(state.snapshots.len(), 1);
        assert_eq!(state.latest_diff, None);
        assert_eq!(state.ui.snapshot_compare, (0, 0));
    }

    #[test]
    fn auto_off_and_missing_pak() {
        let (_dir, mut state) = state();
        state.settings.snapshots.auto = false;
        state.on_changes(&[Change::GameUpdated {
            from: None,
            to: Some("9".into()),
        }]);
        assert!(state.snapshot_job.is_none());

        state.start_snapshot(false);
        finish(&mut state);
        assert!(matches!(&state.last_snapshot, Some(Err(e)) if e.contains("not found")));
        assert!(matches!(&state.status, Some(Status::Error(_))));
        assert!(state.snapshot_inventory().is_err());
    }

    #[test]
    fn fake_running_job_ends_on_cancel() {
        let (_dir, mut state) = state();
        state.inject_snapshot_running();
        let job = state.snapshot_job.as_ref().unwrap();
        assert_eq!((job.progress.done, job.progress.total), (437, 1203));
        job.cancel();
        finish(&mut state);
        assert_eq!(
            state.status,
            Some(Status::Info("Snapshot cancelled.".into()))
        );
        assert!(matches!(&state.last_snapshot, Some(Err(e)) if e == "cancelled"));
    }
}
