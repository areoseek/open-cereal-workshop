//! General MIDI sounds for the bank player: the effect unit that sounds the notes MIDI senders put on its
//! port. The sounds come from a bank file found on the machine (the public DLS and SoundFont 2 layouts); with
//! none, voices of our own stand in, picked by the standard's instrument names, with a drum set worked out
//! from simple models.

use crate::sample::Sample;
use crate::song::Env;
use crate::zones::{Map, Zone};
use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};

/// The bank player's file identifier.
pub const IDENT: &str = "Fruity LSD";

/// The flag on a channel's patch that makes it the drum set.
pub const DRUMS: u32 = 0x8000_0000;

/// What the bank player's block says: the port it listens on and the patch of each of the 16 MIDI channels.
#[derive(Debug, Clone, PartialEq)]
pub struct Set {
    pub port: u32,
    pub patch: [u32; 16],
}

impl Set {
    /// The block is a version word, a flag word, 25 bytes of other settings and one word per MIDI channel.
    /// (Every stored example listens on port 0 with these bytes at zero; that the port is the third word is
    /// our reading.)
    pub fn read(b: &[u8]) -> Option<Set> {
        if b.len() < 33 + 64 {
            return None;
        }
        let w = |o: usize| u32::from_le_bytes([b[o], b[o + 1], b[o + 2], b[o + 3]]);
        let mut patch = [0u32; 16];
        for (k, p) in patch.iter_mut().enumerate() {
            *p = w(33 + 4 * k);
        }
        Some(Set { port: w(8).min(255), patch })
    }
}

/// The standard's 128 instrument names.
pub const NAMES: [&str; 128] = [
    "Acoustic Grand Piano", "Bright Acoustic Piano", "Electric Grand Piano", "Honky-tonk Piano", "Electric Piano 1", "Electric Piano 2", "Harpsichord", "Clavinet",
    "Celesta", "Glockenspiel", "Music Box", "Vibraphone", "Marimba", "Xylophone", "Tubular Bells", "Dulcimer",
    "Drawbar Organ", "Percussive Organ", "Rock Organ", "Church Organ", "Reed Organ", "Accordion", "Harmonica", "Tango Accordion",
    "Acoustic Guitar (nylon)", "Acoustic Guitar (steel)", "Electric Guitar (jazz)", "Electric Guitar (clean)", "Electric Guitar (muted)", "Overdriven Guitar", "Distortion Guitar", "Guitar Harmonics",
    "Acoustic Bass", "Electric Bass (finger)", "Electric Bass (pick)", "Fretless Bass", "Slap Bass 1", "Slap Bass 2", "Synth Bass 1", "Synth Bass 2",
    "Violin", "Viola", "Cello", "Contrabass", "Tremolo Strings", "Pizzicato Strings", "Orchestral Harp", "Timpani",
    "String Ensemble 1", "String Ensemble 2", "Synth Strings 1", "Synth Strings 2", "Choir Aahs", "Voice Oohs", "Synth Voice", "Orchestra Hit",
    "Trumpet", "Trombone", "Tuba", "Muted Trumpet", "French Horn", "Brass Section", "Synth Brass 1", "Synth Brass 2",
    "Soprano Sax", "Alto Sax", "Tenor Sax", "Baritone Sax", "Oboe", "English Horn", "Bassoon", "Clarinet",
    "Piccolo", "Flute", "Recorder", "Pan Flute", "Blown Bottle", "Shakuhachi", "Whistle", "Ocarina",
    "Lead 1 (square)", "Lead 2 (sawtooth)", "Lead 3 (calliope)", "Lead 4 (chiff)", "Lead 5 (charang)", "Lead 6 (voice)", "Lead 7 (fifths)", "Lead 8 (bass + lead)",
    "Pad 1 (new age)", "Pad 2 (warm)", "Pad 3 (polysynth)", "Pad 4 (choir)", "Pad 5 (bowed)", "Pad 6 (metallic)", "Pad 7 (halo)", "Pad 8 (sweep)",
    "FX 1 (rain)", "FX 2 (soundtrack)", "FX 3 (crystal)", "FX 4 (atmosphere)", "FX 5 (brightness)", "FX 6 (goblins)", "FX 7 (echoes)", "FX 8 (sci-fi)",
    "Sitar", "Banjo", "Shamisen", "Koto", "Kalimba", "Bag pipe", "Fiddle", "Shanai",
    "Tinkle Bell", "Agogo", "Steel Drums", "Woodblock", "Taiko Drum", "Melodic Tom", "Synth Drum", "Reverse Cymbal",
    "Guitar Fret Noise", "Breath Noise", "Seashore", "Bird Tweet", "Telephone Ring", "Helicopter", "Applause", "Gunshot",
];

/// What one MIDI channel of the bank player sounds like.
#[derive(Debug, Clone)]
pub enum Voice {
    /// Recordings spread over the keys: from a bank file, or our worked-out drum set.
    Map(Arc<Map>),
    /// A stand-in voice of our own.
    Add(Arc<crate::additive::Patch>),
}

/// The patch word a MIDI channel plays from a bank player's settings (port, 16 patches, 16 drum switches);
/// a patch the sender picks itself goes over the player's.
pub fn patch_of(p: &[f32], chan: u8, own: i32) -> u32 {
    let c = chan as usize & 15;
    let drums = if p.get(17 + c).is_some_and(|d| *d >= 0.5) { DRUMS } else { 0 };
    let num = if own >= 0 { own as u32 } else { p.get(1 + c).map_or(0, |v| v.round().clamp(0.0, 127.0) as u32) };
    (num & 127) | drums
}

/// The block's settings as the unit holds them: port, 16 patches, 16 drum switches.
pub fn decode(b: &[u8]) -> Option<Vec<f32>> {
    let s = Set::read(b)?;
    let mut p = vec![s.port as f32];
    p.extend(s.patch.iter().map(|w| (w & 127) as f32));
    p.extend(s.patch.iter().map(|w| if w & DRUMS != 0 { 1.0 } else { 0.0 }));
    Some(p)
}

/// The settings written into the block they came from, or into a fresh one.
pub fn encode(p: &[f32], base: Option<&[u8]>) -> Vec<u8> {
    let mut out = match base {
        Some(b) if b.len() >= 97 => b.to_vec(),
        _ => {
            let mut v = vec![0u8; 97];
            v[4] = 0x80;
            v
        }
    };
    out[8..12].copy_from_slice(&(p.first().map_or(0.0, |v| v.round().clamp(0.0, 255.0)) as u32).to_le_bytes());
    for c in 0..16u8 {
        let o = 33 + 4 * c as usize;
        let was = u32::from_le_bytes(out[o..o + 4].try_into().unwrap());
        // bits we do not read stay as they came
        let now = (was & !(DRUMS | 127)) | patch_of(p, c, -1);
        out[o..o + 4].copy_from_slice(&now.to_le_bytes());
    }
    out
}

// ---- the public DLS layout ------------------------------------------------------------------------------

/// The pieces of a RIFF body: four-letter name and contents, each padded to an even length.
fn pieces(b: &[u8]) -> Vec<(usize, [u8; 4], &[u8])> {
    let mut out = vec![];
    let mut at = 0;
    while at + 8 <= b.len() {
        let id = [b[at], b[at + 1], b[at + 2], b[at + 3]];
        let len = u32::from_le_bytes([b[at + 4], b[at + 5], b[at + 6], b[at + 7]]) as usize;
        let end = (at + 8 + len).min(b.len());
        out.push((at, id, &b[at + 8..end]));
        at = end + (len & 1);
    }
    out
}

/// A LIST piece of the given kind: its contents after the kind's name.
fn list<'a>(p: &(usize, [u8; 4], &'a [u8]), kind: &[u8; 4]) -> Option<&'a [u8]> {
    (&p.1 == b"LIST" && p.2.len() >= 4 && &p.2[..4] == kind).then(|| &p.2[4..])
}

fn u16at(b: &[u8], o: usize) -> u16 {
    b.get(o..o + 2).map_or(0, |x| u16::from_le_bytes([x[0], x[1]]))
}

fn u32at(b: &[u8], o: usize) -> u32 {
    b.get(o..o + 4).map_or(0, |x| u32::from_le_bytes([x[0], x[1], x[2], x[3]]))
}

#[derive(Debug, Clone, Copy, Default)]
struct Wsmp {
    unity: u16,
    fine: i16,
    gain: i32,
    looped: Option<(u32, u32)>,
}

impl Wsmp {
    fn read(b: &[u8]) -> Option<Wsmp> {
        if b.len() < 20 {
            return None;
        }
        let size = u32at(b, 0) as usize;
        let loops = u32at(b, 16);
        let l = size.max(20);
        let looped = (loops > 0 && b.len() >= l + 16).then(|| (u32at(b, l + 8), u32at(b, l + 8) + u32at(b, l + 12)));
        Some(Wsmp { unity: u16at(b, 4), fine: u16at(b, 6) as i16, gain: u32at(b, 8) as i32, looped })
    }
}

/// The level envelope and pan a region or an instrument asks for; times in seconds.
#[derive(Debug, Clone, Copy, Default)]
struct Art {
    attack: Option<f32>,
    decay: Option<f32>,
    sustain: Option<f32>,
    release: Option<f32>,
    pan: Option<f32>,
}

impl Art {
    /// A connection list: blocks of source, control, destination, transform and a 32-bit scale.
    fn read(&mut self, b: &[u8]) {
        let size = (u32at(b, 0) as usize).max(8);
        let n = u32at(b, 4) as usize;
        for k in 0..n {
            let o = size + k * 12;
            if o + 12 > b.len() {
                break;
            }
            let (src, ctl, dst, scale) = (u16at(b, o), u16at(b, o + 2), u16at(b, o + 4), u32at(b, o + 8) as i32);
            if src != 0 || ctl != 0 {
                continue;
            }
            // times are 1200 * log2(seconds), in 65536ths
            let secs = || if scale == i32::MIN { 0.0 } else { 2f32.powf(scale as f32 / 65536.0 / 1200.0).min(40.0) };
            match dst {
                0x206 => self.attack = Some(secs()),
                0x207 => self.decay = Some(secs()),
                0x209 => self.release = Some(secs()),
                0x20a => self.sustain = Some((scale as f32 / 65536.0 / 1000.0).clamp(0.0, 1.0)),
                0x004 => self.pan = Some((scale as f32 / 65536.0 / 500.0).clamp(-1.0, 1.0)),
                _ => {}
            }
        }
    }

    fn over(self, base: Art) -> Art {
        Art { attack: self.attack.or(base.attack), decay: self.decay.or(base.decay), sustain: self.sustain.or(base.sustain), release: self.release.or(base.release), pan: self.pan.or(base.pan) }
    }
}

fn arts(body: &[u8]) -> Option<Art> {
    let mut a = Art::default();
    let mut any = false;
    for p in pieces(body) {
        if let Some(l) = list(&p, b"lart").or_else(|| list(&p, b"lar2")) {
            for q in pieces(l) {
                if &q.1 == b"art1" || &q.1 == b"art2" {
                    a.read(q.2);
                    any = true;
                }
            }
        }
    }
    any.then_some(a)
}

#[derive(Debug, Clone)]
struct Rgn {
    key: (u16, u16),
    vel: (u16, u16),
    group: u16,
    wsmp: Option<Wsmp>,
    wave: u32,
    art: Option<Art>,
}

#[derive(Debug, Clone)]
struct Ins {
    drum: bool,
    bank: u32,
    patch: u32,
    regions: Vec<Rgn>,
    art: Art,
}

#[derive(Debug, Clone)]
struct Wave {
    rate: u32,
    chans: u16,
    bits: u16,
    /// Where the recording lies in the file.
    data: (usize, usize),
    wsmp: Option<Wsmp>,
}

/// A downloadable sounds bank.
pub struct Dls {
    data: Vec<u8>,
    ins: Vec<Ins>,
    /// The pool table: each entry is a wave, or nothing when its offset names none.
    table: Vec<Option<Wave>>,
}

impl Dls {
    pub fn parse(data: Vec<u8>) -> Option<Dls> {
        if data.len() < 12 || &data[..4] != b"RIFF" || &data[8..12] != b"DLS " {
            return None;
        }
        let base = data.as_ptr() as usize;
        let body = &data[12..];
        let mut ins = vec![];
        let mut cues: Vec<u32> = vec![];
        let mut waves: HashMap<u32, Wave> = HashMap::new();
        for p in pieces(body) {
            if &p.1 == b"ptbl" {
                let (size, n) = (u32at(p.2, 0) as usize, u32at(p.2, 4) as usize);
                cues = (0..n.min(p.2.len() / 4)).map(|k| u32at(p.2, size.max(8) + 4 * k)).collect();
            } else if let Some(l) = list(&p, b"lins") {
                for q in pieces(l) {
                    let Some(ib) = list(&q, b"ins ") else { continue };
                    let mut one = Ins { drum: false, bank: 0, patch: 0, regions: vec![], art: Art::default() };
                    for r in pieces(ib) {
                        if &r.1 == b"insh" {
                            let bank = u32at(r.2, 4);
                            one.drum = bank & DRUMS != 0;
                            one.bank = bank & 0x7fff_ffff;
                            one.patch = u32at(r.2, 8) & 127;
                        } else if let Some(rl) = list(&r, b"lrgn") {
                            for g in pieces(rl) {
                                let Some(gb) = list(&g, b"rgn ").or_else(|| list(&g, b"rgn2")) else { continue };
                                let mut rg = Rgn { key: (0, 127), vel: (0, 127), group: 0, wsmp: None, wave: u32::MAX, art: arts(gb) };
                                for h in pieces(gb) {
                                    match &h.1 {
                                        b"rgnh" => {
                                            rg.key = (u16at(h.2, 0), u16at(h.2, 2));
                                            rg.vel = (u16at(h.2, 4), u16at(h.2, 6));
                                            rg.group = u16at(h.2, 10);
                                        }
                                        b"wsmp" => rg.wsmp = Wsmp::read(h.2),
                                        b"wlnk" => rg.wave = u32at(h.2, 8),
                                        _ => {}
                                    }
                                }
                                one.regions.push(rg);
                            }
                        }
                    }
                    one.art = arts(ib).unwrap_or_default();
                    ins.push(one);
                }
            } else if let Some(l) = list(&p, b"wvpl") {
                for q in pieces(l) {
                    let Some(wb) = list(&q, b"wave") else { continue };
                    let mut w = Wave { rate: 22050, chans: 1, bits: 16, data: (0, 0), wsmp: None };
                    for r in pieces(wb) {
                        match &r.1 {
                            b"fmt " => {
                                w.chans = u16at(r.2, 2).max(1);
                                w.rate = u32at(r.2, 4);
                                w.bits = u16at(r.2, 14);
                            }
                            b"data" => {
                                let at = r.2.as_ptr() as usize - base;
                                w.data = (at, at + r.2.len());
                            }
                            b"wsmp" => w.wsmp = Wsmp::read(r.2),
                            _ => {}
                        }
                    }
                    // the table names a wave by how far its piece lies into the pool
                    waves.insert(q.0 as u32, w);
                }
            }
        }
        if ins.is_empty() {
            return None;
        }
        let table = cues.iter().map(|c| waves.get(c).cloned()).collect();
        Some(Dls { data, ins, table })
    }

    /// The zones of one instrument, or of a drum set.
    pub fn zones(&self, drum: bool, patch: u32) -> Vec<Zone> {
        let of = |d: bool, p: u32| self.ins.iter().filter(|i| i.drum == d && i.patch == p).min_by_key(|i| i.bank);
        let Some(ins) = of(drum, patch).or_else(|| of(drum, 0)).or_else(|| self.ins.iter().find(|i| i.drum == drum)) else { return vec![] };
        let mut cut: HashMap<u32, Arc<Sample>> = HashMap::new();
        let mut out = vec![];
        for r in &ins.regions {
            let Some(Some(w)) = self.table.get(r.wave as usize) else { continue };
            if w.rate == 0 || w.data.1 <= w.data.0 {
                continue;
            }
            let smp = cut
                .entry(r.wave)
                .or_insert_with(|| {
                    let raw = &self.data[w.data.0..w.data.1];
                    let ch = w.chans as usize;
                    let mono: Vec<f32> = if w.bits == 8 {
                        raw.chunks_exact(ch).map(|c| c.iter().map(|&x| (x as f32 - 128.0) / 128.0).sum::<f32>() / ch as f32).collect()
                    } else {
                        raw.chunks_exact(2 * ch).map(|c| c.chunks_exact(2).map(|x| i16::from_le_bytes([x[0], x[1]]) as f32 / 32768.0).sum::<f32>() / ch as f32).collect()
                    };
                    Arc::new(Sample::from_mono("", w.rate, &mono))
                })
                .clone();
            let ws = r.wsmp.or(w.wsmp).unwrap_or(Wsmp { unity: 60, ..Default::default() });
            let art = r.art.unwrap_or_default().over(ins.art);
            let n = smp.frames.len();
            let looped = ws.looped.map(|l| (l.0 as usize, (l.1 as usize).min(n))).filter(|l| l.1 > l.0 + 1);
            // the envelope works on the level in decibels: all the way down is 96 dB under
            let sustain = art.sustain.map_or(1.0, |s| if s <= 0.0 { 0.0 } else { 10f32.powf(-96.0 * (1.0 - s) / 20.0) });
            let env = Env { on: true, delay: 0.0, attack: art.attack.unwrap_or(0.0), hold: 0.0, decay: art.decay.unwrap_or(0.0), sustain, release: art.release.unwrap_or(0.05).max(0.005), amount: 0.0, ..Env::default() };
            out.push(Zone {
                key_lo: r.key.0.min(127) as u8,
                key_hi: r.key.1.min(127) as u8,
                vel_lo: r.vel.0.min(127) as u8,
                vel_hi: if r.vel.1 == 0 { 127 } else { r.vel.1.min(127) as u8 },
                root: ws.unity as i32,
                track: true,
                sample: Some(smp),
                // the stored gain is in 655360ths of a decibel
                gain: 10f32.powf((ws.gain as f32 / 655360.0).clamp(-96.0, 12.0) / 20.0),
                pan: art.pan.unwrap_or(0.0),
                tune_cents: ws.fine as f32,
                vel_scale: true,
                one_shot: drum && looped.is_none(),
                looped,
                attack: 0.001,
                release: 0.01,
                cut: if r.group > 0 { r.group as i32 } else { -1 },
                cut_by: if r.group > 0 { r.group as i32 } else { -1 },
                env: Some(env),
                ..Zone::default()
            });
        }
        out
    }
}

// ---- finding a bank on the machine ----------------------------------------------------------------------

enum Bank {
    Dls(Dls),
    Sf2(crate::sf2::Bank),
}

/// The places a General MIDI bank is looked for: the one named by CEREAL_GM_BANK, the one Windows brings,
/// then the usual sound bank folders.
fn candidates() -> Vec<std::path::PathBuf> {
    use std::path::PathBuf;
    let mut v: Vec<PathBuf> = vec![];
    if let Some(p) = std::env::var_os("CEREAL_GM_BANK") {
        v.push(p.into());
    }
    if let Some(root) = std::env::var_os("SystemRoot") {
        v.push(PathBuf::from(root).join("System32").join("drivers").join("gm.dls"));
    }
    let mut dirs: Vec<PathBuf> = ["/usr/share/soundfonts", "/usr/share/sounds/sf2", "/usr/share/sf2", "/usr/local/share/soundfonts"].iter().map(PathBuf::from).collect();
    if let Some(home) = std::env::var_os("HOME") {
        dirs.push(PathBuf::from(&home).join(".local/share/soundfonts"));
        dirs.push(PathBuf::from(&home).join(".local/share/sounds/sf2"));
    }
    for d in dirs {
        let Ok(rd) = std::fs::read_dir(&d) else { continue };
        let mut found: Vec<PathBuf> = rd.flatten().map(|e| e.path()).filter(|p| p.extension().and_then(|e| e.to_str()).is_some_and(|e| e.eq_ignore_ascii_case("sf2") || e.eq_ignore_ascii_case("dls"))).collect();
        // one that says it is a General MIDI set goes first
        found.sort_by_key(|p| {
            let n = p.file_name().and_then(|n| n.to_str()).unwrap_or("").to_ascii_lowercase();
            (!(n.contains("gm") || n.contains("default") || n.contains("general")), n)
        });
        v.extend(found);
    }
    v
}

fn bank() -> Option<&'static Bank> {
    static BANK: OnceLock<Option<Bank>> = OnceLock::new();
    BANK.get_or_init(|| {
        candidates().into_iter().find_map(|p| {
            let data = std::fs::read(&p).ok()?;
            if data.len() >= 12 && &data[8..12] == b"DLS " {
                Dls::parse(data).map(Bank::Dls)
            } else {
                crate::sf2::Bank::parse(data).map(Bank::Sf2)
            }
        })
    })
    .as_ref()
}

/// Whether the sounds come from a bank file on this machine (otherwise our stand-ins play).
pub fn has_bank() -> bool {
    bank().is_some()
}

/// The voice of one patch word of the bank player.
pub fn voice(patch: u32) -> Voice {
    static MADE: Mutex<Option<HashMap<u32, Voice>>> = Mutex::new(None);
    let (drum, num) = (patch & DRUMS != 0, patch & 127);
    let key = if drum { DRUMS | num } else { num };
    let mut made = MADE.lock().unwrap_or_else(|e| e.into_inner());
    let made = made.get_or_insert_with(HashMap::new);
    if let Some(v) = made.get(&key) {
        return v.clone();
    }
    let zones = match bank() {
        Some(Bank::Dls(d)) => d.zones(drum, num),
        Some(Bank::Sf2(b)) => b.zones(if drum { 128 } else { 0 }, num as u16),
        None => vec![],
    };
    let v = if !zones.is_empty() {
        Voice::Map(Arc::new(Map { zones, ..Map::default() }))
    } else if drum {
        Voice::Map(Arc::new(Map { zones: drum_set(), ..Map::default() }))
    } else {
        Voice::Add(Arc::new(crate::synth::stand_in(NAMES[num as usize])))
    };
    made.insert(key, v.clone());
    v
}

// ---- our own drum set -----------------------------------------------------------------------------------

/// One drum of the stand-in set, worked out from a few numbers: a tone that falls in pitch, noise through a
/// high-pass, and how fast each dies away.
struct Hit {
    /// Tone: start and end pitch in Hz, how fast the pitch falls and the level dies (1/s), level.
    tone: (f32, f32, f32, f32, f32),
    /// Noise: high-pass corner in Hz, how fast it dies (1/s), level.
    noise: (f32, f32, f32),
    /// A ring of two square waves (a bell or a cowbell): the two pitches, how fast it dies, level.
    ring: (f32, f32, f32, f32),
    secs: f32,
}

fn hit_for(key: u8) -> Hit {
    let none = Hit { tone: (0.0, 0.0, 0.0, 0.0, 0.0), noise: (0.0, 0.0, 0.0), ring: (0.0, 0.0, 0.0, 0.0), secs: 0.3 };
    let tom = |hz: f32| Hit { tone: (hz * 1.6, hz, 30.0, 9.0, 0.9), noise: (2000.0, 60.0, 0.12), secs: 0.5, ..none };
    let hand = |hz: f32| Hit { tone: (hz * 1.2, hz, 60.0, 22.0, 0.8), noise: (1500.0, 90.0, 0.1), secs: 0.25, ..none };
    let cymbal = |secs: f32, hp: f32| Hit { noise: (hp, 4.0 / secs, 0.5), ring: (5230.0, 7411.0, 4.0 / secs, 0.08), secs, ..none };
    match key {
        35 | 36 => Hit { tone: (160.0, if key == 35 { 46.0 } else { 52.0 }, 38.0, 9.0, 1.0), noise: (3000.0, 220.0, 0.25), secs: 0.45, ..none },
        37 => Hit { tone: (820.0, 800.0, 10.0, 70.0, 0.5), noise: (2500.0, 110.0, 0.4), secs: 0.08, ..none },
        38 | 40 => Hit { tone: (if key == 38 { 190.0 } else { 220.0 }, 170.0, 25.0, 22.0, 0.6), noise: (1800.0, 17.0, 0.7), secs: 0.3, ..none },
        39 => Hit { noise: (900.0, 22.0, 0.9), secs: 0.25, ..none },
        41 => tom(82.0),
        43 => tom(98.0),
        45 => tom(123.0),
        47 => tom(147.0),
        48 => tom(175.0),
        50 => tom(208.0),
        42 | 44 => Hit { noise: (7000.0, if key == 42 { 70.0 } else { 45.0 }, 0.5), secs: 0.09, ..none },
        46 => Hit { noise: (7000.0, 9.0, 0.5), secs: 0.45, ..none },
        49 | 57 => cymbal(1.6, 4500.0),
        52 => cymbal(1.2, 3000.0),
        55 => cymbal(0.7, 6000.0),
        51 | 59 => Hit { noise: (6500.0, 6.0, 0.2), ring: (3520.0, 5274.0, 4.0, 0.2), secs: 1.0, ..none },
        53 => Hit { ring: (2637.0, 3951.0, 5.0, 0.35), noise: (6500.0, 12.0, 0.08), secs: 0.9, ..none },
        54 | 69 | 70 | 82 => Hit { noise: (8000.0, if key == 54 { 18.0 } else { 40.0 }, 0.45), secs: if key == 54 { 0.25 } else { 0.12 }, ..none },
        56 => Hit { ring: (540.0, 800.0, 14.0, 0.5), secs: 0.3, ..none },
        60 => hand(420.0),
        61 => hand(310.0),
        62 => hand(360.0),
        63 => hand(260.0),
        64 => hand(200.0),
        65 => hand(480.0),
        66 => hand(360.0),
        67 | 68 => Hit { ring: (if key == 67 { 920.0 } else { 620.0 }, if key == 67 { 1380.0 } else { 930.0 }, 16.0, 0.4), secs: 0.25, ..none },
        75 | 76 | 77 => Hit { tone: ([2500.0, 1100.0, 800.0][key as usize - 75], [2500.0, 1100.0, 800.0][key as usize - 75], 1.0, 60.0, 0.7), secs: 0.08, ..none },
        80 | 81 => Hit { ring: (4186.0, 6272.0, if key == 80 { 40.0 } else { 5.0 }, 0.3), secs: if key == 80 { 0.12 } else { 0.9 }, ..none },
        _ => hand(440.0 * 2f32.powf((key as f32 - 69.0) / 12.0)),
    }
}

fn render(h: &Hit, seed: u32) -> Vec<f32> {
    const RATE: f32 = 44100.0;
    let n = (h.secs * RATE) as usize;
    let mut seed = seed.wrapping_mul(747_796_405).wrapping_add(2_891_336_453);
    let (mut phase, mut hp_in, mut hp_out) = (0.0f32, 0.0f32, 0.0f32);
    let k = (-std::f32::consts::TAU * h.noise.0 / RATE).exp();
    let mut out = Vec::with_capacity(n);
    for i in 0..n {
        let t = i as f32 / RATE;
        let mut y = 0.0;
        if h.tone.4 > 0.0 {
            let hz = h.tone.1 + (h.tone.0 - h.tone.1) * (-t * h.tone.2).exp();
            phase += hz / RATE;
            y += (phase * std::f32::consts::TAU).sin() * (-t * h.tone.3).exp() * h.tone.4;
        }
        if h.noise.2 > 0.0 {
            seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            let w = (seed >> 8) as f32 / (1u32 << 23) as f32 - 1.0;
            hp_out = k * (hp_out + w - hp_in);
            hp_in = w;
            y += hp_out * (-t * h.noise.1).exp() * h.noise.2;
        }
        if h.ring.3 > 0.0 {
            let sq = |hz: f32| if (t * hz).fract() < 0.5 { 1.0 } else { -1.0 };
            y += (sq(h.ring.0) + sq(h.ring.1)) * 0.5 * (-t * h.ring.2).exp() * h.ring.3;
        }
        // a short fade at the very end, so nothing clicks
        let left = (n - i) as f32 / (0.004 * RATE);
        out.push(y * left.min(1.0));
    }
    out
}

/// Our drum set on the standard's keys, 35 to 81.
fn drum_set() -> Vec<Zone> {
    (27u8..=87)
        .map(|key| {
            let smp = Arc::new(Sample::from_mono("", 44100, &render(&hit_for(key), key as u32)));
            // the closed and pedal hats stop the open one
            let hat = matches!(key, 42 | 44 | 46);
            Zone { key_lo: key, key_hi: key, root: key as i32, track: false, sample: Some(smp), gain: 0.8, vel_scale: true, one_shot: true, attack: 0.0, release: 0.02, cut: if hat { 1 } else { -1 }, cut_by: if hat { 1 } else { -1 }, ..Zone::default() }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn chunk(id: &[u8; 4], body: &[u8]) -> Vec<u8> {
        let mut v = id.to_vec();
        v.extend((body.len() as u32).to_le_bytes());
        v.extend(body);
        if body.len() & 1 == 1 {
            v.push(0);
        }
        v
    }

    fn lst(kind: &[u8; 4], parts: &[Vec<u8>]) -> Vec<u8> {
        let mut body = kind.to_vec();
        for p in parts {
            body.extend(p);
        }
        chunk(b"LIST", &body)
    }

    fn words(v: &[u32]) -> Vec<u8> {
        v.iter().flat_map(|x| x.to_le_bytes()).collect()
    }

    fn halves(v: &[u16]) -> Vec<u8> {
        v.iter().flat_map(|x| x.to_le_bytes()).collect()
    }

    /// A bank of one instrument (patch 48) with one looped region on keys 36 to 72, and a drum set with one key.
    fn tiny() -> Vec<u8> {
        let pcm: Vec<u8> = (0..400).flat_map(|i| (((i as f32 * 0.2).sin() * 12000.0) as i16).to_le_bytes()).collect();
        let fmt = [halves(&[1, 1]), words(&[22050, 44100]), halves(&[2, 16])].concat();
        let wave = lst(b"wave", &[chunk(b"fmt ", &fmt), chunk(b"data", &pcm)]);
        let wsmp = [words(&[20]), halves(&[60, 25]), words(&[(-6i32 * 655360) as u32, 0, 1, 16, 0, 100, 200])].concat();
        let rgn = |lo: u16, hi: u16, table: u32| {
            lst(b"rgn ", &[chunk(b"rgnh", &halves(&[lo, hi, 0, 127, 0, 0])), chunk(b"wsmp", &wsmp), chunk(b"wlnk", &[halves(&[0, 0]), words(&[1, table])].concat())])
        };
        // release of one second: 1200 * log2(1) = 0; sustain at half
        let art = chunk(b"art1", &[words(&[8, 2]), halves(&[0, 0, 0x209, 0]), words(&[0]), halves(&[0, 0, 0x20a, 0]), words(&[500 * 65536])].concat());
        let ins = |bank: u32, patch: u32, r: Vec<u8>| lst(b"ins ", &[chunk(b"insh", &words(&[1, bank, patch])), lst(b"lrgn", &[r]), lst(b"lart", &[art.clone()])]);
        let wvpl = lst(b"wvpl", &[wave.clone(), wave]);
        let second = (wvpl.len() - 12) as u32 / 2;
        let body = [
            b"DLS ".to_vec(),
            chunk(b"colh", &words(&[2])),
            lst(b"lins", &[ins(0, 48, rgn(36, 72, 1)), ins(DRUMS, 0, rgn(38, 38, 0))]),
            chunk(b"ptbl", &words(&[8, 2, 0, second])),
            wvpl,
        ]
        .concat();
        chunk(b"RIFF", &body)
    }

    #[test]
    fn a_small_bank_gives_its_regions() {
        let d = Dls::parse(tiny()).expect("reads");
        let z = d.zones(false, 48);
        assert_eq!(z.len(), 1);
        assert_eq!((z[0].key_lo, z[0].key_hi, z[0].root), (36, 72, 60));
        assert_eq!(z[0].looped, Some((100, 300)));
        assert_eq!(z[0].tune_cents, 25.0);
        assert!((z[0].gain - 0.501).abs() < 0.01, "{}", z[0].gain);
        let e = z[0].env.unwrap();
        assert!((e.release - 1.0).abs() < 1e-3 && (e.sustain - 10f32.powf(-48.0 / 20.0)).abs() < 1e-4);
        assert_eq!(z[0].sample.as_ref().unwrap().frames.len(), 400);
        // the drum set is apart from the instruments, and a patch that is not there falls back to the first
        let k = d.zones(true, 0);
        assert_eq!((k.len(), k[0].key_lo, k[0].one_shot), (1, 38, false));
        assert_eq!(d.zones(false, 5).len(), 1);
    }

    #[test]
    fn the_block_gives_the_patches() {
        let mut b = vec![0u8; 97];
        b[33 + 4] = 48;
        b[33 + 9 * 4 + 3] = 0x80;
        let s = Set::read(&b).unwrap();
        assert_eq!((s.port, s.patch[1], s.patch[9]), (0, 48, DRUMS));
        assert!(Set::read(&b[..90]).is_none());
        // the unit's settings go back into the same bytes, and a fresh block has the drums on channel 10
        let p = decode(&b).unwrap();
        assert_eq!((p[0], p[2], p[17 + 9], patch_of(&p, 1, -1), patch_of(&p, 9, 30)), (0.0, 48.0, 1.0, 48, DRUMS | 30));
        assert_eq!(encode(&p, Some(&b)), b);
        let fresh = crate::fx::Fx::new(crate::fx::Kind::Gm);
        let block = fresh.encode(None);
        assert_eq!((block.len(), Set::read(&block).unwrap().patch[9]), (97, DRUMS));
    }

    #[test]
    fn our_drums_sound_and_end() {
        let set = drum_set();
        for key in [36u8, 38, 42, 46, 49, 56, 60] {
            let z = set.iter().find(|z| z.key_lo == key).unwrap();
            let f = &z.sample.as_ref().unwrap().frames;
            let peak = f.iter().fold(0f32, |m, x| m.max(x[0].abs()));
            assert!(peak > 0.1 && peak < 1.6, "{key}: {peak}");
            assert!(f.last().unwrap()[0].abs() < 0.01, "{key}");
        }
        // the kick is low, the hat is not: count the turns
        let turns = |key: u8| {
            let z = set.iter().find(|z| z.key_lo == key).unwrap();
            let f = &z.sample.as_ref().unwrap().frames;
            f.windows(2).take(2000).filter(|w| (w[0][0] < 0.0) != (w[1][0] < 0.0)).count()
        };
        assert!(turns(36) < 40 && turns(42) > 300, "{} {}", turns(36), turns(42));
    }
}
