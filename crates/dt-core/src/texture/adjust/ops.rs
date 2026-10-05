//! The pixel arithmetic behind [`super::Adjust`].

use super::Adjust;
use crate::texture::png::RgbaImage;

/// `rgb` after `adjust`; alpha-only operations return it unchanged.
pub fn adjust_rgb(rgb: [u8; 3], adjust: &Adjust) -> [u8; 3] {
    let _ = (rgb, adjust);
    todo!("workstream A")
}

/// `image` after `adjust`, in place. Alpha is kept except by `Opacity`.
pub fn apply(image: &mut RgbaImage, adjust: &Adjust) {
    let _ = (image, adjust);
    todo!("workstream A")
}
