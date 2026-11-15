//! Write a new project: one instrument channel with notes in pattern 1, laid on the playlist. Used to measure
//! an instrument against the reference.
//! mkgen <out.flp> <instrument identifier> [setting=value ...]   (setting counted from 0)
//! MKGEN_NOTES="pos:len:key[:vel] ..." in ticks (default one two-beat C5); MKFX_TEMPO sets the tempo.
use cereal_engine::song::{Chan, Clip, ClipSrc, Note, Song};
fn main() {
    let a: Vec<String> = std::env::args().collect();
    let mut song = Song::new();
    if let Some(t) = std::env::var("MKFX_TEMPO").ok().and_then(|v| v.parse().ok()) {
        song.tempo = t;
    }
    let def = cereal_engine::synth::DEFS.iter().find(|d| d.ident == a[2]).expect("no such instrument");
    let mut c = Chan::instrument(def.kind);
    c.plugin = a[2].clone();
    if let Some(g) = &mut c.gen {
        for kv in &a[3..] {
            let (k, v) = kv.split_once('=').expect("setting=value");
            g.p[k.parse::<usize>().expect("index")] = v.parse().expect("value");
        }
        println!("{:?}", g.p);
    }
    song.channels.clear();
    song.channels.push(c);
    let notes = std::env::var("MKGEN_NOTES").unwrap_or_else(|_| format!("0:{}:60", song.ppq * 2));
    let mut far = 1;
    for w in notes.split_whitespace() {
        let p: Vec<f32> = w.split(':').filter_map(|x| x.parse().ok()).collect();
        if p.len() >= 3 {
            let vel = p.get(3).copied().unwrap_or(100.0) / 100.0;
            song.patterns[0].notes.push(Note { pos: p[0] as u32, len: p[1] as u32, chan: 0, key: p[2] as u8, vel, ..Default::default() });
            far = far.max(p[0] as u32 + p[1] as u32);
        }
    }
    song.clips.push(Clip { pos: 0, len: far, track: 0, src: ClipSrc::Pattern(0), offset: 0, muted: false, group: 0, fine: 0.0, fx: Default::default() });
    std::fs::write(&a[1], cereal_engine::save::to_bytes(&song, None).expect("save")).expect("write");
}
