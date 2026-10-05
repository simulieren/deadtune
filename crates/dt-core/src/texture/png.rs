//! PNG in and out of `RgbaImage`.

use crate::texture::decode::RgbaImage;

#[derive(Debug, thiserror::Error)]
#[error("png: {0}")]
pub struct PngError(pub String);

/// 8-bit RGBA PNG.
pub fn write(image: &RgbaImage) -> Vec<u8> {
    let mut out = Vec::new();
    let mut enc = ::png::Encoder::new(&mut out, image.width, image.height);
    enc.set_color(::png::ColorType::Rgba);
    enc.set_depth(::png::BitDepth::Eight);
    enc.write_header()
        .and_then(|mut w| w.write_image_data(&image.pixels))
        .expect("in-memory PNG encoding cannot fail");
    out
}

/// Any colour type or bit depth, widened to RGBA8: grey copied to r, g and b, a missing
/// alpha set to 255, 16-bit samples truncated to their high byte.
pub fn read(bytes: &[u8]) -> Result<RgbaImage, PngError> {
    let err = |e: ::png::DecodingError| PngError(e.to_string());
    let mut dec = ::png::Decoder::new(std::io::Cursor::new(bytes));
    dec.set_transformations(::png::Transformations::EXPAND | ::png::Transformations::STRIP_16);
    let mut reader = dec.read_info().map_err(err)?;
    let mut buf = vec![
        0;
        reader
            .output_buffer_size()
            .ok_or_else(|| PngError("image too large".into()))?
    ];
    let info = reader.next_frame(&mut buf).map_err(err)?;
    let buf = &buf[..info.buffer_size()];
    let pixels = match info.color_type {
        ::png::ColorType::Rgba => buf.to_vec(),
        ::png::ColorType::Rgb => buf
            .as_chunks::<3>()
            .0
            .iter()
            .flat_map(|p| [p[0], p[1], p[2], 255])
            .collect(),
        ::png::ColorType::GrayscaleAlpha => buf
            .as_chunks::<2>()
            .0
            .iter()
            .flat_map(|p| [p[0], p[0], p[0], p[1]])
            .collect(),
        ::png::ColorType::Grayscale => buf.iter().flat_map(|&g| [g, g, g, 255]).collect(),
        ::png::ColorType::Indexed => {
            return Err(PngError("palette was not expanded".into()));
        }
    };
    Ok(RgbaImage {
        width: info.width,
        height: info.height,
        pixels,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn encode(width: u32, height: u32, color: ::png::ColorType, data: &[u8]) -> Vec<u8> {
        let mut out = Vec::new();
        let mut enc = ::png::Encoder::new(&mut out, width, height);
        enc.set_color(color);
        enc.set_depth(::png::BitDepth::Eight);
        enc.write_header().unwrap().write_image_data(data).unwrap();
        out
    }

    #[test]
    fn rgba_round_trips_exactly() {
        let img = RgbaImage {
            width: 3,
            height: 2,
            pixels: (0..24).map(|i| (i * 37 % 256) as u8).collect(),
        };
        let bytes = write(&img);
        assert_eq!(&bytes[..8], b"\x89PNG\r\n\x1a\n");
        assert_eq!(read(&bytes).unwrap(), img);
    }

    #[test]
    fn gray_and_rgb_widen_to_rgba() {
        let gray = encode(2, 1, ::png::ColorType::Grayscale, &[7, 200]);
        assert_eq!(
            read(&gray).unwrap(),
            RgbaImage {
                width: 2,
                height: 1,
                pixels: vec![7, 7, 7, 255, 200, 200, 200, 255],
            }
        );
        let rgb = encode(1, 2, ::png::ColorType::Rgb, &[1, 2, 3, 4, 5, 6]);
        assert_eq!(
            read(&rgb).unwrap(),
            RgbaImage {
                width: 1,
                height: 2,
                pixels: vec![1, 2, 3, 255, 4, 5, 6, 255],
            }
        );
    }

    #[test]
    fn garbage_is_an_error() {
        assert!(read(b"not a png").is_err());
    }
}
