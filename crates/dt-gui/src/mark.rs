//! DeadTune's mark on a 64-unit grid: a brass cog whose face is a spoked wheel, with a
//! keyhole through the hub. Plain std so the build scripts can load it with `#[path]` and
//! draw the Windows file icon from the same shapes the sidebar paints. Original art; it
//! borrows only the game's era, not its logo.

use std::f32::consts::TAU;

/// The brass of `theme::ACCENT`.
pub const BRASS: [u8; 3] = [240, 179, 65];
pub const GRID: f32 = 64.0;
pub const CENTER: (f32, f32) = (32.0, 32.0);
const TEETH: usize = 8;
const BODY_RADIUS: f32 = 22.0;
const TOOTH_BASE: f32 = 20.0;
const TOOTH_TIP: f32 = 30.0;
/// Half-angles, in radians, of a tooth's base and tip.
const TOOTH_BASE_HALF: f32 = 0.30;
const TOOTH_TIP_HALF: f32 = 0.17;
/// Inner and outer radius of the groove inside the rim, and of the one around the hub.
const RIM_GROOVE: (f32, f32) = (16.6, 18.2);
const HUB_GROOVE: (f32, f32) = (10.8, 12.4);
/// Half-width of the slots between the spokes, which sit between the teeth.
const SLOT_HALF: f32 = 0.75;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Ink {
    Brass,
    /// Cut through to whatever is behind the mark.
    Cut,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Part {
    Circle {
        center: (f32, f32),
        radius: f32,
    },
    /// Convex, clockwise in screen space.
    Polygon(Vec<(f32, f32)>),
}

impl Part {
    fn contains(&self, (x, y): (f32, f32)) -> bool {
        match self {
            Part::Circle { center, radius } => {
                (x - center.0).powi(2) + (y - center.1).powi(2) <= radius * radius
            }
            Part::Polygon(pts) => (0..pts.len()).all(|i| {
                let (a, b) = (pts[i], pts[(i + 1) % pts.len()]);
                (b.0 - a.0) * (y - a.1) - (b.1 - a.1) * (x - a.0) >= 0.0
            }),
        }
    }
}

fn polar(r: f32, a: f32) -> (f32, f32) {
    (CENTER.0 + r * a.sin(), CENTER.1 - r * a.cos())
}

fn disc(radius: f32) -> Part {
    Part::Circle {
        center: CENTER,
        radius,
    }
}

/// The mark as layers painted in order, each over the last; one tooth points straight up.
pub fn layers() -> Vec<(Ink, Part)> {
    let mut out = vec![(Ink::Brass, disc(BODY_RADIUS))];
    out.extend((0..TEETH).map(|i| {
        let a = i as f32 * TAU / TEETH as f32;
        let tooth = Part::Polygon(vec![
            polar(TOOTH_BASE, a - TOOTH_BASE_HALF),
            polar(TOOTH_TIP, a - TOOTH_TIP_HALF),
            polar(TOOTH_TIP, a + TOOTH_TIP_HALF),
            polar(TOOTH_BASE, a + TOOTH_BASE_HALF),
        ]);
        (Ink::Brass, tooth)
    }));
    for (inner, outer) in [RIM_GROOVE, HUB_GROOVE] {
        out.push((Ink::Cut, disc(outer)));
        out.push((Ink::Brass, disc(inner)));
    }
    out.extend((0..TEETH).map(|i| {
        let a = (i as f32 + 0.5) * TAU / TEETH as f32;
        let side = (a.cos() * SLOT_HALF, a.sin() * SLOT_HALF);
        let (inner, outer) = (polar(HUB_GROOVE.1, a), polar(RIM_GROOVE.0, a));
        let slot = Part::Polygon(vec![
            (inner.0 - side.0, inner.1 - side.1),
            (outer.0 - side.0, outer.1 - side.1),
            (outer.0 + side.0, outer.1 + side.1),
            (inner.0 + side.0, inner.1 + side.1),
        ]);
        (Ink::Cut, slot)
    }));
    out.push((
        Ink::Cut,
        Part::Circle {
            center: (32.0, 29.0),
            radius: 4.4,
        },
    ));
    out.push((
        Ink::Cut,
        Part::Polygon(vec![(30.1, 30.5), (33.9, 30.5), (35.2, 40.0), (28.8, 40.0)]),
    ));
    out
}

/// Straight RGBA, `side` pixels square: brass where the mark is, transparent elsewhere
/// and through every cut. Edges are 8x8 supersampled.
pub fn rgba(side: u32) -> Vec<u8> {
    const SAMPLES: u32 = 8;
    let layers = layers();
    let unit = GRID / side as f32;
    let mut out = Vec::with_capacity((side * side * 4) as usize);
    for py in 0..side {
        for px in 0..side {
            let mut hits = 0u32;
            for sy in 0..SAMPLES {
                for sx in 0..SAMPLES {
                    let p = (
                        (px as f32 + (sx as f32 + 0.5) / SAMPLES as f32) * unit,
                        (py as f32 + (sy as f32 + 0.5) / SAMPLES as f32) * unit,
                    );
                    let top = layers.iter().rev().find(|(_, part)| part.contains(p));
                    if matches!(top, Some((Ink::Brass, _))) {
                        hits += 1;
                    }
                }
            }
            let alpha = (hits * 255 + SAMPLES * SAMPLES / 2) / (SAMPLES * SAMPLES);
            out.extend_from_slice(&BRASS);
            out.push(alpha as u8);
        }
    }
    out
}

/// A Windows .ico holding the mark at the sizes Explorer and the taskbar ask for, each an
/// uncompressed 32-bit BMP so no image encoder is needed. Only the build scripts call it.
#[allow(dead_code)]
pub fn ico() -> Vec<u8> {
    const SIZES: [u32; 7] = [16, 20, 24, 32, 48, 64, 256];
    let images: Vec<Vec<u8>> = SIZES.iter().map(|&s| bmp(s)).collect();
    let mut out = Vec::new();
    out.extend_from_slice(&[0, 0, 1, 0]);
    out.extend_from_slice(&(SIZES.len() as u16).to_le_bytes());
    let mut offset = 6 + 16 * SIZES.len() as u32;
    for (&side, image) in SIZES.iter().zip(&images) {
        let byte = if side >= 256 { 0 } else { side as u8 };
        out.extend_from_slice(&[byte, byte, 0, 0]);
        out.extend_from_slice(&1u16.to_le_bytes());
        out.extend_from_slice(&32u16.to_le_bytes());
        out.extend_from_slice(&(image.len() as u32).to_le_bytes());
        out.extend_from_slice(&offset.to_le_bytes());
        offset += image.len() as u32;
    }
    for image in images {
        out.extend_from_slice(&image);
    }
    out
}

/// BITMAPINFOHEADER, bottom-up BGRA rows, then the all-clear AND mask the format requires.
#[allow(dead_code)]
fn bmp(side: u32) -> Vec<u8> {
    let pixels = rgba(side);
    let mask_row = side.div_ceil(32) * 4;
    let mut out = Vec::new();
    for v in [40, side, side * 2] {
        out.extend_from_slice(&v.to_le_bytes());
    }
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&32u16.to_le_bytes());
    for v in [0, side * side * 4 + mask_row * side, 0, 0, 0, 0] {
        out.extend_from_slice(&v.to_le_bytes());
    }
    for row in (0..side as usize).rev() {
        let start = row * side as usize * 4;
        for px in pixels[start..start + side as usize * 4].chunks(4) {
            out.extend_from_slice(&[px[2], px[1], px[0], px[3]]);
        }
    }
    out.resize(out.len() + (mask_row * side) as usize, 0);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn alpha(pixels: &[u8], side: u32, x: u32, y: u32) -> u8 {
        pixels[((y * side + x) * 4 + 3) as usize]
    }

    #[test]
    fn polygons_wind_clockwise_and_stay_on_the_grid() {
        for (_, part) in layers() {
            let Part::Polygon(pts) = part else { continue };
            let n = pts.len();
            for i in 0..n {
                let (a, b, c) = (pts[i], pts[(i + 1) % n], pts[(i + 2) % n]);
                assert!((b.0 - a.0) * (c.1 - b.1) - (b.1 - a.1) * (c.0 - b.0) > 0.0);
                assert!((0.0..=GRID).contains(&a.0) && (0.0..=GRID).contains(&a.1));
            }
        }
    }

    #[test]
    fn the_cuts_and_corners_are_clear_and_the_brass_solid() {
        let side = 64;
        let px = rgba(side);
        assert_eq!(px.len(), (side * side * 4) as usize);
        assert_eq!(alpha(&px, side, 31, 28), 0, "keyhole");
        assert_eq!(alpha(&px, side, 31, 37), 0, "keyhole stem");
        assert_eq!(alpha(&px, side, 31, 40), 255, "hub below the keyhole");
        assert_eq!(alpha(&px, side, 31, 43), 0, "hub groove");
        assert_eq!(alpha(&px, side, 31, 46), 255, "spoke");
        assert_eq!(alpha(&px, side, 31, 49), 0, "rim groove");
        assert_eq!(alpha(&px, side, 31, 52), 255, "rim");
        assert_eq!(alpha(&px, side, 31, 4), 255, "top tooth");
        assert_eq!(alpha(&px, side, 4, 31), 255, "left tooth");
        assert_eq!(alpha(&px, side, 0, 0), 0, "corner");
        assert_eq!(alpha(&px, side, 10, 4), 0, "gap between teeth");
    }

    #[test]
    fn ico_entries_point_at_their_images() {
        let ico = ico();
        let count = u16::from_le_bytes([ico[4], ico[5]]) as usize;
        let mut end = 6 + 16 * count;
        for i in 0..count {
            let e = &ico[6 + 16 * i..6 + 16 * (i + 1)];
            let side = if e[0] == 0 { 256 } else { e[0] as u32 };
            let len = u32::from_le_bytes(e[8..12].try_into().unwrap()) as usize;
            let offset = u32::from_le_bytes(e[12..16].try_into().unwrap()) as usize;
            assert_eq!(offset, end);
            assert_eq!(
                len,
                40 + (side * side * 4 + side.div_ceil(32) * 4 * side) as usize
            );
            end += len;
        }
        assert_eq!(end, ico.len());
    }
}
