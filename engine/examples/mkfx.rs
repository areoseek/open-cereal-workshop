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
    if let Some(t) = std::env::var("MKFX_TEMPO").ok().and_then(|v| v.parse().ok()) {
        song.tempo = t;
    }
    if let Some(t) = std::env::var("MKFX_MAIN").ok().and_then(|v| v.parse().ok()) {
        song.main_pitch = t;
    }
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
            "adjust_pan" => f.adjust_pan = x,
            "adjust_x" => f.adjust_x = x,
            "gate_steps" => f.gate_steps = x,
            "shift" => f.shift = x,
            "arp_dir" => f.arp_dir = x as u8,
            "arp_range" => f.arp_range = x as u8,
            "arp_chord" => f.arp_chord = x as i16,
            "arp_steps" => f.arp_steps = x,
            "arp_gate" => f.arp_gate = x,
            "arp_repeat" => f.arp_repeat = x as u8,
            "arp_slide" => f.arp_slide = x != 0.0,
            "tkey_mid" => f.track_key[0] = x,
            "tkey_pan" => f.track_key[1] = x,
            "tkey_x" => f.track_key[2] = x,
            "tkey_y" => f.track_key[3] = x,
            "tvel_mid" => f.track_vel[0] = x,
            "tvel_pan" => f.track_vel[1] = x,
            "tvel_x" => f.track_vel[2] = x,
            "tvel_y" => f.track_vel[3] = x,
            "echoes" => f.echoes = x as u8,
            "echo_feedback" => f.echo_feedback = x,
            "echo_pan" => f.echo_pan = x,
            "echo_cents" => f.echo_cents = x,
            "old_delay" => f.old_fx[0] = x as u32,
            "old_pogo" => f.old_fx[1] = x as u32,
            "old_ring" => f.old_fx[2] = x as u32,
            "old_reverb" => f.old_fx[3] = x as u32,
            "old_boost" => f.old_fx[4] = x as u32,
            "old_eq" => f.old_fx[5] = x as u32,
            "old_cut" => f.old_fx[6] = x as u32,
            "old_res" => f.old_fx[7] = x as u32,
            "echo_mod_x" => f.echo_mod_x = x,
            "echo_mod_y" => f.echo_mod_y = x,
            "echo_steps" => f.echo_steps = x,
            "echo_ping" => f.echo_ping = x != 0.0,
            "echo_fat" => f.echo_fat = x != 0.0,
            "adjust_y" => f.adjust_y = x,
            "gain" => f.gain = x,
            "max_poly" => f.max_poly = x as u32,
            "main" => f.main_pitch = x != 0.0,
            "mode" => {
                f.stretch_mode = x as i8;
                f.keep_pitch = x != 0.0;
            }
            "normalize" => f.normalize = x != 0.0,
            "reverse" => f.reverse = x != 0.0,
            "fade_in" => f.fade_in = x,
            "fade_out" => f.fade_out = x,
            "cutoff" => f.cutoff = x,
            "resonance" => f.resonance = x,
            "filter" => f.filter = x as u8,
            _ => panic!("unknown treatment {k}"),
        }
    }
    c.set_sample(Some(smp));
    song.channels.push(c);
    let ci = song.channels.len() - 1;
    song.song_mode = true;
    if !sampler {
        song.clips.push(Clip { pos: 0, len, track: 0, src: ClipSrc::Audio(ci), offset: 0, muted: false, group: 0, fine: 0.0, fx: Default::default() });
    } else {
        // MKFX_NOTE=ticks: how long the one note is held (default: the sample's length); MKFX_KEY its key
        let hold = std::env::var("MKFX_NOTE").ok().and_then(|v| v.parse().ok()).unwrap_or(len);
        let key = std::env::var("MKFX_KEY").ok().and_then(|v| v.parse().ok()).unwrap_or(60);
        let pos = std::env::var("MKFX_POS").ok().and_then(|v| v.parse().ok()).unwrap_or(0);
        // MKFX_MOD="x:y": the first note's two free controls, -1..=1
        let md: Vec<f32> = std::env::var("MKFX_MOD").unwrap_or_default().split(':').filter_map(|x| x.parse().ok()).collect();
        let (mod_x, mod_y) = (md.first().copied().unwrap_or(0.0), md.get(1).copied().unwrap_or(0.0));
        song.patterns[0].notes.push(cereal_engine::song::Note { pos, len: hold, chan: ci as u16, key, mod_x, mod_y, ..Default::default() });
        // MKFX_NOTES="pos:len:key[:modx:mody] ...": more notes after the first
        let mut far = len.max(hold) + pos;
        for w in std::env::var("MKFX_NOTES").unwrap_or_default().split_whitespace() {
            let p: Vec<f32> = w.split(':').filter_map(|x| x.parse().ok()).collect();
            if p.len() >= 3 {
                let get = |i: usize| p.get(i).copied().unwrap_or(0.0);
                song.patterns[0].notes.push(cereal_engine::song::Note { pos: p[0] as u32, len: p[1] as u32, chan: ci as u16, key: p[2] as u8, mod_x: get(3), mod_y: get(4), ..Default::default() });
                far = far.max(p[0] as u32 + p[1] as u32);
            }
        }
        song.clips.push(Clip { pos: 0, len: far, track: 0, src: ClipSrc::Pattern(0), offset: 0, muted: false, group: 0, fine: 0.0, fx: Default::default() });
    }
    if let Some(ident) = a.get(4).filter(|s| s.as_str() != "-") {
        let def = cereal_engine::fx::DEFS.iter().find(|d| d.ident == ident).expect("no such effect");
        let mut slot = Slot::with_fx(0, def.kind);
        slot.plugin = ident.clone();
        if let Some(f) = &mut slot.fx {
            for kv in &a[5..] {
                let (k, v) = kv.split_once('=').expect("setting=value");
                // line:K=x:y,x:y,...[,flags:N]  redraws drawn line K (x = distance from the point before)
                if let Some(k) = k.strip_prefix("line:") {
                    let k: usize = k.parse().expect("line number");
                    let mut c = f.line(k).expect("no such line");
                    c.pts.clear();
                    for pt in v.split(',') {
                        let (x, y) = pt.split_once(':').expect("x:y");
                        if x == "flags" {
                            c.flags = y.parse().expect("flags");
                        } else if x == "live" {
                            c.live = y != "0";
                        } else if x == "loop" {
                            c.loop_start = y.parse().expect("loop");
                        } else if x == "sus" {
                            c.sustain = y.parse().expect("sus");
                        } else if x == "decay" {
                            c.decay = y.parse().expect("decay");
                        } else {
                            c.pts.push(cereal_engine::fm::Pt { x: x.parse().expect("x"), y: y.parse().expect("y"), ..Default::default() });
                        }
                    }
                    assert!(f.set_line(k, &c), "line {k} not set");
                    continue;
                }
                // after:K=a:b:c:d  the four numbers behind line K
                if let Some(o) = k.strip_prefix("head:") {
                    assert!(f.set_bank_head(o.parse().unwrap(), v.parse().unwrap()), "header number {o} not set");
                    continue;
                }
                if let Some(k) = k.strip_prefix("after:") {
                    let k: usize = k.parse().expect("line number");
                    let n: Vec<i32> = v.split(':').map(|x| x.parse().expect("number")).collect();
                    assert!(f.set_line_after(k, [n[0], n[1], n[2], n[3]]), "numbers behind line {k} not set");
                    continue;
                }
                f.p[k.parse::<usize>().expect("index")] = v.parse().expect("value");
            }
            println!("{:?}", f.p);
        }
        song.inserts[1].slots.push(slot);
    }
    std::fs::write(&a[2], cereal_engine::save::to_bytes(&song, None).expect("save")).expect("write");
}
