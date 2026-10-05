//! The pixel arithmetic behind [`super::Adjust`].

use super::{Adjust, Blend, Rgb};
use crate::texture::png::RgbaImage;

/// `rgb` after `adjust`; alpha-only operations return it unchanged.
pub fn adjust_rgb(rgb: [u8; 3], adjust: &Adjust) -> [u8; 3] {
    let c = rgb.map(f32::from);
    match *adjust {
        Adjust::Tint { color, strength } => {
            mix(c, per_channel(c, color, |a, b| a * b / 255.0), strength)
        }
        Adjust::Colorize { color, strength } => {
            let (h, s, _) = to_hsl(color.0.map(f32::from));
            mix(c, from_hsl(h, s, luma(c) / 255.0), strength)
        }
        Adjust::Overlay {
            color,
            blend,
            opacity,
        } => mix(
            c,
            per_channel(c, color, |a, b| blended(blend, a, b)),
            opacity,
        ),
        Adjust::Hue { degrees } => {
            let (h, s, l) = to_hsl(c);
            to_u8(from_hsl((h + f32::from(degrees)).rem_euclid(360.0), s, l))
        }
        Adjust::Saturation { percent } => {
            let y = luma(c);
            to_u8(c.map(|v| y + (v - y) * factor(percent)))
        }
        Adjust::Brightness { percent } => {
            let p = f32::from(percent) / 100.0;
            to_u8(c.map(|v| {
                if p > 0.0 {
                    v + (255.0 - v) * p
                } else {
                    v * (1.0 + p)
                }
            }))
        }
        Adjust::Contrast { percent } => to_u8(c.map(|v| 128.0 + (v - 128.0) * factor(percent))),
        Adjust::Opacity { .. } => rgb,
        Adjust::Invert => rgb.map(|v| 255 - v),
        Adjust::Swap { from, to } if from.0 == rgb => to.0,
        Adjust::Swap { .. } => rgb,
    }
}

/// `image` after `adjust`, in place. Alpha is kept except by `Opacity`.
pub fn apply(image: &mut RgbaImage, adjust: &Adjust) {
    for px in image.pixels.as_chunks_mut::<4>().0 {
        if let Adjust::Opacity { percent } = *adjust {
            px[3] = ((u16::from(px[3]) * u16::from(percent) + 50) / 100) as u8;
        } else {
            let rgb = adjust_rgb([px[0], px[1], px[2]], adjust);
            px[..3].copy_from_slice(&rgb);
        }
    }
}

fn blended(blend: Blend, a: f32, b: f32) -> f32 {
    match blend {
        Blend::Normal => b,
        Blend::Multiply => a * b / 255.0,
        Blend::Screen => 255.0 - (255.0 - a) * (255.0 - b) / 255.0,
        Blend::Overlay if a < 128.0 => 2.0 * a * b / 255.0,
        Blend::Overlay => 255.0 - 2.0 * (255.0 - a) * (255.0 - b) / 255.0,
    }
}

fn per_channel(c: [f32; 3], color: Rgb, f: impl Fn(f32, f32) -> f32) -> [f32; 3] {
    let b = color.0.map(f32::from);
    [f(c[0], b[0]), f(c[1], b[1]), f(c[2], b[2])]
}

fn mix(c: [f32; 3], target: [f32; 3], percent: u8) -> [u8; 3] {
    let t = f32::from(percent) / 100.0;
    to_u8([0, 1, 2].map(|i| c[i] + (target[i] - c[i]) * t))
}

fn factor(percent: i16) -> f32 {
    1.0 + f32::from(percent) / 100.0
}

fn luma([r, g, b]: [f32; 3]) -> f32 {
    0.299 * r + 0.587 * g + 0.114 * b
}

fn to_u8(c: [f32; 3]) -> [u8; 3] {
    c.map(|v| v.round().clamp(0.0, 255.0) as u8)
}

/// Hue in degrees, saturation and lightness in 0..=1, from channels in 0..=255.
fn to_hsl(c: [f32; 3]) -> (f32, f32, f32) {
    let [r, g, b] = c.map(|v| v / 255.0);
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let l = (max + min) / 2.0;
    let d = max - min;
    if d == 0.0 {
        return (0.0, 0.0, l);
    }
    let s = d / (1.0 - (2.0 * l - 1.0).abs());
    let h = if max == r {
        ((g - b) / d).rem_euclid(6.0)
    } else if max == g {
        (b - r) / d + 2.0
    } else {
        (r - g) / d + 4.0
    };
    (h * 60.0, s, l)
}

/// Channels in 0..=255 from hue in degrees (0..360), saturation and lightness in 0..=1.
fn from_hsl(h: f32, s: f32, l: f32) -> [f32; 3] {
    let chroma = (1.0 - (2.0 * l - 1.0).abs()) * s;
    let x = chroma * (1.0 - ((h / 60.0).rem_euclid(2.0) - 1.0).abs());
    let (r, g, b) = match (h / 60.0) as u32 % 6 {
        0 => (chroma, x, 0.0),
        1 => (x, chroma, 0.0),
        2 => (0.0, chroma, x),
        3 => (0.0, x, chroma),
        4 => (x, 0.0, chroma),
        _ => (chroma, 0.0, x),
    };
    let m = l - chroma / 2.0;
    [r, g, b].map(|v| (v + m) * 255.0)
}

#[cfg(test)]
mod tests {
    use super::super::{Blend, Rgb, apply_all};
    use super::*;

    const WHITE: [u8; 3] = [255, 255, 255];
    const BLACK: [u8; 3] = [0, 0, 0];
    const GREY: [u8; 3] = [128, 128, 128];
    const RED: [u8; 3] = [255, 0, 0];
    const MIXED: [u8; 3] = [200, 100, 50];
    const SAMPLES: [[u8; 3]; 5] = [WHITE, BLACK, GREY, RED, MIXED];
    const ORANGE: Rgb = Rgb([0xd4, 0x86, 0x0b]);
    const BLUE: Rgb = Rgb([0x4d, 0x75, 0xc3]);

    fn table(adjust: Adjust, expected: [[u8; 3]; 5]) {
        for (rgb, want) in SAMPLES.into_iter().zip(expected) {
            assert_eq!(adjust_rgb(rgb, &adjust), want, "{adjust} on {rgb:?}");
        }
    }

    fn image(pixels: &[[u8; 4]]) -> RgbaImage {
        RgbaImage::new(pixels.len() as u32, 1, pixels.concat()).unwrap()
    }

    #[test]
    fn tint_multiplies_then_mixes_by_strength() {
        let tint = |color, strength| Adjust::Tint { color, strength };
        table(
            tint(Rgb(RED), 100),
            [RED, BLACK, [128, 0, 0], RED, [200, 0, 0]],
        );
        table(
            tint(ORANGE, 60),
            [
                [229, 182, 109],
                BLACK,
                [115, 92, 55],
                [229, 0, 0],
                [180, 72, 21],
            ],
        );
    }

    #[test]
    fn colorize_puts_the_colours_hsl_hue_and_saturation_at_the_pixels_luma_lightness() {
        let colorize = |color, strength| Adjust::Colorize { color, strength };
        table(
            colorize(ORANGE, 100),
            [WHITE, BLACK, [242, 154, 14], [145, 92, 8], [236, 149, 12]],
        );
        table(
            colorize(BLUE, 50),
            [WHITE, BLACK, [97, 118, 159], [147, 32, 57], [131, 102, 118]],
        );
    }

    #[test]
    fn colorize_maps_greys_to_lightness_levels_of_one_hue_with_black_staying_black() {
        let orange = Adjust::Colorize {
            color: ORANGE,
            strength: 100,
        };
        let shades = [WHITE, [230, 230, 230], GREY, BLACK].map(|c| adjust_rgb(c, &orange));
        assert_eq!(
            shades,
            [WHITE, [253, 235, 207], [242, 154, 14], BLACK],
            "hsl(36.7, 0.90, l) for l = luma 1.0, 0.90, 0.50, 0.0"
        );
    }

    #[test]
    fn overlay_blends_the_colour_on_top_then_mixes_by_opacity() {
        let overlay = |color, blend, opacity| Adjust::Overlay {
            color,
            blend,
            opacity,
        };
        table(
            overlay(BLUE, Blend::Normal, 40),
            [
                [184, 200, 231],
                [31, 47, 78],
                [108, 124, 155],
                [184, 47, 78],
                [151, 107, 108],
            ],
        );
        table(
            overlay(ORANGE, Blend::Multiply, 100),
            [ORANGE.0, BLACK, [106, 67, 6], [212, 0, 0], [166, 53, 2]],
        );
        table(
            overlay(BLUE, Blend::Screen, 100),
            [
                WHITE,
                BLUE.0,
                [166, 186, 225],
                [255, 117, 195],
                [217, 171, 207],
            ],
        );
        table(
            overlay(ORANGE, Blend::Overlay, 100),
            [WHITE, BLACK, [212, 134, 12], RED, [236, 105, 4]],
        );
    }

    #[test]
    fn hue_rotates_in_hsl() {
        table(
            Adjust::Hue { degrees: 120 },
            [WHITE, BLACK, GREY, [0, 255, 0], [50, 200, 100]],
        );
        table(
            Adjust::Hue { degrees: -60 },
            [WHITE, BLACK, GREY, [255, 0, 255], [200, 50, 150]],
        );
    }

    #[test]
    fn saturation_scales_the_distance_from_luma() {
        table(
            Adjust::Saturation { percent: -100 },
            [WHITE, BLACK, GREY, [76, 76, 76], [124, 124, 124]],
        );
        table(
            Adjust::Saturation { percent: 50 },
            [WHITE, BLACK, GREY, RED, [238, 88, 13]],
        );
    }

    #[test]
    fn brightness_moves_towards_white_or_black() {
        table(
            Adjust::Brightness { percent: 40 },
            [
                WHITE,
                [102, 102, 102],
                [179, 179, 179],
                [255, 102, 102],
                [222, 162, 132],
            ],
        );
        table(
            Adjust::Brightness { percent: -40 },
            [
                [153, 153, 153],
                BLACK,
                [77, 77, 77],
                [153, 0, 0],
                [120, 60, 30],
            ],
        );
    }

    #[test]
    fn contrast_scales_the_distance_from_128() {
        table(
            Adjust::Contrast { percent: 50 },
            [WHITE, BLACK, GREY, RED, [236, 86, 11]],
        );
        table(
            Adjust::Contrast { percent: -60 },
            [
                [179, 179, 179],
                [77, 77, 77],
                GREY,
                [179, 77, 77],
                [157, 117, 97],
            ],
        );
    }

    #[test]
    fn invert_and_opacity_on_rgb() {
        table(
            Adjust::Invert,
            [BLACK, WHITE, [127, 127, 127], [0, 255, 255], [55, 155, 205]],
        );
        table(Adjust::Opacity { percent: 10 }, SAMPLES);
    }

    #[test]
    fn swap_touches_only_exact_matches() {
        let swap = Adjust::Swap {
            from: Rgb(MIXED),
            to: ORANGE,
        };
        table(swap, [WHITE, BLACK, GREY, RED, ORANGE.0]);
        assert_eq!(adjust_rgb([200, 100, 51], &swap), [200, 100, 51]);
    }

    #[test]
    fn apply_keeps_alpha_and_opacity_scales_only_alpha() {
        let mut img = image(&[[200, 100, 50, 37], [255, 255, 255, 0]]);
        apply(&mut img, &Adjust::Invert);
        assert_eq!(img.pixels, [55, 155, 205, 37, 0, 0, 0, 0]);
        let mut img = image(&[[200, 100, 50, 200], [1, 2, 3, 255], [9, 9, 9, 1]]);
        apply(&mut img, &Adjust::Opacity { percent: 50 });
        assert_eq!(img.pixels, [200, 100, 50, 100, 1, 2, 3, 128, 9, 9, 9, 1]);
    }

    #[test]
    fn apply_all_runs_in_order() {
        let tint = Adjust::Tint {
            color: ORANGE,
            strength: 100,
        };
        let px = [[200, 100, 50, 255], [255, 255, 255, 37]];
        let mut tint_first = image(&px);
        apply_all(&mut tint_first, &[tint, Adjust::Invert]);
        assert_eq!(tint_first.pixels, [89, 202, 253, 255, 43, 121, 244, 37]);
        let mut invert_first = image(&px);
        apply_all(&mut invert_first, &[Adjust::Invert, tint]);
        assert_eq!(invert_first.pixels, [46, 81, 9, 255, 0, 0, 0, 37]);
    }
}
