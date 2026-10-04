//! Bench tab state: imported captures this session, saved history per profile, screenshots.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use dt_core::bench::{self, BenchRecord, Capture, Delta, Metrics};

pub struct Run {
    pub label: String,
    pub capture: Capture,
    pub metrics: Metrics,
}

#[derive(Default)]
pub struct BenchState {
    pub csv_path: String,
    pub label: String,
    /// Imported this session, newest last; only these have frametimes for the chart.
    pub runs: Vec<Run>,
    /// Saved records for the active profile, newest first.
    pub history: Vec<BenchRecord>,
    pub compare: [Option<usize>; 2],
    pub screenshots: Vec<PathBuf>,
    pub picked: BTreeSet<PathBuf>,
}

pub fn history_dir(data_dir: &Path) -> PathBuf {
    data_dir.join("bench")
}

impl BenchState {
    pub fn import(&mut self) -> Result<&Run, String> {
        let path = PathBuf::from(self.csv_path.trim());
        let text =
            std::fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
        let capture = bench::parse_capture(&text).map_err(|e| e.to_string())?;
        let metrics = bench::metrics(&capture).map_err(|e| e.to_string())?;
        let label = match self.label.trim() {
            "" => path
                .file_stem()
                .map_or_else(|| "run".into(), |s| s.to_string_lossy().into_owned()),
            label => label.to_string(),
        };
        self.runs.push(Run {
            label,
            capture,
            metrics,
        });
        Ok(self.runs.last().expect("just pushed"))
    }

    /// Saves run `index` under `profile`, with the picked screenshots attached.
    pub fn save_run(&mut self, index: usize, profile: &str, dir: &Path) -> Result<(), String> {
        let run = self.runs.get(index).ok_or("no such run")?;
        let record = BenchRecord {
            profile: profile.to_string(),
            label: run.label.clone(),
            recorded: chrono::Utc::now(),
            metrics: run.metrics,
            screenshots: self.picked.iter().cloned().collect(),
        };
        bench::save_record(dir, &record).map_err(|e| e.to_string())?;
        self.picked.clear();
        self.reload_history(dir, profile);
        Ok(())
    }

    pub fn reload_history(&mut self, dir: &Path, profile: &str) {
        self.history = bench::load_records(dir, profile).unwrap_or_default();
        self.compare = [None, None];
    }

    pub fn delta(&self) -> Option<Delta> {
        let [Some(a), Some(b)] = self.compare else {
            return None;
        };
        Some(bench::compare(
            &self.history.get(a)?.metrics,
            &self.history.get(b)?.metrics,
        ))
    }
}

/// Image files in `dirs`, newest first, at most `limit`.
pub fn recent_screenshots(dirs: &[PathBuf], limit: usize) -> Vec<PathBuf> {
    let mut files: Vec<(SystemTime, PathBuf)> = dirs
        .iter()
        .filter_map(|d| std::fs::read_dir(d).ok())
        .flat_map(|entries| entries.flatten())
        .filter(|e| {
            e.path().extension().is_some_and(|x| {
                let x = x.to_string_lossy().to_ascii_lowercase();
                x == "jpg" || x == "jpeg" || x == "png"
            })
        })
        .filter_map(|e| Some((e.metadata().ok()?.modified().ok()?, e.path())))
        .collect();
    files.sort_by(|a, b| b.0.cmp(&a.0));
    files.into_iter().take(limit).map(|(_, p)| p).collect()
}

/// Max per bucket, so a single stutter frame stays visible however many frames share a pixel.
pub fn downsample(frametimes: &[f64], buckets: usize) -> Vec<f64> {
    if buckets == 0 || frametimes.len() <= buckets {
        return frametimes.to_vec();
    }
    (0..buckets)
        .map(|i| {
            let start = i * frametimes.len() / buckets;
            let end = ((i + 1) * frametimes.len() / buckets).max(start + 1);
            frametimes[start..end].iter().copied().fold(0.0, f64::max)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIXTURE: &str = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../dt-core/testdata/bench/presentmon_1x.csv"
    );

    #[test]
    fn import_save_and_compare_two_runs() {
        let dir = tempfile::tempdir().unwrap();
        let mut state = BenchState {
            csv_path: FIXTURE.into(),
            ..BenchState::default()
        };
        let run = state.import().unwrap();
        assert_eq!(
            run.label, "presentmon_1x",
            "label defaults to the file stem"
        );
        assert!(run.metrics.avg_fps > 0.0);
        state.label = "farz 6000".into();
        state.import().unwrap();
        state.picked.insert(PathBuf::from("shot1.jpg"));
        state.save_run(0, "Laptop", dir.path()).unwrap();
        state.save_run(1, "Laptop", dir.path()).unwrap();
        assert_eq!(state.history.len(), 2);
        assert_eq!(
            state.history[1].screenshots,
            vec![PathBuf::from("shot1.jpg")]
        );
        assert!(
            state.history[0].screenshots.is_empty(),
            "picks clear after save"
        );
        state.compare = [Some(0), Some(1)];
        let delta = state.delta().unwrap();
        assert_eq!(delta.avg_fps_pct, 0.0, "same capture twice");
    }

    #[test]
    fn import_error_names_the_file() {
        let mut state = BenchState {
            csv_path: "/nope/capture.csv".into(),
            ..BenchState::default()
        };
        assert!(state.import().err().unwrap().contains("/nope/capture.csv"));
    }

    #[test]
    fn downsample_keeps_spikes() {
        let mut frames = vec![10.0; 1000];
        frames[517] = 80.0;
        let out = downsample(&frames, 100);
        assert_eq!(out.len(), 100);
        assert_eq!(out.iter().copied().fold(0.0, f64::max), 80.0);
        assert_eq!(downsample(&[1.0, 2.0], 10), vec![1.0, 2.0]);
    }

    #[test]
    fn recent_screenshots_newest_first_images_only() {
        let dir = tempfile::tempdir().unwrap();
        for name in ["a.jpg", "notes.txt", "b.JPG"] {
            std::fs::write(dir.path().join(name), "x").unwrap();
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        let shots = recent_screenshots(&[dir.path().to_path_buf()], 10);
        let names: Vec<_> = shots
            .iter()
            .map(|p| p.file_name().unwrap().to_string_lossy().into_owned())
            .collect();
        assert_eq!(names, ["b.JPG", "a.jpg"]);
    }
}
