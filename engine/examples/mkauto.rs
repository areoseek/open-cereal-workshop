//! Write a new project holding one sound file as an audio clip and an automation clip that drives a control.
//! mkauto <sound file> <out.flp> <path to store> <chanvol|chanpan|insvol> tick,value,tension,mode ...
use cereal_engine::flp::ChannelKind;
use cereal_engine::song::{AutoPt, Automation, Chan, Clip, ClipSrc, Song, Target};
fn main() {
    let a: Vec<String> = std::env::args().collect();
    let mut lib = cereal_engine::Library::default();
    let smp = lib.load_path(std::path::Path::new(&a[1])).expect("sound");
    let mut song = Song::new();
    let len = ((smp.seconds() * song.tempo / 60.0 * song.ppq as f32).ceil() as u32).max(1);
    let mut c = Chan::default();
    c.name = smp.name.clone();
    c.kind = ChannelKind::AudioClip;
    c.sample_path = a[3].clone();
    c.set_sample(Some(smp));
    song.channels.push(c);
    let ci = song.channels.len() - 1;
    song.song_mode = true;
    song.clips.push(Clip { pos: 0, len, track: 0, src: ClipSrc::Audio(ci), offset: 0, muted: false, group: 0 });
    let target = match a[4].as_str() {
        "chanpan" => Target::ChanPan(ci),
        "insvol" => Target::InsertVolume(0),
        _ => Target::ChanVolume(ci),
    };
    let points: Vec<AutoPt> = a[5..]
        .iter()
        .map(|s| {
            let v: Vec<f64> = s.split(',').map(|x| x.parse().expect("number")).collect();
            AutoPt { tick: v[0], value: v[1] as f32, tension: v[2] as f32, mode: v[3] as u8 }
        })
        .collect();
    let end = points.last().map_or(1.0, |p| p.tick) as u32;
    // the clip's range: the part of a volume knob where level follows the setting gently, so the
    // shape of the curve can be read off a render
    let lo_hi = (0.5f32, 0.78125f32);
    let ac = song.channels.len();
    song.channels.push(Chan { name: "Automation".into(), auto: Some(std::sync::Arc::new(Automation { points, targets: vec![target] })), volume: lo_hi.1, pan: lo_hi.0 * 2.0 - 1.0, ..Chan::automation() });
    song.clips.push(Clip { pos: 0, len: end.max(1), track: 1, src: ClipSrc::Automation(ac), offset: 0, muted: false, group: 0 });
    song.prepare();
    std::fs::write(&a[2], cereal_engine::save::to_bytes(&song, None).expect("save")).expect("write");
}
