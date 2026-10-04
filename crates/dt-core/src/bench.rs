//! Frametime CSV import (PresentMon on Windows, MangoHud on Linux) and metrics.

use std::path::PathBuf;

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
    /// Average FPS of the slowest 1% / 0.1% of frames.
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

#[derive(Debug, thiserror::Error)]
pub enum BenchError {
    #[error("csv: {0}")]
    Csv(#[from] csv::Error),
    #[error("unrecognised capture format")]
    UnknownFormat,
    #[error("capture has no frames")]
    Empty,
}

/// Detects the tool from the header row.
pub fn parse_capture(csv: &str) -> Result<Capture, BenchError> {
    let _ = csv;
    todo!()
}

pub fn metrics(capture: &Capture) -> Result<Metrics, BenchError> {
    let _ = capture;
    todo!()
}
