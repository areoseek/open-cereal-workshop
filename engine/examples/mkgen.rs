//! Write a new project: one instrument channel with notes in pattern 1, laid on the playlist. Used to measure
//! an instrument against the reference.
//! mkgen <out.flp> <instrument identifier> [setting=value ...]   (setting counted from 0)
//! MKGEN_PRESET=<preset file> takes the settings from a preset instead.
//! MKGEN_NOTES="pos:len:key[:vel[:x[:y]]] ..." in ticks, x and y -100..100 (default one two-beat C5); MKFX_TEMPO sets the tempo.
use cereal_engine::song::{Chan, Clip, ClipSrc, Note, Song};
fn main() {
    let a: Vec<String> = std::env::args().collect();
    let mut song = Song::new();
    if let Some(t) = std::env::var("MKFX_TEMPO").ok().and_then(|v| v.parse().ok()) {
        song.tempo = t;
    }
    // MKGEN_PRESET names a preset file to take the instrument's settings from instead of a fresh instance
    let mut c = match std::env::var("MKGEN_PRESET") {
        Ok(p) => {
            let bytes = std::fs::read(&p).expect("preset file");
            match cereal_engine::preset::channel(&bytes, None, None) {
                Ok(c) => c,
                Err(_) => {
                    // a plug-in preset: only the settings block
                    let pr = cereal_engine::preset::parse(&bytes).expect("preset");
                    let g = cereal_engine::synth::Gen::decode(&pr.ident, &pr.state).expect("settings");
                    let mut c = Chan::instrument(g.kind);
                    c.plugin = pr.ident.clone();
                    c.gen = Some(g);
                    c
                }
            }
        }
        // MKGEN_SF2="stored path;bank;patch": a sound bank channel; setting=value then writes the block's 4-byte numbers
        Err(_) if std::env::var("MKGEN_SF2").is_ok() => {
            let sf = std::env::var("MKGEN_SF2").unwrap();
            let mut it = sf.split(';');
            let want = cereal_engine::sf2::Setting { path: it.next().unwrap().into(), bank: it.next().and_then(|x| x.parse().ok()).unwrap_or(0), patch: it.next().and_then(|x| x.parse().ok()).unwrap_or(0) };
            let mut block = cereal_engine::sf2::with(&[], &want);
            for kv in &a[3..] {
                let (k, v) = kv.split_once('=').expect("setting=value");
                let k: usize = k.parse().expect("index");
                block[k * 4..k * 4 + 4].copy_from_slice(&v.parse::<i32>().expect("value").to_le_bytes());
            }
            let mut c = Chan { name: "bank".into(), kind: cereal_engine::flp::ChannelKind::Plugin, plugin: a[2].clone(), ..Chan::default() };
            c.zones = cereal_engine::sf2::open("SoundFont Player", &block, None, None).map(std::sync::Arc::new);
            c
        }
        Err(_) => {
            let def = cereal_engine::synth::DEFS.iter().find(|d| d.ident == a[2]).expect("no such instrument");
            let mut c = Chan::instrument(def.kind);
            c.plugin = a[2].clone();
            c
        }
    };
    if let Some(g) = &mut c.gen {
        for kv in &a[3..] {
            let (k, v) = kv.split_once('=').expect("setting=value");
            g.p[k.parse::<usize>().expect("index")] = v.parse().expect("value");
        }
        println!("{:?}", &g.p[..g.p.len().min(40)]);
    }
    if let Some(v) = std::env::var("MKGEN_VOL").ok().and_then(|x| x.parse().ok()) {
        c.volume = v;
    }
    if let Some(v) = std::env::var("MKGEN_PAN").ok().and_then(|x| x.parse().ok()) {
        c.pan = v;
    }
    song.channels.clear();
    song.channels.push(c);
    let notes = std::env::var("MKGEN_NOTES").unwrap_or_else(|_| format!("0:{}:60", song.ppq * 2));
    let mut far = 1;
    for w in notes.split_whitespace() {
        let p: Vec<f32> = w.split(':').filter_map(|x| x.parse().ok()).collect();
        if p.len() >= 3 {
            let vel = p.get(3).copied().unwrap_or(100.0) / 100.0;
            song.patterns[0].notes.push(Note { pos: p[0] as u32, len: p[1] as u32, chan: 0, key: p[2] as u8, vel, mod_x: p.get(4).copied().unwrap_or(0.0) / 100.0, mod_y: p.get(5).copied().unwrap_or(0.0) / 100.0, ..Default::default() });
            far = far.max(p[0] as u32 + p[1] as u32);
        }
    }
    song.clips.push(Clip { pos: 0, len: far, track: 0, src: ClipSrc::Pattern(0), offset: 0, muted: false, group: 0, fine: 0.0, fx: Default::default() });
    std::fs::write(&a[1], cereal_engine::save::to_bytes(&song, None).expect("save")).expect("write");
}
