//! The health bar previews: the game's ruler from its own art, and the straight, rounded,
//! wedge and number-only styles, at any health level, the way `HealthStyle` places them.

use dt_core::hud::art;
use dt_core::hud::health_style::{
    BarAngle, BarShape, HealthStyle, NumberFont, NumberLayout, Offset, RegenPlace,
};
use dt_core::texture::adjust::Rgb;
use eframe::egui::epaint::TextShape;
use eframe::egui::{
    self, Align2, Color32, FontId, Painter, Pos2, Rect, Stroke, Vec2, emath::Rot2, pos2, vec2,
};

use crate::hud_art::{self, Images, Place};
use crate::theme;

/// Vanilla's number colours: off-white, and `#FF5656` at low health.
const HEALTHY: Color32 = Color32::from_rgb(0xFF, 0xEF, 0xD7);
const LOW: Color32 = Color32::from_rgb(0xFF, 0x56, 0x56);
const HURT: Color32 = Color32::from_rgb(0xFF, 0xB3, 0x47);
/// `vivaciousGreen` (`#142304`), the backer's wash, the same green as the frame.
const BACKER: Color32 = Color32::from_rgb(0x14, 0x23, 0x04);
const FRAME: Color32 = Color32::from_rgb(0x14, 0x23, 0x04);
/// `#health_bar`, 66x212.
const BAR: Rect = Rect::from_min_max(pos2(0.0, 0.0), pos2(66.0, 212.0));
/// `#health_bar_frame`, 68x220, from the bar's top-left less 1 px.
const FRAME_RECT: Rect = Rect::from_min_max(pos2(-1.0, -1.0), pos2(67.0, 219.0));

const fn on_mask(x: f32, y: f32) -> Pos2 {
    pos2(x * 66.0 / 99.0, y * 212.0 / 330.0)
}

const fn on_frame(x: f32, y: f32) -> Pos2 {
    pos2(x * 68.0 / 105.0 - 1.0, y * 220.0 / 340.0 - 1.0)
}

/// `healthbar_backer_mask` (a 99x330 drawing): the ruler, wide at the top, straight on
/// the left, slanting in on the right.
const RULER: [Pos2; 4] = [
    on_mask(1.29, 0.95),
    on_mask(98.8, 0.27),
    on_mask(25.7, 329.5),
    on_mask(0.82, 324.1),
];
/// `healthbar_frame_with_regen` (a 105x340 drawing): its outline, and its hole cut into
/// two convex parts at the regen shelf's bottom, for drawing the frame without the picture.
const FRAME_OUTLINE: [Pos2; 4] = [
    on_frame(0.0, 0.81),
    on_frame(104.96, 0.0),
    on_frame(31.18, 339.19),
    on_frame(0.34, 332.49),
];
const FRAME_HOLE: [[Pos2; 4]; 2] = [
    [
        on_frame(17.79, 35.0),
        on_frame(90.9, 35.0),
        on_frame(28.82, 331.19),
        on_frame(7.15, 327.11),
    ],
    [
        on_frame(83.19, 7.61),
        on_frame(97.01, 6.1),
        on_frame(90.9, 35.0),
        on_frame(69.5, 35.0),
    ],
];
/// The number block leans about 10 degrees in the in-game screenshot.
const NUMBER_TILT: f32 = -10.0;
const MAX_HEALTH: u32 = 4557;
const BODY: Color32 = Color32::from_rgba_unmultiplied_const(0x33, 0x33, 0x33, 0xEA);
/// `healthbar_fill_texture_png`'s average colour, for the fill without the picture.
const PAPER: Color32 = Color32::from_rgb(0xDF, 0xD1, 0xBC);
const OFF_BLACK: Color32 = Color32::from_rgb(0x10, 0x13, 0x0D);
const FRAME_LOW: Color32 = Color32::from_rgb(0xCC, 0x34, 0x0A);

/// The x range of convex `shape` on the line `y`, if the line crosses it.
fn span(shape: &[Pos2], y: f32) -> Option<(f32, f32)> {
    let mut range: Option<(f32, f32)> = None;
    for (i, &a) in shape.iter().enumerate() {
        let b = shape[(i + 1) % shape.len()];
        if (a.y - y) * (b.y - y) > 0.0 || a.y == b.y {
            continue;
        }
        let x = a.x + (b.x - a.x) * (y - a.y) / (b.y - a.y);
        range = Some(range.map_or((x, x), |(l, r)| (l.min(x), r.max(x))));
    }
    range
}

/// `#healthLines` as the screenshot shows them: a tick every 250 health up from the
/// bottom, full width every 1000 and 30% wide between. Heights are fractions of the bar.
fn ticks(max: u32) -> Vec<(f32, bool)> {
    (250..max)
        .step_by(250)
        .map(|hp| (hp as f32 / max as f32, hp % 1000 == 0))
        .collect()
}

fn corners(rect: Rect) -> [Pos2; 4] {
    [
        rect.left_top(),
        rect.right_top(),
        rect.right_bottom(),
        rect.left_bottom(),
    ]
}

/// Text put on screen by `place`, its `align` point at the upright `anchor`, ringed with
/// an offBlack `outline` (screen px, none at 0) the way the game's number is.
#[allow(clippy::too_many_arguments)]
fn placed_text(
    p: &Painter,
    place: Place,
    anchor: Pos2,
    align: Align2,
    text: &str,
    font: FontId,
    color: Color32,
    outline: f32,
) {
    let galley = p.layout_no_wrap(text.to_owned(), font, color);
    let corner = place.at(align.anchor_size(anchor, galley.size() / place.scale).min);
    if outline > 0.0 {
        for step in 0..8 {
            let offset =
                Rot2::from_angle(step as f32 * std::f32::consts::FRAC_PI_4) * vec2(outline, 0.0);
            p.add(
                TextShape::new(corner + offset, galley.clone(), OFF_BLACK)
                    .with_override_text_color(OFF_BLACK)
                    .with_angle(place.angle),
            );
        }
    }
    p.add(TextShape::new(corner, galley, color).with_angle(place.angle));
}

/// Whether `fill` (0 to 1) reads as low health, where the game turns the bar red.
pub(crate) fn is_low(fill: f32) -> bool {
    fill <= 0.3
}

/// The number's colour at `fill` with `style`.
pub(crate) fn number_color(style: &HealthStyle, fill: f32) -> Color32 {
    if is_low(fill) {
        LOW
    } else if style.color_by_health && fill < 0.75 {
        HURT
    } else {
        HEALTHY
    }
}

/// Where `.bars_container` turns: the vanilla lean pivots at the bar's corner (how the
/// calibrated sketch was measured), the other angles at the bar's middle, as CSS does.
const BAR_CENTRE: Pos2 = pos2(33.0, 106.0);
/// The number box, in upright game px, for fitting the block into a preview.
const NUMBER_BOX: Rect = Rect::from_min_max(pos2(16.0, 30.0), pos2(132.0, 100.0));

/// The bar's place: its own rect `[0, w] x [0, h]` set at `offset` in upright game px and
/// turned by `degrees` about `pivot`.
fn bar_place(upright: Place, offset: Pos2, pivot: Pos2, degrees: f32) -> Place {
    let rot = Rot2::from_angle(degrees.to_radians());
    Place {
        origin: upright.at(pivot) + rot * ((offset - pivot) * upright.scale),
        scale: upright.scale,
        angle: degrees.to_radians(),
    }
}

/// The bar's outline in its own px, convex, clockwise from the top left.
fn outline(shape: BarShape, w: f32, h: f32) -> Vec<Pos2> {
    match shape {
        BarShape::Rounded => {
            let r = w.min(h) / 2.0;
            let mut points = Vec::new();
            for (centre, from) in [
                (pos2(w - r, r), -90.0_f32),
                (pos2(w - r, h - r), 0.0),
                (pos2(r, h - r), 90.0),
                (pos2(r, r), 180.0),
            ] {
                let from = from.to_radians();
                points.extend(hud_art::arc(
                    centre,
                    r,
                    from,
                    from + std::f32::consts::FRAC_PI_2,
                    8,
                ));
            }
            points
        }
        BarShape::Wedge => [(27.0, 0.0), (12.47, 91.0), (0.98, 90.05), (0.0, 0.19)]
            .iter()
            .rev()
            .map(|&(x, y)| pos2(x * w / 27.0, y * h / 91.0))
            .collect(),
        _ => corners(Rect::from_min_size(Pos2::ZERO, vec2(w, h))).to_vec(),
    }
}

/// How the bar is laid out for `style`: its place for a given upright place, and its size.
struct BarLayout {
    size: Vec2,
    offset: Pos2,
    pivot: Pos2,
    degrees: f32,
}

impl BarLayout {
    fn of(style: &HealthStyle) -> BarLayout {
        let (w, h) = style.bar_size();
        let size = vec2(w as f32, h as f32);
        let offset = match style.shape {
            BarShape::Ruler => Pos2::ZERO,
            _ => BAR_CENTRE - size / 2.0,
        };
        let degrees = style.angle.degrees() as f32;
        let pivot = if style.angle == BarAngle::Tilted {
            Pos2::ZERO
        } else {
            offset + size / 2.0
        };
        BarLayout {
            size,
            offset,
            pivot,
            degrees,
        }
    }

    fn place(&self, upright: Place) -> Place {
        bar_place(upright, self.offset, self.pivot, self.degrees)
    }
}

/// The number block measured against the in-game capture (`assets/vanilla_hud.jpg` and a
/// close-up of it), in upright game px: the right end of the current health, the backer
/// behind it and the right end of the max health line.
const NUMBER_AT: Pos2 = pos2(119.0, 62.0);
/// The label is pinned 6 px under its box's top, so a bigger number grows downward.
fn number_y(number_scale: f32) -> f32 {
    NUMBER_AT.y + 16.0 * (number_scale - 1.0)
}
const BACKER_RECT: Rect = Rect::from_min_max(pos2(50.0, 34.0), pos2(132.0, 92.0));
const MAX_AT: Pos2 = pos2(117.0, 86.0);
/// `numericOracle`'s digits are narrower and shorter than Inter's at the same size.
const GAME_FONT_FIT: f32 = 0.89;
/// Any colour, for measuring text.
const TEXT_PROBE: Color32 = Color32::WHITE;

fn font(style: &HealthStyle, size: f32) -> FontId {
    match style.font {
        NumberFont::Game => FontId::new(size * GAME_FONT_FIT, theme::semibold()),
        NumberFont::Block => FontId::new(size, theme::semibold()),
        NumberFont::Sans => FontId::proportional(size),
        NumberFont::Mono => FontId::monospace(size * 0.92),
    }
}

/// A piece of the health block the page lets the player drag.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum Part {
    Bar,
    Number,
    Regen,
}

/// What a drawing put where: each part's outline on screen, and the frame its drags move
/// in (screen px per game px, and the frame's turn).
pub(crate) struct Drawn {
    pub real: bool,
    pub parts: Vec<(Part, Rect, Place)>,
}

fn screen_rect(place: Place, local: Rect) -> Rect {
    let mut out = Rect::NOTHING;
    for p in corners(local) {
        out.extend_with(place.at(p));
    }
    out
}

fn moved(place: Place, offset: Offset) -> Place {
    Place {
        origin: place.at(pos2(offset.x.into(), offset.y.into())),
        ..place
    }
}

/// One health block at `fill` with `style`, fitted and centred in `area` (dragged moves
/// left out of the fit, so a drag never rescales it): the game's bar parts where they
/// load, painted shapes where they don't.
pub(crate) fn draw_health(
    painter: &Painter,
    images: &mut Images,
    style: &HealthStyle,
    area: Rect,
    fill: f32,
) -> Drawn {
    let number_scale = f32::from(style.number_scale_pct) / 100.0;
    let layout = BarLayout::of(style);
    let unit = layout.place(Place::SCREEN);
    let mut bounds = Rect::NOTHING;
    if style.shape != BarShape::Hidden {
        for p in corners(Rect::from_min_size(Pos2::ZERO, layout.size)) {
            bounds.extend_with(unit.at(p));
        }
    }
    if !style.hide_number {
        let y = number_y(number_scale);
        let mut number = Rect::from_min_max(
            pos2(
                (116.0 - 100.0 * number_scale).min(NUMBER_BOX.left()),
                y - 22.0 * number_scale,
            ),
            pos2(NUMBER_BOX.right(), y + 38.0 * number_scale),
        );
        if style.regen_shown_at() == RegenPlace::Number && !style.hide_regen {
            let top = regen_top(style);
            number = number.union(Rect::from_min_max(
                pos2(number.left(), top - 2.0),
                pos2(number.right(), top + 18.0),
            ));
        }
        bounds = bounds.union(number);
    }
    let mut drawn = Drawn {
        real: false,
        parts: Vec::new(),
    };
    if !bounds.is_positive() {
        return drawn;
    }
    let scale = (area.width() * 0.94 / bounds.width()).min(area.height() * 0.96 / bounds.height());
    let upright = Place {
        origin: area.center() - bounds.center().to_vec2() * scale,
        scale,
        angle: 0.0,
    };
    let bar_upright = moved(upright, style.bar_offset);
    let bar = layout.place(bar_upright);
    if style.shape != BarShape::Hidden {
        drawn.real = match style.shape {
            BarShape::Ruler => draw_ruler(painter, images, style, bar, fill),
            _ => draw_shaped(painter, images, style, bar, layout.size, fill),
        };
        drawn.parts.push((
            Part::Bar,
            screen_rect(bar, Rect::from_min_size(Pos2::ZERO, layout.size)),
            bar_upright,
        ));
    }
    let number = number_place(style, upright, fill);
    if !style.hide_regen {
        let regen = match style.regen_shown_at() {
            RegenPlace::Bar => regen_on_bar(painter, images, style, moved(bar, style.regen_offset)),
            RegenPlace::Number => {
                regen_by_number(painter, images, style, moved(number, style.regen_offset))
            }
        };
        drawn.parts.push(regen);
    }
    if !style.hide_number {
        let number = moved(number, style.number_offset);
        let rect = draw_number(painter, images, style, number, fill, number_scale);
        drawn.parts.push((Part::Number, rect, number));
    }
    drawn
}

/// The game's own regen at the bar's top end: arrows and the bare number.
fn regen_on_bar(
    painter: &Painter,
    images: &mut Images,
    style: &HealthStyle,
    bar: Place,
) -> (Part, Rect, Place) {
    let scale = bar.scale;
    let font = FontId::new(10.0 * scale, theme::semibold());
    let (place, at, align) = match style.shape {
        BarShape::Ruler => (bar, pos2(23.0, 9.0), Align2::LEFT_CENTER),
        _ => (Place::SCREEN, bar.at(pos2(0.0, -4.0)), Align2::LEFT_BOTTOM),
    };
    if style.shape == BarShape::Ruler {
        let arrows = Rect::from_center_size(pos2(17.0, 9.5), vec2(7.0, 8.0));
        images.paint_placed(
            painter,
            art::REGEN,
            bar,
            arrows,
            &corners(arrows),
            HEALTHY.gamma_multiply(0.3),
        );
    }
    placed_text(
        painter,
        place,
        at,
        align,
        "14.8",
        font.clone(),
        HEALTHY,
        0.0,
    );
    let size = painter.layout_no_wrap("14.8".into(), font, HEALTHY).size();
    let rect = match style.shape {
        BarShape::Ruler => screen_rect(
            bar,
            Rect::from_min_size(pos2(12.0, 2.0), vec2(12.0 + size.x / scale, 14.0)),
        ),
        _ => align.anchor_size(at, size).expand(2.0),
    };
    (Part::Regen, rect, bar)
}

/// The number block's own regen label, "14.8/s" with the arrows, just under the number.
fn regen_by_number(
    painter: &Painter,
    images: &mut Images,
    style: &HealthStyle,
    number: Place,
) -> (Part, Rect, Place) {
    let scale = number.scale;
    let text = "14.8/s";
    let font = FontId::monospace(13.0 * scale);
    let color = Color32::from_white_alpha(0x9F);
    let width = painter
        .layout_no_wrap(text.into(), font.clone(), color)
        .size()
        .x
        / scale;
    let right = NUMBER_AT.x + 2.0;
    let mid = regen_top(style) + 8.0;
    placed_text(
        painter,
        number,
        pos2(right, mid),
        Align2::RIGHT_CENTER,
        text,
        font,
        color,
        0.0,
    );
    let arrows = Rect::from_center_size(pos2(right - width - 9.0, mid), vec2(12.5, 9.5));
    images.paint_placed(
        painter,
        art::REGEN,
        number,
        arrows,
        &corners(arrows),
        Color32::WHITE,
    );
    let local = Rect::from_min_max(
        pos2(arrows.left() - 2.0, mid - 9.0),
        pos2(right + 2.0, mid + 9.0),
    );
    (Part::Regen, screen_rect(number, local), number)
}

/// The regen's middle line in the number block, in our upright px: the CSS top less where
/// `.healthContainer` starts, onto our number box.
fn regen_top(style: &HealthStyle) -> f32 {
    (style.regen_top() - 100.0) as f32 + 40.0
}

/// The number block's frame: upright, leaning with a tilted bar, shaking when low.
fn number_place(style: &HealthStyle, upright: Place, fill: f32) -> Place {
    let shake = if is_low(fill) && !style.no_shake {
        1.5
    } else {
        0.0
    };
    let tilt = if style.angle == BarAngle::Tilted {
        NUMBER_TILT
    } else {
        0.0
    };
    Place {
        origin: upright.origin + vec2(shake, 0.0),
        ..upright
    }
    .turned(tilt)
}

/// The game's slanted ruler with its paper, ticks and frame.
fn draw_ruler(
    painter: &Painter,
    images: &mut Images,
    style: &HealthStyle,
    bar: Place,
    fill: f32,
) -> bool {
    let low = is_low(fill);
    let scale = bar.scale;
    let (fill_tint, paper) = if low {
        (LOW, LOW)
    } else {
        (Color32::WHITE, PAPER)
    };
    let frame_tint = if low { FRAME_LOW } else { FRAME };
    let framed = style.frame
        && images
            .get(art::HEALTH_FRAME, FRAME_RECT.height() * scale)
            .is_some();
    let shown: &[[Pos2; 4]] = if framed || !style.frame {
        std::slice::from_ref(&RULER)
    } else {
        painter.add(bar.polygon(&FRAME_OUTLINE, frame_tint));
        &FRAME_HOLE
    };
    let top = BAR.bottom() - BAR.height() * fill;
    let mut real = false;
    for part in shown {
        painter.add(bar.polygon(part, BODY));
        let level = hud_art::cut_top(part, top);
        match style.fill {
            Some(color) => {
                if level.len() >= 3 {
                    painter.add(bar.polygon(&level, solid_fill(style, color, fill)));
                }
            }
            None => {
                if images.paint_placed(painter, art::HEALTH_FILL, bar, BAR, &level, fill_tint) {
                    real = true;
                } else if level.len() >= 3 {
                    painter.add(bar.polygon(&level, paper));
                }
            }
        }
    }
    if style.ticks {
        let tick = Stroke::new(3.0 * scale, OFF_BLACK.gamma_multiply(0.45));
        for (height, large) in ticks(MAX_HEALTH) {
            let y = BAR.bottom() - BAR.height() * height;
            let reach = BAR.left() + BAR.width() * if large { 1.0 } else { 0.3 };
            for part in shown {
                if let Some((left, right)) = span(part, y)
                    && right.min(reach) > left
                {
                    let ends = [bar.at(pos2(left, y)), bar.at(pos2(right.min(reach), y))];
                    painter.line_segment(ends, tick);
                }
            }
        }
    }
    if framed {
        images.paint_placed(
            painter,
            art::HEALTH_FRAME,
            bar,
            FRAME_RECT,
            &corners(FRAME_RECT),
            frame_tint,
        );
    }
    real
}

fn solid_fill(style: &HealthStyle, color: Rgb, fill: f32) -> Color32 {
    let [r, g, b] = color.0;
    match (style.color_by_health, is_low(fill), fill < 0.75) {
        (true, true, _) => LOW,
        (true, false, true) => HURT,
        _ => Color32::from_rgb(r, g, b),
    }
}

/// A straight, rounded or wedge bar of `size` game px.
fn draw_shaped(
    painter: &Painter,
    images: &mut Images,
    style: &HealthStyle,
    bar: Place,
    size: Vec2,
    fill: f32,
) -> bool {
    let low = is_low(fill);
    let scale = bar.scale;
    let shape = outline(style.shape, size.x, size.y);
    painter.add(bar.polygon(&shape, BODY));
    let level = hud_art::cut_top(&shape, size.y * (1.0 - fill));
    let mut real = false;
    if level.len() >= 3 {
        match style.fill {
            Some(color) => {
                painter.add(bar.polygon(&level, solid_fill(style, color, fill)));
            }
            None => {
                let tint = if low { LOW } else { Color32::WHITE };
                let rect = Rect::from_min_size(Pos2::ZERO, size);
                if images.paint_placed(painter, art::HEALTH_FILL, bar, rect, &level, tint) {
                    real = true;
                } else {
                    painter.add(bar.polygon(&level, if low { LOW } else { PAPER }));
                }
            }
        }
    }
    if style.ticks {
        let tick = Stroke::new((2.0 * scale).max(1.0), OFF_BLACK.gamma_multiply(0.45));
        for (height, large) in ticks(MAX_HEALTH) {
            let y = size.y * (1.0 - height);
            if let Some((left, right)) = span(&shape, y) {
                let reach = left + (right - left) * if large { 1.0 } else { 0.35 };
                painter.line_segment([bar.at(pos2(left, y)), bar.at(pos2(reach, y))], tick);
            }
        }
    }
    if style.frame {
        let color = match (style.shape, low) {
            (BarShape::Wedge, true) => FRAME_LOW,
            _ => FRAME,
        };
        let width = if style.shape == BarShape::Wedge {
            3.0
        } else {
            2.0
        };
        let mut ring: Vec<Pos2> = shape.iter().map(|&p| bar.at(p)).collect();
        ring.push(ring[0]);
        painter.add(egui::Shape::line(ring, Stroke::new(width * scale, color)));
    }
    real
}

/// The number block: backer, current health and max, upright at the game's spot.
fn draw_number(
    painter: &Painter,
    images: &mut Images,
    style: &HealthStyle,
    number: Place,
    fill: f32,
    number_scale: f32,
) -> Rect {
    let low = is_low(fill);
    let scale = number.scale;
    if !style.hide_backer {
        let backer = BACKER_RECT;
        let drawn = images.paint_placed(
            painter,
            art::HEALTH_BACKER,
            number,
            backer,
            &corners(backer),
            BACKER,
        );
        if !drawn {
            let at = |x: f32, y: f32| {
                pos2(
                    backer.left() + backer.width() * x / 260.0,
                    backer.top() + backer.height() * y / 208.0,
                )
            };
            let shape = [
                at(13.6, 98.0),
                at(239.1, 13.6),
                at(259.1, 166.0),
                at(3.6, 195.0),
            ];
            painter.add(number.polygon(&shape, BACKER));
        }
    }
    let digits = if low { 36.0 } else { 32.0 };
    let current = ((fill * MAX_HEALTH as f32) as u32).to_string();
    // The game writes "/ 4557"; numericSans draws that slash nearly upright.
    let max = format!("| {MAX_HEALTH}");
    let big = font(style, digits * scale * number_scale);
    let small = font(style, 13.0 * scale);
    let max_color = HEALTHY.gamma_multiply(if style.clear_max_health { 0.85 } else { 0.5 });
    let width = |text: &str, font: &FontId| {
        painter
            .layout_no_wrap(text.to_string(), font.clone(), TEXT_PROBE)
            .size()
            .x
            / scale
    };
    let current_width = width(&current, &big);
    let row = style.number_layout == NumberLayout::Row && !style.hide_max;
    let y = number_y(number_scale);
    let (current_right, max_right, max_y) = if row {
        let max_width = width(&max, &small);
        let right = NUMBER_AT.x + 8.0;
        (right - max_width - 6.0, right, y + 9.0 * number_scale)
    } else {
        (
            NUMBER_AT.x,
            MAX_AT.x,
            y + MAX_AT.y - NUMBER_AT.y + 14.0 * (number_scale - 1.0),
        )
    };
    placed_text(
        painter,
        number,
        pos2(current_right, y),
        Align2::RIGHT_CENTER,
        &current,
        big,
        number_color(style, fill),
        (3.0 * scale).max(1.4),
    );
    if !style.hide_max {
        let turn = if row || style.angle != BarAngle::Tilted {
            0.0
        } else {
            -3.0
        };
        placed_text(
            painter,
            number.turned(turn),
            pos2(max_right, max_y),
            Align2::RIGHT_CENTER,
            &max,
            small,
            max_color,
            0.0,
        );
    }
    let half = 18.0 * number_scale;
    let bottom = if row || style.hide_max {
        y + half
    } else {
        max_y + 8.0
    };
    screen_rect(
        number,
        Rect::from_min_max(
            pos2(current_right - current_width - 4.0, y - half),
            pos2(max_right.max(current_right) + 4.0, bottom),
        ),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn span_reads_the_ruler_width_at_a_height() {
        let (left, right) = span(&RULER, 106.0).unwrap();
        assert!((left - 0.7).abs() < 0.2, "{left}");
        assert!((right - 41.5).abs() < 0.5, "{right}");
        assert_eq!(span(&RULER, -5.0), None);
        assert_eq!(span(&RULER, 230.0), None);
    }

    #[test]
    fn ticks_every_250_and_full_width_every_1000() {
        let ticks = ticks(4557);
        assert_eq!(ticks.len(), 18);
        let large: Vec<f32> = ticks.iter().filter(|t| t.1).map(|t| t.0).collect();
        assert_eq!(large.len(), 4);
        assert!((large[0] - 1000.0 / 4557.0).abs() < 1e-6);
        assert!(ticks.iter().all(|t| t.0 > 0.0 && t.0 < 1.0));
    }

    #[test]
    fn the_frame_hole_parts_sit_inside_the_frame() {
        for part in FRAME_HOLE {
            for at in part {
                let (left, right) = span(&FRAME_OUTLINE, at.y).unwrap();
                assert!(at.x >= left && at.x <= right, "{at:?}");
            }
        }
    }
}
