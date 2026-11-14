//! Write a new project: one sound file as an audio clip on mixer track 1, with one of our effect units in its
//! first slot. Used to measure an effect against the reference with a known input.
//! mkfx <sound file> <out.flp> <stored path> <effect identifier> [setting=value ...]   (setting counted from 0)
use cereal_engine::flp::ChannelKind;
use cereal_engine::song::{Chan, Clip, ClipSrc, Slot, Song};
fn main() {
    let a: Vec<String> = std::env::args().collect();
    let mut lib = cereal_engine::Library::default();
    let smp = lib.load_path(std::path::Path::new(&a[1])).expect("sound");
    let mut song = Song::new();
    let len = ((smp.seconds() * song.tempo / 60.0 * song.ppq as f32).ceil() as u32).max(1);
    let mut c = Chan::default();
    c.name = smp.name.clone();
    let sampler = std::env::var("MKFX_SAMPLER").is_ok();
    c.kind = if sampler { ChannelKind::Sampler } else { ChannelKind::AudioClip };
    c.sample_path = a[3].clone();
    c.insert = 1;
    c.smp.use_loop = std::env::var("MKFX_LOOP").is_ok();
    // MKFX_SMP="fade_stereo=1 smp_start=0.25 ..." sets sample treatments
    for kv in std::env::var("MKFX_SMP").unwrap_or_default().split_whitespace() {
        let (k, v) = kv.split_once('=').expect("key=value");
        let x: f32 = v.parse().expect("number");
        let f = &mut c.smp;
        match k {
            "fade_stereo" => f.fade_stereo = x != 0.0,
            "remove_dc" => f.remove_dc = x != 0.0,
            "smp_start" => f.smp_start = x,
            "smp_len" => f.smp_len = x,
            "crossfade" => f.crossfade = x,
            "trim" => f.trim = x,
            "start_offset" => f.start_offset = x,
            "declick" => f.declick = x as u8,
            "bend_range" => f.bend_range = x as u8,
            "mode" => {
                f.stretch_mode = x as i8;
                f.keep_pitch = x != 0.0;
            }
            "normalize" => f.normalize = x != 0.0,
            "reverse" => f.reverse = x != 0.0,
            "fade_in" => f.fade_in = x,
            "fade_out" => f.fade_out = x,
            _ => panic!("unknown treatment {k}"),
        }
    }
    c.set_sample(Some(smp));
    song.channels.push(c);
    let ci = song.channels.len() - 1;
    song.song_mode = true;
    if !sampler {
        song.clips.push(Clip { pos: 0, len, track: 0, src: ClipSrc::Audio(ci), offset: 0, muted: false, group: 0 });
    }
    if let Some(ident) = a.get(4).filter(|s| s.as_str() != "-") {
        let def = cereal_engine::fx::DEFS.iter().find(|d| d.ident == ident).expect("no such effect");
        let mut slot = Slot::with_fx(0, def.kind);
        slot.plugin = ident.clone();
        if let Some(f) = &mut slot.fx {
            for kv in &a[5..] {
                let (k, v) = kv.split_once('=').expect("setting=value");
                f.p[k.parse::<usize>().expect("index")] = v.parse().expect("value");
            }
            println!("{:?}", f.p);
        }
        song.inserts[1].slots.push(slot);
    }
    std::fs::write(&a[2], cereal_engine::save::to_bytes(&song, None).expect("save")).expect("write");
}
