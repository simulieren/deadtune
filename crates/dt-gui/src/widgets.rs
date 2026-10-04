//! Small painted widgets for the advanced view, in the simple view's design language.

use eframe::egui::{
    self, Align2, Color32, CornerRadius, FontId, Rect, Response, RichText, Sense, Stroke,
    StrokeKind, Ui, vec2,
};

use crate::theme::{ACCENT, BORDER, CARD, CARD_HOVER, ON_ACCENT, RAIL, TEXT, WEAK, semibold};

/// Small uppercase caption above a group of rows.
pub fn caption(ui: &mut Ui, text: &str) -> Response {
    ui.label(
        RichText::new(text.to_uppercase())
            .size(10.5)
            .strong()
            .color(WEAK),
    )
}

/// Page title and a weak one-line subtitle, the same on every tab.
pub fn page_title(ui: &mut Ui, title: &str, subtitle: &str) {
    ui.label(
        RichText::new(title)
            .size(18.0)
            .strong()
            .family(semibold())
            .color(TEXT),
    );
    if !subtitle.is_empty() {
        ui.label(RichText::new(subtitle).small().color(WEAK));
    }
    ui.add_space(8.0);
}

/// Weak small help line.
pub fn hint(ui: &mut Ui, text: &str) {
    ui.label(RichText::new(text).size(11.0).color(WEAK));
}

/// A full-width card with a caption, the block every tab page is built from.
pub fn section<R>(ui: &mut Ui, title: &str, add: impl FnOnce(&mut Ui) -> R) -> R {
    card(ui, |ui| {
        caption(ui, title);
        ui.add_space(2.0);
        add(ui)
    })
}

/// A full-width card without a caption, for content that brings its own heading.
pub fn card<R>(ui: &mut Ui, add: impl FnOnce(&mut Ui) -> R) -> R {
    let inner = egui::Frame::new()
        .fill(CARD)
        .stroke(Stroke::new(1.0, BORDER))
        .corner_radius(CornerRadius::same(6))
        .inner_margin(egui::Margin::symmetric(12, 10))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            add(ui)
        })
        .inner;
    ui.add_space(10.0);
    inner
}

/// One rail entry: label left, weak count right, and an amber badge with the changed count.
pub fn nav_item(
    ui: &mut Ui,
    label: &str,
    selected: bool,
    count: usize,
    changed: usize,
) -> Response {
    let (rect, response) = ui.allocate_exact_size(vec2(ui.available_width(), 24.0), Sense::click());
    let painter = ui.painter();
    if selected {
        painter.rect_filled(rect, CornerRadius::same(5), CARD_HOVER);
        painter.rect_filled(
            Rect::from_min_size(rect.min + vec2(0.0, 6.0), vec2(3.0, rect.height() - 12.0)),
            CornerRadius::same(2),
            ACCENT,
        );
    } else if response.hovered() {
        painter.rect_filled(rect, CornerRadius::same(5), CARD_HOVER.gamma_multiply(0.6));
    }
    let dim = count == 0 && !selected;
    let color = match (selected, dim) {
        (true, _) => TEXT,
        (false, true) => WEAK.gamma_multiply(0.6),
        (false, false) => WEAK.gamma_multiply(1.15),
    };
    painter.text(
        rect.left_center() + vec2(12.0, 0.0),
        Align2::LEFT_CENTER,
        label,
        if selected {
            FontId::new(12.5, semibold())
        } else {
            FontId::proportional(12.5)
        },
        color,
    );
    let right = rect.right_center() - vec2(8.0, 0.0);
    let count_rect = painter.text(
        right,
        Align2::RIGHT_CENTER,
        count.to_string(),
        FontId::proportional(11.0),
        WEAK.gamma_multiply(if dim { 0.6 } else { 0.9 }),
    );
    if changed > 0 {
        let text = changed.to_string();
        let center = egui::pos2(count_rect.left() - 14.0 - 3.0 * text.len() as f32, right.y);
        let badge = Rect::from_center_size(center, vec2(10.0 + 6.0 * text.len() as f32, 16.0));
        painter.rect_filled(badge, CornerRadius::same(255), ACCENT.gamma_multiply(0.22));
        painter.text(
            center,
            Align2::CENTER_CENTER,
            text,
            FontId::proportional(10.5),
            ACCENT,
        );
    }
    let response = response.on_hover_cursor(egui::CursorIcon::PointingHand);
    if changed > 0 {
        response.on_hover_text(format!("{changed} changed in your profile"))
    } else {
        response
    }
}

/// Underlined tab strip; returns the clicked tab's index.
pub fn tab_bar(ui: &mut Ui, labels: &[&str], selected: usize) -> Option<usize> {
    let mut picked = None;
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 2.0;
        for (i, label) in labels.iter().enumerate() {
            let font = FontId::proportional(13.0);
            let galley = ui.painter().layout_no_wrap(label.to_string(), font, TEXT);
            let size = vec2(galley.size().x + 20.0, 28.0);
            let (rect, response) = ui.allocate_exact_size(size, Sense::click());
            let on = i == selected;
            let color = if on {
                TEXT
            } else if response.hovered() {
                TEXT.gamma_multiply(0.85)
            } else {
                WEAK
            };
            let painter = ui.painter();
            if response.hovered() && !on {
                painter.rect_filled(rect.shrink2(vec2(0.0, 3.0)), CornerRadius::same(4), CARD);
            }
            painter.galley(
                rect.center() - galley.size() / 2.0 - vec2(0.0, 1.0),
                galley,
                color,
            );
            if on {
                painter.rect_filled(
                    Rect::from_min_max(
                        egui::pos2(rect.left() + 8.0, rect.bottom() - 2.0),
                        egui::pos2(rect.right() - 8.0, rect.bottom()),
                    ),
                    CornerRadius::same(1),
                    ACCENT,
                );
            }
            if response
                .on_hover_cursor(egui::CursorIcon::PointingHand)
                .clicked()
            {
                picked = Some(i);
            }
        }
    });
    picked
}

fn pill_shape(ui: &mut Ui, text: &str, height: f32, sense: Sense) -> (Rect, Response, f32) {
    let font = FontId::proportional(11.5);
    let width = ui
        .painter()
        .layout_no_wrap(text.into(), font, TEXT)
        .size()
        .x;
    let (rect, response) = ui.allocate_exact_size(vec2(width + 22.0, height), sense);
    (rect, response, width)
}

/// Status chip with a coloured dot; `on` fills it, `None` makes it read-only.
pub fn chip(ui: &mut Ui, text: &str, dot: Color32, on: Option<bool>) -> Response {
    let sense = if on.is_some() {
        Sense::click()
    } else {
        Sense::hover()
    };
    let (rect, response, _) = pill_shape(ui, text, 22.0, sense);
    let lit = on == Some(true);
    let fill = if lit {
        dot.gamma_multiply(0.18)
    } else if on.is_some() && response.hovered() {
        CARD_HOVER
    } else {
        CARD
    };
    let stroke = if lit {
        Stroke::new(1.0, dot.gamma_multiply(0.7))
    } else {
        Stroke::new(1.0, BORDER)
    };
    let painter = ui.painter();
    painter.rect(
        rect,
        CornerRadius::same(255),
        fill,
        stroke,
        StrokeKind::Inside,
    );
    painter.circle_filled(rect.left_center() + vec2(10.0, 0.0), 3.5, dot);
    painter.text(
        rect.left_center() + vec2(17.0, 0.0),
        Align2::LEFT_CENTER,
        text,
        FontId::proportional(11.5),
        if lit || on.is_none() { TEXT } else { WEAK },
    );
    if on.is_some() {
        response.on_hover_cursor(egui::CursorIcon::PointingHand)
    } else {
        response
    }
}

/// Two-option segmented switch (e.g. Simple | Advanced); returns the clicked index.
pub fn segmented(ui: &mut Ui, labels: &[&str], selected: usize) -> Option<usize> {
    let mut picked = None;
    let font = FontId::proportional(12.0);
    let widths: Vec<f32> = labels
        .iter()
        .map(|l| {
            ui.painter()
                .layout_no_wrap(l.to_string(), font.clone(), TEXT)
                .size()
                .x
                + 18.0
        })
        .collect();
    let (outer, _) =
        ui.allocate_exact_size(vec2(widths.iter().sum::<f32>() + 4.0, 24.0), Sense::hover());
    ui.painter().rect_filled(outer, CornerRadius::same(5), RAIL);
    let mut x = outer.left() + 2.0;
    for (i, (label, w)) in labels.iter().zip(&widths).enumerate() {
        let rect = Rect::from_min_size(egui::pos2(x, outer.top() + 2.0), vec2(*w, 20.0));
        x += w;
        let response = ui
            .interact(rect, ui.id().with(("segmented", label)), Sense::click())
            .on_hover_cursor(egui::CursorIcon::PointingHand);
        let on = i == selected;
        let painter = ui.painter();
        if on {
            painter.rect_filled(rect, CornerRadius::same(4), CARD_HOVER);
        }
        painter.text(
            rect.center(),
            Align2::CENTER_CENTER,
            *label,
            font.clone(),
            if on || response.hovered() { TEXT } else { WEAK },
        );
        if response.clicked() && !on {
            picked = Some(i);
        }
    }
    picked
}

/// Rounded tag such as Live / Cheat / Restart.
pub fn badge(ui: &mut Ui, text: &str, color: Color32) -> Response {
    let font = FontId::proportional(10.5);
    let width = ui
        .painter()
        .layout_no_wrap(text.into(), font.clone(), color)
        .size()
        .x;
    let (rect, response) = ui.allocate_exact_size(vec2(width + 12.0, 16.0), Sense::hover());
    let painter = ui.painter();
    painter.rect_filled(rect, CornerRadius::same(255), color.gamma_multiply(0.16));
    painter.text(rect.center(), Align2::CENTER_CENTER, text, font, color);
    response
}

/// The amber "Reset" chip changed rows carry.
pub fn reset_pill(ui: &mut Ui) -> Response {
    ui.add(
        egui::Button::new(RichText::new("Reset").size(11.0).color(ACCENT))
            .fill(ACCENT.gamma_multiply(0.14))
            .corner_radius(CornerRadius::same(255))
            .min_size(vec2(0.0, 18.0)),
    )
}

/// Full-width amber button for the one primary action of a panel.
pub fn primary_button(ui: &mut Ui, enabled: bool, text: &str) -> Response {
    let button =
        egui::Button::new(RichText::new(text).size(13.5).strong().color(ON_ACCENT)).fill(ACCENT);
    wide(ui, enabled, 30.0, button)
}

pub fn wide_button(ui: &mut Ui, enabled: bool, text: &str) -> Response {
    wide(ui, enabled, 26.0, egui::Button::new(text))
}

/// `add_sized` centres the label, which `min_size` alone does not.
fn wide(ui: &mut Ui, enabled: bool, height: f32, button: egui::Button) -> Response {
    let size = vec2(ui.available_width(), height);
    ui.add_enabled_ui(enabled, |ui| ui.add_sized(size, button))
        .inner
}

/// Pill switch, smaller than the simple view's.
pub fn switch(ui: &mut Ui, on: bool) -> Response {
    let (rect, response) = ui.allocate_exact_size(vec2(30.0, 16.0), Sense::click());
    let t = ui.ctx().animate_bool_responsive(response.id, on);
    let fill = if on {
        ACCENT
    } else {
        ui.visuals().widgets.inactive.bg_fill
    };
    let painter = ui.painter();
    painter.rect_filled(rect, CornerRadius::same(255), fill);
    let x = egui::lerp(rect.left() + 8.0..=rect.right() - 8.0, t);
    painter.circle_filled(
        egui::pos2(x, rect.center().y),
        5.5,
        if on { ON_ACCENT } else { TEXT },
    );
    response.on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// A small painted icon button; `glyph` draws inside the given rect in the given colour.
pub fn icon_button(
    ui: &mut Ui,
    visible: bool,
    color: Color32,
    glyph: impl FnOnce(&egui::Painter, Rect, Color32),
) -> Response {
    let (rect, response) = ui.allocate_exact_size(vec2(20.0, 20.0), Sense::click());
    if visible || response.hovered() {
        let painter = ui.painter();
        if response.hovered() {
            painter.rect_filled(rect, CornerRadius::same(4), CARD_HOVER);
        }
        let color = if response.hovered() { TEXT } else { color };
        glyph(painter, rect.shrink(4.0), color);
    }
    response.on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// Circle with a slash: "this line is disabled".
pub fn disable_glyph(painter: &egui::Painter, rect: Rect, color: Color32) {
    let r = rect.width() / 2.0;
    painter.circle_stroke(rect.center(), r, Stroke::new(1.4, color));
    let d = r * std::f32::consts::FRAC_1_SQRT_2;
    painter.line_segment(
        [rect.center() + vec2(-d, -d), rect.center() + vec2(d, d)],
        Stroke::new(1.4, color),
    );
}

/// Five-point star, filled or outlined.
pub fn star_glyph(filled: bool) -> impl FnOnce(&egui::Painter, Rect, Color32) {
    move |painter, rect, color| {
        let c = rect.center();
        let (outer, inner) = (rect.width() / 2.0 + 0.5, rect.width() / 4.6);
        let points: Vec<egui::Pos2> = (0..10)
            .map(|i| {
                let r = if i % 2 == 0 { outer } else { inner };
                let a = -std::f32::consts::FRAC_PI_2 + i as f32 * std::f32::consts::PI / 5.0;
                c + vec2(a.cos(), a.sin()) * r
            })
            .collect();
        if !filled {
            painter.add(egui::Shape::closed_line(points, Stroke::new(1.2, color)));
            return;
        }
        let inner: Vec<egui::Pos2> = points.iter().skip(1).step_by(2).copied().collect();
        painter.add(egui::Shape::convex_polygon(inner, color, Stroke::NONE));
        for i in (0..10).step_by(2) {
            let tip = vec![points[(i + 9) % 10], points[i], points[i + 1]];
            painter.add(egui::Shape::convex_polygon(tip, color, Stroke::NONE));
        }
    }
}
