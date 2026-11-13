//! The path scratcher: every key has a phrase, a drawn path that carries a play head back and forth through a
//! stretch of the sample over a set length of time, with a drawn level and muted stretches beside it.
//! The block's layout was worked out from stored examples; the times of the fades are estimates.

use crate::sample::Sample;
use std::sync::Arc;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Point {
    pub x: f32,
    pub y: f32,
    /// The shape of the piece that leaves this point: 0 half cosine, 1 straight, 2 handles, 3 tension,
    /// 4 quarter sine, 5 quarter cosine.
    pub kind: u8,
    pub tension: f32,
    /// The handles toward the piece before and the piece after, as offsets from the point.
    pub before: [f32; 2],
    pub after: [f32; 2],
    /// Keeps the two handles in one line.
    pub locked: bool,
}

impl Point {
    pub fn plain(x: f32, y: f32) -> Self {
        Self { x, y, kind: 1, tension: 0.5, before: [-0.042, 0.0], after: [0.042, 0.0], locked: false }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Phrase {
    pub name: String,
    /// The phrase's length in beats when it keeps time with the song (measured against a render).
    pub beats: f32,
    /// The share of the sample's own speed a straight path plays at when it does not.
    pub speed: f32,
    pub own_speed: bool,
    /// Fade in and fade out, in the stored unit (taken as milliseconds).
    pub attack: f32,
    pub release: f32,
    /// The stretch of the sample the path moves in, as shares of its length.
    pub start: f32,
    pub end: f32,
    /// Places along the phrase where the sound turns on (true) or off.
    pub mutes: Vec<(bool, f32)>,
    pub wave: Vec<Point>,
    pub level: Vec<Point>,
}

impl Default for Phrase {
    fn default() -> Self {
        Self {
            name: String::new(),
            beats: 4.0,
            speed: 1.0,
            own_speed: false,
            attack: 1.0,
            release: 1.0,
            start: 0.0,
            end: 1.0,
            mutes: vec![(true, 0.0)],
            wave: vec![Point::plain(0.0, 0.0), Point::plain(1.0, 1.0)],
            level: vec![Point::plain(0.0, 1.0), Point::plain(1.0, 1.0)],
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Set {
    /// The key whose phrase is shown.
    pub shown: usize,
    pub phrases: Vec<(u8, Phrase)>,
}

fn int(s: &[u8], at: usize) -> Option<i32> {
    Some(i32::from_le_bytes(s.get(at..at + 4)?.try_into().ok()?))
}

fn real(s: &[u8], at: usize) -> Option<f32> {
    Some(f32::from_le_bytes(s.get(at..at + 4)?.try_into().ok()?))
}

const POINT: usize = 60;

fn points(s: &[u8], at: &mut usize) -> Option<Vec<Point>> {
    let n = int(s, *at)? as usize;
    *at += 4;
    if n > 4096 {
        return None;
    }
    let mut out = Vec::with_capacity(n);
    for _ in 0..n {
        let r = s.get(*at..*at + POINT)?;
        out.push(Point {
            x: real(r, 8)?,
            y: real(r, 12)?,
            before: [real(r, 20)?, real(r, 24)?],
            after: [real(r, 32)?, real(r, 36)?],
            tension: real(r, 40)?,
            kind: int(r, 48)?.clamp(0, 5) as u8,
            locked: int(r, 56)? != 0,
        });
        *at += POINT;
    }
    Some(out)
}

impl Set {
    pub fn parse(state: &[u8]) -> Self {
        Self::read(state).unwrap_or_default()
    }

    fn read(s: &[u8]) -> Option<Self> {
        let shown = int(s, 12)?.clamp(0, 127) as usize;
        let mut at = 20 + int(s, 16)?.max(0) as usize;
        let mut phrases = Vec::new();
        while let (Some(2000), Some(size)) = (int(s, at), int(s, at + 4)) {
            // the stored size does not always reach past the drawn points, so the phrase is read to its own end
            let end = at + 8 + size.max(0) as usize;
            let r = s;
            let mut q = at + 8;
            let key = int(r, q)?.clamp(0, 127) as u8;
            let name_len = int(r, q + 32)?.max(0) as usize;
            let name = String::from_utf8_lossy(r.get(q + 36..q + 36 + name_len)?).into_owned();
            let mut ph = Phrase {
                name,
                beats: real(r, q + 4)?,
                speed: real(r, q + 8)?,
                attack: real(r, q + 12)?,
                release: real(r, q + 16)?,
                start: real(r, q + 20)?,
                end: real(r, q + 24)?,
                own_speed: int(r, q + 28)? != 0,
                mutes: Vec::new(),
                wave: Vec::new(),
                level: Vec::new(),
            };
            q += 36 + name_len;
            let n = int(r, q)?.max(0) as usize;
            q += 4;
            for _ in 0..n.min(4096) {
                ph.mutes.push((int(r, q)? != 0, real(r, q + 4)?));
                q += 8;
            }
            ph.wave = points(r, &mut q)?;
            ph.level = points(r, &mut q)?;
            phrases.push((key, ph));
            at = if int(s, q) == Some(2000) || int(s, end) != Some(2000) { q } else { end };
        }
        Some(Self { shown, phrases })
    }

    /// The block for these phrases; the sample's name is carried over from the block they were read from.
    pub fn store(&self, old: &[u8]) -> Vec<u8> {
        let name_len = int(old, 16).map_or(0, |n| n.max(0) as usize).min(old.len().saturating_sub(20));
        let mut b = Vec::new();
        for v in [1000, 12 + name_len as i32, int(old, 8).unwrap_or(1_000_005), self.shown as i32, name_len as i32] {
            b.extend_from_slice(&v.to_le_bytes());
        }
        b.extend_from_slice(old.get(20..20 + name_len).unwrap_or(&[]));
        for (key, ph) in &self.phrases {
            let mut r = Vec::new();
            r.extend_from_slice(&(*key as i32).to_le_bytes());
            for v in [ph.beats, ph.speed, ph.attack, ph.release, ph.start, ph.end] {
                r.extend_from_slice(&v.to_le_bytes());
            }
            r.extend_from_slice(&(ph.own_speed as i32).to_le_bytes());
            r.extend_from_slice(&(ph.name.len() as i32).to_le_bytes());
            r.extend_from_slice(ph.name.as_bytes());
            r.extend_from_slice(&(ph.mutes.len() as i32).to_le_bytes());
            for m in &ph.mutes {
                r.extend_from_slice(&(m.0 as i32).to_le_bytes());
                r.extend_from_slice(&m.1.to_le_bytes());
            }
            for line in [&ph.wave, &ph.level] {
                r.extend_from_slice(&(line.len() as i32).to_le_bytes());
                for p in line {
                    let mut q = [0u8; POINT];
                    for (at, v) in [(8, p.x), (12, p.y), (20, p.before[0]), (24, p.before[1]), (32, p.after[0]), (36, p.after[1]), (40, p.tension)] {
                        q[at..at + 4].copy_from_slice(&v.to_le_bytes());
                    }
                    q[48] = p.kind;
                    q[56] = p.locked as u8;
                    r.extend_from_slice(&q);
                }
            }
            b.extend_from_slice(&2000i32.to_le_bytes());
            b.extend_from_slice(&(r.len() as i32).to_le_bytes());
            b.extend_from_slice(&r);
        }
        b.extend_from_slice(&3000i32.to_le_bytes());
        b.extend_from_slice(&0i32.to_le_bytes());
        b
    }

    /// The phrase of a key to change, made plain first when the key had none.
    pub fn phrase_mut(&mut self, key: u8) -> &mut Phrase {
        let at = match self.phrases.iter().position(|p| p.0 == key) {
            Some(at) => at,
            None => {
                let at = self.phrases.partition_point(|p| p.0 < key);
                self.phrases.insert(at, (key, Phrase::default()));
                at
            }
        };
        &mut self.phrases[at].1
    }

    pub fn phrase(&self, key: u8) -> Option<&Phrase> {
        self.phrases.iter().find(|p| p.0 == key).map(|p| &p.1)
    }
}

/// A new unit's block: no sample yet, a plain phrase on the middle key.
pub fn fresh() -> Vec<u8> {
    let mut old = vec![0u8; 20];
    old[8..12].copy_from_slice(&1_000_005i32.to_le_bytes());
    Set { shown: 60, phrases: vec![(60, Phrase::default())] }.store(&old)
}

/// The block with another sample named in it.
pub fn with_sample(old: &[u8], path: &str) -> Vec<u8> {
    let mut head = vec![0u8; 20];
    head[8..12].copy_from_slice(&int(old, 8).unwrap_or(1_000_005).to_le_bytes());
    head[16..20].copy_from_slice(&(path.len() as i32).to_le_bytes());
    head.extend_from_slice(path.as_bytes());
    Set::parse(old).store(&head)
}

/// Where the sound is on along a phrase.
pub fn open_at(mutes: &[(bool, f32)], x: f32) -> bool {
    mutes.iter().rev().find(|m| m.1 <= x).is_none_or(|m| m.0)
}

/// Where a drawn line stands at `x`.
pub fn at(line: &[Point], x: f32) -> f32 {
    let Some(first) = line.first() else { return 0.0 };
    if x <= first.x {
        return first.y;
    }
    for w in line.windows(2) {
        let (a, b) = (&w[0], &w[1]);
        if x > b.x {
            continue;
        }
        let span = b.x - a.x;
        if span <= 1e-9 {
            return b.y;
        }
        let u = ((x - a.x) / span).clamp(0.0, 1.0);
        let half = std::f32::consts::FRAC_PI_2;
        // measured against a render: a piece takes its shape from the point it leaves, and handles left
        // where they rest give a plain half cosine
        let rest = |h: &[f32; 2], x: f32| (h[0].abs() - x).abs() < 1e-3 && h[1].abs() < 1e-6;
        let s = match a.kind {
            0 => (1.0 - (u * std::f32::consts::PI).cos()) * 0.5,
            2 if rest(&a.after, 0.042) && rest(&b.before, 0.042) => (1.0 - (u * std::f32::consts::PI).cos()) * 0.5,
            2 => {
                // the handles may lean past the ends; time only runs forward, so those parts are left out
                let xs = [a.x, (a.x + a.after[0]).clamp(a.x, b.x), (b.x + b.before[0]).clamp(a.x, b.x), b.x];
                let ys = [a.y, a.y + a.after[1], b.y + b.before[1], b.y];
                let cubic = |p: &[f32; 4], t: f32| {
                    let m = 1.0 - t;
                    m * m * m * p[0] + 3.0 * m * m * t * p[1] + 3.0 * m * t * t * p[2] + t * t * t * p[3]
                };
                let (mut lo, mut hi) = (0.0f32, 1.0f32);
                for _ in 0..22 {
                    let mid = (lo + hi) * 0.5;
                    if cubic(&xs, mid) < x { lo = mid } else { hi = mid }
                }
                return cubic(&ys, (lo + hi) * 0.5);
            }
            3 => {
                // the handle sits where the piece stands at its middle
                let t = a.tension.clamp(0.02, 0.98);
                u.powf(t.ln() / 0.5f32.ln())
            }
            4 => (u * half).sin(),
            5 => 1.0 - (u * half).cos(),
            _ => u,
        };
        return a.y + (b.y - a.y) * s;
    }
    line.last().map_or(0.0, |p| p.y)
}

const TABLE: usize = 4096;

#[derive(Debug, Clone)]
pub struct Voice {
    smp: Arc<Sample>,
    /// The play head's place (in sample frames) and the level along the phrase.
    place: Vec<f32>,
    level: Vec<f32>,
    /// How far along the phrase the voice is, and how much of it one output frame covers.
    t: f64,
    dt: f64,
    vel: f32,
    /// The fades: the gain now, what one frame adds while coming in, what one takes while going out.
    gain: f32,
    up: f32,
    down: f32,
    held: bool,
    /// The muted stretches, smoothed.
    open: f32,
    /// The rumble filter: its factor, and the last input and output of each side.
    hp_k: f32,
    hp: [[f32; 2]; 2],
}

impl Voice {
    pub fn start(smp: &Arc<Sample>, ph: &Phrase, sr: f32, vel: f32, tempo: f32) -> Self {
        let n = smp.frames.len().max(1) as f32 - 1.0;
        let (lo, hi) = (ph.start.clamp(0.0, 1.0), ph.end.clamp(0.0, 1.0));
        let place = (0..=TABLE).map(|i| (lo + (hi - lo) * at(&ph.wave, i as f32 / TABLE as f32).clamp(0.0, 1.0)) * n).collect();
        let mut level: Vec<f32> = (0..=TABLE).map(|i| at(&ph.level, i as f32 / TABLE as f32).clamp(0.0, 1.0)).collect();
        for (i, l) in level.iter_mut().enumerate() {
            let x = i as f32 / TABLE as f32;
            if !open_at(&ph.mutes, x) {
                *l = 0.0;
            }
        }
        let secs = if ph.own_speed {
            (hi - lo).abs() * smp.frames.len() as f32 / smp.rate.max(1) as f32 / ph.speed.max(0.01)
        } else {
            ph.beats.max(0.01) * 60.0 / tempo.max(1.0)
        };
        let fade = |ms: f32| 1.0 / (ms.clamp(0.1, 10_000.0) * 0.001 * sr).max(1.0);
        Self { smp: smp.clone(), place, level, t: 0.0, dt: 1.0 / (secs.max(0.001) * sr) as f64, vel, gain: 0.0, up: fade(ph.attack), down: fade(ph.release), held: true, open: 1.0, hp_k: (-std::f32::consts::TAU * 18.0 / sr).exp(), hp: [[0.0; 2]; 2] }
    }

    /// Lets go of the key; hands back how long the voice still sounds.
    pub fn release(&mut self) -> f32 {
        self.held = false;
        self.gain / self.down.max(1e-9) / 48_000.0 + 0.01
    }

    pub fn frame(&mut self) -> [f32; 2] {
        if self.t >= 1.0 || self.smp.frames.is_empty() {
            return [0.0; 2];
        }
        let x = self.t * TABLE as f64;
        let i = (x as usize).min(TABLE - 1);
        let f = (x - i as f64) as f32;
        let at = self.place[i] + (self.place[i + 1] - self.place[i]) * f;
        // the muted stretches open and shut over a few milliseconds so they do not click
        let want = self.level[i] + (self.level[i + 1] - self.level[i]) * f;
        self.open += (want - self.open) * 0.02;
        self.gain = if self.held { (self.gain + self.up).min(1.0) } else { (self.gain - self.down).max(0.0) };
        self.t += self.dt;
        let fr = &self.smp.frames;
        let last = fr.len() - 1;
        let k = (at as usize).min(last);
        // a smooth curve through four frames: the head often creeps, and straight lines between frames dull that
        let (a, b, c, d) = (fr[k.saturating_sub(1)], fr[k], fr[(k + 1).min(last)], fr[(k + 2).min(last)]);
        let m = at - k as f32;
        // measured: the instrument sits 3 dB under the sample's own level, and a slow rumble filter follows it
        let g = self.gain * self.open * self.vel * std::f32::consts::FRAC_1_SQRT_2;
        let mut out = [0.0; 2];
        for (i, o) in out.iter_mut().enumerate() {
            let x = (b[i] + 0.5 * m * (c[i] - a[i] + m * (2.0 * a[i] - 5.0 * b[i] + 4.0 * c[i] - d[i] + m * (3.0 * (b[i] - c[i]) + d[i] - a[i])))) * g;
            let y = self.hp_k * (self.hp[i][1] + x - self.hp[i][0]);
            self.hp[i] = [x, y];
            *o = y;
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn block(key: i32, wave: &[(f32, f32, i32)], mutes: &[(i32, f32)]) -> Vec<u8> {
        let mut b = Vec::new();
        for v in [1000i32, 12, 1_000_005, key, 5] {
            b.extend_from_slice(&v.to_le_bytes());
        }
        b.extend_from_slice(b"a.wav");
        let mut r = Vec::new();
        r.extend_from_slice(&key.to_le_bytes());
        for v in [8.0f32, 1.0, 1.0, 1.0, 0.25, 0.75] {
            r.extend_from_slice(&v.to_le_bytes());
        }
        r.extend_from_slice(&0i32.to_le_bytes());
        r.extend_from_slice(&2i32.to_le_bytes());
        r.extend_from_slice(b"ab");
        r.extend_from_slice(&(mutes.len() as i32).to_le_bytes());
        for m in mutes {
            r.extend_from_slice(&m.0.to_le_bytes());
            r.extend_from_slice(&m.1.to_le_bytes());
        }
        let line = |r: &mut Vec<u8>, pts: &[(f32, f32, i32)]| {
            r.extend_from_slice(&(pts.len() as i32).to_le_bytes());
            for p in pts {
                let mut q = [0u8; POINT];
                q[8..12].copy_from_slice(&p.0.to_le_bytes());
                q[12..16].copy_from_slice(&p.1.to_le_bytes());
                q[40..44].copy_from_slice(&0.5f32.to_le_bytes());
                q[48..52].copy_from_slice(&p.2.to_le_bytes());
                r.extend_from_slice(&q);
            }
        };
        line(&mut r, wave);
        line(&mut r, &[(0.0, 1.0, 1), (1.0, 1.0, 1)]);
        b.extend_from_slice(&2000i32.to_le_bytes());
        b.extend_from_slice(&(r.len() as i32).to_le_bytes());
        b.extend_from_slice(&r);
        b.extend_from_slice(&3000i32.to_le_bytes());
        b.extend_from_slice(&0i32.to_le_bytes());
        b
    }

    #[test]
    fn a_stored_phrase_reads_back() {
        let set = Set::parse(&block(62, &[(0.0, 0.0, 0), (0.5, 1.0, 1), (1.0, 0.0, 1)], &[(1, 0.0), (0, 0.5)]));
        assert_eq!(set.shown, 62);
        let ph = set.phrase(62).unwrap();
        assert_eq!((ph.name.as_str(), ph.beats, ph.start, ph.end), ("ab", 8.0, 0.25, 0.75));
        assert_eq!(ph.mutes, vec![(true, 0.0), (false, 0.5)]);
        assert_eq!(ph.wave.len(), 3);
        // the half cosine eases out of the point it is set on; the straight piece does not
        assert!(at(&ph.wave, 0.05) < 0.05);
        assert!((at(&ph.wave, 0.25) - 0.5).abs() < 1e-4);
        assert!((at(&ph.wave, 0.75) - 0.5).abs() < 1e-4);
        assert!(set.phrase(60).is_none());
    }

    #[test]
    fn the_play_head_follows_the_path_there_and_back_and_the_muted_half_is_silent() {
        // a ramp whose value tells where the head is
        let n = 48_000;
        let smp = Arc::new(Sample::from_mono("r", 48_000, &(0..n).map(|i| i as f32 / n as f32).collect::<Vec<_>>()));
        let set = Set::parse(&block(60, &[(0.0, 0.0, 1), (0.25, 1.0, 1), (0.5, 0.0, 1), (1.0, 1.0, 1)], &[(1, 0.0), (0, 0.5)]));
        // 8 beats at 480 = one second
        let mut v = Voice::start(&smp, set.phrase(60).unwrap(), 48_000.0, 1.0, 480.0);
        let out: Vec<f32> = (0..48_000).map(|_| v.frame()[0]).collect();
        // the head is half way through the stretch (0.25 to 0.75 of the sample) an eighth of the way in
        let frames = (n - 1) as f32;
        assert!((v.place[TABLE / 8] / frames - 0.5).abs() < 0.01);
        assert!((v.place[TABLE / 4] / frames - 0.75).abs() < 0.01);
        assert!((v.place[TABLE * 3 / 8] / frames - 0.5).abs() < 0.01);
        assert!(out[6_000] > 0.001, "{}", out[6_000]);
        assert!(out[40_000].abs() < 1e-3 && out[47_000].abs() < 1e-3);
        assert_eq!(v.frame(), [0.0; 2]);
    }

    /// With CEREAL_FLP_CORPUS set: the stock phrase sets read whole, a phrase for every key.
    #[test]
    fn real_phrase_sets_read() {
        let Ok(root) = std::env::var("CEREAL_FLP_CORPUS") else { return };
        let dir = std::path::Path::new(&root).join("Patches/Plugin presets/Generators/Wave Traveller");
        let mut seen = 0;
        for e in std::fs::read_dir(&dir).into_iter().flatten().flatten() {
            let Ok(pre) = crate::preset::read(&e.path()) else { continue };
            let set = Set::parse(&pre.state);
            assert!(!set.phrases.is_empty(), "{}", e.path().display());
            let again = set.store(&pre.state);
            assert_eq!(again.len(), pre.state.len(), "{}", e.path().display());
            assert_eq!(Set::parse(&again), set, "{}", e.path().display());
            assert_eq!(crate::synth::sample_named_in("Wave Traveller", &again), crate::synth::sample_named_in("Wave Traveller", &pre.state));
            for (_, ph) in &set.phrases {
                assert!(ph.wave.len() >= 2 && ph.level.len() >= 2 && !ph.mutes.is_empty(), "{}", e.path().display());
                assert!(ph.wave.windows(2).all(|w| w[0].x <= w[1].x) && ph.beats > 0.0 && ph.start <= ph.end, "{}", e.path().display());
            }
            seen += 1;
        }
        assert!(seen == 0 || seen >= 5, "{seen}");
    }

    #[test]
    fn a_fresh_block_reads_and_takes_a_sample_name() {
        let b = fresh();
        let set = Set::parse(&b);
        assert_eq!((set.shown, set.phrases.len()), (60, 1));
        assert_eq!(crate::synth::sample_named_in("Wave Traveller", &b), None);
        let named = with_sample(&b, "/home/me/loop one.wav");
        assert_eq!(crate::synth::sample_named_in("Wave Traveller", &named).as_deref(), Some("/home/me/loop one.wav"));
        assert_eq!(Set::parse(&named), set);
    }

    #[test]
    fn a_handle_piece_stays_between_its_ends_and_eases() {
        let mut a = Point::plain(0.0, 0.0);
        let mut b = Point::plain(0.1, 1.0);
        a.kind = 2;
        b.kind = 2;
        let line = [a, b];
        assert!(at(&line, 0.01) < 0.1);
        assert!((at(&line, 0.05) - 0.5).abs() < 0.01);
        assert!(at(&line, 0.09) > 0.9);
    }
}
