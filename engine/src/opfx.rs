//! The effects section of the operator synth: a chorus, three echoes and a room. Nothing passes straight
//! through it: what is heard is what the chorus, the echoes and the room put out. Every law here was measured
//! on rendered notes.

use crate::fx::{Ctx, Fx, Kind, Unit};

/// Most voices a side of the chorus can have.
pub const VOICES: usize = 9;

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Chorus {
    /// Voices per side, 1 to 9.
    pub order: u8,
    /// All of these are the knob's travel, 0 to 1 (cross -1 to 1).
    pub depth: f32,
    pub speed: f32,
    pub delay: f32,
    pub spread: f32,
    pub cross: f32,
    pub vol: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Echo {
    /// What each repeat keeps of the one before, -1 to 1.
    pub fb: f32,
    /// Sixteenth notes between repeats.
    pub steps: f32,
    /// Milliseconds the right side is late (the left one when negative).
    pub late_ms: f32,
    /// Level of the first repeat against what goes in; negative turns it over.
    pub vol: f32,
    /// 0 plain, 1 with the sides swapped, 2 bouncing from side to side.
    pub mode: u8,
    /// The time follows the tempo; otherwise a sixteenth is an eighth of a second.
    pub tempo: bool,
    /// Feeds the next echo instead of being heard.
    pub serial: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Room {
    pub low_hz: f32,
    /// Hundreds of hertz; 221 is off.
    pub high: f32,
    pub predelay: f32,
    pub size: f32,
    pub diffusion: f32,
    /// Tenths of a second.
    pub decay: f32,
    pub damping: f32,
    pub colour: u8,
    pub wet: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Set {
    pub chorus: Option<Chorus>,
    pub echoes: [Option<Echo>; 3],
    pub room: Option<Room>,
}

/// The knob law most of this synth's dials follow.
pub fn grid(x: f32) -> f32 {
    x.signum() * (11f32.powf(x.abs()) - 1.0) / 10.0
}

impl Set {
    pub fn any(&self) -> bool {
        self.chorus.is_some() || self.echoes.iter().any(|e| e.is_some()) || self.room.is_some()
    }

    /// Frames the section still sounds after its input stops.
    pub fn tail(&self, sr: f32, step: f32) -> usize {
        let mut n = 0.0;
        if let Some(c) = &self.chorus {
            n += (c.base() + c.swing()) * sr / 44100.0;
        }
        for e in self.echoes.iter().flatten() {
            let time = e.frames(sr, step);
            let fb = e.fb.abs().min(0.999);
            let repeats = if fb > 0.001 { (0.001f32.ln() / fb.ln()).ceil().min(200.0) } else { 0.0 };
            n += time * (repeats + 1.0);
        }
        if let Some(r) = &self.room {
            n += (r.decay * 0.1 * 1.5 + r.predelay / 384.0 + 0.5) * sr;
        }
        n as usize + 2048
    }
}

impl Chorus {
    /// Shortest delay of the deepest voice, in frames at 44.1 kHz.
    pub fn base(&self) -> f32 {
        4410.0 * grid(self.delay.clamp(0.0, 1.0))
    }

    /// How far the slow sweep moves the delay of the deepest voice, in the same frames.
    pub fn swing(&self) -> f32 {
        20.0 + 2185.0 * grid(self.depth.clamp(0.0, 1.0))
    }

    /// Sweeps per second of the deepest voice. The speed knob sets how fast the delay moves, so a deeper sweep
    /// takes longer.
    pub fn hz(&self) -> f32 {
        pace(self.speed) / (2.0 * self.swing())
    }
}

/// Frames the room's answer trails what it hears.
const ROOM_LATE: usize = 18;

/// Frames of delay (at 44.1 kHz) a voice travels per second for a place on the speed knob. The time a frame
/// takes falls a hundredfold over the knob (fitted to eighteen measured places, all within 0.02 %).
fn pace(speed: f32) -> f32 {
    88.2 / (15.1488 * 100.98f32.powf(-speed.clamp(0.0, 1.0)) - 0.05002)
}

/// A sweep made of two parabolas: 0 at the start of the cycle, 1 in the middle.
fn sweep(phase: f64) -> f64 {
    let u = phase.rem_euclid(1.0);
    let u = if u > 0.5 { 1.0 - u } else { u };
    if u < 0.25 {
        8.0 * u * u
    } else {
        1.0 - 8.0 * (0.5 - u) * (0.5 - u)
    }
}

/// One over the level of a voice when `n` voices land on a side (a curve through the measured levels of one
/// to nine voices; voices crossing over from the other side count for the share that crosses).
fn crowd(n: f32) -> f32 {
    0.580 + 0.2842 * n + 0.4017 * 2.4f32.powf(-n)
}

/// How much faster (and later) each voice runs at full spread than its share alone gives. Every order draws
/// from the same row: the left side takes the first `order` backwards (its deepest voice is always the first
/// entry), the right side the next `order` forwards. Two entries all but stand still. Measured on nine voices.
const PULL: [f32; 18] = [
    0.93509, 1.07952, 1.04246, 0.0102, 0.95454, 1.0671, 0.99399, 0.94284, 1.12571, 0.89681, 1.06295, 0.0102, 1.12995, 0.90822, 0.98205, 1.10044,
    0.96626, 1.10457,
];

fn pull(order: usize, side: usize, voice: usize) -> f32 {
    if side == 0 { PULL[order - 1 - voice] } else { PULL[order + voice] }
}

/// How far the spread knob pulls the voices apart: nothing for most of its travel, everything at the end.
fn apart(spread: f32) -> f32 {
    (1000f32.powf(spread.clamp(0.0, 1.0)) - 1.0) / 999.0
}

impl Echo {
    fn frames(&self, sr: f32, step: f32) -> f32 {
        if self.tempo {
            self.steps * step
        } else {
            self.steps * sr * 0.125
        }
    }
}

struct Line {
    buf: Vec<[f32; 2]>,
    at: usize,
}

impl Line {
    fn new() -> Self {
        Self { buf: Vec::new(), at: 0 }
    }

    fn fit(&mut self, n: usize) {
        if self.buf.len() != n {
            self.buf = vec![[0.0; 2]; n];
            self.at = 0;
        }
    }
}

pub struct State {
    sr: f32,
    cho: Line,
    phase: [[f64; VOICES]; 2],
    started: bool,
    /// The song frame the next block should start on if nothing was skipped.
    pub next: u64,
    /// How far the song had run in frames when this block began (set by the caller).
    pub frame: u64,
    /// The room answers a little late.
    late: std::collections::VecDeque<[f32; 2]>,
    echo: [Line; 3],
    room: Option<(Room, Box<Unit>)>,
    feed: Vec<[f32; 2]>,
    carry: Vec<[f32; 2]>,
    wet: Vec<[f32; 2]>,
    out: Vec<[f32; 2]>,
}

impl State {
    pub fn new(sr: f32) -> Self {
        Self { sr, cho: Line::new(), phase: [[0.0; VOICES]; 2], started: false, frame: 0, next: 0, late: std::collections::VecDeque::new(), echo: [Line::new(), Line::new(), Line::new()], room: None, feed: Vec::new(), carry: Vec::new(), wet: Vec::new(), out: Vec::new() }
    }

    /// Turn what was sent to the effects (`buf`) into what they put out. `step` is frames per sixteenth note.
    pub fn process(&mut self, set: &Set, buf: &mut [[f32; 2]], step: f32) {
        let n = buf.len();
        self.out.clear();
        self.out.resize(n, [0.0; 2]);
        self.feed.clear();
        self.feed.extend_from_slice(buf);
        if let Some(c) = &set.chorus {
            self.chorus(c, n);
        }
        self.echoes(set, n, step);
        if let Some(r) = &set.room {
            self.room(r, n, step);
        } else {
            self.room = None;
        }
        buf.copy_from_slice(&self.out);
    }

    /// The chorus reads `feed`, adds its voices to `out` and to `feed` (the echoes and the room hear it too).
    ///
    /// Every voice sweeps its own depth, the last one the whole depth. With no spread they all turn together
    /// from the same shortest delay, left and right half a sweep apart. Spread lets each keep the same pace
    /// instead, so shallow voices turn sooner and sit closer, and shifts where the sweeps start.
    fn chorus(&mut self, c: &Chorus, n: usize) {
        let sr = self.sr;
        let order = (c.order as usize).clamp(1, VOICES);
        let fr = sr / 44100.0;
        let (base, full, travel) = (c.base(), c.swing(), pace(c.speed));
        self.cho.fit(((0.16 * sr) as usize).next_power_of_two());
        let mask = self.cho.buf.len() - 1;
        let w = apart(c.spread);
        let cross = c.cross.clamp(-1.0, 1.0);
        let own = c.vol / crowd(order as f32 + cross.abs() * (order + 1) as f32 * 0.5);
        let over = cross * own;
        // per side and voice: lowest delay, sweep, share that crosses over; and cycles per frame
        let mut v = [[(0.0f32, 0.0f32, 0.0f32); VOICES]; 2];
        let mut rate = [[0.0f64; VOICES]; 2];
        for side in 0..2 {
            for k in 0..order {
                let part = (k + 1) as f32 / order as f32;
                let p = 1.0 + (pull(order, side, k) - 1.0) * w;
                let deep = 20.0 + (full - 20.0) * part;
                // no delay gets under eight frames
                let low = (base * (1.0 - w + w * part * p)).max(8.0);
                let moved = travel * ((1.0 - w) * deep / full + w * p);
                v[side][k] = (low * fr, deep * fr, over * part);
                rate[side][k] = moved as f64 / (2.0 * deep as f64) / sr as f64;
            }
        }
        // the sweeps turn from the moment the song starts, not from the first note (measured: a note five
        // seconds in finds every voice that much further round)
        if !self.started {
            self.started = true;
            for side in 0..2 {
                for k in 0..order {
                    let ahead = 0.5 * w as f64 * (k + 1) as f64 / order as f64 + 0.5 * side as f64;
                    self.phase[side][k] = (ahead + self.frame as f64 * rate[side][k]).rem_euclid(1.0);
                }
            }
        } else if self.frame != self.next {
            // the section rested while nothing sounded (or the song jumped): the sweeps went on turning
            let gap = self.frame as f64 - self.next as f64;
            for side in 0..2 {
                for k in 0..order {
                    self.phase[side][k] = (self.phase[side][k] + gap * rate[side][k]).rem_euclid(1.0);
                }
            }
        }
        self.next = self.frame + n as u64;
        for i in 0..n {
            let x = self.feed[i];
            let at = self.cho.at;
            self.cho.buf[at] = x;
            let mut o = [0.0f32; 2];
            for side in 0..2 {
                for (k, &(low, swing, over)) in v[side].iter().enumerate().take(order) {
                    let ph = &mut self.phase[side][k];
                    let d = low as f64 + swing as f64 * sweep(*ph);
                    *ph += rate[side][k];
                    if *ph >= 1.0 {
                        *ph -= 1.0;
                    }
                    let whole = d as usize;
                    let f = (d - whole as f64) as f32;
                    // a straight line between the two samples around the place read
                    let a = self.cho.buf[(at + mask + 1 - whole) & mask][side];
                    let b = self.cho.buf[(at + mask - whole) & mask][side];
                    let y = a + (b - a) * f;
                    o[side] += own * y;
                    o[1 - side] += over * y;
                }
            }
            self.cho.at = (at + 1) & mask;
            self.out[i][0] += o[0];
            self.out[i][1] += o[1];
            self.feed[i][0] += o[0];
            self.feed[i][1] += o[1];
        }
    }

    fn echoes(&mut self, set: &Set, n: usize, step: f32) {
        let sr = self.sr;
        self.carry.clear();
        self.carry.resize(n, [0.0; 2]);
        self.wet.clear();
        self.wet.resize(n, [0.0; 2]);
        for k in 0..3 {
            let Some(e) = &set.echoes[k] else {
                self.echo[k].buf = Vec::new();
                continue;
            };
            let len = ((sr * 12.0) as usize).next_power_of_two();
            let line = &mut self.echo[k];
            line.fit(len);
            let mask = len - 1;
            // the repeats come a sample early; the late side is held back after the loop, so it does not add up
            let time = ((e.frames(sr, step) - 1.0).round() as usize).clamp(1, len / 2);
            let late = ((e.late_ms.abs() * 0.001 * sr).round() as usize).min(len / 4);
            let hold = if e.late_ms < 0.0 { [late, 0] } else { [0, late] };
            let passes_on = e.serial && set.echoes[k + 1..].iter().any(|m| m.is_some());
            let fb = e.fb.clamp(-1.0, 1.0);
            for i in 0..n {
                let x = [self.feed[i][0] + self.carry[i][0], self.feed[i][1] + self.carry[i][1]];
                let at = line.at;
                let y = line.buf[(at + len - time) & mask];
                line.buf[at] = if e.mode == 2 { [x[0] + fb * y[1], x[1] + fb * y[0]] } else { [x[0] + fb * y[0], x[1] + fb * y[1]] };
                line.at = (at + 1) & mask;
                let y = [line.buf[(at + len - time - hold[0]) & mask][0], line.buf[(at + len - time - hold[1]) & mask][1]];
                let o = if e.mode == 0 { [y[0] * e.vol, y[1] * e.vol] } else { [y[1] * e.vol, y[0] * e.vol] };
                if passes_on {
                    self.carry[i] = o;
                } else {
                    self.carry[i] = [0.0; 2];
                    self.wet[i][0] += o[0];
                    self.wet[i][1] += o[1];
                }
            }
        }
        for i in 0..n {
            self.out[i][0] += self.wet[i][0];
            self.out[i][1] += self.wet[i][1];
        }
    }

    fn room(&mut self, r: &Room, n: usize, step: f32) {
        if self.room.as_ref().map(|(was, _)| was != r).unwrap_or(true) {
            let mut fx = Fx::new(Kind::Reverb);
            fx.p[0] = if r.low_hz <= 19.0 { 19.0 } else { r.low_hz.min(2500.0) };
            fx.p[1] = r.high.clamp(5.0, 221.0);
            fx.p[2] = r.predelay.clamp(0.0, 1000.0);
            fx.p[3] = r.size.clamp(1.0, 100.0);
            fx.p[4] = r.diffusion.clamp(0.0, 100.0);
            fx.p[5] = r.decay.clamp(1.0, 200.0);
            fx.p[6] = r.damping.clamp(5.0, 221.0);
            fx.p[7] = ROOM_BASS[(r.colour as usize).min(ROOM_BASS.len() - 1)];
            fx.p[8] = 500.0;
            fx.p[9] = 0.0;
            fx.p[10] = 0.0;
            fx.p[11] = 0.0;
            fx.p[12] = 128.0;
            fx.p[13] = 0.0;
            fx.p[14] = 0.0;
            match &mut self.room {
                // only the level moved: the room keeps ringing
                Some((was, _)) if Room { wet: r.wet, ..*was } == *r => was.wet = r.wet,
                _ => self.room = Some((*r, Box::new(Unit::new(fx, self.sr)))),
            }
        }
        let Some((_, unit)) = &mut self.room else { return };
        let cx = Ctx { sr: self.sr, step, clock: Default::default() };
        self.wet.clear();
        while self.late.len() < ROOM_LATE {
            self.late.push_back([0.0; 2]);
        }
        for i in 0..n {
            self.late.push_back(self.feed[i]);
            let x = self.late.pop_front().unwrap_or_default();
            self.wet.push(x);
        }
        for c in self.wet.chunks_mut(256) {
            unit.process(c, &cx);
        }
        for i in 0..n {
            self.out[i][0] += self.wet[i][0] * r.wet * ROOM_GAIN;
            self.out[i][1] += self.wet[i][1] * r.wet * ROOM_GAIN;
        }
    }
}

/// Bass setting of the room for each colour.
const ROOM_BASS: [f32; 6] = [20.0, 60.0, 100.0, 150.0, 300.0, 100.0];
/// Level of the room at full wet against the stand-alone room unit at its own full wet.
const ROOM_GAIN: f32 = 2.16;

#[cfg(test)]
mod tests {
    use super::*;

    fn click(n: usize) -> Vec<[f32; 2]> {
        let mut v = vec![[0.0; 2]; n];
        v[0] = [1.0, 0.0];
        v
    }

    #[test]
    fn nothing_passes_straight_through() {
        let mut s = State::new(44100.0);
        let mut b = click(4096);
        s.process(&Set::default(), &mut b, 5512.5);
        assert!(b.iter().all(|f| *f == [0.0; 2]));
    }

    #[test]
    fn an_echo_repeats_at_its_time_and_the_modes_pick_the_side() {
        let e = Echo { fb: 0.5, steps: 0.5, vol: 1.0, tempo: true, ..Default::default() };
        let run = |e: Echo| {
            let mut s = State::new(44100.0);
            let mut b = click(8192);
            s.process(&Set { echoes: [Some(e), None, None], ..Default::default() }, &mut b, 4000.0);
            b
        };
        let b = run(e);
        assert_eq!((b[0], b[1999], b[3998], b[5997]), ([0.0; 2], [1.0, 0.0], [0.5, 0.0], [0.25, 0.0]));
        let b = run(Echo { mode: 1, ..e });
        assert_eq!((b[1999], b[3998]), ([0.0, 1.0], [0.0, 0.5]));
        let b = run(Echo { mode: 2, fb: -0.5, ..e });
        assert_eq!((b[1999], b[3998], b[5997]), ([0.0, 1.0], [-0.5, 0.0], [0.0, 0.25]));
    }

    #[test]
    fn the_room_answers_late() {
        let room = Room { high: 221.0, size: 50.0, diffusion: 0.5, decay: 10.0, wet: 1.0, ..Default::default() };
        let mut s = State::new(44100.0);
        let mut io = click(8192);
        s.process(&Set { room: Some(room), ..Default::default() }, &mut io, 4000.0);
        assert!(io[..ROOM_LATE].iter().all(|f| *f == [0.0; 2]));
        assert!(io.iter().any(|f| f[0].abs() > 1e-4));
    }

    #[test]
    fn a_serial_echo_is_heard_only_through_the_next_one() {
        let a = Echo { fb: 0.0, steps: 0.25, vol: 0.5, tempo: true, serial: true, ..Default::default() };
        let b = Echo { fb: 0.0, steps: 0.5, vol: 1.0, tempo: true, ..Default::default() };
        let mut s = State::new(44100.0);
        let mut io = click(8192);
        s.process(&Set { echoes: [Some(a), Some(b), None], ..Default::default() }, &mut io, 4000.0);
        assert_eq!((io[999], io[1999], io[2998]), ([0.0; 2], [1.0, 0.0], [0.5, 0.0]));
        // with nothing after it the switch changes nothing
        let mut s = State::new(44100.0);
        let mut io = click(8192);
        s.process(&Set { echoes: [Some(a), None, None], ..Default::default() }, &mut io, 4000.0);
        assert_eq!(io[999], [0.5, 0.0]);
    }

    #[test]
    fn the_chorus_sweeps_between_its_delays() {
        let c = Chorus { order: 1, depth: 0.5, speed: 0.75, delay: 0.0625, spread: 0.0, cross: 0.0, vol: 1.0 };
        assert!((c.base() - 71.3).abs() < 0.1 && (c.swing() - 526.18).abs() < 0.05 && (1.0 / c.hz() - 5.075).abs() < 0.01, "{} {} {}", c.base(), c.swing(), 1.0 / c.hz());
        // measured places on the speed knob (frames a second)
        for (knob, want) in [(0.0, 5.8407), (0.25, 18.649), (0.5, 60.512), (0.75, 207.26), (1.0, 881.92)] {
            assert!((pace(knob) / want - 1.0).abs() < 0.001, "{knob} {}", pace(knob));
        }
        assert!(pull(1, 0, 0) == PULL[0] && pull(4, 0, 0) == PULL[3] && pull(4, 1, 0) == PULL[4] && pull(9, 1, 8) == PULL[17]);
        assert!((sweep(0.0), sweep(0.5), sweep(0.25)) == (0.0, 1.0, 0.5) && (sweep(0.75) - 0.5).abs() < 1e-6);
        let mut s = State::new(44100.0);
        let mut b: Vec<[f32; 2]> = (0..44100).map(|i| [(i as f32 * 0.05).sin(); 2]).collect();
        s.process(&Set { chorus: Some(c), ..Default::default() }, &mut b, 5512.5);
        let rms = (b[22050..].iter().map(|f| f[0] * f[0]).sum::<f32>() / 22050.0).sqrt();
        assert!((rms - 0.9694 * 0.7071).abs() < 0.02, "{rms}");
        // measured levels of a voice: alone, among four, among nine, and with everything crossing over
        for (n, want) in [(1.0, 0.9686), (2.0, 0.8222), (4.0, 0.5783), (6.5, 0.4123), (9.0, 0.3187)] {
            assert!((1.0 / crowd(n) - want).abs() < 0.002, "{n}");
        }
        // a hard left sound crossed all the way comes out the same on both sides, quieter than uncrossed
        let run = |cross: f32| {
            let mut s = State::new(44100.0);
            let mut b: Vec<[f32; 2]> = (0..22050).map(|i| [(i as f32 * 0.05).sin(), 0.0]).collect();
            s.process(&Set { chorus: Some(Chorus { cross, ..c }), ..Default::default() }, &mut b, 5512.5);
            let rms = |k: usize| (b[11025..].iter().map(|f| f[k] * f[k]).sum::<f32>() / 11025.0).sqrt();
            (rms(0), rms(1))
        };
        let (plain, full, back) = (run(0.0), run(1.0), run(-0.5));
        assert!(plain.1 == 0.0 && (full.0 - full.1).abs() < 1e-6 && (full.0 / plain.0 - 0.848).abs() < 0.01, "{plain:?} {full:?}");
        assert!((back.1 / back.0 - 0.5).abs() < 1e-3);
        // spread does nothing until late in its travel, and nothing at all turned the other way
        assert!(apart(-1.0) == 0.0 && apart(0.5) < 0.04 && (apart(0.875) - 0.421).abs() < 0.002 && apart(1.0) == 1.0);
    }
}
