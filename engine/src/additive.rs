//! The additive instrument: every note is a stack of harmonics whose levels are shaped by the patch (timbre, harmonic
//! equaliser, mask, pluck, filter, phaser) and turned into one wave cycle a few hundred times a second.
//!
//! The stored settings are read from the matching generator block. Which field is which control was worked out from
//! value ranges across many project and preset files plus the public manual; every scale below (times, octaves,
//! depths) is our own estimate until measured against the reference (docs/PARITY.md, "Instruments").

use std::f32::consts::TAU;
use std::sync::OnceLock;

/// Frames between spectrum updates; the two neighbouring waves are cross-faded in between.
const TICK: u32 = 128;
const MAX_M: usize = 2048;
const MAX_UNISON: usize = 9;

/// Index of each setting in the instrument's table (`synth::DEFS`).
pub mod ix {
    pub const PHASE: usize = 0;
    pub const LHP: usize = 1;
    pub const SUB: usize = 2;
    pub const EQ: usize = 5;
    pub const MASK: usize = 11;
    pub const MASK_MIX: usize = 23;
    pub const MASK_LFO: usize = 24;
    pub const TREMOLO: usize = 25;
    pub const ATTACK: usize = 26;
    pub const DECAY: usize = 27;
    pub const RELEASE: usize = 28;
    pub const PLUCK: usize = 29;
    pub const PLUCK_MASKED: usize = 30;
    pub const VOLUME: usize = 33;
    pub const VEL_VOLUME: usize = 34;
    pub const PITCH: usize = 35;
    pub const DETUNE: usize = 36;
    pub const GRIT: usize = 37;
    pub const VIBRATO: usize = 38;
    pub const PITCH_LFO: usize = 39;
    pub const PORTAMENTO: usize = 40;
    pub const LEGATO: usize = 41;
    pub const SLIDE_TIME: usize = 42;
    pub const FILTER_ON: usize = 43;
    pub const F_ATTACK: usize = 44;
    pub const F_DECAY: usize = 45;
    pub const F_AMOUNT: usize = 48;
    pub const CUTOFF: usize = 50;
    pub const KEY_TRACK: usize = 53;
    pub const CUTOFF_LFO: usize = 54;
    pub const RESONANCE: usize = 55;
    pub const RES_OFFSET: usize = 57;
    pub const UNI_ORDER: usize = 61;
    pub const UNI_PAN: usize = 62;
    pub const UNI_PITCH: usize = 63;
    pub const UNI_PHASE: usize = 64;
    pub const PH_AMOUNT: usize = 67;
    pub const PH_SCALE: usize = 68;
    pub const PH_WIDTH: usize = 69;
    pub const PH_WIDTH_LFO: usize = 70;
    pub const PH_OFFSET: usize = 71;
    pub const PH_OFFSET_LFO: usize = 72;
    pub const PH_SPEED: usize = 73;
    pub const HZ_AMOUNT: usize = 75;
    pub const HZ_WIDTH: usize = 76;
    pub const HZ_LFO: usize = 77;
    pub const HZ_STRENGTH: usize = 79;
    pub const HZ_TYPE: usize = 80;
    pub const LFO_SHAPE: usize = 82;
    pub const LFO_ATTACK: usize = 84;
    pub const LFO_SPEED: usize = 85;
    pub const CHORUS_TYPE: usize = 86;
    pub const CHORUS_MIX: usize = 87;
    pub const DELAY_TYPE: usize = 88;
    pub const DELAY_VOL: usize = 89;
    pub const DELAY_FB: usize = 90;
    pub const DELAY_TIME: usize = 91;
    pub const REVERB_TYPE: usize = 92;
    pub const REVERB_VOL: usize = 93;
    pub const FILTER_TYPE: usize = 98;
    pub const TIMBRE: usize = 99;
    pub const COUNT: usize = 100;
}

pub const TIMBRES: [&str; 12] = ["Saw", "Square", "Soft saw", "Triangle", "Pulse", "Even", "Stairs", "Bell", "Felt", "Organ", "Drawbars", "Custom"];
pub const LFO_SHAPES: [&str; 14] =
    ["Sine", "Triangle", "Saw up", "Saw down", "Square", "Random", "Smooth random", "Half sine", "Filter envelope", "Filter frequency", "Voice random", "Velocity", "Key", "Pitch"];

/// The chorus, echo and room after the voices. Each one's type at 0 is off (a fresh one plays dry against
/// the reference, and the first type of each is the only one that does), as is a knob at nothing. The
/// chorus styles and room sizes are our own (the reference names none); the echo time is stored in ticks of
/// a 96-tick beat.
pub fn tail(p: &[f32]) -> crate::tailfx::Set {
    use crate::tailfx::{Chorus, Delay, Reverb, Set};
    if p.len() < ix::COUNT {
        return Set::default();
    }
    let u = |i: usize| (p[i] / 1000.0).clamp(0.0, 1.0);
    const STYLES: [(f32, f32); 10] = [(0.5, 0.6), (0.3, 0.3), (0.7, 0.4), (0.4, 1.2), (0.8, 0.8), (0.2, 2.5), (0.6, 0.15), (0.9, 0.25), (0.35, 4.0), (1.0, 0.5)];
    const ROOMS: [f32; 7] = [0.5, 0.9, 1.4, 2.0, 2.8, 4.0, 6.0];
    let kind = |i: usize| p[i].max(0.0) as usize;
    let style = STYLES[kind(ix::CHORUS_TYPE).clamp(1, 10) - 1];
    let steps = p[ix::DELAY_TIME].clamp(1.0, 384.0) / 24.0;
    Set {
        chorus: (kind(ix::CHORUS_TYPE) > 0 && u(ix::CHORUS_MIX) > 0.0).then(|| Chorus { depth: style.0, hz: style.1, wet: u(ix::CHORUS_MIX) }),
        delay: (kind(ix::DELAY_TYPE) > 0 && u(ix::DELAY_VOL) > 0.0).then(|| Delay { left: steps, right: steps, fb: u(ix::DELAY_FB) * 0.95, high_hz: 6000.0, wet: u(ix::DELAY_VOL) * u(ix::DELAY_VOL), ..Default::default() }),
        reverb: (kind(ix::REVERB_TYPE) > 0 && u(ix::REVERB_VOL) > 0.0).then(|| Reverb { secs: ROOMS[kind(ix::REVERB_TYPE).clamp(1, 7) - 1], high_hz: 6000.0, wet: u(ix::REVERB_VOL) * u(ix::REVERB_VOL) }),
        ..Default::default()
    }
}

/// Level of harmonic `k` (1 is the note itself) for the built-in timbres.
fn timbre(kind: u8, k: usize) -> f32 {
    let kf = k as f32;
    let odd = k % 2 == 1;
    match kind {
        1 => if odd { 1.0 / kf } else { 0.0 },
        2 => 1.0 / kf.powf(1.5),
        3 => if odd { 1.0 / (kf * kf) } else { 0.0 },
        4 => (kf * std::f32::consts::PI * 0.25).sin().abs() / kf * 1.4,
        5 => if k == 1 || !odd { 1.0 / kf } else { 0.0 },
        6 => if k % 4 == 1 { 1.0 / kf } else if odd { 0.5 / kf } else { 0.0 },
        7 => match k {
            1 => 1.0,
            3 => 0.5,
            4 => 0.35,
            7 => 0.3,
            9 => 0.2,
            13 => 0.12,
            _ => 0.02 / kf,
        },
        8 => 1.0 / (kf * kf),
        9 => if k.is_power_of_two() && k <= 16 { 1.0 / kf.sqrt() } else { 0.0 },
        10 => match k {
            1 => 1.0,
            2 => 0.8,
            3 => 0.7,
            4 => 0.5,
            6 => 0.35,
            8 => 0.3,
            _ => 0.0,
        },
        _ => 1.0 / kf,
    }
}

/// A patch, ready for voices: the settings in table order plus an analysed wave if the block carries one.
#[derive(Debug, Clone, Default)]
pub struct Patch {
    pub p: Vec<f32>,
    /// Harmonic levels and phases (in cycles) of a custom timbre.
    pub custom: Option<(Vec<f32>, Vec<f32>)>,
}

impl Patch {
    /// `raw` is the stored block; a custom timbre sits after the fixed part as a count, the levels, then the phases.
    pub fn new(p: &[f32], raw: &[u8]) -> Self {
        let mut custom = None;
        if raw.len() >= 436 {
            let n = i32::from_le_bytes(raw[432..436].try_into().unwrap());
            if n > 0 && n <= 4096 && raw.len() >= 436 + n as usize * 8 {
                let n = n as usize;
                let f = |i: usize| f32::from_le_bytes(raw[436 + i * 4..440 + i * 4].try_into().unwrap());
                let amp: Vec<f32> = (0..n).map(f).map(|v| if v.is_finite() { v.clamp(0.0, 16.0) } else { 0.0 }).collect();
                let ph: Vec<f32> = (n..2 * n).map(f).map(|v| if v.is_finite() { v } else { 0.0 }).collect();
                custom = Some((amp, ph));
            }
        }
        Self { p: p.to_vec(), custom }
    }

    fn v(&self, i: usize) -> f32 {
        self.p.get(i).copied().unwrap_or(0.0)
    }

    /// With legato on, one note sounds at a time; the seconds a held note takes to reach the next
    /// (none unless portamento is on too). The time scale is an estimate.
    pub fn mono(&self) -> Option<f32> {
        (self.v(ix::LEGATO) != 0.0).then(|| if self.v(ix::PORTAMENTO) != 0.0 { 0.005 + (self.v(ix::SLIDE_TIME) / 1000.0).powi(2) * 2.0 } else { 0.0 })
    }

    /// Seconds for the level to rise at the start of a note.
    pub fn attack_s(&self) -> f32 {
        3.0 * (self.v(ix::ATTACK) / 1000.0).powi(3)
    }

    /// Time constant, in seconds, of the fall once the note is let go. Measured on the factory test: a
    /// quarter of the knob falls 613 dB a second.
    pub fn release_s(&self) -> f32 {
        (0.2267 * (self.v(ix::RELEASE) / 1000.0).powi(2)).max(0.001)
    }
}

fn twiddles() -> &'static [(f32, f32)] {
    static T: OnceLock<Vec<(f32, f32)>> = OnceLock::new();
    T.get_or_init(|| (0..MAX_M / 2).map(|i| { let a = TAU * i as f32 / MAX_M as f32; (a.cos(), a.sin()) }).collect())
}

/// In-place transform from harmonics to one wave (positive exponent, no scaling). `re.len()` is a power of two up to
/// `MAX_M`.
fn synth_fft(re: &mut [f32], im: &mut [f32]) {
    let n = re.len();
    let tw = twiddles();
    let mut j = 0usize;
    for i in 1..n {
        let mut bit = n >> 1;
        while j & bit != 0 {
            j ^= bit;
            bit >>= 1;
        }
        j |= bit;
        if i < j {
            re.swap(i, j);
            im.swap(i, j);
        }
    }
    let mut len = 2;
    while len <= n {
        let step = MAX_M / len;
        let half = len / 2;
        let mut i = 0;
        while i < n {
            for k in 0..half {
                let (c, s) = tw[k * step];
                let (a, b) = (i + k, i + k + half);
                let (xr, xi) = (re[b] * c - im[b] * s, re[b] * s + im[b] * c);
                re[b] = re[a] - xr;
                im[b] = im[a] - xi;
                re[a] += xr;
                im[a] += xi;
            }
            i += len;
        }
        len <<= 1;
    }
}

/// One sounding note.
#[derive(Debug, Clone)]
pub struct Voice {
    sr: f32,
    key: f32,
    vel: f32,
    seed: u32,
    /// Wave length in frames of the table; the table holds two cycles of the note so the sub harmonics fit.
    m: usize,
    tab: [Vec<f32>; 2],
    cur: usize,
    blend: bool,
    fade: f32,
    count: u32,
    frames: u64,
    re: Vec<f32>,
    im: Vec<f32>,
    /// Random start phase per harmonic, in cycles.
    hphase: Vec<f32>,
    /// How far each harmonic has run ahead of its place, in cycles (harmonic detune and grit).
    drift: Vec<f32>,
    last: [f32; 8],
    primed: bool,
    ph: [f32; MAX_UNISON],
    ratio: [f32; MAX_UNISON],
    gl: [f32; MAX_UNISON],
    gr: [f32; MAX_UNISON],
    order: usize,
    pitch_mul: f32,
    level: f32,
    trem: f32,
    /// The note's own rise and fall: the target, what it is multiplied by each frame once let go, and the
    /// smoothed level that is heard (measured: a lag follows every move, longer on the way down).
    env: f32,
    env_mul: f32,
    env_out: f32,
    held: bool,
    lfo_ph: f32,
    lfo_hold: f32,
    lfo_prev: f32,
    vrand: f32,
}

fn rnd(seed: &mut u32) -> f32 {
    *seed ^= *seed << 13;
    *seed ^= *seed >> 17;
    *seed ^= *seed << 5;
    *seed as f32 / u32::MAX as f32
}

impl Voice {
    /// `dphase` is the note's cycles per frame, `vel` its velocity 0..1.
    pub fn start(patch: &Patch, sr: f32, key: u8, vel: f32, dphase: f32, seed: u32) -> Self {
        let mut seed = seed | 1;
        let n0 = (0.45 / dphase.max(1e-6)) as usize;
        let m = if n0 > 128 { MAX_M } else { MAX_M / 2 };
        let order = (patch.v(ix::UNI_ORDER) as usize).clamp(1, MAX_UNISON);
        let mut v = Self {
            sr,
            key: key as f32,
            vel,
            seed,
            m,
            tab: [vec![0.0; m + 1], vec![0.0; m + 1]],
            cur: 0,
            blend: false,
            fade: 0.0,
            count: 0,
            frames: 0,
            re: vec![0.0; m],
            im: vec![0.0; m],
            hphase: Vec::new(),
            drift: Vec::new(),
            last: [f32::NAN; 8],
            primed: false,
            ph: [0.0; MAX_UNISON],
            ratio: [1.0; MAX_UNISON],
            gl: [0.0; MAX_UNISON],
            gr: [0.0; MAX_UNISON],
            order,
            pitch_mul: 1.0,
            level: 1.0,
            trem: 0.0,
            env: 0.0,
            env_mul: 1.0,
            env_out: 0.0,
            held: true,
            lfo_ph: 0.0,
            lfo_hold: 0.0,
            lfo_prev: 0.0,
            vrand: 0.0,
        };
        v.vrand = rnd(&mut seed) * 2.0 - 1.0;
        v.lfo_hold = rnd(&mut seed) * 2.0 - 1.0;
        let phase = patch.v(ix::PHASE);
        if phase < 0.0 {
            let amt = -phase / 1000.0;
            v.hphase = (0..m / 4).map(|_| rnd(&mut seed) * amt).collect();
        }
        let free = if phase > 0.0 { rnd(&mut seed) * phase / 1000.0 } else { 0.0 };
        let pan = patch.v(ix::UNI_PAN) / 128.0;
        let det = patch.v(ix::UNI_PITCH) / 128.0 * 22.0;
        let uph = patch.v(ix::UNI_PHASE) / 128.0;
        let g = 1.0 / (order as f32).sqrt();
        for u in 0..order {
            let s = if order > 1 { u as f32 / (order - 1) as f32 * 2.0 - 1.0 } else { 0.0 };
            v.ratio[u] = 2f32.powf(s * det / 1200.0);
            // neighbours go to opposite sides so the detuned pairs spread across the field
            let side = if u % 2 == 0 { s.abs() } else { -s.abs() } * pan;
            v.gl[u] = g * (1.0 - side.max(0.0));
            v.gr[u] = g * (1.0 + side.min(0.0));
            // measured: a note starts half a cycle in, at the middle of the saw's ramp (the table holds two cycles)
            v.ph[u] = (0.25 + free + if u > 0 { rnd(&mut seed) * uph } else { 0.0 }).fract();
        }
        v.seed = seed;
        v
    }

    fn lfo(&mut self, patch: &Patch, t: f32, fenv: f32) -> f32 {
        let shape = patch.v(ix::LFO_SHAPE) as u8;
        let speed = patch.v(ix::LFO_SPEED);
        let hz = (speed.abs() / 8000.0).powi(2) * 32.0 * speed.signum();
        let was = self.lfo_ph;
        self.lfo_ph = (self.lfo_ph + hz * TICK as f32 / self.sr).rem_euclid(1.0);
        let x = self.lfo_ph;
        let raw = match shape {
            1 => 1.0 - 4.0 * ((x + 0.25).fract() - 0.5).abs(),
            2 => 2.0 * x - 1.0,
            3 => 1.0 - 2.0 * x,
            4 => if x < 0.5 { 1.0 } else { -1.0 },
            5 | 6 => {
                if (hz >= 0.0 && x < was) || (hz < 0.0 && x > was) {
                    self.lfo_prev = self.lfo_hold;
                    self.lfo_hold = rnd(&mut self.seed) * 2.0 - 1.0;
                }
                if shape == 5 { self.lfo_hold } else { self.lfo_prev + (self.lfo_hold - self.lfo_prev) * x }
            }
            7 => (x * std::f32::consts::PI).sin(),
            8 => fenv,
            9 => patch.v(ix::CUTOFF) / 4096.0 - 1.0,
            10 => self.vrand,
            11 => self.vel * 2.0 - 1.0,
            12 | 13 => ((self.key - 60.0) / 36.0).clamp(-1.0, 1.0),
            _ => (x * TAU).sin(),
        };
        let att = 4.0 * (patch.v(ix::LFO_ATTACK) / 1000.0).powi(2);
        if att > 0.0 && shape < 8 { raw * (t / att).min(1.0) } else { raw }
    }

    /// Work out the harmonic levels for this moment and, if they moved, build the next wave.
    fn tick(&mut self, patch: &Patch, dphase: f32) {
        let t = self.frames as f32 / self.sr;
        let m = self.m;
        let n = ((0.45 / dphase.max(1e-6)) as usize).clamp(1, m / 4 - 1);
        let f0 = dphase * self.sr;

        // filter envelope
        let fa = 3.0 * (patch.v(ix::F_ATTACK) / 1000.0).powi(3);
        // measured on the factory test: the fall is a straight line in octaves, two a second at the rest settings
        let fd = (10.9 * (patch.v(ix::F_DECAY) / 1000.0).powi(3)).max(0.002);
        let fenv = if t < fa { t / fa } else { (1.0 - (t - fa) / fd).max(0.0) };
        let lfo = self.lfo(patch, t, fenv);

        // level and pitch movement
        let dec = patch.v(ix::DECAY);
        self.level = if dec < 1000.0 { (-t / (10.0 * (dec / 1000.0).powi(3) + 0.004)).exp() } else { 1.0 } * patch.v(ix::VOLUME) / 1000.0;
        if patch.v(ix::VEL_VOLUME) == 0.0 && self.vel > 0.0 {
            // the link from velocity is off: every note is as loud as a full one
            self.level *= (1.0 / self.vel).min(16.0);
        }
        let trem = patch.v(ix::TREMOLO) / 1000.0;
        self.trem = if trem > 0.0 { (t * 5.5 * TAU).sin() * trem * 0.8 } else { 0.0 };
        let vib = patch.v(ix::VIBRATO) / 1000.0 * 60.0 * (t * 6.0 * TAU).sin();
        let cents = patch.v(ix::PITCH) + vib + patch.v(ix::PITCH_LFO) / 1000.0 * 1200.0 * lfo;
        self.pitch_mul = 2f32.powf(cents / 1200.0);

        // filter position, in octaves above 20 Hz
        let filter_on = patch.v(ix::FILTER_ON) != 0.0;
        let cut = patch.v(ix::CUTOFF) / 8192.0 * 10.0
            + patch.v(ix::F_AMOUNT) / 1000.0 * 5.343 * fenv
            + patch.v(ix::CUTOFF_LFO) / 1000.0 * 4.0 * lfo
            + patch.v(ix::KEY_TRACK) / 1000.0 * (self.key - 60.0) / 12.0;
        let res = patch.v(ix::RESONANCE) / 1000.0;
        let open = !filter_on || (cut >= 9.99 && res == 0.0);

        let pluck = patch.v(ix::PLUCK);
        let pt = 6.0 * (pluck / 1000.0).powi(3) + 0.01;
        let pluck_on = pluck < 1000.0;
        let pluck_t = if pluck_on { (t / pt).min(12.0) } else { 0.0 };

        let mask_mix = (patch.v(ix::MASK_MIX) / 1000.0 + patch.v(ix::MASK_LFO) / 1000.0 * (lfo * 0.5 + 0.5)).clamp(0.0, 1.0);
        let ph_amt = patch.v(ix::PH_AMOUNT) / 1000.0;
        let ph_speed = patch.v(ix::PH_SPEED);
        let ph_hz = (ph_speed.abs() / 8000.0).powi(2) * 8.0 * ph_speed.signum();
        let ph_pos = if ph_amt > 0.0 { patch.v(ix::PH_OFFSET) / 1000.0 + ph_hz * t + patch.v(ix::PH_OFFSET_LFO) / 1000.0 * lfo } else { 0.0 };
        let ph_width = if ph_amt > 0.0 { (patch.v(ix::PH_WIDTH) / 5000.0 * (1.0 + patch.v(ix::PH_WIDTH_LFO) / 1000.0 * lfo)).max(0.02) } else { 0.0 };
        let hz_amt = (patch.v(ix::HZ_AMOUNT) / 1000.0 * (1.0 + patch.v(ix::HZ_LFO) / 1000.0 * lfo)).clamp(0.0, 1.0);

        // harmonic detune spreads the harmonics apart (or draws them together) the higher they lie, and
        // grit shakes each one; both are done by letting the harmonics' phases run, so the wave is redrawn
        // on every tick while either is on. How far the knobs reach is an estimate.
        let detune = patch.v(ix::DETUNE);
        let grit = patch.v(ix::GRIT) / 1000.0;
        let moving = detune != 0.0 || grit != 0.0;
        if moving && self.drift.len() < n {
            self.drift.resize(n, 0.0);
        }
        let key = [n as f32, if open { 99.0 } else { cut }, pluck_t, mask_mix, ph_pos, ph_width, hz_amt, if moving { t } else { 0.0 }];
        if self.primed && key.iter().zip(&self.last).all(|(a, b)| (a - b).abs() < 1e-4) {
            self.blend = false;
            return;
        }
        self.last = key;

        // harmonic levels
        let kind = patch.v(ix::TIMBRE) as u8;
        let custom = patch.custom.as_ref().filter(|_| kind == 11);
        let lhp = patch.v(ix::LHP) / 256.0;
        let ftype = (patch.v(ix::FILTER_TYPE) as usize).min(4);
        let slope = [8.0f32, 4.0, 3.0, 2.0, 1.0][ftype];
        let fc = 20.0 * 2f32.powf(cut.clamp(-2.0, 11.0));
        let res_at = (fc * 2f32.powf(patch.v(ix::RES_OFFSET) / 1200.0)).max(1.0);
        let eq: [f32; 6] = std::array::from_fn(|b| {
            let v = patch.v(ix::EQ + b) / 1024.0;
            if v < 0.0 { (1.0 + v).powi(2) } else { 1.0 + 3.0 * v }
        });
        let scale = patch.v(ix::PH_SCALE) as u8;
        let masked_pluck = patch.v(ix::PLUCK_MASKED) != 0.0;
        self.re[..m].fill(0.0);
        self.im[..m].fill(0.0);
        let hz_type = patch.v(ix::HZ_TYPE) as usize;
        let hz_mul = [2usize, 3, 4, 5, 2][hz_type.min(4)];
        let hz_reach = 1 + (patch.v(ix::HZ_WIDTH) / 1000.0 * 4.0) as usize;
        let hz_str = patch.v(ix::HZ_STRENGTH) / 1024.0;
        for k in 1..=n {
            let kf = k as f32;
            let mut a = match custom {
                Some((amp, _)) => amp.get(k - 1).copied().unwrap_or(0.0),
                None => timbre(kind, k),
            };
            if hz_amt > 0.0 {
                // clones of the lower harmonics, moved up by the chosen ratio
                let mut src = k;
                let mut g = hz_amt;
                for _ in 0..hz_reach {
                    if src % hz_mul != 0 {
                        break;
                    }
                    src /= hz_mul;
                    g *= 0.5 + hz_str * 0.5;
                    a += g * match custom {
                        Some((amp, _)) => amp.get(src - 1).copied().unwrap_or(0.0),
                        None => timbre(kind, src),
                    };
                }
            }
            // floating equaliser: bands sit on harmonics 1, 2, 4, 8, 16, 32
            let x = kf.log2().min(5.0);
            let b = (x as usize).min(4);
            a *= eq[b] + (eq[b + 1] - eq[b]) * (x - b as f32);
            let keep = (lhp - (kf - 1.0)).clamp(0.0, 1.0);
            let mval = patch.v(ix::MASK + (k - 1) % 12) / 1000.0;
            if mask_mix > 0.0 {
                a *= 1.0 - mask_mix * (1.0 - mval) * (1.0 - keep);
            }
            if pluck_on {
                let rate = if masked_pluck { (kf - 1.0).powf(0.6) * (1.5 - mval) } else { (kf - 1.0).powf(0.6) };
                a *= (-pluck_t * rate).exp();
            }
            if !open {
                let f = kf * f0;
                // the first type measured as a straight 18 dB an octave from a sharp corner
                let low = if ftype == 0 { (fc / f).powi(3).min(1.0) } else { 1.0 / (1.0 + (f / fc).powf(2.0 * slope)).sqrt() };
                let d = (f / res_at).log2() / 0.18;
                a *= low + res * 5.0 * (-d * d).exp();
            }
            if ph_amt > 0.0 {
                let pos = match scale {
                    1 => kf * f0 / 1000.0,
                    2 => kf / 8.0,
                    _ => kf.log2(),
                };
                a *= 1.0 - ph_amt * (0.5 - 0.5 * ((pos * ph_width - ph_pos) * TAU).cos());
            }
            if a == 0.0 {
                continue;
            }
            let phi = match custom {
                Some((_, ph)) => ph.get(k - 1).copied().unwrap_or(0.0),
                None => 0.0,
            } + self.hphase.get(k - 1).copied().unwrap_or(0.0);
            let phi = if moving {
                let d = &mut self.drift[k - 1];
                if detune != 0.0 {
                    // the sixteenth harmonic moves by the knob's own cents, lower ones by less
                    let ratio = 2f32.powf(detune * kf.log2() / 4800.0) - 1.0;
                    *d += (kf * dphase * ratio * TICK as f32).clamp(-0.45, 0.45);
                }
                if grit != 0.0 {
                    // turned left, the shaking grows with the harmonic number
                    let reach = if grit < 0.0 { (kf / 16.0).min(1.0) } else { 1.0 };
                    *d += (rnd(&mut self.seed) - 0.5) * grit.abs() * 0.5 * reach;
                }
                *d = d.fract();
                phi + *d
            } else {
                phi
            };
            let (s, c) = (phi * TAU).sin_cos();
            self.re[2 * k] = a * s;
            self.im[2 * k] = -a * c;
        }
        // the three sub sliders: an octave under the note, then its third and fourth harmonics
        for (i, bin) in [1usize, 3, 4].into_iter().enumerate() {
            let a = patch.v(ix::SUB + i) / 1024.0;
            if a > 0.0 && bin < m / 2 {
                self.im[bin] -= a;
            }
        }
        synth_fft(&mut self.re[..m], &mut self.im[..m]);
        let next = if self.primed { 1 - self.cur } else { self.cur };
        self.tab[next][..m].copy_from_slice(&self.re[..m]);
        self.tab[next][m] = self.re[0];
        self.blend = self.primed;
        if self.primed {
            // `cur` stays the wave being left until the fade is done
            self.fade = 0.0;
        }
        self.primed = true;
    }

    /// Let the note go; returns the seconds it still sounds.
    pub fn release(&mut self, patch: &Patch) -> f32 {
        let tau = patch.release_s();
        self.held = false;
        self.env_mul = (-1.0 / (tau * self.sr)).exp();
        tau * 9.2 + 0.05
    }

    /// One frame. `dphase` is the note's cycles per frame.
    #[inline]
    pub fn frame(&mut self, patch: &Patch, dphase: f32) -> [f32; 2] {
        if self.count == 0 {
            if self.blend {
                self.cur = 1 - self.cur;
            }
            self.tick(patch, dphase * self.pitch_mul);
        }
        self.count += 1;
        if self.count >= TICK {
            self.count = 0;
        }
        self.frames += 1;
        let m = self.m as f32;
        let (a, b) = (&self.tab[self.cur], &self.tab[1 - self.cur]);
        let inc = dphase * self.pitch_mul * 0.5;
        let mut out = [0.0f32; 2];
        for u in 0..self.order {
            let x = self.ph[u] * m;
            let i = (x as usize).min(self.m - 1);
            let fr = x - i as f32;
            let mut s = a[i] + (a[i + 1] - a[i]) * fr;
            if self.blend {
                let s2 = b[i] + (b[i + 1] - b[i]) * fr;
                s += (s2 - s) * self.fade;
            }
            out[0] += s * self.gl[u];
            out[1] += s * self.gr[u];
            let p = self.ph[u] + (inc * self.ratio[u]).min(0.24);
            self.ph[u] = if p >= 1.0 { p - 1.0 } else { p };
        }
        if self.blend {
            self.fade = (self.fade + 1.0 / TICK as f32).min(1.0);
        }
        if self.held {
            let a = patch.attack_s() * self.sr;
            self.env = if a < 1.0 { 1.0 } else { (self.env + 1.0 / a).min(1.0) };
        } else {
            self.env *= self.env_mul;
        }
        let lag = if self.held { 0.00215 } else { 0.006 };
        self.env_out += (self.env - self.env_out) * (1.0 - (-1.0 / (lag * self.sr)).exp());
        let g = self.level * 0.5304 * self.env_out;
        [out[0] * g * (1.0 - self.trem.max(0.0)), out[1] * g * (1.0 + self.trem.min(0.0))]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn patch() -> Patch {
        let g = crate::synth::Gen::new(crate::synth::Kind::Additive);
        Patch::new(&g.p, &g.raw)
    }

    #[test]
    fn the_effects_follow_their_knobs() {
        let mut g = crate::synth::Gen::new(crate::synth::Kind::Additive);
        // a fresh one has all three switched off
        assert!(!g.tail().any());
        for i in [ix::CHORUS_TYPE, ix::DELAY_TYPE, ix::REVERB_TYPE] {
            g.p[i] = 1.0;
        }
        let t = g.tail();
        // the echo of a fresh one is a beat and a half long
        assert_eq!(t.delay.unwrap().left, 6.0);
        assert!(t.chorus.is_some() && t.reverb.is_some());
        for i in [ix::CHORUS_MIX, ix::DELAY_VOL, ix::REVERB_VOL] {
            g.p[i] = 0.0;
        }
        assert!(!g.tail().any());
    }

    fn render(p: &Patch, hz: f32, frames: usize) -> Vec<f32> {
        let sr = 48_000.0;
        let mut v = Voice::start(p, sr, 57, 0.8, hz / sr, 7);
        (0..frames).map(|_| v.frame(p, hz / sr)[0]).collect()
    }

    /// Level of the component at `hz` in `x`.
    fn level_at(x: &[f32], hz: f32) -> f32 {
        let (mut s, mut c) = (0.0f32, 0.0f32);
        for (i, v) in x.iter().enumerate() {
            let a = TAU * hz * i as f32 / 48_000.0;
            s += v * a.sin();
            c += v * a.cos();
        }
        (s * s + c * c).sqrt() * 2.0 / x.len() as f32
    }

    #[test]
    fn harmonic_detune_moves_the_upper_harmonics_and_grit_roughens_them() {
        let mut p = patch();
        p.p[ix::FILTER_ON] = 0.0;
        p.p[ix::PHASE] = 0.0;
        let x = render(&p, 200.0, 48_000);
        p.p[ix::DETUNE] = 240.0;
        let y = render(&p, 200.0, 48_000);
        // the fourth harmonic goes up by 120 cents, the first stays
        let moved = 800.0 * 2f32.powf(120.0 / 1200.0);
        assert!(level_at(&y, moved) > 4.0 * level_at(&y, 800.0), "{} {}", level_at(&y, moved), level_at(&y, 800.0));
        assert!(level_at(&y, moved) > 0.5 * level_at(&x, 800.0));
        assert!((level_at(&y, 200.0) / level_at(&x, 200.0) - 1.0).abs() < 0.05);
        p.p[ix::DETUNE] = 0.0;
        p.p[ix::GRIT] = 600.0;
        let z = render(&p, 200.0, 48_000);
        assert!(level_at(&z, 800.0) < 0.8 * level_at(&x, 800.0));
        assert!(level_at(&z, 730.0) > 3.0 * level_at(&x, 730.0));
    }

    #[test]
    fn an_open_saw_has_harmonics_falling_as_one_over_k() {
        let mut p = patch();
        p.p[ix::FILTER_ON] = 0.0;
        p.p[ix::PHASE] = 0.0;
        let x = render(&p, 220.0, 48_000);
        let (h1, h2, h3) = (level_at(&x, 220.0), level_at(&x, 440.0), level_at(&x, 660.0));
        assert!((h1 / h2 - 2.0).abs() < 0.1, "h1/h2 {}", h1 / h2);
        assert!((h1 / h3 - 3.0).abs() < 0.15, "h1/h3 {}", h1 / h3);
        // nothing under the note without the sub sliders
        assert!(level_at(&x, 110.0) < h1 * 0.01);
    }

    #[test]
    fn the_filter_and_the_mask_cut_what_they_should() {
        let mut p = patch();
        p.p[ix::PHASE] = 0.0;
        p.p[ix::F_AMOUNT] = 0.0;
        p.p[ix::CUTOFF] = 3500.0; // a little above the note
        let x = render(&p, 220.0, 48_000);
        assert!(level_at(&x, 2200.0) < level_at(&x, 220.0) * 0.02, "the tenth harmonic is filtered out");
        // the default mask keeps odd harmonics; fully mixed in, the even ones go
        let mut p = patch();
        p.p[ix::PHASE] = 0.0;
        p.p[ix::FILTER_ON] = 0.0;
        p.p[ix::MASK_MIX] = 1000.0;
        let x = render(&p, 220.0, 48_000);
        assert!(level_at(&x, 440.0) < level_at(&x, 660.0) * 0.05);
        // a sub slider adds the octave below
        p.p[ix::SUB] = 1024.0;
        let x = render(&p, 220.0, 48_000);
        assert!(level_at(&x, 110.0) > level_at(&x, 220.0) * 0.5);
    }

    #[test]
    fn a_pluck_darkens_the_note_over_time_and_unison_widens_it() {
        let mut p = patch();
        p.p[ix::PHASE] = 0.0;
        p.p[ix::FILTER_ON] = 0.0;
        p.p[ix::PLUCK] = 400.0;
        let x = render(&p, 220.0, 96_000);
        let early = level_at(&x[..9_600], 1100.0);
        let late = level_at(&x[86_400..], 1100.0);
        assert!(late < early * 0.3, "early {early} late {late}");
        assert!(level_at(&x[86_400..], 220.0) > 0.1, "the note itself stays");

        let mut p = patch();
        p.p[ix::UNI_ORDER] = 5.0;
        let sr = 48_000.0;
        let mut v = Voice::start(&p, sr, 57, 0.8, 220.0 / sr, 3);
        let mut diff = 0.0f32;
        for _ in 0..48_000 {
            let f = v.frame(&p, 220.0 / sr);
            assert!(f[0].is_finite() && f[0].abs() < 4.0);
            diff += (f[0] - f[1]).abs();
        }
        assert!(diff > 100.0, "left and right differ with unison on: {diff}");
    }

    #[test]
    fn a_custom_wave_in_the_block_is_played() {
        let g = crate::synth::Gen::new(crate::synth::Kind::Additive);
        let mut raw = g.raw[..432].to_vec();
        raw.extend_from_slice(&3i32.to_le_bytes());
        for a in [0.0f32, 0.0, 1.0] {
            raw.extend_from_slice(&a.to_le_bytes());
        }
        for _ in 0..3 {
            raw.extend_from_slice(&0f32.to_le_bytes());
        }
        let mut p = Patch::new(&g.p, &raw);
        p.p[ix::TIMBRE] = 11.0;
        p.p[ix::FILTER_ON] = 0.0;
        let x = render(&p, 220.0, 48_000);
        assert!(level_at(&x, 660.0) > 0.2);
        assert!(level_at(&x, 220.0) < 0.01);
    }
}
