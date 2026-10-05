//! Area-averaging resample of 8-bit images, for shrinking a texture whose only mip is
//! too large. Every output pixel is the mean of the source area it covers, with
//! fractional weights where that area cuts through a source pixel, so a constant stays
//! constant, integer ratios are exact, and hard alpha edges never ring the way a
//! windowed-sinc filter would.

/// An 8-bit-per-channel image, rows top-down, `channels` bytes per pixel, any channel order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Image {
    pub width: u32,
    pub height: u32,
    pub channels: usize,
    pub data: Vec<u8>,
}

impl Image {
    /// `None` when `data` is not exactly `width * height * channels` bytes.
    pub fn new(width: u32, height: u32, channels: usize, data: Vec<u8>) -> Option<Image> {
        (data.len() == width as usize * height as usize * channels).then_some(Image {
            width,
            height,
            channels,
            data,
        })
    }

    /// The image at `width` x `height`, each output pixel the area mean of the source it
    /// covers, per channel, rounded to nearest. Separable: one weighted source row is
    /// accumulated at a time, so memory stays at one row whatever the input size.
    pub fn resample(&self, width: u32, height: u32) -> Image {
        let cols = coverage(self.width, width);
        let rows = coverage(self.height, height);
        let c = self.channels;
        let stride = self.width as usize * c;
        let mut out = Vec::with_capacity(width as usize * height as usize * c);
        let mut acc = vec![0f32; stride];
        for taps in &rows {
            acc.fill(0.0);
            for &(sy, wy) in taps {
                let row = &self.data[sy * stride..(sy + 1) * stride];
                for (a, &p) in acc.iter_mut().zip(row) {
                    *a += f32::from(p) * wy;
                }
            }
            for taps in &cols {
                for k in 0..c {
                    let v: f32 = taps.iter().map(|&(sx, wx)| acc[sx * c + k] * wx).sum();
                    out.push((v + 0.5).clamp(0.0, 255.0) as u8);
                }
            }
        }
        Image {
            width,
            height,
            channels: c,
            data: out,
        }
    }
}

/// For each output index, the source indices it covers and each one's share of the
/// output's span (the shares sum to 1).
fn coverage(src: u32, dst: u32) -> Vec<Vec<(usize, f32)>> {
    let ratio = f64::from(src) / f64::from(dst);
    (0..dst)
        .map(|o| {
            let start = f64::from(o) * ratio;
            let end = f64::from(o + 1) * ratio;
            let first = start.floor() as usize;
            let last = (end.ceil() as usize).min(src as usize);
            (first..last)
                .filter_map(|i| {
                    let overlap = end.min(i as f64 + 1.0) - start.max(i as f64);
                    (overlap > 0.0).then_some((i, (overlap / ratio) as f32))
                })
                .collect()
        })
        .collect()
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// Peak signal-to-noise ratio in dB over all bytes; infinite for identical inputs.
    pub(crate) fn psnr(a: &[u8], b: &[u8]) -> f64 {
        assert_eq!(a.len(), b.len());
        let mse: f64 = a
            .iter()
            .zip(b)
            .map(|(&x, &y)| (f64::from(x) - f64::from(y)).powi(2))
            .sum::<f64>()
            / a.len() as f64;
        if mse == 0.0 {
            f64::INFINITY
        } else {
            10.0 * (255.0 * 255.0 / mse).log10()
        }
    }

    fn noise(width: u32, height: u32, channels: usize) -> Image {
        let mut state = 0x2545_f491u32;
        let data = (0..width as usize * height as usize * channels)
            .map(|_| {
                state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                (state >> 24) as u8
            })
            .collect();
        Image::new(width, height, channels, data).unwrap()
    }

    /// Smooth test content: sine-cosine products with a different phase per channel.
    fn smooth(width: u32, height: u32, channels: usize) -> Image {
        let mut data = Vec::new();
        for y in 0..height {
            for x in 0..width {
                for k in 0..channels {
                    let u = f64::from(x) / f64::from(width) * 6.0 + k as f64;
                    let v = f64::from(y) / f64::from(height) * 4.0;
                    data.push((127.5 + 120.0 * (u.sin() * v.cos())).round() as u8);
                }
            }
        }
        Image::new(width, height, channels, data).unwrap()
    }

    /// The same area mean, written the slow way: every output pixel integrates its
    /// rectangle over the source in 2D with exact overlaps.
    fn brute_force(img: &Image, width: u32, height: u32) -> Image {
        let rx = f64::from(img.width) / f64::from(width);
        let ry = f64::from(img.height) / f64::from(height);
        let c = img.channels;
        let mut data = Vec::new();
        for oy in 0..height {
            for ox in 0..width {
                let (x0, x1) = (f64::from(ox) * rx, f64::from(ox + 1) * rx);
                let (y0, y1) = (f64::from(oy) * ry, f64::from(oy + 1) * ry);
                for k in 0..c {
                    let mut sum = 0.0;
                    for sy in y0.floor() as usize..(y1.ceil() as usize).min(img.height as usize) {
                        let hy = y1.min(sy as f64 + 1.0) - y0.max(sy as f64);
                        for sx in x0.floor() as usize..(x1.ceil() as usize).min(img.width as usize)
                        {
                            let wx = x1.min(sx as f64 + 1.0) - x0.max(sx as f64);
                            let p = img.data[(sy * img.width as usize + sx) * c + k];
                            sum += f64::from(p) * hy * wx;
                        }
                    }
                    data.push((sum / (rx * ry)).round() as u8);
                }
            }
        }
        Image::new(width, height, c, data).unwrap()
    }

    #[test]
    fn new_checks_the_length() {
        assert!(Image::new(3, 2, 4, vec![0; 24]).is_some());
        assert!(Image::new(3, 2, 4, vec![0; 23]).is_none());
    }

    #[test]
    fn same_size_is_identity() {
        for c in [1, 2, 4] {
            let img = noise(37, 23, c);
            assert_eq!(img.resample(37, 23), img, "{c} channels");
        }
    }

    #[test]
    fn integer_ratio_is_the_exact_block_mean() {
        let img = noise(8, 6, 3);
        let out = img.resample(4, 3);
        for oy in 0..3usize {
            for ox in 0..4usize {
                for k in 0..3 {
                    let at = |x: usize, y: usize| u32::from(img.data[(y * 8 + x) * 3 + k]);
                    let sum = at(2 * ox, 2 * oy)
                        + at(2 * ox + 1, 2 * oy)
                        + at(2 * ox, 2 * oy + 1)
                        + at(2 * ox + 1, 2 * oy + 1);
                    assert_eq!(
                        u32::from(out.data[(oy * 4 + ox) * 3 + k]),
                        (sum + 2) / 4,
                        "({ox},{oy})[{k}]"
                    );
                }
            }
        }
    }

    #[test]
    fn constant_stays_constant_at_an_odd_ratio() {
        let img = Image::new(100, 60, 3, vec![77; 100 * 60 * 3]).unwrap();
        let out = img.resample(37, 11);
        assert_eq!((out.width, out.height, out.channels), (37, 11, 3));
        assert!(out.data.iter().all(|&p| p == 77));
    }

    #[test]
    fn agrees_with_brute_force_area_integration() {
        let img = smooth(512, 384, 4);
        let fast = img.resample(135, 101);
        let slow = brute_force(&img, 135, 101);
        assert_eq!((fast.width, fast.height), (135, 101));
        let worst = fast
            .data
            .iter()
            .zip(&slow.data)
            .map(|(&a, &b)| (i16::from(a) - i16::from(b)).abs())
            .max()
            .unwrap();
        assert!(worst <= 1, "max abs diff {worst}");
        let db = psnr(&fast.data, &slow.data);
        assert!(db > 50.0, "psnr {db}");
    }

    #[test]
    fn conserves_mass() {
        let img = smooth(512, 384, 2);
        let out = img.resample(135, 101);
        let total = |d: &[u8]| d.iter().map(|&p| f64::from(p)).sum::<f64>();
        let scaled = total(&out.data) * (512.0 * 384.0) / (135.0 * 101.0);
        let rel = (scaled - total(&img.data)).abs() / total(&img.data);
        assert!(rel < 0.002, "relative mass error {rel}");
    }

    #[test]
    fn coverage_sums_to_one_and_spans_the_source() {
        for (src, dst) in [(4096, 1080), (10, 3), (7, 7), (5, 9)] {
            let cov = coverage(src, dst);
            assert_eq!(cov.len(), dst as usize);
            for taps in &cov {
                let sum: f32 = taps.iter().map(|t| t.1).sum();
                assert!((sum - 1.0).abs() < 1e-5, "{src}->{dst}: {sum}");
            }
            assert_eq!(cov[0][0].0, 0);
            assert_eq!(cov.last().unwrap().last().unwrap().0, src as usize - 1);
        }
        assert!(coverage(10, 0).is_empty());
        let empty = Image::new(4, 4, 1, vec![9; 16]).unwrap().resample(0, 3);
        assert!(empty.data.is_empty());
    }
}
