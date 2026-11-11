//! Units that work on notes rather than on sound. They live in a patch bay, on the note cables between the
//! bay's note input and the instruments it holds: a key remapper, a keyboard splitter with sixteen zones, a
//! router by note colour and a scaler of the per-note levels.
//!
//! Their blocks, as far as they have been worked out from stored presets:
//!
//! * key remapper: version, a switch, two numbers we take for the labelling key and the offset, what to do
//!   with several targets (0 all, 1 all at the loudness of one, 2 one at random, 3 the next up, 4 the next
//!   down), a glide length, then for each of the twelve keys of an octave a count and that many 16-bit
//!   targets, in semitones from the octave's first key
//! * splitter: a four byte head (five when its third byte has bit 4 set), then sixteen zones of two drawn
//!   lines (how much of the zone each key gets, over 132 keys; what each velocity becomes), a transpose in
//!   semitones and three switches
//! * colour router: a zero, then for each of the sixteen colours the output it leaves by, then the colour
//!   it leaves as
//! * level scaler: a 29 byte head, then for velocity, release, fine pitch, pan and the two free controls six
//!   numbers each: centre, multiply (8192 is once), two we take for tension and offset, the random amount
//!   and how the random amount is applied

use crate::song::Note;

/// A note on its way through the shapers.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Ev {
    pub key: i32,
    /// 1.0 = the default velocity, as in a stored note.
    pub vel: f32,
    pub pan: f32,
    pub fine_cents: f32,
    pub release: f32,
    pub mod_x: f32,
    pub mod_y: f32,
    pub color: u8,
    /// Loudness on top of the velocity.
    pub gain: f32,
    /// The key the pitch glides in from, and over how many steps.
    pub glide: Option<(i32, f32)>,
}

impl Ev {
    pub fn of(n: &Note) -> Self {
        Self { key: n.key as i32, vel: n.vel, pan: n.pan, fine_cents: n.fine_cents, release: n.release, mod_x: n.mod_x, mod_y: n.mod_y, color: n.color, gain: 1.0, glide: None }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Zone {
    /// How much of the zone each of the 132 keys gets, 0..=1.
    pub keys: Vec<f32>,
    /// What a velocity becomes, or is multiplied by; 129 entries over the velocity range.
    pub vel: Vec<f32>,
    pub transpose: i32,
    pub absolute: bool,
    pub random: bool,
}

/// One per-note level's scaling: everything in parts of the level's whole range.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Level {
    pub centre: f32,
    pub multiply: f32,
    pub tension: f32,
    pub offset: f32,
    pub random: f32,
    /// 0 adds, 1 spreads both ways, 2 takes away.
    pub mode: u8,
}

impl Level {
    pub fn map(&self, x: f32, dice: f32) -> f32 {
        let d = x - self.centre;
        let span = if d >= 0.0 { 1.0 - self.centre } else { self.centre };
        let d = if span > 1e-6 && self.tension != 0.0 { d.signum() * span * crate::fm::bend((d.abs() / span).clamp(0.0, 1.0), self.tension) } else { d };
        let r = match self.mode {
            0 => dice,
            2 => -dice,
            _ => dice * 2.0 - 1.0,
        };
        (self.centre + d * self.multiply + self.offset + r * self.random).clamp(0.0, 1.0)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Shaper {
    Keys { on: bool, offset: i32, mode: u8, glide: f32, rows: [Vec<i16>; 12] },
    Zones(Vec<Zone>),
    Colours { port: [u8; 16], colour: [u8; 16] },
    Levels([Level; 6]),
}

/// A shaper on the way to an instrument, the output of it the cable leaves by, and which held unit it is.
#[derive(Debug, Clone, PartialEq)]
pub struct Link {
    pub shaper: Shaper,
    pub port: u32,
    pub node: usize,
}

fn int(b: &[u8], at: usize) -> Option<i32> {
    b.get(at..at + 4).map(|b| i32::from_le_bytes([b[0], b[1], b[2], b[3]]))
}

/// The stored velocity range against ours: a stored note's 100 of 128 is our 1.0.
const VEL: f32 = 100.0 / 128.0;
pub const KEYS: usize = 132;

impl Shaper {
    /// Our shaper for a held unit, when it is one of the note units we know.
    pub fn decode(ident: &str, b: &[u8]) -> Option<Self> {
        match ident.to_ascii_lowercase().as_str() {
            "vfx key mapper" => {
                let mut rows: [Vec<i16>; 12] = Default::default();
                let mut at = 24;
                for row in rows.iter_mut() {
                    let n = int(b, at)?;
                    if !(0..=64).contains(&n) {
                        return None;
                    }
                    at += 4;
                    for _ in 0..n {
                        row.push(i16::from_le_bytes(b.get(at..at + 2)?.try_into().ok()?));
                        at += 2;
                    }
                }
                Some(Shaper::Keys { on: int(b, 4)? != 0, offset: int(b, 12)?.clamp(-12, 12), mode: int(b, 16)?.clamp(0, 4) as u8, glide: int(b, 20)?.max(0) as f32 / 24.0, rows })
            }
            "vfx keyboard splitter" => {
                let mut at = if *b.get(2)? & 0x10 != 0 { 5 } else { 4 };
                let mut zones = Vec::with_capacity(16);
                for _ in 0..16 {
                    let mut keys = Vec::with_capacity(KEYS + 1);
                    at = crate::fx::drawn_table_n(b, at, KEYS + 1, &mut keys)?;
                    let mut vel = Vec::with_capacity(129);
                    at = crate::fx::drawn_table_n(b, at, 129, &mut vel)?;
                    let sw = b.get(at + 4..at + 7)?;
                    zones.push(Zone { keys, vel, transpose: int(b, at)?.clamp(-48, 48), absolute: sw[0] != 0, random: sw[1] != 0 });
                    at += 7;
                }
                Some(Shaper::Zones(zones))
            }
            "vfx color mapper" => {
                let (mut port, mut colour) = ([0u8; 16], [0u8; 16]);
                for k in 0..16 {
                    port[k] = int(b, 4 + 4 * k)?.clamp(0, 15) as u8;
                    colour[k] = int(b, 68 + 4 * k)?.clamp(0, 15) as u8;
                }
                Some(Shaper::Colours { port, colour })
            }
            "vfx level scaler" => {
                let mut levels = [Level { centre: 0.0, multiply: 1.0, tension: 0.0, offset: 0.0, random: 0.0, mode: 1 }; 6];
                for (k, l) in levels.iter_mut().enumerate() {
                    let f = |j: usize| int(b, 29 + 24 * k + 4 * j).map(|v| v as f32 / 8192.0);
                    *l = Level { centre: f(0)?.clamp(0.0, 1.0), multiply: f(1)?, tension: f(2)?.clamp(-1.0, 1.0), offset: f(3)?, random: f(4)?, mode: int(b, 29 + 24 * k + 20)?.clamp(0, 2) as u8 };
                }
                Some(Shaper::Levels(levels))
            }
            _ => None,
        }
    }
}

/// Whose turn it is where a remapper hands its targets out one at a time: (channel, held unit, key row) and
/// the count so far.
pub type Turns = Vec<((u16, usize, u8), usize)>;

fn roll(seed: &mut u32) -> f32 {
    *seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
    (*seed >> 8) as f32 / (1u32 << 24) as f32
}

/// Send a note down a chain of shapers. What comes out may be nothing, the note changed, or several notes.
pub fn apply(chain: &[Link], n: Ev, chan: u16, seed: &mut u32, turns: &mut Turns) -> Vec<Ev> {
    let mut now = vec![n];
    for link in chain {
        let mut next = Vec::with_capacity(now.len());
        for e in now {
            match &link.shaper {
                Shaper::Keys { on, offset, mode, glide, rows } => {
                    if !on {
                        next.push(e);
                        continue;
                    }
                    let k = e.key - offset;
                    let row = k.rem_euclid(12);
                    let base = k - row + offset;
                    let mut to: Vec<i32> = rows[row as usize].iter().map(|v| base + *v as i32).collect();
                    if to.is_empty() {
                        continue;
                    }
                    let glide = (*glide > 0.0).then_some((e.key, *glide));
                    let mut one = |key: i32, gain: f32| next.push(Ev { key, gain: e.gain * gain, glide: glide.filter(|g| g.0 != key).or(e.glide), ..e });
                    match mode {
                        0 => to.iter().for_each(|k| one(*k, 1.0)),
                        1 => to.iter().for_each(|k| one(*k, 1.0 / to.len() as f32)),
                        2 => one(to[((roll(seed) * to.len() as f32) as usize).min(to.len() - 1)], 1.0),
                        _ => {
                            to.sort();
                            let id = (chan, link.node, row as u8);
                            let at = match turns.iter().position(|t| t.0 == id) {
                                Some(i) => i,
                                None => {
                                    turns.push((id, 0));
                                    turns.len() - 1
                                }
                            };
                            let turn = turns[at].1 % to.len();
                            turns[at].1 = turn + 1;
                            one(if *mode == 3 { to[turn] } else { to[to.len() - 1 - turn] }, 1.0);
                        }
                    }
                }
                Shaper::Zones(zones) => {
                    let Some(z) = zones.get(link.port as usize) else { continue };
                    let share = z.keys.get(e.key.clamp(0, KEYS as i32 - 1) as usize).copied().unwrap_or(0.0);
                    if share <= 0.0005 {
                        continue;
                    }
                    let x = (e.vel * VEL).clamp(0.0, 1.0) * 128.0;
                    let i = (x as usize).min(127);
                    let got = z.vel[i] + (z.vel[i + 1] - z.vel[i]) * (x - i as f32);
                    if z.random {
                        if roll(seed) < share * got {
                            next.push(Ev { key: e.key + z.transpose, ..e });
                        }
                        continue;
                    }
                    let vel = if z.absolute { got / VEL } else { e.vel * got } * share;
                    if vel > 0.0005 {
                        next.push(Ev { key: e.key + z.transpose, vel, ..e });
                    }
                }
                Shaper::Colours { port, colour } => {
                    let c = (e.color & 15) as usize;
                    if port[c] as u32 == link.port {
                        next.push(Ev { color: colour[c], ..e });
                    }
                }
                Shaper::Levels(l) => {
                    let two = |v: f32, l: &Level, seed: &mut u32| l.map(v * 0.5 + 0.5, roll(seed)) * 2.0 - 1.0;
                    next.push(Ev {
                        vel: l[0].map((e.vel * VEL).clamp(0.0, 1.0), roll(seed)) / VEL,
                        release: l[1].map(e.release, roll(seed)),
                        fine_cents: two(e.fine_cents / 1200.0, &l[2], seed) * 1200.0,
                        pan: two(e.pan, &l[3], seed),
                        mod_x: two(e.mod_x, &l[4], seed),
                        mod_y: two(e.mod_y, &l[5], seed),
                        ..e
                    });
                }
            }
        }
        now = next;
    }
    now
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ev(key: i32) -> Ev {
        Ev { key, vel: 1.0, pan: 0.0, fine_cents: 0.0, release: 0.5, mod_x: 0.0, mod_y: 0.0, color: 0, gain: 1.0, glide: None }
    }

    fn keys_block(mode: i32, rows: [&[i16]; 12]) -> Vec<u8> {
        let mut b = vec![];
        for v in [2i32, 1, 0, 0, mode, 0] {
            b.extend_from_slice(&v.to_le_bytes());
        }
        for r in rows {
            b.extend_from_slice(&(r.len() as i32).to_le_bytes());
            for v in r {
                b.extend_from_slice(&v.to_le_bytes());
            }
        }
        b
    }

    #[test]
    fn a_key_remapper_moves_keys_makes_chords_and_takes_turns() {
        let triad: [&[i16]; 12] = [&[0, 4, 7], &[1], &[2], &[3], &[4], &[5], &[6], &[7], &[8], &[9], &[10], &[23]];
        let link = |mode| vec![Link { shaper: Shaper::decode("VFX Key Mapper", &keys_block(mode, triad)).unwrap(), port: 0, node: 3 }];
        let (mut seed, mut turns) = (1u32, Turns::new());
        let keys = |c: &[Link], k: i32, seed: &mut u32, turns: &mut Turns| apply(c, ev(k), 0, seed, turns).iter().map(|e| e.key).collect::<Vec<_>>();
        assert_eq!(keys(&link(0), 60, &mut seed, &mut turns), [60, 64, 67]);
        assert_eq!(keys(&link(0), 71, &mut seed, &mut turns), [83], "the last key of the octave goes up an octave");
        assert_eq!(keys(&link(0), 49, &mut seed, &mut turns), [49]);
        let quiet = apply(&link(1), ev(60), 0, &mut seed, &mut turns);
        assert!(quiet.len() == 3 && (quiet[0].gain - 1.0 / 3.0).abs() < 1e-6);
        let ups: Vec<i32> = (0..4).flat_map(|_| keys(&link(3), 48, &mut seed, &mut turns)).collect();
        assert_eq!(ups, [48, 52, 55, 48]);
        let mut turns = Turns::new();
        let downs: Vec<i32> = (0..3).flat_map(|_| keys(&link(4), 48, &mut seed, &mut turns)).collect();
        assert_eq!(downs, [55, 52, 48]);
        for _ in 0..20 {
            let one = keys(&link(2), 60, &mut seed, &mut turns);
            assert!(one.len() == 1 && [60, 64, 67].contains(&one[0]));
        }
    }

    #[test]
    fn colours_leave_by_their_own_output() {
        let mut b = vec![0u8; 4];
        for k in 0..16i32 {
            b.extend_from_slice(&(if k == 2 { 1 } else { k }).to_le_bytes());
        }
        for k in 0..16i32 {
            b.extend_from_slice(&(if k == 2 { 9 } else { k }).to_le_bytes());
        }
        let s = Shaper::decode("VFX Color Mapper", &b).unwrap();
        let at = |port| vec![Link { shaper: s.clone(), port, node: 0 }];
        let (mut seed, mut turns) = (1u32, Turns::new());
        let n = Ev { color: 2, ..ev(60) };
        assert!(apply(&at(2), n, 0, &mut seed, &mut turns).is_empty());
        assert_eq!(apply(&at(1), n, 0, &mut seed, &mut turns)[0].color, 9);
        assert_eq!(apply(&at(0), ev(60), 0, &mut seed, &mut turns).len(), 1);
    }

    #[test]
    fn a_level_scaler_left_alone_changes_nothing_and_scales_when_asked() {
        let mut b = vec![0u8; 29];
        for k in 0..6 {
            for v in [if k < 2 { 0i32 } else { 4096 }, 8192, 0, 0, 0, 1] {
                b.extend_from_slice(&v.to_le_bytes());
            }
        }
        let plain = vec![Link { shaper: Shaper::decode("VFX Level Scaler", &b).unwrap(), port: 0, node: 0 }];
        let (mut seed, mut turns) = (1u32, Turns::new());
        let n = Ev { vel: 0.8, pan: -0.25, mod_x: 0.5, fine_cents: 30.0, ..ev(60) };
        let out = apply(&plain, n, 0, &mut seed, &mut turns)[0];
        assert!((out.vel - 0.8).abs() < 1e-4 && (out.pan + 0.25).abs() < 1e-4 && (out.mod_x - 0.5).abs() < 1e-4 && (out.fine_cents - 30.0).abs() < 0.01, "{out:?}");
        b[33..37].copy_from_slice(&4096i32.to_le_bytes());
        let half = vec![Link { shaper: Shaper::decode("VFX Level Scaler", &b).unwrap(), port: 0, node: 0 }];
        assert!((apply(&half, n, 0, &mut seed, &mut turns)[0].vel - 0.4).abs() < 1e-4);
    }
    /// Every stored preset of the note units reads, and every bay preset that holds one still opens.
    #[test]
    fn stored_note_units_read() {
        let Ok(dir) = std::env::var("CEREAL_FLP_CORPUS") else { return };
        let (mut own, mut held, mut chains) = (0, 0, 0);
        let mut stack = vec![std::path::PathBuf::from(dir).join("Patches/Plugin presets")];
        while let Some(d) = stack.pop() {
            for e in std::fs::read_dir(&d).into_iter().flatten().flatten() {
                let p = e.path();
                if p.is_dir() {
                    stack.push(p);
                    continue;
                }
                let Ok(pre) = crate::preset::read(&p) else { continue };
                let id = pre.ident.to_ascii_lowercase();
                if ["vfx key mapper", "vfx keyboard splitter", "vfx color mapper", "vfx level scaler"].contains(&id.as_str()) {
                    assert!(Shaper::decode(&pre.ident, &pre.state).is_some(), "{}", p.display());
                    own += 1;
                } else if id == "patcher" {
                    if let Some(bay) = crate::fxpatch::decode(&pre.state) {
                        held += bay.nodes.iter().filter(|n| n.ident.to_ascii_lowercase().starts_with("vfx ")).count();
                    }
                    if let Some((bay, ..)) = crate::fxpatch::decode_gen(&pre.state) {
                        chains += bay.shape.len();
                    }
                }
            }
        }
        assert!(own > 40 && held > 100 && chains > 0, "{own} {held} {chains}");
    }

    #[test]
    fn a_split_keyboard_sends_each_half_its_own_way() {
        let Ok(dir) = std::env::var("CEREAL_FLP_CORPUS") else { return };
        let at = |name: &str| std::path::PathBuf::from(&dir).join("Patches/Plugin presets/Effects/VFX Keyboard Splitter").join(name);
        let pre = crate::preset::read(&at("Keyboard/3 zone split transposed.fst")).unwrap();
        let s = Shaper::decode(&pre.ident, &pre.state).unwrap();
        let (mut seed, mut turns) = (1u32, Turns::new());
        let mut out = |port, key| apply(&[Link { shaper: s.clone(), port, node: 0 }], ev(key), 0, &mut seed, &mut turns).iter().map(|e| e.key).collect::<Vec<_>>();
        assert_eq!(out(0, 40), [76]);
        assert!(out(1, 40).is_empty() && out(2, 40).is_empty());
        assert_eq!(out(1, 60), [60]);
        assert!(out(0, 60).is_empty());
        assert_eq!(out(2, 100), [64]);
        let pre = crate::preset::read(&at("Velocity/Max.fst")).unwrap();
        let s = Shaper::decode(&pre.ident, &pre.state).unwrap();
        let (mut seed, mut turns) = (1u32, Turns::new());
        let loud = apply(&[Link { shaper: s, port: 0, node: 0 }], Ev { vel: 0.3, ..ev(60) }, 0, &mut seed, &mut turns);
        assert!((loud[0].vel - 1.28).abs() < 0.01, "{:?}", loud[0]);
    }

}
