//! The picture instrument: a line sweeps a stored picture from left to right; every row is a sine wave whose
//! loudness on the left follows the red of the pixel under the line and on the right the green. The picture
//! travels inside the block as a plain uncompressed bitmap. The header numbers were placed from presets with
//! the public manual beside them; pitch and time scales are estimates (docs/PARITY.md, "Instruments").

use std::f32::consts::TAU;

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Image {
    pub w: usize,
    pub h: usize,
    /// Red then green, 0..1, row 0 at the bottom, `w * h * 2` long.
    pub px: Vec<f32>,
    /// The span of the whole height: cents when `cents`, else cycles per second.
    pub range: f32,
    /// Frames (at 44100 a second) one pixel lasts.
    pub length: f32,
    pub cents: bool,
    pub looped: bool,
}

fn int(b: &[u8], o: usize) -> Option<i32> {
    Some(i32::from_le_bytes(b.get(o..o + 4)?.try_into().ok()?))
}

impl Image {
    pub fn parse(state: &[u8]) -> Option<Self> {
        let range = int(state, 4)? as f32;
        let length = int(state, 8)? as f32;
        let cents = int(state, 12)? != 0;
        let looped = int(state, 24)? != 0;
        let limit = (int(state, 60)?.clamp(1, 256)) as usize;
        let size = int(state, 64)? as usize;
        let bmp = state.get(68..68 + size)?;
        if bmp.get(..2)? != b"BM" || int(bmp, 30)? != 0 {
            return None;
        }
        let data = int(bmp, 10)? as usize;
        let w = int(bmp, 18)?;
        let rows = int(bmp, 22)?;
        let bits = u16::from_le_bytes(bmp.get(28..30)?.try_into().ok()?) as usize;
        if !(1..=8192).contains(&w) || rows == 0 || rows.unsigned_abs() > 4096 || !matches!(bits, 16 | 24 | 32) {
            return None;
        }
        let (w, full) = (w as usize, rows.unsigned_abs() as usize);
        let stride = (w * bits / 8 + 3) & !3;
        let body = bmp.get(data..data + stride * full)?;
        // taller pictures are squeezed to the height the block allows
        let h = full.min(limit);
        let mut px = vec![0.0f32; w * h * 2];
        let mut hits = vec![0u16; h];
        for row in 0..full {
            // bitmaps are stored bottom row first unless the height is negative
            let y = if rows > 0 { row } else { full - 1 - row } * h / full;
            hits[y] += 1;
            let line = &body[row * stride..];
            for x in 0..w {
                let (r, g) = match bits {
                    16 => {
                        let v = u16::from_le_bytes([line[x * 2], line[x * 2 + 1]]);
                        (((v >> 10) & 31) as f32 / 31.0, ((v >> 5) & 31) as f32 / 31.0)
                    }
                    n => (line[x * n / 8 + 2] as f32 / 255.0, line[x * n / 8 + 1] as f32 / 255.0),
                };
                px[(y * w + x) * 2] += r;
                px[(y * w + x) * 2 + 1] += g;
            }
        }
        for y in 0..h {
            let n = hits[y].max(1) as f32;
            for v in &mut px[y * w * 2..(y + 1) * w * 2] {
                *v /= n;
            }
        }
        Some(Self { w, h, px, range, length: length.max(1.0), cents, looped })
    }

    /// Seconds one pass over the picture takes.
    pub fn seconds(&self) -> f32 {
        self.w as f32 * self.length / 44100.0
    }
}

#[derive(Debug, Clone)]
pub struct Voice {
    img: std::sync::Arc<Image>,
    sr: f32,
    /// One turning pointer per row: (cos, sin) and the turn per frame.
    rows: Vec<[f32; 4]>,
    /// The column the line is on, in pixels.
    at: f32,
    step: f32,
    level: [Vec<f32>; 2],
    k: u32,
    vel: f32,
    fade: f32,
    fading: bool,
}

impl Voice {
    pub fn start(img: &std::sync::Arc<Image>, sr: f32, vel: f32, dphase: f32) -> Self {
        let base = dphase * sr;
        let rows = (0..img.h)
            .map(|y| {
                let up = y as f32 / img.h as f32 * img.range;
                // the bottom row sits on the played key
                let hz = if img.cents { base * 2f32.powf(up / 1200.0) } else { base + up * base / 261.6256 };
                let w = if hz < sr * 0.45 { TAU * hz / sr } else { 0.0 };
                [1.0, 0.0, w.cos(), w.sin()]
            })
            .collect();
        Self { img: img.clone(), sr, rows, at: 0.0, step: 44100.0 / sr / img.length, level: [vec![0.0; img.h], vec![0.0; img.h]], k: 0, vel, fade: 1.0, fading: false }
    }

    pub fn release(&mut self) -> f32 {
        self.fading = true;
        0.06
    }

    /// Seconds until a picture that plays once has run out.
    pub fn left(&self) -> f32 {
        if self.img.looped { f32::MAX } else { ((self.img.w as f32 - self.at) / self.step / self.sr).max(0.0) }
    }

    pub fn frame(&mut self) -> [f32; 2] {
        let im = &*self.img;
        if self.at >= im.w as f32 {
            if !im.looped {
                return [0.0; 2];
            }
            self.at -= im.w as f32;
        }
        // the levels are read a few hundred times a second and glide in between
        if self.k == 0 {
            self.k = 32;
            let x = (self.at as usize).min(im.w - 1);
            for y in 0..im.h {
                self.level[0][y] = im.px[(y * im.w + x) * 2];
                self.level[1][y] = im.px[(y * im.w + x) * 2 + 1];
            }
        }
        self.k -= 1;
        self.at += self.step;
        let mut o = [0.0f32; 2];
        for (y, r) in self.rows.iter_mut().enumerate() {
            let (l, g) = (self.level[0][y], self.level[1][y]);
            if l + g > 0.0 && r[3] != 0.0 {
                o[0] += r[1] * l;
                o[1] += r[1] * g;
            }
            let c = r[0] * r[2] - r[1] * r[3];
            r[1] = r[0] * r[3] + r[1] * r[2];
            r[0] = c;
        }
        // keep the pointers from drifting off the unit circle
        if self.k == 0 {
            for r in &mut self.rows {
                let n = (r[0] * r[0] + r[1] * r[1]).sqrt().max(1e-9);
                r[0] /= n;
                r[1] /= n;
            }
        }
        if self.fading {
            self.fade *= 1.0 - 1.0 / (0.012 * self.sr);
        }
        let g = 1.2 / (im.h as f32).sqrt() * self.vel * self.fade;
        [o[0] * g, o[1] * g]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A block holding a 4 by 2 picture in 24 bits: the bottom row red on the left half, the top row green on the right half.
    fn block() -> Vec<u8> {
        let mut bmp = vec![0u8; 54];
        bmp[..2].copy_from_slice(b"BM");
        bmp[10..14].copy_from_slice(&54u32.to_le_bytes());
        bmp[14..18].copy_from_slice(&40u32.to_le_bytes());
        bmp[18..22].copy_from_slice(&4u32.to_le_bytes());
        bmp[22..26].copy_from_slice(&2u32.to_le_bytes());
        bmp[26] = 1;
        bmp[28] = 24;
        bmp.extend_from_slice(&[0, 0, 255, 0, 0, 255, 0, 0, 0, 0, 0, 0]);
        bmp.extend_from_slice(&[0, 0, 0, 0, 0, 0, 0, 255, 0, 0, 255, 0]);
        let mut b = vec![0u8; 64];
        b[4..8].copy_from_slice(&1200u32.to_le_bytes());
        b[8..12].copy_from_slice(&1000u32.to_le_bytes());
        b[12] = 1;
        b[60] = 96;
        b.extend_from_slice(&(bmp.len() as u32).to_le_bytes());
        b.extend_from_slice(&bmp);
        b
    }

    #[test]
    fn red_plays_on_the_left_first_and_green_on_the_right_after() {
        let im = std::sync::Arc::new(Image::parse(&block()).unwrap());
        assert_eq!((im.w, im.h), (4, 2));
        assert_eq!(&im.px[..4], &[1.0, 0.0, 1.0, 0.0]);
        let mut v = Voice::start(&im, 44100.0, 1.0, 440.0 / 44100.0);
        let mut sum = |n: usize| (0..n).fold([0.0f32; 2], |a, _| { let f = v.frame(); [a[0] + f[0].abs(), a[1] + f[1].abs()] });
        let first = sum(1900);
        let _ = sum(200);
        let second = sum(1800);
        assert!(first[0] > 100.0 && first[1] < 1.0, "{first:?}");
        assert!(second[1] > 100.0 && second[0] < 1.0, "{second:?}");
        // played once, it ends
        assert!(sum(2000)[0] < 1.0 && v.left() == 0.0);
        assert!(Image::parse(&[0; 40]).is_none());
    }
}
