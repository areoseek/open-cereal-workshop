//! Write a new project holding one sound file as an audio clip at the start of the first track.
//! mkclip <sound file> <out.flp> [path to store in the project]
use cereal_engine::flp::ChannelKind;
use cereal_engine::song::{Chan, Clip, ClipSrc, Song};
fn main() {
    let a: Vec<String> = std::env::args().collect();
    let mut lib = cereal_engine::Library::default();
    let smp = lib.load_path(std::path::Path::new(&a[1])).expect("sound");
    let mut song = Song::new();
    let len = ((smp.seconds() * song.tempo / 60.0 * song.ppq as f32).ceil() as u32).max(1);
    let mut c = Chan::default();
    c.name = smp.name.clone();
    c.kind = ChannelKind::AudioClip;
    c.sample_path = a.get(3).cloned().unwrap_or_else(|| a[1].clone());
    c.set_sample(Some(smp));
    song.channels.push(c);
    let ci = song.channels.len() - 1;
    song.song_mode = true;
    song.clips.push(Clip { pos: 0, len, track: 0, src: ClipSrc::Audio(ci), offset: 0, muted: false, group: 0 });
    std::fs::write(&a[2], cereal_engine::save::to_bytes(&song, None).expect("save")).expect("write");
}
