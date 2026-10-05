//! PNG in and out, as 8-bit RGBA. Every PNG colour type and depth is normalised to
//! RGBA8 on read, so the encoders see one pixel layout.

use std::io::Cursor;

/// An 8-bit RGBA image, rows top-down, `width * height * 4` bytes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RgbaImage {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u8>,
}

impl RgbaImage {
    /// `None` when `pixels` is not exactly `width * height * 4` bytes.
    pub fn new(width: u32, height: u32, pixels: Vec<u8>) -> Option<RgbaImage> {
        (pixels.len() == width as usize * height as usize * 4).then_some(RgbaImage {
            width,
            height,
            pixels,
        })
    }
}

/// Decoding never allocates more than this, whatever the file claims.
pub const DECODE_LIMIT: usize = 256 << 20;

#[derive(Debug, thiserror::Error)]
pub enum PngError {
    #[error("not a readable PNG: {0}")]
    Decode(String),
    #[error("could not write PNG: {0}")]
    Encode(String),
}

pub fn read(bytes: &[u8]) -> Result<RgbaImage, PngError> {
    let bad = |e: ::png::DecodingError| PngError::Decode(e.to_string());
    let mut decoder = ::png::Decoder::new_with_limits(
        Cursor::new(bytes),
        ::png::Limits {
            bytes: DECODE_LIMIT,
        },
    );
    decoder.set_transformations(::png::Transformations::normalize_to_color8());
    let mut reader = decoder.read_info().map_err(bad)?;
    let size = reader
        .output_buffer_size()
        .ok_or_else(|| PngError::Decode("image too large".into()))?;
    let mut buf = vec![0; size];
    let info = reader.next_frame(&mut buf).map_err(bad)?;
    buf.truncate(info.buffer_size());
    let pixels = match info.color_type {
        ::png::ColorType::Rgba => buf,
        ::png::ColorType::Rgb => buf
            .as_chunks::<3>()
            .0
            .iter()
            .flat_map(|&[r, g, b]| [r, g, b, 255])
            .collect(),
        ::png::ColorType::GrayscaleAlpha => buf
            .as_chunks::<2>()
            .0
            .iter()
            .flat_map(|&[g, a]| [g, g, g, a])
            .collect(),
        ::png::ColorType::Grayscale => buf.iter().flat_map(|&g| [g, g, g, 255]).collect(),
        ::png::ColorType::Indexed => {
            return Err(PngError::Decode("palette was not expanded".into()));
        }
    };
    RgbaImage::new(info.width, info.height, pixels)
        .ok_or_else(|| PngError::Decode("pixel data does not match the size".into()))
}

pub fn write(image: &RgbaImage) -> Result<Vec<u8>, PngError> {
    let bad = |e: ::png::EncodingError| PngError::Encode(e.to_string());
    let mut out = Vec::new();
    let mut encoder = ::png::Encoder::new(&mut out, image.width, image.height);
    encoder.set_color(::png::ColorType::Rgba);
    encoder.set_depth(::png::BitDepth::Eight);
    let mut writer = encoder.write_header().map_err(bad)?;
    writer.write_image_data(&image.pixels).map_err(bad)?;
    writer.finish().map_err(bad)?;
    Ok(out)
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// A `width` x `height` image whose pixel (x, y) is (x, y, x ^ y, 255 - x) mod 256.
    pub fn pattern(width: u32, height: u32) -> RgbaImage {
        let mut pixels = Vec::new();
        for y in 0..height {
            for x in 0..width {
                pixels.extend_from_slice(&[x as u8, y as u8, (x ^ y) as u8, 255 - x as u8]);
            }
        }
        RgbaImage::new(width, height, pixels).unwrap()
    }

    fn encode(width: u32, height: u32, color: ::png::ColorType, data: &[u8]) -> Vec<u8> {
        let mut out = Vec::new();
        let mut e = ::png::Encoder::new(&mut out, width, height);
        e.set_color(color);
        e.set_depth(::png::BitDepth::Eight);
        let mut w = e.write_header().unwrap();
        w.write_image_data(data).unwrap();
        w.finish().unwrap();
        out
    }

    #[test]
    fn round_trips_rgba() {
        let img = pattern(37, 21);
        let bytes = write(&img).unwrap();
        assert_eq!(&bytes[..8], b"\x89PNG\r\n\x1a\n");
        assert_eq!(read(&bytes).unwrap(), img);
    }

    #[test]
    fn normalises_rgb_and_grey_to_rgba() {
        let rgb = read(&encode(2, 1, ::png::ColorType::Rgb, &[1, 2, 3, 4, 5, 6])).unwrap();
        assert_eq!(rgb.pixels, [1, 2, 3, 255, 4, 5, 6, 255]);
        let grey = read(&encode(2, 1, ::png::ColorType::Grayscale, &[7, 9])).unwrap();
        assert_eq!(grey.pixels, [7, 7, 7, 255, 9, 9, 9, 255]);
        let ga = read(&encode(1, 1, ::png::ColorType::GrayscaleAlpha, &[8, 100])).unwrap();
        assert_eq!(ga.pixels, [8, 8, 8, 100]);
    }

    #[test]
    fn refuses_junk_and_truncation() {
        assert!(matches!(read(b"not a png"), Err(PngError::Decode(_))));
        let bytes = write(&pattern(16, 16)).unwrap();
        assert!(read(&bytes[..bytes.len() / 2]).is_err());
        assert!(RgbaImage::new(2, 2, vec![0; 15]).is_none());
    }
}
