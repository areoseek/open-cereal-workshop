//! The piano roll's helper tools: each takes the notes it was opened on and its settings and gives back the
//! notes to show, so a tool's window can be dragged about and previewed without piling change on change.

use cereal_engine::song::Note;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Prop {
    Vel,
    Pan,
    Release,
    ModX,
    ModY,
    Fine,
}

pub const PROPS: [(Prop, &str); 6] = [(Prop::Vel, "Velocity"), (Prop::Pan, "Pan"), (Prop::Release, "Release"), (Prop::ModX, "Mod X"), (Prop::ModY, "Mod Y"), (Prop::Fine, "Fine pitch")];

/// A note property as 0..=1.
pub fn get(n: &Note, p: Prop) -> f32 {
    match p {
        Prop::Vel => n.vel / 1.28,
        Prop::Pan => n.pan * 0.5 + 0.5,
        Prop::Release => n.release,
        Prop::ModX => n.mod_x * 0.5 + 0.5,
        Prop::ModY => n.mod_y * 0.5 + 0.5,
        Prop::Fine => n.fine_cents / 2400.0 + 0.5,
    }
}

pub fn set(n: &mut Note, p: Prop, v: f32) {
    let v = v.clamp(0.0, 1.0);
    match p {
        Prop::Vel => n.vel = v * 1.28,
        Prop::Pan => n.pan = v * 2.0 - 1.0,
        Prop::Release => n.release = v,
        Prop::ModX => n.mod_x = v * 2.0 - 1.0,
        Prop::ModY => n.mod_y = v * 2.0 - 1.0,
        Prop::Fine => n.fine_cents = ((v - 0.5) * 2400.0).round(),
    }
}

pub const SCALES: [(&str, &[u8]); 39] = [
    ("None", &[]),
    ("Major (Ionian)", &[0, 2, 4, 5, 7, 9, 11]),
    ("Major Bebop", &[0, 2, 4, 5, 7, 8, 9, 11]),
    ("Major Bulgarian", &[0, 2, 5, 6, 8, 9, 11]),
    ("Major Hexatonic", &[0, 2, 4, 5, 7, 9]),
    ("Major Pentatonic", &[0, 2, 4, 7, 9]),
    ("Major Persian", &[0, 1, 4, 5, 6, 8, 11]),
    ("Major Polymode", &[0, 2, 4, 5, 7, 8, 9, 10, 11]),
    ("Minor Harmonic", &[0, 2, 3, 5, 7, 8, 11]),
    ("Minor Hungarian", &[0, 2, 3, 6, 7, 8, 11]),
    ("Minor Melodic", &[0, 2, 3, 5, 7, 9, 11]),
    ("Minor Natural (Aeolian)", &[0, 2, 3, 5, 7, 8, 10]),
    ("Minor Neapolitan", &[0, 1, 3, 5, 7, 8, 11]),
    ("Minor Pentatonic", &[0, 3, 5, 7, 10]),
    ("Minor Polymode", &[0, 1, 2, 3, 5, 7, 8, 10]),
    ("Minor Romanian", &[0, 2, 3, 6, 7, 9, 10]),
    ("Other Arabic", &[0, 2, 4, 5, 6, 8, 10]),
    ("Other Bebop Dominant", &[0, 2, 4, 5, 7, 9, 10, 11]),
    ("Other Blues", &[0, 3, 5, 6, 7, 10]),
    ("Other Blues Nonatonic", &[0, 2, 3, 4, 5, 6, 7, 9, 10]),
    ("Other Diminished", &[0, 2, 3, 5, 6, 8, 9, 11]),
    ("Other Dorian", &[0, 2, 3, 5, 7, 9, 10]),
    ("Other Eastern", &[0, 2, 3, 5, 7, 8, 10, 11]),
    ("Other Egyptian", &[0, 2, 5, 7, 10]),
    ("Other Enigmatic", &[0, 1, 4, 6, 8, 10, 11]),
    ("Other Hirajoshi", &[0, 2, 3, 7, 8]),
    ("Other Iwato", &[0, 1, 5, 6, 10]),
    ("Other Japanese Insen", &[0, 1, 5, 7, 10]),
    ("Other Locrian", &[0, 1, 3, 5, 6, 8, 10]),
    ("Other Locrian Super", &[0, 1, 3, 4, 6, 8, 10]),
    ("Other Lydian", &[0, 2, 4, 6, 7, 9, 11]),
    ("Other Mixolydian", &[0, 2, 4, 5, 7, 9, 10]),
    ("Other Neapolitan", &[0, 1, 3, 5, 7, 9, 11]),
    ("Other Phrygian", &[0, 1, 3, 5, 7, 8, 10]),
    ("Other Phrygian Dominant", &[0, 1, 4, 5, 7, 8, 10]),
    ("Other Piongio", &[0, 2, 5, 7, 9, 10]),
    ("Other Prometheus", &[0, 2, 4, 6, 9, 10]),
    ("Other Whole Tone", &[0, 2, 4, 6, 8, 10]),
    ("Chromatic", &[0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11]),
];

/// True when `key` belongs to the scale on `root` (any key does when there is no scale).
pub fn in_scale(key: u8, root: u8, scale: usize) -> bool {
    let s = SCALES[scale.min(SCALES.len() - 1)].1;
    s.is_empty() || s.contains(&((key as i32 - root as i32).rem_euclid(12) as u8))
}

/// The nearest key in the scale, looking down first.
pub fn to_scale(key: u8, root: u8, scale: usize) -> u8 {
    for d in 0..12i32 {
        for k in [key as i32 - d, key as i32 + d] {
            if (0..=131).contains(&k) && in_scale(k as u8, root, scale) {
                return k as u8;
            }
        }
    }
    key
}

#[derive(Clone, PartialEq, Debug)]
pub enum Helper {
    /// Pull starts and ends towards a grid.
    Quantise { start: f32, end: f32, grid: u32 },
    /// Cut notes into pieces.
    Chop { every: u32, gate: f32, fade: f32 },
    /// Play each chord one note at a time. `dir`: 0 up, 1 down, 2 up and down, 3 random.
    Arp { dir: u8, octaves: u8, every: u32, gate: f32, seed: u32 },
    /// Spread the starts of each chord's notes; negative strums from the top down.
    Strum { spread: i32, fade: f32 },
    /// Add grace notes before (negative) or echoes after each note.
    Flam { offset: i32, vel: f32, count: u8 },
    /// Fold notes into a key range and onto a scale.
    Limit { lo: u8, hi: u8, root: u8, scale: usize },
    Flip { vertical: bool, horizontal: bool },
    /// Scatter the note properties, and optionally scatter new notes over the range.
    Random { amount: [f32; 6], seed: u32, notes: u8, every: u32, len: u32, lo: u8, hi: u8, root: u8, scale: usize, bars: u8 },
    /// Offset, scale and bend one property.
    Levels { prop: Prop, offset: f32, mul: f32, tension: f32 },
    /// Sweep one property with a slow wave. `shape`: 0 sine, 1 triangle, 2 square, 3 saw.
    Lfo { prop: Prop, base: f32, depth: f32, period: u32, shape: u8, phase: f32 },
    /// Set note lengths from the gaps between notes.
    Articulate { mul: f32, gap: i32, vary: f32, seed: u32 },
    /// Throw away every so-many-th note.
    Claw { period: u8, skip: u8 },
    /// Write a riff from nothing.
    Riff { root: u8, scale: usize, bars: u8, every: u32, gate: f32, range: u8, density: f32, chord: u8, seed: u32 },
    /// Write a run of chords from the scale. `each` is one chord's length in ticks, `close` keeps every chord
    /// inside one octave, `bass` adds the chord's root an octave below.
    Chords { root: u8, scale: usize, count: u8, each: u32, notes: u8, octave: u8, close: bool, bass: bool, gate: f32, seed: u32 },
}

impl Helper {
    pub fn title(&self) -> &'static str {
        match self {
            Helper::Quantise { .. } => "Quantise",
            Helper::Chop { .. } => "Chop",
            Helper::Arp { .. } => "Arpeggiate",
            Helper::Strum { .. } => "Strum",
            Helper::Flam { .. } => "Flam",
            Helper::Limit { .. } => "Limit",
            Helper::Flip { .. } => "Flip",
            Helper::Random { .. } => "Randomise",
            Helper::Levels { .. } => "Scale levels",
            Helper::Lfo { .. } => "LFO",
            Helper::Articulate { .. } => "Articulate",
            Helper::Claw { .. } => "Claw machine",
            Helper::Riff { .. } => "Riff machine",
            Helper::Chords { .. } => "Chord progression",
        }
    }
}

struct Rng(u32);
impl Rng {
    fn new(seed: u32) -> Self {
        Self(seed.wrapping_mul(0x9e37_79b9) | 1)
    }
    fn next(&mut self) -> f32 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 17;
        self.0 ^= self.0 << 5;
        (self.0 >> 8) as f32 / (1u32 << 24) as f32
    }
    fn below(&mut self, n: usize) -> usize {
        ((self.next() * n as f32) as usize).min(n.saturating_sub(1))
    }
}

/// Group notes that start together (within a 32nd) into chords, low key first.
fn chords(mut notes: Vec<Note>, ppq: u32) -> Vec<Vec<Note>> {
    notes.sort_by_key(|n| (n.pos, n.key));
    let mut out: Vec<Vec<Note>> = vec![];
    for n in notes {
        match out.last_mut() {
            Some(c) if n.pos - c[0].pos <= ppq / 8 => c.push(n),
            _ => out.push(vec![n]),
        }
    }
    for c in &mut out {
        c.sort_by_key(|n| n.key);
    }
    out
}

fn len_of(n: &Note, ppq: u32) -> u32 {
    if n.len == 0 {
        ppq / 4
    } else {
        n.len
    }
}

/// Run a tool over the notes at `targets` in `base`; every other note comes back untouched.
pub fn apply(h: &Helper, base: &[Note], targets: &[usize], chan: u16, ppq: u32) -> Vec<Note> {
    let mut rest: Vec<Note> = base.iter().enumerate().filter(|(i, _)| !targets.contains(i)).map(|(_, n)| n.clone()).collect();
    let mut sel: Vec<Note> = targets.iter().filter_map(|&i| base.get(i).cloned()).collect();
    sel.sort_by_key(|n| (n.pos, n.key));
    let first = sel.iter().map(|n| n.pos).min().unwrap_or(0);
    let last = sel.iter().map(|n| n.pos + len_of(n, ppq)).max().unwrap_or(0);
    let made: Vec<Note> = match *h {
        Helper::Quantise { start, end, grid } => {
            let g = grid.max(1) as f32;
            let snap = |t: f32| (t / g).round() * g;
            sel.into_iter()
                .map(|mut n| {
                    let (a, b) = (n.pos as f32, (n.pos + len_of(&n, ppq)) as f32);
                    let a2 = a + (snap(a) - a) * start;
                    // with no pull on the end the note keeps its length
                    let b2 = if end > 0.0 { b + (snap(b) - b) * end } else { a2 + (b - a) };
                    n.pos = a2.round().max(0.0) as u32;
                    if n.len != 0 {
                        n.len = (b2 - a2).round().max(1.0) as u32;
                    }
                    n
                })
                .collect()
        }
        Helper::Chop { every, gate, fade } => {
            let every = every.max(1);
            let mut out = vec![];
            for n in sel {
                let total = len_of(&n, ppq);
                let pieces = total.div_ceil(every).max(1);
                for k in 0..pieces {
                    let at = k * every;
                    let room = (total - at).min(every);
                    let mut m = n.clone();
                    m.pos = n.pos + at;
                    m.len = ((room as f32 * gate).round() as u32).max(1);
                    m.vel = n.vel * (1.0 - fade * k as f32 / pieces as f32).max(0.0);
                    out.push(m);
                }
            }
            out
        }
        Helper::Arp { dir, octaves, every, gate, seed } => {
            let every = every.max(1);
            let mut rng = Rng::new(seed);
            let mut out = vec![];
            for c in chords(sel, ppq) {
                let start = c[0].pos;
                let end = c.iter().map(|n| n.pos + len_of(n, ppq)).max().unwrap_or(start);
                let mut seq: Vec<(usize, i32)> = vec![];
                for o in 0..octaves.max(1) as i32 {
                    for i in 0..c.len() {
                        seq.push((i, o * 12));
                    }
                }
                match dir {
                    1 => seq.reverse(),
                    2 if seq.len() > 2 => {
                        let back: Vec<(usize, i32)> = seq[1..seq.len() - 1].iter().rev().cloned().collect();
                        seq.extend(back);
                    }
                    _ => {}
                }
                let count = (end - start).div_ceil(every).max(1);
                for k in 0..count {
                    let (i, up) = if dir == 3 { seq[rng.below(seq.len())] } else { seq[k as usize % seq.len()] };
                    let mut m = c[i].clone();
                    m.pos = start + k * every;
                    m.len = ((every.min(end - m.pos) as f32 * gate).round() as u32).max(1);
                    m.key = (m.key as i32 + up).clamp(0, 131) as u8;
                    out.push(m);
                }
            }
            out
        }
        Helper::Strum { spread, fade } => {
            let mut out = vec![];
            for mut c in chords(sel, ppq) {
                if spread < 0 {
                    c.reverse();
                }
                let count = c.len().max(1) as f32;
                for (k, mut n) in c.into_iter().enumerate() {
                    let push = (spread.unsigned_abs() * k as u32).min(len_of(&n, ppq).saturating_sub(1));
                    n.pos += push;
                    if n.len != 0 {
                        n.len -= push;
                    }
                    n.vel *= (1.0 - fade * k as f32 / count).max(0.0);
                    out.push(n);
                }
            }
            out
        }
        Helper::Flam { offset, vel, count } => {
            let mut out = vec![];
            for n in sel {
                for k in 1..=count.max(1) as i32 {
                    let at = n.pos as i64 + offset as i64 * k as i64;
                    if at < 0 {
                        continue;
                    }
                    let mut m = n.clone();
                    m.pos = at as u32;
                    m.len = if n.len == 0 { 0 } else { n.len.min(offset.unsigned_abs().max(1)) };
                    m.vel = n.vel * vel.powi(k);
                    out.push(m);
                }
                out.push(n);
            }
            out
        }
        Helper::Limit { lo, hi, root, scale } => {
            let (lo, hi) = (lo.min(hi) as i32, lo.max(hi) as i32);
            sel.into_iter()
                .map(|mut n| {
                    let mut k = n.key as i32;
                    while k > hi && k - 12 >= 0 {
                        k -= 12;
                    }
                    while k < lo && k + 12 <= 131 {
                        k += 12;
                    }
                    n.key = to_scale(k.clamp(0, 131) as u8, root, scale);
                    n
                })
                .collect()
        }
        Helper::Flip { vertical, horizontal } => {
            let (kl, kh) = (sel.iter().map(|n| n.key).min().unwrap_or(60) as i32, sel.iter().map(|n| n.key).max().unwrap_or(60) as i32);
            sel.into_iter()
                .map(|mut n| {
                    if vertical {
                        n.key = (kl + kh - n.key as i32).clamp(0, 131) as u8;
                    }
                    if horizontal {
                        n.pos = first + (last - (n.pos + len_of(&n, ppq)));
                    }
                    n
                })
                .collect()
        }
        Helper::Random { amount, seed, notes, every, len, lo, hi, root, scale, bars } => {
            let mut rng = Rng::new(seed);
            let mut out = sel;
            if notes > 0 {
                // new notes on a grid over the chosen number of bars
                let every = every.max(1);
                let slots = (bars.max(1) as u32 * ppq * 4 / every).max(1);
                let (lo, hi) = (lo.min(hi), lo.max(hi));
                out.clear();
                for s in 0..slots {
                    if rng.next() * 100.0 < notes as f32 {
                        let key = to_scale(lo + rng.below((hi - lo) as usize + 1) as u8, root, scale);
                        out.push(Note { pos: first + s * every, len: len.max(1), chan, key, ..Default::default() });
                    }
                }
            }
            for n in &mut out {
                for (i, (p, _)) in PROPS.iter().enumerate() {
                    if amount[i] != 0.0 {
                        let v = get(n, *p) + (rng.next() * 2.0 - 1.0) * amount[i] * 0.5;
                        set(n, *p, v);
                    }
                }
            }
            out
        }
        Helper::Levels { prop, offset, mul, tension } => sel
            .into_iter()
            .map(|mut n| {
                let mut v = get(&n, prop).clamp(0.0, 1.0);
                // tension bends the curve: above zero lifts the quiet values, below pushes them down
                if tension != 0.0 {
                    v = v.powf(2f32.powf(-tension * 2.0));
                }
                set(&mut n, prop, (v - 0.5) * mul + 0.5 + offset * 0.5);
                n
            })
            .collect(),
        Helper::Lfo { prop, base: mid, depth, period, shape, phase } => sel
            .into_iter()
            .map(|mut n| {
                let x = ((n.pos - first) as f32 / period.max(1) as f32 + phase).rem_euclid(1.0);
                let w = match shape {
                    1 => 1.0 - 4.0 * (x - 0.5).abs(),
                    2 => {
                        if x < 0.5 {
                            1.0
                        } else {
                            -1.0
                        }
                    }
                    3 => 1.0 - 2.0 * x,
                    _ => (x * std::f32::consts::TAU).sin(),
                };
                set(&mut n, prop, mid + w * depth * 0.5);
                n
            })
            .collect(),
        Helper::Articulate { mul, gap, vary, seed } => {
            let mut rng = Rng::new(seed);
            let cs = chords(sel, ppq);
            let starts: Vec<u32> = cs.iter().map(|c| c[0].pos).collect();
            let mut out = vec![];
            for (k, c) in cs.into_iter().enumerate() {
                for mut n in c {
                    // up to the next chord when there is one, else the note's own length
                    let room = starts.get(k + 1).map(|&s| s - n.pos).unwrap_or(len_of(&n, ppq)) as f32;
                    let v = 1.0 + (rng.next() * 2.0 - 1.0) * vary;
                    n.len = ((room * mul * v) as i64 - gap as i64).max(1) as u32;
                    out.push(n);
                }
            }
            out
        }
        Helper::Claw { period, skip } => {
            let period = period.max(2) as usize;
            sel.into_iter().enumerate().filter(|(i, _)| (i + skip as usize) % period != period - 1).map(|(_, n)| n).collect()
        }
        Helper::Riff { root, scale, bars, every, gate, range, density, chord, seed } => {
            let mut rng = Rng::new(seed);
            let every = every.max(1);
            let tones: Vec<u8> = (0..=131u8).filter(|&k| in_scale(k, root, scale) && k >= 48 + root % 12 && k <= 48 + root % 12 + 12 * range.max(1)).collect();
            let mut out = vec![];
            if !tones.is_empty() {
                let slots = bars.max(1) as u32 * ppq * 4 / every;
                let mut at = rng.below(tones.len().min(4));
                for s in 0..slots {
                    // a strong beat always plays, the rest by chance
                    let strong = (s * every) % (ppq * 4) == 0;
                    if !strong && rng.next() > density {
                        continue;
                    }
                    // walk a few scale steps at a time, turning back at the ends
                    let jump = rng.below(5) as i32 - 2;
                    at = (at as i32 + jump).clamp(0, tones.len() as i32 - 1) as usize;
                    if strong {
                        at -= at % 2;
                    }
                    let vel = if strong { 1.0 } else { 0.7 + rng.next() * 0.25 };
                    for c in 0..chord.clamp(1, 4) as usize {
                        if let Some(&key) = tones.get(at + c * 2) {
                            out.push(Note { pos: first + s * every, len: ((every as f32 * gate) as u32).max(1), chan, key, vel, ..Default::default() });
                        }
                    }
                }
            }
            out
        }
        Helper::Chords { root, scale, count, each, notes, octave, close, bass, gate, seed } => {
            let mut rng = Rng::new(seed);
            let low = (12 * octave.min(9) + root % 12).min(119);
            let tones: Vec<u8> = (low..=131u8).filter(|&k| in_scale(k, root, scale)).collect();
            let per = tones.iter().take_while(|&&k| k < low + 12).count().max(1);
            // where each degree likes to go next: the home chord anywhere, the others towards home
            const NEXT: [&[usize]; 7] = [&[3, 4, 5, 1, 2, 6], &[4, 4, 6, 3], &[5, 3, 5, 1], &[4, 0, 1, 4, 5], &[0, 0, 5, 3], &[3, 1, 4, 3, 2], &[0, 2, 0]];
            let mut out = vec![];
            let mut deg = 0usize;
            let count = count.max(1) as u32;
            for c in 0..count {
                if c > 0 {
                    let to = NEXT[deg % 7];
                    deg = to[rng.below(to.len())] % per;
                    // the last chord leans on the fifth or fourth so the loop turns round
                    if c + 1 == count && count > 2 && per == 7 && rng.next() < 0.6 {
                        deg = if rng.next() < 0.65 { 4 } else { 3 };
                    }
                }
                let len = ((each.max(1) as f32 * gate) as u32).max(1);
                let mut keys: Vec<u8> = (0..notes.clamp(2, 5) as usize).filter_map(|k| tones.get(deg + k * 2).copied()).collect();
                if close {
                    for k in keys.iter_mut() {
                        while *k >= low + 12 {
                            *k -= 12;
                        }
                    }
                }
                if bass {
                    if let Some(b) = tones.get(deg).and_then(|k| k.checked_sub(12)) {
                        keys.push(b);
                    }
                }
                keys.sort_unstable();
                keys.dedup();
                for key in keys {
                    out.push(Note { pos: first + c * each.max(1), len, chan, key, vel: 0.78, ..Default::default() });
                }
            }
            out
        }
    };
    rest.extend(made);
    rest.sort_by_key(|n| (n.pos, n.chan, n.key));
    rest
}

#[cfg(test)]
mod tests {
    use super::*;

    fn chord() -> Vec<Note> {
        [60u8, 64, 67].iter().map(|&key| Note { pos: 0, len: 96, key, ..Default::default() }).collect()
    }

    #[test]
    fn a_chord_run_starts_at_home_and_stays_in_the_scale() {
        let h = Helper::Chords { root: 2, scale: 1, count: 4, each: 384, notes: 3, octave: 4, close: true, bass: true, gate: 1.0, seed: 3 };
        let made = apply(&h, &[], &[], 1, 96);
        assert_eq!(made.len(), 16, "four chords of three notes and a bass note");
        assert!(made.iter().all(|n| in_scale(n.key, 2, 1) && n.chan == 1 && n.len == 384));
        let first: Vec<u8> = made.iter().filter(|n| n.pos == 0).map(|n| n.key).collect();
        assert_eq!(first, vec![38, 50, 54, 57], "D major on its root, with the D below");
        assert_eq!(made.iter().map(|n| n.pos).max(), Some(3 * 384));
        // close voicing never leaves the octave above the root
        assert!(made.iter().filter(|n| n.key >= 50).all(|n| n.key < 62));
    }

    #[test]
    fn an_arpeggio_walks_the_chord_and_fills_its_length() {
        let out = apply(&Helper::Arp { dir: 0, octaves: 1, every: 24, gate: 1.0, seed: 1 }, &chord(), &[0, 1, 2], 0, 96);
        assert_eq!(out.iter().map(|n| (n.pos, n.key, n.len)).collect::<Vec<_>>(), vec![(0, 60, 24), (24, 64, 24), (48, 67, 24), (72, 60, 24)]);
        let down = apply(&Helper::Arp { dir: 1, octaves: 2, every: 24, gate: 0.5, seed: 1 }, &chord(), &[0, 1, 2], 0, 96);
        assert_eq!(down.iter().map(|n| (n.key, n.len)).collect::<Vec<_>>(), vec![(79, 12), (76, 12), (72, 12), (67, 12)]);
    }

    #[test]
    fn strum_flip_limit_and_claw_do_what_they_say() {
        let s = apply(&Helper::Strum { spread: 6, fade: 0.0 }, &chord(), &[0, 1, 2], 0, 96);
        assert_eq!(s.iter().map(|n| (n.pos, n.key, n.len)).collect::<Vec<_>>(), vec![(0, 60, 96), (6, 64, 90), (12, 67, 84)]);
        let f = apply(&Helper::Flip { vertical: true, horizontal: false }, &chord(), &[0, 1, 2], 0, 96);
        assert_eq!(f.iter().map(|n| n.key).collect::<Vec<_>>(), vec![60, 63, 67]);
        let l = apply(&Helper::Limit { lo: 48, hi: 59, root: 0, scale: 1 }, &chord(), &[0, 1, 2], 0, 96);
        assert_eq!(l.iter().map(|n| n.key).collect::<Vec<_>>(), vec![48, 52, 55]);
        let c = apply(&Helper::Claw { period: 3, skip: 0 }, &chord(), &[0, 1, 2], 0, 96);
        assert_eq!(c.len(), 2);
    }

    #[test]
    fn untouched_notes_stay_and_quantise_pulls_part_way() {
        let base = vec![Note { pos: 10, len: 20, ..Default::default() }, Note { pos: 50, len: 20, key: 62, ..Default::default() }];
        let out = apply(&Helper::Quantise { start: 0.5, end: 0.0, grid: 24 }, &base, &[0], 0, 96);
        assert_eq!((out[0].pos, out[0].len), (5, 20));
        assert_eq!(out[1], base[1]);
        let riff = apply(&Helper::Riff { root: 0, scale: 1, bars: 2, every: 24, gate: 0.9, range: 2, density: 0.6, chord: 1, seed: 7 }, &[], &[], 3, 96);
        assert!(riff.len() >= 2 && riff.iter().all(|n| n.chan == 3 && in_scale(n.key, 0, 1)));
    }
}
