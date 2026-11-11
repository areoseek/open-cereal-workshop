//! Kick Designer: a bass drum built from two sliding sine oscillators, a click, a sub tone an octave under them,
//! a drive stage and one layered sample. It sounds for its set duration whatever the length of the note.
//!
//! The stored block is a name (256 bytes), the path of the layered sample (1024 bytes) and 23 numbers from 0 to 1.
//! The places of the numbers follow the order of the controls; the curves behind them are our own.

use std::path::PathBuf;
use std::sync::{Arc, Mutex, OnceLock};

use crate::duo::{Ctl, Info};
use crate::sample::Sample;

pub const COUNT: usize = 23;
/// Where the numbers start in the block.
pub const FIRST: usize = 1292;
const SIZE: usize = FIRST + COUNT * 4;
const NAME_AT: usize = 12;
const PATH_AT: usize = 268;
const TRIM: f32 = 0.7;

pub const GROUPS: [&str; 6] = ["Overall", "Main oscillator", "Second oscillator", "Click", "Master", "Sample layer"];

const DEFS: [f32; COUNT] = [
    0.5, 0.5, 0.6, 0.7, 0.5, 0.0, 0.5, 0.5, 0.5, 0.0, 0.4, 0.6, 0.3, 0.3, 0.1, 1.0, 0.0, 0.5, 0.5, 0.5, 0.5, 1.0, 0.0,
];

const PLACED: [(usize, &str, u8); COUNT] = [
    (1, "Duration", 0),
    (0, "Output level", 0),
    (15, "Sampling ratio", 0),
    (2, "Base", 1),
    (3, "Peak", 1),
    (4, "Slide", 1),
    (6, "Base offset", 2),
    (7, "Peak offset", 2),
    (8, "Slide offset", 2),
    (5, "Phase", 2),
    (9, "Level", 3),
    (10, "Cut", 3),
    (11, "Frequency", 3),
    (12, "Drive", 4),
    (13, "Sub bass", 4),
    (14, "Attack", 4),
    (16, "Attack", 5),
    (17, "Decay", 5),
    (18, "Sustain", 5),
    (19, "Delay", 5),
    (20, "Pitch", 5),
    (21, "Low pass", 5),
    (22, "Mix", 5),
];

pub fn params() -> &'static [Info] {
    static P: OnceLock<Vec<Info>> = OnceLock::new();
    P.get_or_init(|| {
        // shown in the order of the panel, stored by place
        let mut v: Vec<Info> = DEFS.iter().map(|&def| Info { name: "", ctl: Ctl::Kept, def, group: 0 }).collect();
        for (i, name, group) in PLACED {
            v[i] = Info { name, ctl: Ctl::Knob, def: DEFS[i], group };
        }
        v
    })
}

pub fn defaults() -> Vec<f32> {
    DEFS.to_vec()
}

/// True when the block is one of ours to read.
pub fn fits(raw: &[u8]) -> bool {
    raw.len() >= SIZE && &raw[..4] == b"SSLF" && &raw[9..12] == b"kiK"
}

pub fn values(raw: &[u8]) -> Vec<f32> {
    if !fits(raw) {
        return Vec::new();
    }
    (0..COUNT)
        .map(|i| {
            let o = FIRST + i * 4;
            let v = f32::from_le_bytes(raw[o..o + 4].try_into().unwrap());
            if v.is_finite() {
                v
            } else {
                DEFS[i]
            }
        })
        .collect()
}

/// A block of our own for a new instrument.
pub fn fresh() -> Vec<u8> {
    let mut out = Vec::with_capacity(SIZE);
    out.extend_from_slice(b"SSLF");
    out.extend_from_slice(&((SIZE - 8) as u32).to_le_bytes());
    out.extend_from_slice(b"2kiK");
    out.resize(FIRST, 0);
    out[NAME_AT..NAME_AT + 7].copy_from_slice(b"Default");
    for v in DEFS {
        out.extend_from_slice(&v.to_le_bytes());
    }
    out
}

fn text(raw: &[u8], at: usize, len: usize) -> String {
    let b = &raw[at..at + len];
    let end = b.iter().position(|&c| c == 0).unwrap_or(len);
    String::from_utf8_lossy(&b[..end]).to_string()
}

/// The stored path of the layered sample ("" when there is none).
pub fn sample_path(raw: &[u8]) -> String {
    if fits(raw) {
        text(raw, PATH_AT, FIRST - PATH_AT)
    } else {
        String::new()
    }
}

/// The name of the layered sample, without folders or ending.
pub fn sample_name(raw: &[u8]) -> String {
    let p = sample_path(raw);
    let file = p.rsplit(['/', '\\']).next().unwrap_or("");
    file.rsplit_once('.').map(|(a, _)| a).unwrap_or(file).to_string()
}

/// Store another layered sample by path ("" for none).
pub fn set_sample_path(raw: &mut [u8], path: &str) {
    if !fits(raw) {
        return;
    }
    let room = FIRST - PATH_AT - 1;
    let b = path.as_bytes();
    let n = b.len().min(room);
    raw[PATH_AT..FIRST].fill(0);
    raw[PATH_AT..PATH_AT + n].copy_from_slice(&b[..n]);
}

fn sample_dirs() -> Vec<PathBuf> {
    let mut out = vec![];
    for r in crate::keys::roots() {
        let mut cur = Some(r.clone());
        for part in ["Plugins", "Fruity", "Generators", "BassDrum", "Noise Samples"] {
            cur = cur.and_then(|c| crate::keys::child_ci(&c, part));
        }
        out.extend(cur);
    }
    out
}

/// The layered samples found in the user's own content: (name, path to store).
pub fn sample_list() -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = vec![];
    for d in sample_dirs() {
        let Ok(rd) = std::fs::read_dir(&d) else { continue };
        for e in rd.flatten() {
            let p = e.path();
            if p.extension().and_then(|x| x.to_str()).is_some_and(|x| x.eq_ignore_ascii_case("wav")) {
                let name = p.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
                if !out.iter().any(|o| o.0.eq_ignore_ascii_case(&name)) {
                    let file = p.file_name().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
                    out.push((name, format!("%FLStudioFactoryData%/Plugins/Fruity/Generators/BassDrum/Noise Samples/{file}")));
                }
            }
        }
    }
    out.sort_by_key(|o| o.0.to_lowercase());
    out
}

fn find(stored: &str) -> Option<PathBuf> {
    let parts: Vec<&str> = stored.split(['/', '\\']).filter(|s| !s.is_empty()).collect();
    let file = *parts.last()?;
    // a path into an install: follow it from "Plugins" under each known root
    if let Some(at) = parts.iter().position(|p| p.eq_ignore_ascii_case("Plugins")) {
        for r in crate::keys::roots() {
            let mut cur = Some(r);
            for part in &parts[at..] {
                cur = cur.and_then(|c| crate::keys::child_ci(&c, part));
            }
            if let Some(p) = cur.filter(|p| p.is_file()) {
                return Some(p);
            }
        }
    }
    for d in sample_dirs() {
        if let Some(p) = crate::keys::child_ci(&d, file).filter(|p| p.is_file()) {
            return Some(p);
        }
    }
    let plain = PathBuf::from(stored);
    (plain.is_absolute() && plain.is_file()).then_some(plain)
}

struct Store {
    held: Vec<(String, u32, Arc<Sample>)>,
    next: u32,
}

fn store() -> &'static Mutex<Store> {
    static S: OnceLock<Mutex<Store>> = OnceLock::new();
    S.get_or_init(|| Mutex::new(Store { held: Vec::new(), next: 1 }))
}

const KEPT: usize = 32;

fn hold(key: String, smp: Arc<Sample>) -> f32 {
    let mut st = store().lock().unwrap();
    let id = st.next;
    st.next += 1;
    if st.held.len() >= KEPT {
        st.held.remove(0);
    }
    st.held.push((key, id, smp));
    id as f32
}

/// The number a voice finds the layered sample of this block under (0 when there is none to be found).
pub fn sample_id(raw: &[u8]) -> f32 {
    let stored = sample_path(raw);
    if stored.is_empty() {
        return 0.0;
    }
    if let Some(found) = store().lock().unwrap().held.iter().find(|o| o.0 == stored) {
        return found.1 as f32;
    }
    let Some(smp) = find(&stored).and_then(|p| Sample::load(&p).ok()) else { return 0.0 };
    hold(stored, Arc::new(smp))
}

fn sample(id: f32) -> Option<Arc<Sample>> {
    let id = if id.is_finite() && id >= 1.0 { id.round() as u32 } else { return None };
    store().lock().unwrap().held.iter().find(|o| o.1 == id).map(|o| o.2.clone())
}

#[derive(Debug, Clone)]
pub struct Set {
    /// Seconds of sound.
    dur: f32,
    gain: f32,
    /// Steps of the inner clock per frame, 1 and under.
    ratio: f32,
    /// End and start frequency in Hz and the seconds of the slide, for the two oscillators.
    osc: [[f32; 3]; 2],
    /// Seconds the second oscillator starts late.
    late: f32,
    click: [f32; 3],
    drive: f32,
    sub: f32,
    attack: f32,
    smp: Option<Arc<Sample>>,
    /// Attack and decay seconds and the sustain level of the layer.
    env: [f32; 3],
    /// Seconds the layer comes after the oscillators (before them when under zero).
    delay: f32,
    speed: f32,
    low_hz: f32,
    mix: f32,
}

fn unit(p: &[f32], i: usize) -> f32 {
    p.get(i).copied().filter(|v| v.is_finite()).unwrap_or(DEFS[i]).clamp(0.0, 1.0)
}

/// `p` is the list of numbers, then the number of the layered sample (see `sample_id`).
pub fn set(p: &[f32]) -> Set {
    let u = |i: usize| unit(p, i);
    let off = |i: usize| (u(i) - 0.5) * 2.0;
    let base = 10.0 * 2f32.powf(u(2) * 4.0);
    let peak = (20.0 * 2f32.powf(u(3) * 8.0)).max(base);
    let slide = 0.002 + u(4).powi(3) * 0.5;
    let second = [(base * (1.0 + off(6))).max(8.0), (peak * (1.0 + off(7))).max(8.0), (slide * (1.0 + off(8))).max(0.001)];
    let smp = if p.len() > COUNT { sample(p[COUNT]) } else { None };
    Set {
        dur: 0.03 + u(1).powi(2) * 2.0,
        gain: 10f32.powf((u(0) - 0.5) * 40.0 / 20.0) * TRIM,
        ratio: u(15).max(0.02),
        osc: [[base, peak, slide], second],
        late: u(5) * 0.01,
        click: [u(9).powi(2), 20.0 * 2f32.powf(u(10) * 9.0), 200.0 * 2f32.powf(u(11) * 5.0)],
        drive: u(12),
        sub: u(13),
        attack: u(14).powi(2) * 0.05,
        smp,
        env: [u(16).powi(2) * 0.5, 0.005 + u(17).powi(2) * 2.0, u(18)],
        delay: off(19) * 0.05,
        speed: 2f32.powf(off(20) * 2.0),
        low_hz: 100.0 * 2f32.powf(u(21) * 7.7),
        mix: u(22),
    }
}

#[derive(Debug, Clone)]
pub struct Voice {
    sr: f32,
    /// Inner frames done, and the part of the next one reached.
    n: u32,
    acc: f32,
    held: [f32; 2],
    ph: [f32; 2],
    click_ph: f32,
    click_lp: f32,
    pos: f64,
    lp: [[f32; 2]; 2],
    vel: f32,
    /// Frequency against the middle key.
    tune: f32,
}

const TAU: f32 = std::f32::consts::TAU;

impl Voice {
    pub fn start(_set: &Set, sr: f32, vel: f32, tune: f32) -> Self {
        Self { sr, n: 0, acc: 1.0, held: [0.0; 2], ph: [0.0; 2], click_ph: 0.0, click_lp: 0.0, pos: 0.0, lp: [[0.0; 2]; 2], vel, tune: if tune.is_finite() && tune > 0.0 { tune } else { 1.0 } }
    }

    /// Seconds of sound left: there is no release, the drum runs to its duration.
    pub fn release(&mut self, set: &Set) -> f32 {
        ((set.dur - self.n as f32 / self.sr) / set.ratio).max(0.0) + 0.01
    }

    fn inner(&mut self, set: &Set) -> [f32; 2] {
        let dt = 1.0 / self.sr;
        let t = self.n as f32 * dt;
        self.n += 1;
        if t >= set.dur {
            return [0.0; 2];
        }
        // with the layer set to lead, the oscillators wait instead
        let wait = (-set.delay).max(0.0);
        let mut body = 0.0;
        let mut sub = 0.0;
        for k in 0..2 {
            let tk = t - wait - if k == 1 { set.late } else { 0.0 };
            if tk < 0.0 {
                continue;
            }
            let [base, peak, slide] = set.osc[k];
            let hz = (base + (peak - base) * (-tk * 5.0 / slide).exp()) * self.tune;
            self.ph[k] += hz.min(self.sr * 0.45) * dt;
            if self.ph[k] >= 2.0 {
                self.ph[k] -= 2.0;
            }
            body += (self.ph[k] * TAU).sin() * 0.5;
            sub += (self.ph[k] * 0.5 * TAU).sin() * 0.5;
        }
        let to = t - wait;
        let mut click = 0.0;
        if to >= 0.0 && to < 0.05 && set.click[0] > 0.0 {
            self.click_ph += set.click[2].min(self.sr * 0.45) * dt;
            let raw = (self.click_ph * TAU).sin() * (-to / 0.004).exp();
            let a = 1.0 - (-TAU * set.click[1].min(self.sr * 0.45) * dt).exp();
            self.click_lp += (raw - self.click_lp) * a;
            click = (raw - self.click_lp) * set.click[0];
        }
        let mut x = body + sub * set.sub + click;
        if set.drive > 0.0 {
            let g = 1.0 + set.drive * 7.0;
            x = (x * g).tanh() / g.tanh();
        }
        if set.attack > 0.0 && to < set.attack {
            x *= (to / set.attack).max(0.0);
        }
        let mut out = [x, x];
        if let (Some(smp), true) = (&set.smp, set.mix > 0.0) {
            let ts = t - set.delay.max(0.0);
            if ts >= 0.0 {
                let [a, d, s] = set.env;
                let env = if ts < a { ts / a } else { s + (1.0 - s) * (-(ts - a) * 5.0 / d).exp() };
                let f = smp.at(self.pos);
                self.pos += (smp.rate as f32 / self.sr * set.speed) as f64;
                let c = 1.0 - (-TAU * set.low_hz.min(self.sr * 0.45) * dt).exp();
                for ch in 0..2 {
                    self.lp[0][ch] += (f[ch] - self.lp[0][ch]) * c;
                    self.lp[1][ch] += (self.lp[0][ch] - self.lp[1][ch]) * c;
                    out[ch] += self.lp[1][ch] * env * set.mix;
                }
            }
        }
        // the one envelope over everything: a fall that reaches nothing at the duration
        let left = 1.0 - t / set.dur;
        let amp = left * left.sqrt() * set.gain * self.vel;
        [out[0] * amp, out[1] * amp]
    }

    pub fn frame(&mut self, set: &Set) -> [f32; 2] {
        // a lowered inner rate: frames are worked more seldom and held, steps and all
        self.acc += set.ratio;
        while self.acc >= 1.0 {
            self.acc -= 1.0;
            self.held = self.inner(set);
        }
        self.held
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn play(p: &[f32], secs: f32) -> Vec<[f32; 2]> {
        let s = set(p);
        let mut v = Voice::start(&s, 48000.0, 1.0, 1.0);
        (0..(secs * 48000.0) as usize).map(|_| v.frame(&s)).collect()
    }

    fn peak(b: &[[f32; 2]]) -> f32 {
        b.iter().fold(0.0f32, |m, f| m.max(f[0].abs()).max(f[1].abs()))
    }

    #[test]
    fn a_new_block_reads_back_and_sounds() {
        let raw = fresh();
        assert_eq!(raw.len(), 1384);
        assert!(fits(&raw));
        assert_eq!(values(&raw), defaults());
        assert_eq!(sample_path(&raw), "");
        let out = play(&defaults(), 0.3);
        let top = peak(&out);
        assert!(top > 0.2 && top < 1.5, "{top}");
    }

    #[test]
    fn it_stops_at_its_duration_and_falls_in_pitch() {
        let mut p = defaults();
        p[1] = 0.4;
        let s = set(&p);
        let out = play(&p, 1.0);
        let end = (s.dur * 48000.0) as usize;
        assert!(peak(&out[end + 2..]) == 0.0);
        assert!(peak(&out[..end / 2]) > 0.2);
        // crossings come closer together early than late
        let cross = |b: &[[f32; 2]]| b.windows(2).filter(|w| w[0][0] <= 0.0 && w[1][0] > 0.0).count();
        p[9] = 0.0;
        p[13] = 0.0;
        let out = play(&p, 1.0);
        assert!(cross(&out[..2400]) > cross(&out[4800..7200]) + 2);
    }

    #[test]
    fn a_lower_sampling_ratio_slows_and_steps_it() {
        let mut p = defaults();
        p[15] = 0.5;
        let out = play(&p, 0.2);
        assert!(out[1..].chunks(2).take(2000).all(|c| c[0] == c[1]));
        let s = set(&p);
        let mut v = Voice::start(&s, 48000.0, 1.0, 1.0);
        let whole = Voice::start(&set(&defaults()), 48000.0, 1.0, 1.0).release(&set(&defaults()));
        assert!((v.release(&s) - whole * 2.0).abs() < 0.03);
    }

    #[test]
    fn the_layered_sample_is_heard_and_its_path_is_stored() {
        let mut raw = fresh();
        set_sample_path(&mut raw, "%FLStudioFactoryData%/Plugins/Fruity/Generators/BassDrum/Noise Samples/Soft Hit.wav");
        assert_eq!(sample_name(&raw), "Soft Hit");
        assert_eq!(raw.len(), 1384);
        assert_eq!(values(&raw), defaults());
        let noise: Vec<f32> = (0..48000u32).map(|i| ((i.wrapping_mul(2654435761) >> 8) & 0xffff) as f32 / 32768.0 - 1.0).collect();
        let id = hold("test noise".into(), Arc::new(Sample::from_mono("n", 48000, &noise)));
        let mut p = defaults();
        // no oscillators to speak of: only the layer
        p[9] = 0.0;
        p[13] = 0.0;
        let dry = play(&p, 0.2);
        p[22] = 1.0;
        p.push(id);
        let wet = play(&p, 0.2);
        let rough = |b: &[[f32; 2]]| b.windows(2).map(|w| (w[1][0] - w[0][0]).abs()).sum::<f32>();
        assert!(rough(&wet) > rough(&dry) * 3.0);
    }

    #[test]
    fn a_broken_block_is_left_alone() {
        assert!(values(b"SSLF").is_empty());
        assert_eq!(sample_id(b"SSLF"), 0.0);
        let s = set(&[]);
        assert!(s.dur > 0.0);
    }
}
