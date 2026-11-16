//! Run a sound file through one effect unit set from a stored preset file, and write what comes out with the
//! unit's own lateness taken off. Used to compare a preset's behaviour against a reference render.
//! maxiprobe <in.wav> <out.wav> <preset file> [setting=value ...]   (setting counted from 0; one past the end adds one)
//! With - for the output it prints the settings instead.
use cereal_engine::fx::{Ctx, Fx, Unit};
fn main() {
    let a: Vec<String> = std::env::args().collect();
    let mut lib = cereal_engine::Library::default();
    let smp = lib.load_path(std::path::Path::new(&a[1])).expect("sound");
    // @room stands for a fresh room unit instead of a stored one
    let mut fx = if a[3] == "@room" {
        Fx::new(cereal_engine::fx::Kind::Reverb)
    } else {
        let pre = cereal_engine::preset::read(std::path::Path::new(&a[3])).expect("preset");
        Fx::decode(&pre.ident, &pre.state).expect("a unit we know")
    };
    for kv in &a[4..] {
        let (k, v) = kv.split_once('=').expect("setting=value");
        let (k, v): (usize, f32) = (k.parse().expect("index"), v.parse().expect("number"));
        if k == fx.p.len() {
            fx.p.push(v);
        } else {
            fx.p[k] = v;
        }
    }
    if a[2] == "-" {
        // only say what the settings are
        let head: Vec<String> = fx.p.iter().take(cereal_engine::fxmaxi::N).map(|v| format!("{v}")).collect();
        println!("{}", head.join(" "));
        for b in 0..4 {
            let t = &fx.p[cereal_engine::fxmaxi::N + b * cereal_engine::fxmaxi::TABLE..][..cereal_engine::fxmaxi::TABLE];
            let pick: Vec<String> = (0..=16).map(|i| format!("{:.3}", t[i * 16])).collect();
            println!("line {b} span {} : {}", fx.p[cereal_engine::fxmaxi::SPANS + b], pick.join(" "));
        }
        println!("tail {:?}", &fx.p[cereal_engine::fxmaxi::SPANS + 4..]);
        return;
    }
    let sr = smp.rate as f32;
    let late = fx.latency(sr);
    let mut buf: Vec<[f32; 2]> = smp.frames.clone();
    buf.extend(std::iter::repeat([0.0; 2]).take(late));
    let mut unit = Unit::new(fx, sr);
    let cx = Ctx { sr, step: sr * 60.0 / 130.0 / 4.0, clock: Default::default() };
    for block in buf.chunks_mut(512) {
        unit.process(block, &cx);
    }
    let flat: Vec<f32> = buf[late..].iter().flat_map(|f| [f[0], f[1]]).collect();
    let file = std::fs::File::create(&a[2]).expect("out");
    cereal_engine::wav::write_pcm16(std::io::BufWriter::new(file), &flat, smp.rate, 2).expect("write");
}
