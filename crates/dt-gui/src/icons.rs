//! Line icons drawn on a 24-unit grid, like an SVG viewBox, and painted with egui shapes
//! so no image or SVG crate ships.

use eframe::egui::{Color32, CornerRadius, Painter, Pos2, Rect, Shape, Stroke, StrokeKind};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Icon {
    Overview,
    Display,
    Shadows,
    Effects,
    World,
    Performance,
    Hud,
    Minimap,
    Addons,
    System,
    Safety,
    Search,
    Play,
    ChevronDown,
    MiniWindow,
    Sliders,
    Refresh,
}

#[cfg(test)]
impl Icon {
    pub const ALL: [Icon; 17] = [
        Icon::Overview,
        Icon::Display,
        Icon::Shadows,
        Icon::Effects,
        Icon::World,
        Icon::Performance,
        Icon::Hud,
        Icon::Minimap,
        Icon::Addons,
        Icon::System,
        Icon::Safety,
        Icon::Search,
        Icon::Play,
        Icon::ChevronDown,
        Icon::MiniWindow,
        Icon::Sliders,
        Icon::Refresh,
    ];
}

/// One drawing command in grid units.
enum Part {
    Line(&'static [(f32, f32)]),
    Closed(&'static [(f32, f32)]),
    /// Convex only.
    Fill(&'static [(f32, f32)]),
    Circle(f32, f32, f32),
    Dot(f32, f32, f32),
    Rect(f32, f32, f32, f32, f32),
    FillRect(f32, f32, f32, f32, f32),
    /// Centre, radius, start and end angle in degrees, clockwise from three o'clock.
    Arc(f32, f32, f32, f32, f32),
}

fn parts(icon: Icon) -> &'static [Part] {
    use Part::*;
    match icon {
        Icon::Overview => &[
            Rect(3.5, 3.5, 7.0, 7.0, 1.5),
            Rect(13.5, 3.5, 7.0, 7.0, 1.5),
            Rect(3.5, 13.5, 7.0, 7.0, 1.5),
            Rect(13.5, 13.5, 7.0, 7.0, 1.5),
        ],
        Icon::Display => &[
            Rect(2.5, 4.0, 19.0, 12.5, 2.0),
            Line(&[(12.0, 16.5), (12.0, 20.0)]),
            Line(&[(8.0, 20.5), (16.0, 20.5)]),
        ],
        Icon::Shadows => &[
            Circle(12.0, 12.0, 8.5),
            Fill(&[
                (12.0, 3.5),
                (15.3, 4.2),
                (18.0, 6.0),
                (19.8, 8.7),
                (20.5, 12.0),
                (19.8, 15.3),
                (18.0, 18.0),
                (15.3, 19.8),
                (12.0, 20.5),
            ]),
        ],
        Icon::Effects => &[
            Closed(&[
                (11.0, 3.0),
                (12.9, 9.1),
                (19.0, 11.0),
                (12.9, 12.9),
                (11.0, 19.0),
                (9.1, 12.9),
                (3.0, 11.0),
                (9.1, 9.1),
            ]),
            Line(&[(19.0, 15.5), (19.0, 21.5)]),
            Line(&[(16.0, 18.5), (22.0, 18.5)]),
        ],
        Icon::World => &[
            Closed(&[
                (2.5, 19.5),
                (8.5, 9.0),
                (12.5, 15.0),
                (15.5, 11.0),
                (21.5, 19.5),
            ]),
            Circle(17.0, 5.5, 2.0),
        ],
        Icon::Performance => &[Closed(&[
            (13.5, 2.5),
            (4.5, 13.5),
            (11.0, 13.5),
            (10.0, 21.5),
            (19.5, 10.0),
            (13.0, 10.0),
        ])],
        Icon::Hud => &[
            Rect(2.5, 4.0, 19.0, 16.0, 2.0),
            Line(&[(8.0, 7.5), (16.0, 7.5)]),
            FillRect(5.5, 14.0, 5.0, 3.0, 0.8),
            Circle(17.0, 15.5, 2.0),
        ],
        Icon::Minimap => &[
            Circle(12.0, 12.0, 8.5),
            Fill(&[(12.0, 6.0), (14.6, 12.0), (9.4, 12.0)]),
            Closed(&[(9.4, 12.0), (14.6, 12.0), (12.0, 18.0)]),
        ],
        Icon::Addons => &[
            Closed(&[
                (12.0, 2.5),
                (20.5, 7.0),
                (20.5, 17.0),
                (12.0, 21.5),
                (3.5, 17.0),
                (3.5, 7.0),
            ]),
            Line(&[(3.5, 7.0), (12.0, 11.5), (20.5, 7.0)]),
            Line(&[(12.0, 11.5), (12.0, 21.5)]),
        ],
        Icon::System => &[Line(&[
            (2.5, 12.0),
            (6.5, 12.0),
            (9.0, 5.0),
            (15.0, 19.0),
            (17.5, 12.0),
            (21.5, 12.0),
        ])],
        Icon::Safety => &[
            Closed(&[
                (12.0, 2.5),
                (19.5, 5.5),
                (19.5, 11.5),
                (17.8, 16.2),
                (12.0, 21.5),
                (6.2, 16.2),
                (4.5, 11.5),
                (4.5, 5.5),
            ]),
            Line(&[(8.8, 12.0), (11.0, 14.2), (15.4, 9.6)]),
        ],
        Icon::Search => &[Circle(10.5, 10.5, 6.5), Line(&[(15.3, 15.3), (20.5, 20.5)])],
        Icon::Play => &[Fill(&[(7.5, 4.5), (19.5, 12.0), (7.5, 19.5)])],
        Icon::ChevronDown => &[Line(&[(6.0, 9.0), (12.0, 15.0), (18.0, 9.0)])],
        Icon::MiniWindow => &[
            Rect(2.5, 4.0, 19.0, 16.0, 2.0),
            FillRect(12.0, 11.5, 7.0, 6.0, 1.0),
        ],
        Icon::Sliders => &[
            Line(&[(3.5, 6.5), (20.5, 6.5)]),
            Line(&[(3.5, 12.0), (20.5, 12.0)]),
            Line(&[(3.5, 17.5), (20.5, 17.5)]),
            Dot(9.0, 6.5, 2.4),
            Dot(15.5, 12.0, 2.4),
            Dot(7.5, 17.5, 2.4),
        ],
        Icon::Refresh => &[
            Arc(12.0, 12.0, 7.5, -50.0, 250.0),
            Line(&[(15.0, 3.5), (17.2, 6.3), (14.0, 7.4)]),
        ],
    }
}

/// The shapes for `icon` fitted to the square centred in `rect`.
pub fn shapes(icon: Icon, rect: Rect, color: Color32) -> Vec<Shape> {
    let side = rect.width().min(rect.height());
    let scale = side / 24.0;
    let origin = rect.center() - eframe::egui::vec2(side, side) / 2.0;
    let at = |(x, y): (f32, f32)| origin + eframe::egui::vec2(x, y) * scale;
    let points = |pts: &[(f32, f32)]| pts.iter().copied().map(at).collect::<Vec<Pos2>>();
    let stroke = Stroke::new((1.9 * scale).max(1.2), color);
    let rect_of =
        |x: f32, y: f32, w: f32, h: f32| Rect::from_min_max(at((x, y)), at((x + w, y + h)));
    let radius = |r: f32| CornerRadius::same((r * scale).round().clamp(0.0, 255.0) as u8);
    parts(icon)
        .iter()
        .map(|part| match *part {
            Part::Line(pts) => Shape::line(points(pts), stroke),
            Part::Closed(pts) => Shape::closed_line(points(pts), stroke),
            Part::Fill(pts) => Shape::convex_polygon(points(pts), color, Stroke::NONE),
            Part::Circle(x, y, r) => Shape::circle_stroke(at((x, y)), r * scale, stroke),
            Part::Dot(x, y, r) => Shape::circle_filled(at((x, y)), r * scale, color),
            Part::Rect(x, y, w, h, r) => {
                Shape::rect_stroke(rect_of(x, y, w, h), radius(r), stroke, StrokeKind::Middle)
            }
            Part::FillRect(x, y, w, h, r) => {
                Shape::rect_filled(rect_of(x, y, w, h), radius(r), color)
            }
            Part::Arc(x, y, r, from, to) => {
                let steps = 24;
                let pts = (0..=steps)
                    .map(|i| {
                        let deg = from + (to - from) * i as f32 / steps as f32;
                        let a = deg.to_radians();
                        at((x + r * a.cos(), y + r * a.sin()))
                    })
                    .collect();
                Shape::line(pts, stroke)
            }
        })
        .collect()
}

pub fn paint(painter: &Painter, rect: Rect, icon: Icon, color: Color32) {
    painter.extend(shapes(icon, rect, color));
}

#[cfg(test)]
mod tests {
    use super::*;
    use eframe::egui::{pos2, vec2};

    #[test]
    fn every_icon_draws_inside_its_box() {
        let rect = Rect::from_min_size(pos2(100.0, 40.0), vec2(16.0, 16.0));
        for icon in Icon::ALL {
            let shapes = shapes(icon, rect, Color32::WHITE);
            assert!(!shapes.is_empty(), "{icon:?}");
            for shape in shapes {
                let bounds = shape.visual_bounding_rect();
                assert!(
                    rect.expand(1.0).contains_rect(bounds),
                    "{icon:?} spills out: {bounds:?}"
                );
            }
        }
    }

    #[test]
    fn filled_polygons_are_convex() {
        for icon in Icon::ALL {
            for part in parts(icon) {
                let Part::Fill(pts) = part else { continue };
                let n = pts.len();
                let signs: Vec<bool> = (0..n)
                    .map(|i| {
                        let (a, b, c) = (pts[i], pts[(i + 1) % n], pts[(i + 2) % n]);
                        (b.0 - a.0) * (c.1 - b.1) - (b.1 - a.1) * (c.0 - b.0) >= 0.0
                    })
                    .collect();
                assert!(
                    signs.iter().all(|s| *s) || signs.iter().all(|s| !*s),
                    "{icon:?} has a concave fill"
                );
            }
        }
    }
}
