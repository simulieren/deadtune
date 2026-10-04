//! Frametime CSV import (PresentMon on Windows, MangoHud on Linux), metrics and per-profile history.

use std::collections::HashMap;
use std::fs;
use std::io::{ErrorKind, Write};
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CaptureTool {
    PresentMon,
    MangoHud,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Capture {
    pub tool: CaptureTool,
    pub frametimes_ms: Vec<f64>,
}

#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Metrics {
    pub frames: usize,
    pub duration_s: f64,
    pub avg_fps: f64,
    /// Average FPS of the slowest 1% / 0.1% of frames. See [`metrics`] for the exact definition.
    pub low_1_fps: f64,
    pub low_01_fps: f64,
}

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct BenchRecord {
    pub profile: String,
    pub label: String,
    pub recorded: chrono::DateTime<chrono::Utc>,
    pub metrics: Metrics,
    pub screenshots: Vec<PathBuf>,
}

/// Percent change from a baseline to a candidate; positive means the candidate is faster.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Delta {
    pub avg_fps_pct: f64,
    pub low_1_pct: f64,
    pub low_01_pct: f64,
}

#[derive(Debug, thiserror::Error)]
pub enum BenchError {
    #[error("csv: {0}")]
    Csv(#[from] csv::Error),
    #[error("unrecognised capture format")]
    UnknownFormat,
    #[error("capture has no frames")]
    Empty,
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("history file: {0}")]
    HistoryRead(#[from] toml::de::Error),
    #[error("history file: {0}")]
    HistoryWrite(#[from] toml::ser::Error),
}

/// Where a frame's duration lives in a given CSV dialect.
#[derive(Clone, Copy, Debug)]
enum FrametimeColumn {
    Single(usize),
    /// PresentMon 2.0 to 2.2 has no frame interval column; CPUBusy + CPUWait spans CPUStart to CPUStart.
    Sum(usize, usize),
}

struct Layout {
    tool: CaptureTool,
    frametime: FrametimeColumn,
    /// PresentMon records every presenting process unless filtered; keep only the busiest one.
    application: Option<usize>,
}

/// Picks the column that measures CPU frame pacing (present to present), not display pacing:
/// `MsBetweenDisplayChange` is 0 or NA for dropped frames and would hide stutter in the CPU.
///
/// - PresentMon 1.x and 2.3+ console: `MsBetweenPresents`.
/// - PresentMon 2.0 to 2.2 console and the PresentMon app: `FrameTime`, else `CPUBusy` + `CPUWait`.
/// - MangoHud: `fps,frametime,...` with frametime in ms (`frametime_ns / 1e6` in overlay.cpp).
fn layout(headers: &csv::StringRecord) -> Option<Layout> {
    let col = |name: &str| headers.iter().position(|h| h == name);
    if headers.get(0) == Some("fps") && headers.get(1) == Some("frametime") {
        return Some(Layout {
            tool: CaptureTool::MangoHud,
            frametime: FrametimeColumn::Single(1),
            application: None,
        });
    }
    let application = col("Application")?;
    let frametime = col("MsBetweenPresents")
        .or_else(|| col("FrameTime"))
        .map(FrametimeColumn::Single)
        .or_else(|| Some(FrametimeColumn::Sum(col("CPUBusy")?, col("CPUWait")?)))
        .or_else(|| Some(FrametimeColumn::Sum(col("MsCPUBusy")?, col("MsCPUWait")?)))?;
    Some(Layout {
        tool: CaptureTool::PresentMon,
        frametime,
        application: Some(application),
    })
}

fn frametime(record: &csv::StringRecord, column: FrametimeColumn) -> Option<f64> {
    let num = |i: usize| record.get(i)?.parse::<f64>().ok();
    let ms = match column {
        FrametimeColumn::Single(i) => num(i)?,
        FrametimeColumn::Sum(a, b) => num(a)? + num(b)?,
    };
    (ms.is_finite() && ms > 0.0).then_some(ms)
}

/// Detects the tool from the header row. MangoHud logs put system info (and optionally a
/// version banner) above the frame header, so the first few lines are scanned for it.
/// Rows whose frametime is `NA`, zero or unparseable are skipped.
pub fn parse_capture(csv: &str) -> Result<Capture, BenchError> {
    let csv = csv.strip_prefix('\u{feff}').unwrap_or(csv);
    let mut offset = 0;
    for line in csv.split_inclusive('\n').take(10) {
        let body = &csv[offset..];
        offset += line.len();
        let mut reader = csv::ReaderBuilder::new()
            .flexible(true)
            .trim(csv::Trim::All)
            .from_reader(body.as_bytes());
        let Some(layout) = layout(reader.headers()?) else {
            continue;
        };
        let mut rows = Vec::new();
        for record in reader.records() {
            let record = record?;
            if let Some(ms) = frametime(&record, layout.frametime) {
                let app = layout
                    .application
                    .and_then(|i| record.get(i))
                    .unwrap_or_default()
                    .to_owned();
                rows.push((app, ms));
            }
        }
        let busiest = busiest_application(&rows);
        let frametimes_ms: Vec<f64> = rows
            .iter()
            .filter(|(app, _)| Some(app.as_str()) == busiest)
            .map(|&(_, ms)| ms)
            .collect();
        if frametimes_ms.is_empty() {
            return Err(BenchError::Empty);
        }
        return Ok(Capture {
            tool: layout.tool,
            frametimes_ms,
        });
    }
    Err(BenchError::UnknownFormat)
}

fn busiest_application(rows: &[(String, f64)]) -> Option<&str> {
    let mut counts: HashMap<&str, usize> = HashMap::new();
    for (app, _) in rows {
        *counts.entry(app).or_default() += 1;
    }
    counts
        .into_iter()
        .max_by(|a, b| a.1.cmp(&b.1).then(b.0.cmp(a.0)))
        .map(|(app, _)| app)
}

/// - `duration_s` is the sum of all frametimes.
/// - `avg_fps` is `frames / duration_s`.
/// - `low_1_fps` / `low_01_fps`: sort frames by frametime, take the slowest `ceil(frames / 100)`
///   (resp. `ceil(frames / 1000)`, at least one), and report `count / sum_of_their_seconds`.
///   That is the FPS the game would run at if every frame were as slow as those, the same
///   frames-over-time average as `avg_fps`, not the frametime at the 99th percentile.
pub fn metrics(capture: &Capture) -> Result<Metrics, BenchError> {
    let frames = capture.frametimes_ms.len();
    if frames == 0 {
        return Err(BenchError::Empty);
    }
    let mut slowest_first = capture.frametimes_ms.clone();
    slowest_first.sort_by(|a, b| b.total_cmp(a));
    let fps_of = |ms: &[f64]| ms.len() as f64 * 1000.0 / ms.iter().sum::<f64>();
    let duration_s = slowest_first.iter().sum::<f64>() / 1000.0;
    Ok(Metrics {
        frames,
        duration_s,
        avg_fps: fps_of(&slowest_first),
        low_1_fps: fps_of(&slowest_first[..frames.div_ceil(100)]),
        low_01_fps: fps_of(&slowest_first[..frames.div_ceil(1000)]),
    })
}

/// Percent change from `a` (baseline) to `b`.
pub fn compare(a: &Metrics, b: &Metrics) -> Delta {
    let pct = |from: f64, to: f64| (to - from) / from * 100.0;
    Delta {
        avg_fps_pct: pct(a.avg_fps, b.avg_fps),
        low_1_pct: pct(a.low_1_fps, b.low_1_fps),
        low_01_pct: pct(a.low_01_fps, b.low_01_fps),
    }
}

#[derive(Default, serde::Serialize, serde::Deserialize)]
struct History {
    #[serde(default)]
    record: Vec<BenchRecord>,
}

/// Profile names are free text; the file name keeps only characters safe on every filesystem.
/// Two names that sanitize alike share a file, which is harmless because loads filter by profile.
fn history_path(dir: &Path, profile: &str) -> PathBuf {
    let stem: String = profile
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect();
    dir.join(format!("{stem}.bench.toml"))
}

fn read_history(path: &Path) -> Result<History, BenchError> {
    match fs::read_to_string(path) {
        Ok(text) => Ok(toml::from_str(&text)?),
        Err(e) if e.kind() == ErrorKind::NotFound => Ok(History::default()),
        Err(e) => Err(e.into()),
    }
}

/// Appends to `<dir>/<profile>.bench.toml`, written via temp file + rename.
pub fn save_record(dir: &Path, record: &BenchRecord) -> Result<(), BenchError> {
    fs::create_dir_all(dir)?;
    let path = history_path(dir, &record.profile);
    let mut history = read_history(&path)?;
    history.record.push(record.clone());
    let tmp = path.with_extension("toml.tmp");
    let mut file = fs::File::create(&tmp)?;
    file.write_all(toml::to_string(&history)?.as_bytes())?;
    file.sync_all()?;
    drop(file);
    fs::rename(&tmp, &path)?;
    Ok(())
}

/// Records for `profile`, newest first. A profile with no history yields an empty list.
pub fn load_records(dir: &Path, profile: &str) -> Result<Vec<BenchRecord>, BenchError> {
    let mut records: Vec<BenchRecord> = read_history(&history_path(dir, profile))?
        .record
        .into_iter()
        .filter(|r| r.profile == profile)
        .collect();
    records.sort_by_key(|r| std::cmp::Reverse(r.recorded));
    Ok(records)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn fixture(name: &str) -> String {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("testdata/bench")
            .join(name);
        fs::read_to_string(path).unwrap()
    }

    fn assert_close(actual: f64, expected: f64, what: &str) {
        assert!(
            (actual - expected).abs() < 1e-9,
            "{what}: got {actual}, want {expected}"
        );
    }

    #[test]
    fn presentmon_1x_uses_ms_between_presents_and_drops_other_processes() {
        let c = parse_capture(&fixture("presentmon_1x.csv")).unwrap();
        assert_eq!(c.tool, CaptureTool::PresentMon);
        assert_eq!(
            c.frametimes_ms,
            [10.0, 10.0, 20.0, 10.0],
            "dwm.exe rows ignored"
        );
    }

    #[test]
    fn presentmon_1x_with_crlf_line_endings() {
        let crlf = fixture("presentmon_1x.csv").replace('\n', "\r\n");
        let c = parse_capture(&crlf).unwrap();
        assert_eq!(c.frametimes_ms, [10.0, 10.0, 20.0, 10.0]);
    }

    #[test]
    fn presentmon_2x_uses_frametime_and_skips_na() {
        let c = parse_capture(&fixture("presentmon_2x_frametime.csv")).unwrap();
        assert_eq!(c.tool, CaptureTool::PresentMon);
        assert_eq!(c.frametimes_ms, [8.333, 8.333, 25.0, 8.334]);
    }

    #[test]
    fn presentmon_2_0_sums_cpu_busy_and_wait() {
        let c = parse_capture(&fixture("presentmon_2_0_cpubusy.csv")).unwrap();
        assert_eq!(c.frametimes_ms, [15.0, 15.0, 30.0]);
    }

    #[test]
    fn presentmon_2x_current_prefers_ms_between_presents_over_display_change() {
        let c = parse_capture(&fixture("presentmon_2x_current.csv")).unwrap();
        assert_eq!(c.frametimes_ms, [6.944, 6.944, 13.889]);
    }

    #[test]
    fn mangohud_skips_system_info_block() {
        let c = parse_capture(&fixture("mangohud.csv")).unwrap();
        assert_eq!(c.tool, CaptureTool::MangoHud);
        assert_eq!(c.frametimes_ms, [16.666, 16.666, 33.332, 16.666]);
    }

    #[test]
    fn mangohud_with_log_versioning_banner() {
        let c = parse_capture(&fixture("mangohud_versioned.csv")).unwrap();
        assert_eq!(c.tool, CaptureTool::MangoHud);
        assert_eq!(c.frametimes_ms, [25.0, 20.0]);
    }

    #[test]
    fn mangohud_summary_file_is_not_a_capture() {
        let summary =
            "0.1% Min FPS,1% Min FPS,97% Percentile FPS,Average FPS\n41.2,55.0,140.1,118.3\n";
        assert!(matches!(
            parse_capture(summary),
            Err(BenchError::UnknownFormat)
        ));
    }

    #[test]
    fn unknown_and_header_only_inputs() {
        assert!(matches!(
            parse_capture("a,b,c\n1,2,3\n"),
            Err(BenchError::UnknownFormat)
        ));
        assert!(matches!(parse_capture(""), Err(BenchError::UnknownFormat)));
        assert!(matches!(
            parse_capture("Application,MsBetweenPresents\n"),
            Err(BenchError::Empty)
        ));
    }

    #[test]
    fn metrics_hand_computed() {
        let mut frametimes_ms = vec![10.0; 990];
        frametimes_ms.extend([20.0; 9]);
        frametimes_ms.push(50.0);
        let m = metrics(&Capture {
            tool: CaptureTool::PresentMon,
            frametimes_ms,
        })
        .unwrap();
        assert_eq!(m.frames, 1000);
        assert_close(m.duration_s, 10.13, "9900 + 180 + 50 ms");
        assert_close(m.avg_fps, 1000.0 / 10.13, "avg");
        assert_close(
            m.low_1_fps,
            10.0 / 0.230,
            "slowest 10 frames: 50 + 9 * 20 ms",
        );
        assert_close(m.low_01_fps, 20.0, "slowest frame: 50 ms");
    }

    #[test]
    fn metrics_rounds_slow_frame_count_up() {
        let m = metrics(&Capture {
            tool: CaptureTool::MangoHud,
            frametimes_ms: vec![10.0, 10.0, 40.0],
        })
        .unwrap();
        assert_close(m.avg_fps, 50.0, "3 frames in 60 ms");
        assert_close(m.low_1_fps, 25.0, "one frame is the 1% of three");
        assert_close(m.low_01_fps, 25.0, "one frame is the 0.1% of three");
    }

    #[test]
    fn metrics_of_empty_capture_is_an_error() {
        let empty = Capture {
            tool: CaptureTool::MangoHud,
            frametimes_ms: vec![],
        };
        assert!(matches!(metrics(&empty), Err(BenchError::Empty)));
    }

    fn sample_metrics(avg: f64, low1: f64, low01: f64) -> Metrics {
        Metrics {
            frames: 1000,
            duration_s: 10.0,
            avg_fps: avg,
            low_1_fps: low1,
            low_01_fps: low01,
        }
    }

    #[test]
    fn compare_reports_percent_change_from_baseline() {
        let d = compare(
            &sample_metrics(100.0, 50.0, 40.0),
            &sample_metrics(110.0, 45.0, 40.0),
        );
        assert_close(d.avg_fps_pct, 10.0, "avg");
        assert_close(d.low_1_pct, -10.0, "1% low");
        assert_close(d.low_01_pct, 0.0, "0.1% low");
    }

    fn record(profile: &str, label: &str, day: u32) -> BenchRecord {
        BenchRecord {
            profile: profile.into(),
            label: label.into(),
            recorded: chrono::Utc
                .with_ymd_and_hms(2026, 10, day, 12, 0, 0)
                .unwrap(),
            metrics: sample_metrics(100.0 + day as f64, 50.25, 40.125),
            screenshots: vec![PathBuf::from("shots/a.jpg")],
        }
    }

    #[test]
    fn history_round_trips_newest_first_per_profile() {
        let dir = tempfile::tempdir().unwrap();
        let older = record("Kaiz + farz 6000", "battery", 1);
        let newer = record("Kaiz + farz 6000", "plugged", 3);
        let other = record("vanilla", "baseline", 2);
        save_record(dir.path(), &older).unwrap();
        save_record(dir.path(), &other).unwrap();
        save_record(dir.path(), &newer).unwrap();

        assert_eq!(
            load_records(dir.path(), "Kaiz + farz 6000").unwrap(),
            [newer, older]
        );
        assert_eq!(load_records(dir.path(), "vanilla").unwrap(), [other]);
    }

    #[test]
    fn profiles_whose_file_names_collide_stay_separate() {
        let dir = tempfile::tempdir().unwrap();
        let a = record("a b", "x", 1);
        let b = record("a_b", "y", 2);
        save_record(dir.path(), &a).unwrap();
        save_record(dir.path(), &b).unwrap();
        assert_eq!(load_records(dir.path(), "a b").unwrap(), [a]);
        assert_eq!(load_records(dir.path(), "a_b").unwrap(), [b]);
    }

    #[test]
    fn missing_history_is_empty_and_save_creates_dir() {
        let dir = tempfile::tempdir().unwrap();
        let nested = dir.path().join("bench");
        assert!(load_records(&nested, "p").unwrap().is_empty());
        save_record(&nested, &record("p", "x", 1)).unwrap();
        assert_eq!(load_records(&nested, "p").unwrap().len(), 1);
        let leftovers: Vec<_> = fs::read_dir(&nested)
            .unwrap()
            .map(|e| e.unwrap().file_name().into_string().unwrap())
            .collect();
        assert_eq!(leftovers, ["p.bench.toml"], "no temp file left behind");
    }
}
