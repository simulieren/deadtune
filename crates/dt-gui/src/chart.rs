//! Frametime chart drawn with the egui painter (egui_plot would add a dependency for one chart).

use eframe::egui::{self, Color32, Pos2, Stroke};

use crate::bench::{Run, downsample};

const COLORS: [Color32; 2] = [
    Color32::from_rgb(110, 170, 250),
    Color32::from_rgb(250, 160, 80),
];

/// The last two imported runs overlaid, x = frame position (normalized), y = frametime in ms.
pub fn frametimes(ui: &mut egui::Ui, runs: &[Run]) {
    let shown = &runs[runs.len().saturating_sub(2)..];
    let width = ui.available_width().min(900.0);
    let (rect, _) = ui.allocate_exact_size(egui::vec2(width, 180.0), egui::Sense::hover());
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 4.0, ui.visuals().extreme_bg_color);
    let max_ms = shown
        .iter()
        .flat_map(|r| r.capture.frametimes_ms.iter().copied())
        .fold(1.0_f64, f64::max)
        .min(100.0);
    let y = |ms: f64| rect.bottom() - (ms.min(max_ms) / max_ms) as f32 * (rect.height() - 14.0);
    for target_fps in [60.0, 144.0] {
        let ms = 1000.0 / target_fps;
        if ms < max_ms {
            let yy = y(ms);
            painter.hline(rect.x_range(), yy, Stroke::new(1.0, Color32::from_gray(70)));
            painter.text(
                Pos2::new(rect.left() + 4.0, yy - 2.0),
                egui::Align2::LEFT_BOTTOM,
                format!("{target_fps:.0} fps"),
                egui::FontId::proportional(10.0),
                Color32::GRAY,
            );
        }
    }
    for (i, run) in shown.iter().enumerate() {
        let points = downsample(&run.capture.frametimes_ms, rect.width() as usize);
        let n = points.len().max(2) - 1;
        let line: Vec<Pos2> = points
            .iter()
            .enumerate()
            .map(|(x, ms)| Pos2::new(rect.left() + rect.width() * x as f32 / n as f32, y(*ms)))
            .collect();
        painter.add(egui::Shape::line(line, Stroke::new(1.2, COLORS[i])));
        painter.text(
            Pos2::new(rect.right() - 6.0, rect.top() + 4.0 + 12.0 * i as f32),
            egui::Align2::RIGHT_TOP,
            &run.label,
            egui::FontId::proportional(11.0),
            COLORS[i],
        );
    }
    painter.text(
        Pos2::new(rect.left() + 4.0, rect.top() + 4.0),
        egui::Align2::LEFT_TOP,
        format!("frametime, max {max_ms:.1} ms"),
        egui::FontId::proportional(10.0),
        Color32::GRAY,
    );
}
