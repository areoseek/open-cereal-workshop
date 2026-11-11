//! Breakfast Cereal DAW engine: the project model, `.flp` reading and writing, the sampler, the sequencer and
//! the mixer. No UI and no audio device live here; the app feeds `Player::render` from its audio callback.

pub mod flp;
pub mod fx;
pub mod fxbank;
pub mod fxpatch;
pub mod fxrack;
pub mod fxvoc;
pub mod synth;
pub mod law;
pub mod player;
pub mod additive;
pub mod analog;
pub mod fm;
pub mod minis;
pub mod keys;
pub mod slicer;
pub mod beep;
pub mod conv;
pub mod drumbank;
pub mod grain;
pub mod wavpack;
pub mod metal;
pub mod zones;
pub mod preset;
pub mod sample;
pub mod save;
pub mod song;
pub mod wav;

pub use player::{new_player, Cmd, Handle, Player, Transport};
pub use sample::{Library, Sample};
pub use song::Song;

use std::path::Path;

/// Read a project file from disk into a song, loading its samples through `lib`.
pub fn load_flp(path: &Path, lib: &mut Library) -> Result<(Song, flp::Flp), String> {
    let bytes = std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let f = flp::parse(&bytes)?;
    let song = Song::from_flp(&f, lib, path.parent());
    Ok((song, f))
}

/// Render a song offline into interleaved stereo at `sample_rate`. In song mode the whole playlist is rendered;
/// otherwise `pattern` once. `tail` seconds are added for sounds still ringing. No audio device is touched.
pub fn render_song(song: &Song, sample_rate: f32, song_mode: bool, pattern: usize, tail: f32) -> Vec<f32> {
    let (h, mut p) = new_player(song.clone(), sample_rate);
    h.transport.set_song_mode(song_mode);
    h.transport.set_pattern(pattern);
    h.transport.set_looping(false);
    h.transport.set_playing(true);
    let max_tail = (tail.max(0.0) * sample_rate) as usize;
    // automation can change the tempo, so the length is only known by playing it through
    let cap = (sample_rate as usize) * 3600 * 4;
    let mut out: Vec<f32> = Vec::new();
    let _ = pattern;
    while h.transport.playing() && out.len() / 2 < cap {
        let at = out.len();
        out.resize(at + 2 * 1024, 0.0);
        p.render(&mut out[at..], 2);
    }
    let mut extra = 0;
    while p.busy() && extra < max_tail {
        let at = out.len();
        out.resize(at + 2 * 1024, 0.0);
        p.render(&mut out[at..], 2);
        extra += 1024;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    #[test]
    fn offline_render_has_the_length_of_the_pattern_plus_tail() {
        let mut s = Song::new();
        s.tempo = 120.0;
        let data: Vec<f32> = (0..48_000).map(|i| (i as f32 * 0.05).sin() * 0.5).collect();
        s.channels[0].sample = Some(Arc::new(Sample::from_mono("tone", 48_000, &data)));
        s.patterns[0].toggle_step(0, 360); // last step of the bar: rings past the end
        let buf = render_song(&s, 48_000.0, false, 0, 2.0);
        assert!(buf.len() >= 96_000 * 2);
        assert!(buf.len() <= (96_000 + 48_000) * 2);
        assert!(buf.iter().all(|x| x.is_finite() && x.abs() <= 1.0));
        assert!(buf[96_000 * 2..].iter().any(|x| x.abs() > 0.01), "tail should carry the ringing sample");
    }
}
