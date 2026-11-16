//! String Model: a touch (a click or noise, shaped by a filter and its own envelope) sets two strings ringing.
//! Each string is a loop as long as one turn of its note, losing a little on every pass; the two are mixed,
//! spread and levelled by an envelope. A free envelope, a slow oscillator, the key and how hard it was struck
//! can each be tied to a setting through eight links. After the voices come a body of eight resonances, an
//! echo, a chorus and a room (`tailfx.rs`).
//!
//! The stored settings are a list of numbers, most from 0 to 1. Which number is which was worked out from how
//! they move together across many stored sounds and from the public manual's description of the controls;
//! the scales, and the places of a few of the controls, are our estimates.

use std::f32::consts::{PI, TAU};
use std::sync::OnceLock;

use crate::duo::{Ctl, Info};
use crate::tailfx;

/// Numbers we read from the stored list.
pub const COUNT: usize = 114;

pub const GROUPS: [&str; 21] = [
    "Master",
    "Level envelope",
    "String 1",
    "String 2",
    "Strings",
    "Touch",
    "Low cut",
    "Free envelope",
    "Slow oscillator",
    "Body",
    "Echo",
    "Chorus",
    "Room",
    "Link 1",
    "Link 2",
    "Link 3",
    "Link 4",
    "Link 5",
    "Link 6",
    "Link 7",
    "Link 8",
];

const RATIOS: &[&str] = &["1 : 4", "1 : 3", "1 : 2", "1 : 1", "2 : 1", "3 : 1", "4 : 1"];
const RATIO: [f32; 7] = [0.25, 1.0 / 3.0, 0.5, 1.0, 2.0, 3.0, 4.0];
const SYNCS: &[&str] = &["Off", "1 step", "2 steps", "3 steps", "4 steps", "6 steps", "8 steps", "12 steps", "16 steps"];
const SYNC: [f32; 9] = [0.0, 1.0, 2.0, 3.0, 4.0, 6.0, 8.0, 12.0, 16.0];
const SOURCES: &[&str] = &["Off", "Envelope", "Slow oscillator", "Mod wheel", "Pitch wheel", "Aftertouch", "Velocity", "Key", "Release velocity"];
const TARGETS: &[&str] = &[
    "Slow oscillator depth",
    "Level",
    "Pan",
    "Pitch",
    "Low cut",
    "Touch cutoff",
    "Touch attack",
    "Touch decay",
    "String 1 decay",
    "String 2 decay",
    "Both decays",
    "String 1 damping",
    "String 2 damping",
    "Both dampings",
    "Noise rate",
];
const STAGE_PITCH: [&str; 8] = ["Pitch 1", "Pitch 2", "Pitch 3", "Pitch 4", "Pitch 5", "Pitch 6", "Pitch 7", "Pitch 8"];
const STAGE_RING: [&str; 8] = ["Ring 1", "Ring 2", "Ring 3", "Ring 4", "Ring 5", "Ring 6", "Ring 7", "Ring 8"];
const STAGE_FLIP: [&str; 8] = ["Flip 1", "Flip 2", "Flip 3", "Flip 4", "Flip 5", "Flip 6", "Flip 7", "Flip 8"];

/// Every stored number in file order: its name, control, starting value and group.
pub fn params() -> &'static [Info] {
    static P: OnceLock<Vec<Info>> = OnceLock::new();
    P.get_or_init(|| {
        let mut v: Vec<Info> = (0..COUNT).map(|_| Info { name: "", ctl: Ctl::Kept, def: 0.0, group: 0 }).collect();
        let mut put = |i: usize, name: &'static str, ctl: Ctl, def: f32, group: u8| v[i] = Info { name, ctl, def, group };
        put(0, "Volume", Ctl::Knob, 0.3525, 0);
        put(1, "Tune", Ctl::Knob, 0.5, 0);
        put(113, "Transpose", Ctl::Knob, 0.5, 0);
        for (k, (name, def)) in [("Attack", 0.0), ("Decay", 1.0), ("Sustain", 1.0), ("Release", 0.0)].into_iter().enumerate() {
            put(2 + k, name, Ctl::Knob, def, 1);
        }
        for s in 0..2 {
            let (at, group) = (6 + s * 5, 2 + s as u8);
            put(at, "Decay", Ctl::Knob, 1.0, group);
            put(at + 1, "Flip", Ctl::Switch, 0.0, group);
            put(at + 2, "Damping", Ctl::Knob, 0.0, group);
            put(at + 3, "Sharpness", Ctl::Knob, 0.0, group);
            put(at + 4, "Offset", Ctl::Knob, 0.0, group);
        }
        put(111, "Ratio", Ctl::Choice(RATIOS), 3.0, 3);
        put(17, "Mix", Ctl::Knob, 0.0, 4);
        put(18, "Spread", Ctl::Knob, 0.5, 4);
        put(20, "Saturation", Ctl::Knob, 0.0, 4);
        put(31, "Noise / click", Ctl::Knob, 1.0, 5);
        put(16, "Noise rate", Ctl::Knob, 0.0, 5);
        put(29, "Cutoff", Ctl::Knob, 0.5, 5);
        put(30, "Resonance", Ctl::Knob, 0.0, 5);
        put(32, "Overdrive", Ctl::Knob, 0.0, 5);
        put(26, "Attack", Ctl::Knob, 0.0, 5);
        put(27, "Decay", Ctl::Knob, 0.2, 5);
        put(28, "Sustain", Ctl::Knob, 0.0, 5);
        put(24, "Cutoff", Ctl::Knob, 0.0, 6);
        put(25, "Resonance", Ctl::Knob, 0.0, 6);
        for (k, (name, def)) in [("Attack", 0.0), ("Decay", 1.0), ("Sustain", 1.0), ("Release", 1.0)].into_iter().enumerate() {
            put(34 + k, name, Ctl::Knob, def, 7);
        }
        put(38, "Rate", Ctl::Knob, 0.0, 8);
        for k in 0..8 {
            put(39 + k, STAGE_PITCH[k], Ctl::Knob, 0.0, 9);
            put(47 + k, STAGE_RING[k], Ctl::Knob, 0.0, 9);
            put(55 + k, STAGE_FLIP[k], Ctl::Switch, 0.0, 9);
        }
        put(63, "On", Ctl::Switch, 0.0, 10);
        put(64, "Time", Ctl::Knob, 0.5, 10);
        put(112, "Sync", Ctl::Choice(SYNCS), 0.0, 10);
        put(65, "Feedback", Ctl::Knob, 0.6, 10);
        put(69, "Mix", Ctl::Knob, 0.35, 10);
        put(70, "On", Ctl::Switch, 0.0, 11);
        put(71, "Depth", Ctl::Knob, 0.5, 11);
        put(72, "Speed", Ctl::Knob, 0.5, 11);
        put(73, "Mix", Ctl::Knob, 0.4, 11);
        put(74, "On", Ctl::Switch, 0.0, 12);
        put(75, "Decay", Ctl::Knob, 0.5, 12);
        put(76, "Colour", Ctl::Knob, 0.5, 12);
        put(77, "Mix", Ctl::Knob, 0.5, 12);
        for k in 0..8 {
            let (at, group) = (79 + k * 4, 13 + k as u8);
            // a new one ties how hard a key is struck to the level and the pitch wheel to the pitch
            let (source, target) = [(6.0, 1.0), (4.0, 3.0)].get(k).copied().unwrap_or((0.0, 0.0));
            put(at, "Source", Ctl::Choice(SOURCES), source, group);
            put(at + 1, "Amount", Ctl::Knob, if k < 2 { 1.0 } else { 0.0 }, group);
            put(at + 2, "Flip", Ctl::Switch, 0.0, group);
            put(at + 3, "Target", Ctl::Choice(TARGETS), target, group);
        }
        // not placed yet: kept as they are
        for (i, def) in [(19, 0.5), (66, 0.24), (67, 0.2), (68, 0.5)] {
            v[i].def = def;
        }
        v
    })
}

/// The values of a new instrument.
pub fn defaults() -> Vec<f32> {
    params().iter().map(|i| i.def).collect()
}

/// A fresh stored block: the mark, the length of what follows, the version mark, the numbers and a name.
pub fn fresh() -> Vec<u8> {
    let mut v = Vec::with_capacity(596);
    v.extend_from_slice(b"SSLF");
    v.extend_from_slice(&588u32.to_le_bytes());
    v.extend_from_slice(b"1rkS");
    for x in defaults() {
        v.extend_from_slice(&x.to_le_bytes());
    }
    v.extend_from_slice(b"Default");
    v.resize(596, 0);
    v
}

#[derive(Debug, Clone, Copy, Default)]
struct Adsr {
    a: f32,
    d: f32,
    s: f32,
    r: f32,
}

#[derive(Debug, Clone, Copy, Default)]
struct Wire {
    decay: f32,
    flip: bool,
    damp: f32,
    sharp: f32,
    offset: f32,
}

#[derive(Debug, Clone, Copy, Default)]
struct Link {
    source: u8,
    /// Signed: a flipped link pulls the other way.
    amount: f32,
    target: u8,
}

/// The settings a voice plays from.
#[derive(Debug, Clone, Default)]
pub struct Set {
    gain: f32,
    /// What the transpose and tune settings multiply the pitch by.
    ratio: f32,
    amp: Adsr,
    free: Adsr,
    touch: [f32; 3],
    wire: [Wire; 2],
    second: f32,
    mix: f32,
    spread: f32,
    sat: f32,
    click: f32,
    noise: f32,
    cut: f32,
    reso: f32,
    drive: f32,
    low: f32,
    low_reso: f32,
    lfo_hz: f32,
    links: [Link; 8],
    /// A link sets how deep the slow oscillator goes; without one it runs at full depth.
    depth_linked: bool,
}

/// Seconds of an envelope stage.
fn time(v: f32) -> f32 {
    0.001 * 8000f32.powf(v.clamp(0.0, 1.0))
}

pub fn set(p: &[f32]) -> Set {
    if p.len() < COUNT {
        return set(&defaults());
    }
    let u = |i: usize| if p[i].is_finite() { p[i].clamp(0.0, 1.0) } else { 0.0 };
    let whole = |i: usize, top: usize| if p[i].is_finite() { (p[i].round().max(0.0) as usize).min(top) } else { 0 };
    let wire = |at: usize| Wire { decay: u(at), flip: p[at + 1] >= 0.5, damp: u(at + 2), sharp: u(at + 3), offset: u(at + 4) };
    let mut links = [Link::default(); 8];
    for (k, l) in links.iter_mut().enumerate() {
        let at = 79 + k * 4;
        let amount = u(at + 1) * if p[at + 2] >= 0.5 { -1.0 } else { 1.0 };
        *l = Link { source: if amount == 0.0 { 0 } else { whole(at, 8) as u8 }, amount, target: whole(at + 3, 14) as u8 };
    }
    let level = u(0) / 0.3525;
    Set {
        gain: level * 0.5,
        ratio: 2f32.powf(((u(113) - 0.5) * 72.0 + (u(1) - 0.5)) / 12.0),
        amp: Adsr { a: time(u(2)), d: time(u(3)), s: u(4), r: time(u(5)) },
        free: Adsr { a: time(u(34)), d: time(u(35)), s: u(36), r: time(u(37)) },
        touch: [u(26), u(27), u(28)],
        wire: [wire(6), wire(11)],
        second: RATIO[whole(111, 6)],
        mix: u(17),
        spread: (u(18) - 0.5) * 2.0,
        sat: u(20),
        click: u(31),
        noise: u(16),
        cut: u(29),
        reso: u(30),
        drive: u(32),
        low: u(24),
        low_reso: u(25),
        lfo_hz: 0.1 * 200f32.powf(u(38)),
        depth_linked: links.iter().any(|l| l.source != 0 && l.target == 0),
        links,
    }
}

/// The effects after the voices.
pub fn tail(p: &[f32]) -> tailfx::Set {
    if p.len() < COUNT {
        return tailfx::Set::default();
    }
    let u = |i: usize| if p[i].is_finite() { p[i].clamp(0.0, 1.0) } else { 0.0 };
    let (mut hz, mut fb, mut flip) = ([0.0; 8], [0.0; 8], 0u8);
    for k in 0..8 {
        hz[k] = 60.0 * 2f32.powf(u(39 + k) * 7.5);
        fb[k] = u(47 + k);
        if p[55 + k] >= 0.5 {
            flip |= 1 << k;
        }
    }
    let sync = SYNC[if p[112].is_finite() { (p[112].round().max(0.0) as usize).min(8) } else { 0 }];
    tailfx::Set {
        split: false,
        tone_at: 0,
        op: None,
        zone: None,
        stack: None,
        level: false,
        wide: false,
        tone: None,
        more: [None; 2],
        rack: None,
        body: fb.iter().any(|&f| f > 0.0).then_some(tailfx::Body { hz, fb, flip }),
        delay: (p[63] >= 0.5 && u(69) > 0.0).then(|| tailfx::Delay {
            left: sync,
            right: sync,
            fb: u(65) * 0.95,
            wobble: 0.0,
            low_hz: 0.0,
            high_hz: 9000.0,
            wet: u(69),
            free: if sync > 0.0 { 0.0 } else { 0.004 + u(64) * u(64) },
        }),
        chorus: (p[70] >= 0.5 && u(73) > 0.0).then(|| tailfx::Chorus { depth: u(71), hz: 0.05 * 100f32.powf(u(72)), wet: u(73) }),
        gate: None,
        reverb: (p[74] >= 0.5 && u(77) > 0.0).then(|| tailfx::Reverb { secs: 0.3 * 30f32.powf(u(75)), high_hz: 500.0 * 2f32.powf(u(76) * 5.3), wet: u(77) }),
    }
}

#[derive(Debug, Clone, Copy, Default)]
struct Env {
    v: f32,
    stage: u8,
}

impl Env {
    fn step(&mut self, e: &Adsr, dt: f32) -> f32 {
        match self.stage {
            0 => {
                self.v += dt / e.a.max(0.0005);
                if self.v >= 1.0 {
                    self.v = 1.0;
                    self.stage = 1;
                }
            }
            1 => self.v = e.s + (self.v - e.s) * (-dt * 4.6 / e.d.max(0.001)).exp(),
            _ => self.v *= (-dt * 4.6 / e.r.max(0.001)).exp(),
        }
        self.v
    }
}

#[derive(Debug, Clone, Default)]
struct Loop {
    line: Vec<f32>,
    at: usize,
    /// Frames back the loop reads, and where that is heading.
    back: f32,
    back_to: f32,
    low: [f32; 3],
    /// Per pass: what is kept, and the three losses of highs.
    keep: f32,
    c: [f32; 2],
    sharp: f32,
    gl: f32,
    gr: f32,
}

const EVERY: u32 = 16;

#[derive(Debug, Clone, Default)]
pub struct Voice {
    sr: f32,
    key: f32,
    vel: f32,
    seed: u32,
    t: u32,
    amp: Env,
    free: Env,
    touch: Env,
    touch_now: f32,
    lfo: f32,
    w: [Loop; 2],
    hold: f32,
    turn: f32,
    rate: f32,
    feed: f32,
    hc: [f32; 2],
    hg: f32,
    hk: f32,
    lc: [f32; 2],
    lg: f32,
    lk: f32,
    level: f32,
    level_to: f32,
}

fn rnd(seed: &mut u32) -> f32 {
    *seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
    (*seed >> 8) as f32 / 8_388_608.0 - 1.0
}

impl Voice {
    pub fn start(_set: &Set, sr: f32, key: u8, vel: f32, seed: u32) -> Self {
        let n = ((sr / 18.0) as usize + 8).next_power_of_two();
        let wire = Loop { line: vec![0.0; n], ..Default::default() };
        Self { sr, key: key as f32, vel: vel.clamp(0.0, 1.0), seed: seed | 1, w: [wire.clone(), wire], ..Default::default() }
    }

    /// The key lifts: the touch stops and the level envelope lets go. Returns the seconds the voice still sounds.
    pub fn release(&mut self, set: &Set) -> f32 {
        self.amp.stage = 2;
        self.free.stage = 2;
        self.touch.stage = 2;
        set.amp.r * 1.5 + 0.02
    }

    /// The slow work: envelopes, the links and every setting they move.
    fn control(&mut self, set: &Set, dphase: f32) {
        let sr = self.sr;
        let dt = EVERY as f32 / sr;
        let env = self.free.step(&set.free, dt);
        self.lfo = (self.lfo + set.lfo_hz * dt).fract();
        let wave = (self.lfo * TAU).sin();
        let key = self.key / 127.0;
        // what each source gives now; the wheels and key pressure rest at nothing
        let plain = |source: u8| match source {
            1 => env,
            6 => self.vel,
            7 => key,
            _ => 0.0,
        };
        let depth = if set.depth_linked {
            set.links.iter().filter(|l| l.target == 0 && l.source != 2).map(|l| l.amount * plain(l.source)).sum::<f32>().clamp(0.0, 1.0)
        } else {
            1.0
        };
        // one sum per target; the level is a product
        let mut add = [0.0f32; 15];
        let mut level = 1.0f32;
        for l in &set.links {
            let (value, swings) = match l.source {
                0 => continue,
                2 => (wave * depth, true),
                s => (plain(s), false),
            };
            if l.target == 1 {
                level *= if swings {
                    (1.0 + l.amount * value * 0.5).max(0.0)
                } else if l.amount >= 0.0 {
                    1.0 - l.amount + l.amount * value
                } else {
                    1.0 + l.amount * value
                };
            } else {
                add[l.target as usize] += l.amount * value;
            }
        }
        let knob = |v: f32| v.clamp(0.0, 1.0);
        // the touch
        let touch = Adsr { a: 0.0005 * 4000f32.powf(knob(set.touch[0] + add[6])), d: 0.002 * 2500f32.powf(knob(set.touch[1] + add[7])), s: set.touch[2], r: 0.03 };
        self.touch_now = self.touch.step(&touch, dt);
        let noise = knob(set.noise + add[14]);
        self.rate = 0.02 + 0.98 * noise * noise;
        let cut = (40.0 * 2f32.powf(knob(set.cut + add[5]) * 9.0)).min(sr * 0.45);
        self.hg = (PI * cut / sr).tan();
        self.hk = 2.0 - 1.9 * set.reso;
        let low = (20.0 * 100f32.powf(knob(set.low + add[4]))).min(sr * 0.45);
        self.lg = (PI * low / sr).tan();
        self.lk = 2.0 - 1.9 * set.low_reso;
        // the strings
        let hz = (dphase * sr * set.ratio * 2f32.powf(add[3] * 2.0 / 12.0)).clamp(20.0, sr * 0.45);
        let pan = add[2].clamp(-1.0, 1.0);
        let mut keeps = 0.0f32;
        for s in 0..2 {
            let wire = &set.wire[s];
            let f = (hz * if s == 1 { set.second } else { 1.0 }).clamp(20.0, sr * 0.45);
            let decay = knob(wire.decay + add[8 + s] + add[10]);
            let damp = knob(wire.damp + add[11 + s] + add[13]);
            let secs = 0.03 * 1000f32.powf(decay);
            let w = &mut self.w[s];
            w.keep = 10f32.powf(-3.0 / (f * secs)) * if wire.flip { -1.0 } else { 1.0 };
            keeps = keeps.max(w.keep.abs());
            let coef = |fc: f32| 1.0 - (-TAU * fc.min(sr * 0.49) / sr).exp();
            w.c = [coef(f * 2f32.powf(8.0 * (1.0 - damp))), coef(f * 2f32.powf(8.0 - 6.0 * wire.offset))];
            w.sharp = wire.sharp;
            // the losses hold the wave back a little: the loop is shortened by as much, so the note stays in tune
            let turn = TAU * f / sr;
            // one loss as a turn and a size at the note's own pitch
            let through = |c: f32| {
                let a = 1.0 - c;
                let (re, im) = (1.0 - a * turn.cos(), a * turn.sin());
                let size = c / (re * re + im * im);
                (size * re, -size * im)
            };
            let (re, im) = through(w.c[0]);
            let (re2, im2) = through(w.c[1]);
            // sharpness blends in a second pass through the first loss
            let (bre, bim) = (1.0 - w.sharp + w.sharp * re, w.sharp * im);
            let held = -(im.atan2(re) + bim.atan2(bre) + im2.atan2(re2)) / turn;
            w.back_to = (sr / f - held).clamp(2.0, w.line.len() as f32 - 4.0);
            if self.t == 0 {
                w.back = w.back_to;
            }
            // mix: both strings in full at the middle; spread sends them to opposite sides
            let share = if s == 0 { ((1.0 - set.mix) * 2.0).min(1.0) } else { (set.mix * 2.0).min(1.0) };
            let side = ((if s == 0 { set.spread } else { -set.spread }) + pan).clamp(-1.0, 1.0);
            let turn = (side + 1.0) * PI / 4.0;
            w.gl = share * turn.cos() * std::f32::consts::SQRT_2;
            w.gr = share * turn.sin() * std::f32::consts::SQRT_2;
        }
        // steady noise builds up in a loop that keeps nearly all it gets: a touch that holds on feeds it less
        self.feed = 1.0 - set.touch[2] * (1.0 - (1.0 - keeps * keeps).max(1e-6).powf(0.25));
        self.level_to = self.amp.step(&set.amp, dt) * level * set.gain;
        if self.t == 0 {
            self.level = self.level_to;
        }
    }

    pub fn frame(&mut self, set: &Set, dphase: f32) -> [f32; 2] {
        if self.t % EVERY == 0 {
            self.control(set, dphase);
        }
        self.t = self.t.wrapping_add(1);
        // the touch: a held push for the click, stepped noise for the bow
        self.turn += self.rate;
        if self.turn >= 1.0 {
            self.turn -= 1.0;
            self.hold = rnd(&mut self.seed);
        }
        let mut x = self.touch_now * (set.click + (1.0 - set.click) * self.hold * self.feed);
        // its high cut
        let (g, k) = (self.hg, self.hk);
        let hp = (x - (g + k) * self.hc[0] - self.hc[1]) / (1.0 + g * (g + k));
        let bp = g * hp + self.hc[0];
        self.hc[0] = g * hp + bp;
        let lp = g * bp + self.hc[1];
        self.hc[1] = g * bp + lp;
        x = lp;
        if set.drive > 0.0 {
            let push = 1.0 + 3.0 * set.drive;
            x = (x * push).tanh() / push.sqrt();
        }
        // and its low cut, which also keeps a held push from piling up in the strings
        let (g, k) = (self.lg, self.lk);
        let hp = (x - (g + k) * self.lc[0] - self.lc[1]) / (1.0 + g * (g + k));
        let bp = g * hp + self.lc[0];
        self.lc[0] = g * hp + bp;
        let lp = g * bp + self.lc[1];
        self.lc[1] = g * bp + lp;
        x = hp;
        let mut out = [0.0f32; 2];
        for w in &mut self.w {
            w.back += (w.back_to - w.back) * 0.02;
            let n = w.line.len();
            let whole = w.back.floor();
            let fr = w.back - whole;
            let i = w.at + n - whole as usize;
            let (a, b, c, d) = (w.line[(i + 1) & (n - 1)], w.line[i & (n - 1)], w.line[(i - 1) & (n - 1)], w.line[(i - 2) & (n - 1)]);
            // a smooth curve through four frames
            let y = b + 0.5 * fr * (c - a + fr * (2.0 * a - 5.0 * b + 4.0 * c - d + fr * (3.0 * (b - c) + d - a)));
            w.low[0] += (y - w.low[0]) * w.c[0];
            w.low[1] += (w.low[0] - w.low[1]) * w.c[0];
            let damped = w.low[0] + (w.low[1] - w.low[0]) * w.sharp;
            w.low[2] += (damped - w.low[2]) * w.c[1];
            let mut next = x + w.low[2] * w.keep;
            if !next.is_finite() {
                next = 0.0;
            }
            w.line[w.at] = next.clamp(-8.0, 8.0);
            w.at = (w.at + 1) & (n - 1);
            out[0] += y * w.gl;
            out[1] += y * w.gr;
        }
        self.level += (self.level_to - self.level) * (1.0 / EVERY as f32);
        for o in &mut out {
            if set.sat > 0.0 {
                let push = 1.0 + 5.0 * set.sat;
                *o = (*o * push).tanh() / push.sqrt();
            }
            // loud sums bend over instead of running away
            *o = *o / (1.0 + o.abs() * 0.5) * self.level;
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn play(p: &[f32], key: u8, frames: usize, hold: usize) -> Vec<[f32; 2]> {
        let s = set(p);
        let sr = 48000.0;
        let dphase = 440.0 * 2f32.powf((key as f32 - 69.0) / 12.0) / sr;
        let mut v = Voice::start(&s, sr, key, 1.0, 7);
        (0..frames)
            .map(|i| {
                if i == hold {
                    v.release(&s);
                }
                v.frame(&s, dphase)
            })
            .collect()
    }

    /// The pitch from upward crossings of the left side between two frames.
    fn pitch(buf: &[[f32; 2]], from: usize, to: usize) -> f32 {
        // smooth first, so overtones do not add crossings
        let mut low = 0.0f32;
        let smooth: Vec<f32> = buf.iter().map(|f| {
            low += (f[0] - low) * 0.05;
            low
        }).collect();
        let hits: Vec<usize> = (from + 1..to).filter(|&i| smooth[i - 1] < 0.0 && smooth[i] >= 0.0).collect();
        (hits.len() - 1) as f32 * 48000.0 / (hits[hits.len() - 1] - hits[0]) as f32
    }

    #[test]
    fn the_table_and_the_fresh_block_agree() {
        let p = params();
        assert_eq!(p.len(), COUNT);
        assert!(p.iter().all(|i| (i.group as usize) < GROUPS.len()));
        let b = fresh();
        assert_eq!(b.len(), 596);
        assert_eq!(f32::from_le_bytes(b[12..16].try_into().unwrap()), 0.3525);
        assert_eq!(f32::from_le_bytes(b[12 + 113 * 4..12 + 114 * 4].try_into().unwrap()), 0.5);
        assert_eq!(&b[468..475], b"Default");
    }

    #[test]
    fn a_plucked_string_sounds_in_tune_and_fades() {
        let mut p = defaults();
        p[16] = 1.0;
        p[6] = 0.6;
        let buf = play(&p, 57, 48000, 40000);
        let hz = pitch(&buf, 4000, 30000);
        assert!((hz - 220.0).abs() < 1.5, "{hz}");
        let peak = |from: usize, to: usize| buf[from..to].iter().map(|f| f[0].abs()).fold(0.0f32, f32::max);
        assert!(peak(0, 4000) > 0.05, "{}", peak(0, 4000));
        assert!(peak(30000, 34000) < peak(2000, 6000) * 0.7);
        // the key lifted: the default release is short
        assert!(peak(44000, 48000) < 1e-3);
        // heavy damping still lands on the note
        p[8] = 0.6;
        p[9] = 0.5;
        let hz = pitch(&play(&p, 57, 30000, 30000), 3000, 28000);
        assert!((hz - 220.0).abs() < 2.5, "{hz}");
    }

    #[test]
    fn a_flipped_string_sounds_an_octave_down_and_the_second_string_follows_its_ratio() {
        let mut p = defaults();
        p[16] = 1.0;
        p[7] = 1.0;
        let hz = pitch(&play(&p, 69, 30000, 30000), 3000, 28000);
        assert!((hz - 220.0).abs() < 2.0, "{hz}");
        let mut p = defaults();
        p[16] = 1.0;
        p[17] = 1.0;
        p[111] = 4.0;
        let hz = pitch(&play(&p, 45, 30000, 30000), 3000, 28000);
        assert!((hz - 220.0).abs() < 2.0, "{hz}");
        // spread sends the two strings to opposite sides
        p[17] = 0.5;
        p[18] = 1.0;
        let buf = play(&p, 45, 8000, 8000);
        assert!(buf.iter().any(|f| (f[0] - f[1]).abs() > 0.02));
    }

    #[test]
    fn the_links_move_what_they_point_at() {
        // a bowed note: steady noise held by the touch
        let mut p = defaults();
        p[28] = 1.0;
        p[31] = 0.0;
        p[16] = 1.0;
        p[6] = 0.5;
        let rms = |buf: &[[f32; 2]]| (buf.iter().map(|f| f[0] * f[0]).sum::<f32>() / buf.len() as f32).sqrt();
        let held = play(&p, 57, 30000, 30000);
        assert!(rms(&held[20000..]) > 0.01, "{}", rms(&held[20000..]));
        // struck softly it is quieter, through the first link
        let s = set(&p);
        let mut soft = Voice::start(&s, 48000.0, 57, 0.2, 7);
        let quiet: Vec<[f32; 2]> = (0..30000).map(|_| soft.frame(&s, 220.0 / 48000.0)).collect();
        assert!(rms(&quiet[20000..]) < rms(&held[20000..]) * 0.5);
        // the slow oscillator tied to the pitch bends the note up and down
        let mut q = defaults();
        q[16] = 1.0;
        q[38] = 0.6;
        q[87..91].copy_from_slice(&[2.0, 1.0, 0.0, 3.0]);
        let buf = play(&q, 57, 48000, 48000);
        let spans: Vec<f32> = (0..10).map(|k| pitch(&buf, 4000 + k * 4000, 8000 + k * 4000)).collect();
        let (lo, hi) = spans.iter().fold((f32::MAX, 0.0f32), |(lo, hi), &h| (lo.min(h), hi.max(h)));
        assert!(hi - lo > 4.0, "{spans:?}");
        // the effects follow their switches
        assert!(!tail(&defaults()).any());
        let mut e = defaults();
        e[63] = 1.0;
        e[112] = 3.0;
        e[74] = 1.0;
        e[47] = 0.5;
        let t = tail(&e);
        assert!(t.delay.is_some_and(|d| d.left == 3.0 && d.free == 0.0) && t.reverb.is_some() && t.body.is_some() && t.chorus.is_none());
        e[112] = 0.0;
        assert!(tail(&e).delay.is_some_and(|d| d.free > 0.2));
    }
}
