//! A pitch mover that keeps the colour of a voice: the sound is cut into grains two cycles long, and the grains
//! are laid down closer together or further apart. The grains themselves are not played faster or slower, so the
//! mouth stays the size it was; playing them faster or slower on purpose moves the colour alone.

pub struct Shifter {
    ring: Vec<[f32; 2]>,
    ola: Vec<[f32; 2]>,
    t: usize,
    next: f64,
    centre: f64,
    /// The listener, for those who do not bring a pitch of their own: a thinned mono ring and its last reading.
    mono: Vec<f32>,
    acc: (f32, u32),
    since: usize,
    heard: f32,
}

impl Shifter {
    pub fn new(sr: f32) -> Self {
        let n = (sr * 0.25) as usize + 64;
        Self { ring: vec![[0.0; 2]; n], ola: vec![[0.0; 2]; n], t: n, next: 0.0, centre: 0.0, mono: vec![0.0; 1024], acc: (0.0, 0), since: 0, heard: 0.0 }
    }

    /// How late the sound comes out, in frames, at the longest cycle.
    pub fn delay(sr: f32) -> usize {
        (sr / 40.0 * 3.0) as usize
    }

    /// Listen to a frame and give the cycle length heard of late, in frames; 0 when there is no clear note.
    pub fn listen(&mut self, x: [f32; 2], sr: f32) -> f32 {
        let by = ((sr / 11_000.0) as u32).max(1);
        self.acc.0 += x[0] + x[1];
        self.acc.1 += 1;
        if self.acc.1 >= by {
            let n = self.mono.len();
            self.mono.rotate_left(1);
            self.mono[n - 1] = self.acc.0 / (2 * by) as f32;
            self.acc = (0.0, 0);
        }
        self.since += 1;
        if self.since >= 512 {
            self.since = 0;
            let rate = sr / by as f32;
            self.heard = match crate::scope::pitch(&self.mono, rate) {
                Some((hz, sure)) if sure > 0.8 && (40.0..=1600.0).contains(&hz) => sr / hz,
                _ => 0.0,
            };
        }
        self.heard
    }

    /// One frame in, one out. `period` is the cycle length in frames (0 = no clear note: the sound passes as it
    /// is), `ratio` how much higher it should sound, `colour` how much higher the colour should sit (1 = kept).
    pub fn step(&mut self, x: [f32; 2], period: f32, ratio: f32, colour: f32, sr: f32) -> [f32; 2] {
        let n = self.ring.len();
        self.ring[self.t % n] = x;
        let voiced = period > 0.0;
        let p = if voiced { period.clamp(16.0, sr / 40.0) } else { sr * 0.005 } as f64;
        self.next -= 1.0;
        if self.next <= 0.0 {
            let f = colour.clamp(0.5, 2.0) as f64;
            let ratio = if voiced { ratio.clamp(0.25, 4.0) as f64 } else { 1.0 };
            // the newest grain that is whole
            let want = self.t as f64 - (sr as f64 / 40.0 * 2.0 + 2.0);
            let mut c = want;
            if voiced {
                // a whole number of cycles on from the last grain, so the grains line up with the voice
                let m = ((want - self.centre) / p).round();
                let lined = self.centre + m * p;
                if (lined - want).abs() <= p {
                    c = lined;
                }
            }
            self.centre = c;
            let half = p.round() as isize;
            let gain = (1.0 / ratio.max(1.0).sqrt()) as f32;
            for k in -half..half {
                let src = c + k as f64 * f;
                if src < (self.t - n + 2) as f64 || src >= (self.t - 1) as f64 {
                    continue;
                }
                let i = src as usize;
                let fr = (src - i as f64) as f32;
                let (a, b) = (self.ring[i % n], self.ring[(i + 1) % n]);
                let w = 0.5 * (1.0 + (std::f32::consts::PI * k as f32 / half as f32).cos()) * gain;
                let o = &mut self.ola[(self.t + (k + half) as usize) % n];
                o[0] += (a[0] + (b[0] - a[0]) * fr) * w;
                o[1] += (a[1] + (b[1] - a[1]) * fr) * w;
            }
            self.next += (p / ratio).max(1.0);
        }
        let out = std::mem::take(&mut self.ola[self.t % n]);
        self.t += 1;
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A tone with a fixed bump in its colour near 1 kHz, whatever the note.
    fn voice(hz: f32, secs: f32, sr: f32) -> Vec<[f32; 2]> {
        (0..(secs * sr) as usize)
            .map(|i| {
                let mut s = 0.0;
                let mut h = 1.0;
                while h * hz < 4000.0 {
                    let d = (h * hz - 1000.0) / 300.0;
                    s += (0.1 + (-d * d).exp()) * (i as f32 * h * hz / sr * std::f32::consts::TAU).sin();
                    h += 1.0;
                }
                [s * 0.2, s * 0.2]
            })
            .collect()
    }

    fn level_at(x: &[f32], hz: f32, sr: f32) -> f32 {
        let (mut re, mut im) = (0.0f32, 0.0f32);
        for (i, v) in x.iter().enumerate() {
            let a = i as f32 * hz / sr * std::f32::consts::TAU;
            re += v * a.cos();
            im += v * a.sin();
        }
        (re * re + im * im).sqrt() / x.len() as f32
    }

    #[test]
    fn the_note_moves_and_the_colour_stays() {
        let sr = 44_100.0;
        let x = voice(200.0, 1.0, sr);
        let mut s = Shifter::new(sr);
        let ratio = 1.25f32;
        let y: Vec<f32> = x.iter().map(|f| s.step(*f, sr / 200.0, ratio, 1.0, sr)[0]).collect();
        let tail = &y[22_050..];
        let (hz, _) = crate::scope::pitch(&tail[..4096], sr).unwrap();
        assert!((hz - 250.0).abs() < 3.0, "{hz}");
        // the overtone at 1 kHz is still the loud one; had the grains been sped up it would sit at 1250
        let (at_1000, at_1250, at_750) = (level_at(tail, 1000.0, sr), level_at(tail, 1250.0, sr), level_at(tail, 750.0, sr));
        assert!(at_1000 > at_1250 * 1.5 && at_1000 > at_750 * 1.5, "{at_750} {at_1000} {at_1250}");
    }

    #[test]
    fn nothing_asked_gives_the_sound_back_late() {
        let sr = 44_100.0;
        let x = voice(180.0, 0.5, sr);
        let mut s = Shifter::new(sr);
        let y: Vec<f32> = x.iter().map(|f| s.step(*f, sr / 180.0, 1.0, 1.0, sr)[0]).collect();
        // find the delay, then compare
        let d = (1000..4000).min_by(|a, b| {
            let e = |d: usize| (8000..12_000).map(|i| (y[i + d] - x[i][0]).abs()).sum::<f32>();
            e(*a).total_cmp(&e(*b))
        });
        let d = d.unwrap();
        let err = (8000..16_000).map(|i| (y[i + d] - x[i][0]).abs()).sum::<f32>() / 8000.0;
        assert!(err < 0.01, "{err} at {d}");
    }

    #[test]
    fn the_listener_hears_the_note() {
        let sr = 44_100.0;
        let mut s = Shifter::new(sr);
        let mut p = 0.0;
        for f in voice(150.0, 0.4, sr) {
            p = s.listen(f, sr);
        }
        assert!((sr / p - 150.0).abs() < 2.0, "{p}");
    }
}
