//! Compiled textures to RGBA8 pixels: the top mip, any mip, or a thumbnail.

use crate::texture::resample::Image;

/// RGBA8, rows top-down, no padding.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RgbaImage {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u8>,
}

impl RgbaImage {
    /// Transparent black.
    pub fn new(width: u32, height: u32) -> RgbaImage {
        RgbaImage {
            width,
            height,
            pixels: vec![0; width as usize * height as usize * 4],
        }
    }

    pub fn pixel(&self, x: u32, y: u32) -> [u8; 4] {
        let i = (y as usize * self.width as usize + x as usize) * 4;
        self.pixels[i..i + 4].try_into().expect("in bounds")
    }

    /// The image area-resampled so its longer side is `max_side`, aspect kept; itself
    /// when already within.
    pub fn fit(&self, max_side: u32) -> RgbaImage {
        let long = self.width.max(self.height);
        if long <= max_side || max_side == 0 {
            return self.clone();
        }
        let scale = |d: u32| ((u64::from(d) * u64::from(max_side)) / u64::from(long)).max(1) as u32;
        let out = Image::new(self.width, self.height, 4, self.pixels.clone())
            .expect("pixel count matches")
            .resample(scale(self.width), scale(self.height));
        RgbaImage {
            width: out.width,
            height: out.height,
            pixels: out.data,
        }
    }
}
