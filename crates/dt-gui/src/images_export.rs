//! "Save all images": every UI image exported as PNG and SVG on a thread
//! (`dt_core::snapshot::images::export_all`), with progress, cancel and a finish summary.
//! The UI images and Game files pages both start it; `images_export_view` draws it.

use std::ops::ControlFlow;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, channel};
use std::time::Duration;

use dt_core::snapshot::images::{self, ExportOptions, ImagesManifest};

use crate::images::ImageSource;
use crate::snapshots::SnapshotProgress;
use crate::state::AppState;

/// The export's state on the page: the running job, the last result, the zip choice.
#[derive(Default)]
pub struct ExportAll {
    pub job: Option<ExportJob>,
    pub last: Option<Result<ExportDone, String>>,
    pub zip: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExportDone {
    pub folder: PathBuf,
    pub zip: Option<PathBuf>,
    pub manifest: ImagesManifest,
    pub elapsed: Duration,
}

impl ExportDone {
    pub fn failures_file(&self) -> PathBuf {
        self.folder.join(images::FAILURES)
    }

    /// One sentence for the page and the status line.
    pub fn summary(&self) -> String {
        let m = &self.manifest;
        let mut out = format!(
            "Saved {} of {} images in {:.1} s.",
            m.exported,
            m.total,
            self.elapsed.as_secs_f64()
        );
        if m.failed > 0 {
            out.push_str(&format!(
                " {} couldn't be saved; failures.txt in the folder says why.",
                m.failed
            ));
        }
        if self.zip.is_some() {
            out.push_str(" A zip of the folder is next to it.");
        }
        out
    }
}

enum Msg {
    Progress(SnapshotProgress),
    Done(Box<Result<ExportDone, String>>),
}

pub struct ExportJob {
    rx: Receiver<Msg>,
    cancel: Arc<AtomicBool>,
    /// Below `panorama/images/`, when only one folder is exported.
    pub folder: Option<String>,
    pub progress: SnapshotProgress,
}

impl ExportJob {
    /// The thread stops after the image it is on; a folder it created is removed.
    pub fn cancel(&self) {
        self.cancel.store(true, Ordering::Relaxed);
    }

    fn spawn(
        folder: Option<String>,
        work: impl FnOnce(
            &mut dyn FnMut(SnapshotProgress) -> ControlFlow<()>,
        ) -> Result<ExportDone, String>
        + Send
        + 'static,
    ) -> ExportJob {
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
        ExportJob {
            rx,
            cancel,
            folder,
            progress: SnapshotProgress::default(),
        }
    }
}

impl AppState {
    /// Exports every image of the page's source (or those under `folder`) on a thread into
    /// `<data>/exports/ui-images-<build>-<date>/`; `poll_images_export` finishes it.
    pub fn start_images_export(&mut self, folder: Option<String>) {
        if self.images.export_all.job.is_some() {
            return;
        }
        self.load_images();
        match self.images.library.as_ref() {
            Some(Ok(library)) => self.export_images_from(library.source.clone(), folder),
            Some(Err(e)) => self.images.export_all.last = Some(Err(e.clone())),
            None => {}
        }
    }

    /// `start_images_export` for a page that holds the library itself.
    pub fn export_images_from(&mut self, source: Arc<ImageSource>, folder: Option<String>) {
        if self.images.export_all.job.is_some() {
            return;
        }
        let buildid = source.buildid(&self.paths);
        let out = images::default_folder(&self.data_dir, buildid.as_deref(), folder.as_deref());
        let options = ExportOptions {
            folder: folder.clone(),
            zip: self.images.export_all.zip,
        };
        self.images.export_all.last = None;
        self.images.export_all.job = Some(ExportJob::spawn(folder, move |progress| {
            let done = images::export_all(&source, &out, buildid, &options, &mut |p| {
                progress(SnapshotProgress {
                    done: p.done,
                    total: p.total,
                    path: p.path.to_string(),
                    bytes: p.bytes,
                })
            })
            .map_err(|e| e.to_string())?;
            Ok(ExportDone {
                folder: done.folder,
                zip: done.zip,
                manifest: done.manifest,
                elapsed: done.elapsed,
            })
        }));
    }

    pub fn cancel_images_export(&self) {
        if let Some(job) = &self.images.export_all.job {
            job.cancel();
        }
    }

    pub fn images_export_running(&self) -> bool {
        self.images.export_all.job.is_some()
    }

    /// Drains the job's messages; when it is done, the result and the status line follow.
    pub fn poll_images_export(&mut self) {
        let Some(job) = &mut self.images.export_all.job else {
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
        self.images.export_all.job = None;
        self.status = Some(match &result {
            Ok(done) => crate::state::Status::Info(done.summary()),
            Err(e) if e == "cancelled" => {
                crate::state::Status::Info("Saving the images was cancelled.".into())
            }
            Err(e) => crate::state::Status::Error(format!("Couldn't save the images: {e}")),
        });
        self.images.export_all.last = Some(result);
    }

    /// Waits for the running export, polling it (screenshot lever and tests).
    pub fn wait_images_export(&mut self) {
        let deadline = std::time::Instant::now() + Duration::from_secs(120);
        while self.images_export_running() && std::time::Instant::now() < deadline {
            self.poll_images_export();
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    /// Screenshot lever: an export of the game's 2706 images that only ends when cancelled.
    pub fn inject_images_export_running(&mut self) {
        let fake = SnapshotProgress {
            done: 1204,
            total: 2706,
            path: "panorama/images/heroes/inferno_card_psd.vtex_c".into(),
            bytes: 61 << 20,
        };
        let tick = fake.clone();
        let mut job = ExportJob::spawn(None, move |progress| {
            loop {
                std::thread::sleep(Duration::from_millis(100));
                if progress(tick.clone()).is_break() {
                    return Err("cancelled".into());
                }
            }
        });
        job.progress = fake;
        self.images.export_all.job = Some(job);
    }
}

#[cfg(test)]
mod tests {
    use dt_core::snapshot::images::{ImageKind, MANIFEST_JSON};

    use super::*;
    use crate::images::tests::{BROKEN, ITEM, TEXTURE, TOP_BAR, install_images};
    use crate::state::Status;
    use crate::state::testutil;

    fn finish(state: &mut AppState) {
        state.wait_images_export();
        assert!(!state.images_export_running(), "export never finished");
    }

    #[test]
    fn saves_every_image_with_progress_a_manifest_and_failures() {
        let (_dir, mut state) = testutil::state();
        install_images(&state);
        state.images.export_all.zip = true;
        state.start_images_export(None);
        assert!(state.images_export_running());
        state.start_images_export(Some("hud".into()));
        assert_eq!(
            state.images.export_all.job.as_ref().unwrap().folder,
            None,
            "a second start while one runs is ignored"
        );
        finish(&mut state);
        let done = state.images.export_all.last.clone().unwrap().unwrap();
        let m = &done.manifest;
        assert_eq!((m.total, m.exported, m.failed), (4, 3, 1));
        assert!(
            done.folder.starts_with(state.data_dir.join("exports")),
            "{}",
            done.folder.display()
        );
        let name = done.folder.file_name().unwrap().to_string_lossy();
        assert!(name.starts_with("ui-images-unknown-"), "{name}");
        assert_eq!(&ImagesManifest::load(&done.folder).unwrap(), m);
        let paths: Vec<&str> = m.images.iter().map(|i| i.path.as_str()).collect();
        assert_eq!(paths, [BROKEN, TOP_BAR, ITEM, TEXTURE]);
        let texture = m.image(TEXTURE).unwrap();
        assert_eq!(
            (texture.kind, texture.width, texture.height),
            (ImageKind::Texture, Some(32), Some(16))
        );
        assert_eq!(texture.files, ["panorama/images/minimap/hero_ally_psd.png"]);
        let icon = m.image(TOP_BAR).unwrap();
        assert_eq!(
            icon.files,
            [
                "panorama/images/hud/top_bar/icon_ultimate.svg",
                "panorama/images/hud/top_bar/icon_ultimate.png"
            ]
        );
        for image in m.images.iter().filter(|i| i.error.is_none()) {
            for file in &image.files {
                assert!(done.folder.join(file).is_file(), "{file}");
            }
            assert_eq!(image.sha256.as_ref().map(String::len), Some(64));
        }
        let failures = std::fs::read_to_string(done.failures_file()).unwrap();
        assert!(failures.contains(BROKEN), "{failures}");
        assert!(done.folder.join(MANIFEST_JSON).is_file());
        assert!(done.zip.as_ref().unwrap().is_file());
        assert!(
            matches!(&state.status, Some(Status::Info(s)) if s.starts_with("Saved 3 of 4 images") && s.contains("1 couldn't be saved") && s.contains("zip")),
            "{:?}",
            state.status
        );
    }

    #[test]
    fn one_folder_and_a_missing_game() {
        let (_dir, mut state) = testutil::state();
        install_images(&state);
        state.start_images_export(Some("hud/top_bar".into()));
        finish(&mut state);
        let done = state.images.export_all.last.clone().unwrap().unwrap();
        assert_eq!(done.manifest.total, 1);
        assert!(done.folder.to_string_lossy().ends_with("-hud-top_bar"));
        assert!(done.zip.is_none());

        let (_dir, mut state) = testutil::state();
        state.start_images_export(None);
        assert!(!state.images_export_running());
        assert!(
            matches!(&state.images.export_all.last, Some(Err(e)) if e.starts_with("Couldn't read the game's images"))
        );
    }

    #[test]
    fn cancel_stops_it_and_says_so() {
        let (_dir, mut state) = testutil::state();
        state.inject_images_export_running();
        let job = state.images.export_all.job.as_ref().unwrap();
        assert_eq!((job.progress.done, job.progress.total), (1204, 2706));
        state.cancel_images_export();
        finish(&mut state);
        assert_eq!(
            state.status,
            Some(Status::Info("Saving the images was cancelled.".into()))
        );
        assert!(matches!(&state.images.export_all.last, Some(Err(e)) if e == "cancelled"));
    }

    #[test]
    fn cancelling_a_real_export_leaves_no_folder() {
        let (_dir, mut state) = testutil::state();
        install_images(&state);
        state.start_images_export(None);
        state.cancel_images_export();
        finish(&mut state);
        assert!(matches!(&state.images.export_all.last, Some(Err(e)) if e == "cancelled"));
        let left = std::fs::read_dir(state.data_dir.join("exports"))
            .map(|d| d.count())
            .unwrap_or(0);
        assert_eq!(left, 0, "the half-made folder is removed");
    }
}
