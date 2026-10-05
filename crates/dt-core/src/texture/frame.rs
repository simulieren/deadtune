//! Where a player's picture sits in the game's image slot: a window over the picture, in
//! its own pixels, that becomes the whole slot. The window may reach past the picture's
//! edges (that part is transparent), so one rectangle covers cropping, zooming in, shrinking
//! the art inside the slot and moving it off centre.

use super::png::RgbaImage;

/// A window over a picture, in the picture's pixels; `x` and `y` may be negative.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct Crop {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

/// Smallest and largest scale [`Crop::window`] makes, relative to the picture's pixels.
pub const MIN_SCALE: f64 = 0.02;
pub const MAX_SCALE: f64 = 16.0;

/// Slot pixels per picture pixel that show the whole picture inside the slot.
pub fn fit_scale(picture: (u32, u32), slot: (u32, u32)) -> f64 {
    (f64::from(slot.0) / f64::from(picture.0.max(1)))
        .min(f64::from(slot.1) / f64::from(picture.1.max(1)))
}

/// Slot pixels per picture pixel that cover the whole slot with the picture.
pub fn fill_scale(picture: (u32, u32), slot: (u32, u32)) -> f64 {
    (f64::from(slot.0) / f64::from(picture.0.max(1)))
        .max(f64::from(slot.1) / f64::from(picture.1.max(1)))
}

impl Crop {
    /// The slot-shaped window drawing the picture at `scale` slot pixels per picture pixel,
    /// centred on `centre` (picture pixels), which stays on the picture.
    pub fn window(picture: (u32, u32), slot: (u32, u32), scale: f64, centre: (f64, f64)) -> Crop {
        let scale = scale.clamp(MIN_SCALE, MAX_SCALE);
        let side = |s: u32| ((f64::from(s) / scale).round() as u32).max(1);
        let (width, height) = (side(slot.0), side(slot.1));
        let cx = centre.0.clamp(0.0, f64::from(picture.0));
        let cy = centre.1.clamp(0.0, f64::from(picture.1));
        Crop {
            x: (cx - f64::from(width) / 2.0).round() as i32,
            y: (cy - f64::from(height) / 2.0).round() as i32,
            width,
            height,
        }
    }

    /// Slot pixels per picture pixel this window draws at.
    pub fn scale(&self, slot: (u32, u32)) -> f64 {
        f64::from(slot.0) / f64::from(self.width.max(1))
    }

    pub fn centre(&self) -> (f64, f64) {
        (
            f64::from(self.x) + f64::from(self.width) / 2.0,
            f64::from(self.y) + f64::from(self.height) / 2.0,
        )
    }

    /// The window's pixels, transparent where it reaches past the picture.
    pub fn apply(&self, image: &RgbaImage) -> RgbaImage {
        let (w, h) = (self.width as usize, self.height as usize);
        let mut pixels = vec![0u8; w * h * 4];
        let x0 = i64::from(self.x).max(0);
        let x1 = (i64::from(self.x) + w as i64).min(i64::from(image.width));
        for row in 0..h {
            let sy = i64::from(self.y) + row as i64;
            if sy < 0 || sy >= i64::from(image.height) || x0 >= x1 {
                continue;
            }
            let from = (sy as usize * image.width as usize + x0 as usize) * 4;
            let len = (x1 - x0) as usize * 4;
            let to = (row * w + (x0 - i64::from(self.x)) as usize) * 4;
            pixels[to..to + len].copy_from_slice(&image.pixels[from..from + len]);
        }
        RgbaImage {
            width: self.width,
            height: self.height,
            pixels,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn numbered(width: u32, height: u32) -> RgbaImage {
        let pixels = (0..width * height)
            .flat_map(|i| [i as u8, 0, 0, 255])
            .collect();
        RgbaImage::new(width, height, pixels).unwrap()
    }

    #[test]
    fn fit_and_fill_scales() {
        assert_eq!(fit_scale((200, 100), (50, 50)), 0.25);
        assert_eq!(fill_scale((200, 100), (50, 50)), 0.5);
        assert_eq!(fit_scale((10, 10), (40, 20)), 2.0);
    }

    #[test]
    fn a_fill_window_is_slot_shaped_and_centred() {
        let crop = Crop::window((200, 100), (50, 50), 0.5, (100.0, 50.0));
        assert_eq!(
            crop,
            Crop {
                x: 50,
                y: 0,
                width: 100,
                height: 100
            }
        );
        assert_eq!(crop.scale((50, 50)), 0.5);
        assert_eq!(crop.centre(), (100.0, 50.0));
    }

    #[test]
    fn a_fit_window_reaches_past_the_picture() {
        let crop = Crop::window((200, 100), (50, 50), 0.25, (100.0, 50.0));
        assert_eq!(
            (crop.x, crop.y, crop.width, crop.height),
            (0, -50, 200, 200)
        );
    }

    #[test]
    fn the_centre_stays_on_the_picture_and_the_scale_in_range() {
        let crop = Crop::window((10, 10), (10, 10), 1.0, (-40.0, 99.0));
        assert_eq!(crop.centre(), (0.0, 10.0));
        let tiny = Crop::window((10, 10), (10, 10), 0.0, (5.0, 5.0));
        assert_eq!(tiny.width, 500);
        let huge = Crop::window((10, 10), (10, 10), 1000.0, (5.0, 5.0));
        assert_eq!(huge.width, 1);
    }

    #[test]
    fn apply_copies_the_overlap_and_leaves_the_rest_clear() {
        let image = numbered(4, 3);
        let inside = Crop {
            x: 1,
            y: 1,
            width: 2,
            height: 2,
        }
        .apply(&image);
        assert_eq!(
            inside
                .pixels
                .as_chunks::<4>()
                .0
                .iter()
                .map(|p| p[0])
                .collect::<Vec<_>>(),
            [5, 6, 9, 10]
        );
        let past = Crop {
            x: -1,
            y: 2,
            width: 3,
            height: 2,
        }
        .apply(&image);
        let px: Vec<[u8; 4]> = past.pixels.as_chunks::<4>().0.to_vec();
        assert_eq!(px[0], [0, 0, 0, 0]);
        assert_eq!(px[1], [8, 0, 0, 255]);
        assert_eq!(px[2], [9, 0, 0, 255]);
        assert!(px[3..].iter().all(|p| *p == [0, 0, 0, 0]));
        let away = Crop {
            x: 50,
            y: 50,
            width: 2,
            height: 2,
        }
        .apply(&image);
        assert!(away.pixels.iter().all(|b| *b == 0));
    }
}
