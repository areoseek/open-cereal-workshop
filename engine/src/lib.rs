//! Open Cereal Workshop engine: the project model, `.flp` reading and writing, the sampler, the sequencer and
//! the mixer. No UI and no audio device live here; the app feeds `Player::render` from its audio callback.

pub mod flp;
pub mod flac;
pub mod fx;
pub mod host;
pub mod fxbank;
pub mod fxpatch;
pub mod fxrack;
pub mod fxmulti;
pub mod fxsplit;
pub mod noteshape;
pub mod pyrt;
pub mod script;
pub mod surface;
pub mod fxvoc;
pub mod synth;
pub mod law;
pub mod lossy;
pub mod sf2;
pub mod notes;
pub mod player;
pub mod additive;
pub mod analog;
pub mod duo;
pub mod tailfx;
pub mod strmodel;
pub mod hybrid;
pub mod morph;
pub mod drumkit;
pub mod kickd;
pub mod membrane;
pub mod triple;
pub mod fm;
pub mod formula;
pub mod scope;
pub mod waveed;
pub mod starter;
pub mod minis;
pub mod midifile;
pub mod midiout;
pub mod keys;
pub mod slicer;
pub mod beep;
pub mod conv;
pub mod convset;
#[cfg(test)]
mod probe;
pub mod drumbank;
pub mod grain;
pub mod dash;
pub mod travel;
pub mod guitar;
pub mod wavpack;
pub mod metal;
pub mod zones;
pub mod zipfile;
pub mod preset;
pub mod sample;
pub mod save;
pub mod score;
pub mod song;
pub mod wav;

pub use player::{new_player, Cmd, CmdSender, Handle, Live, PerfSeg, Player, Transport};
pub use sample::{Library, Sample};
pub use song::Song;

use std::path::Path;

/// Read a project file from disk into a song, loading its samples through `lib`.
pub fn load_flp(path: &Path, lib: &mut Library) -> Result<(Song, flp::Flp), String> {
    let bytes = std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let f = flp::parse(&bytes)?;
    if f.file.events.iter().take(8).any(|e| e.id == 28 && e.value == flp::Value::Byte(0)) {
        // a save made by the unregistered original: its contents are locked and are not read here
        return Err("This project was saved by the trial version of FL Studio, which locks what it saves. Open it in a registered copy and save it again, then it opens here.".into());
    }
    let song = Song::from_flp(&f, lib, path.parent());
    Ok((song, f))
}

/// A project as one archive: the project file and every sound file it leads to, side by side. `roots` is a
/// library that knows the same folders as the one in use; returns the archive and how many sounds went in.
pub fn package(song: &mut Song, base: Option<&flp::Flp>, name: &str, roots: &Library, project_dir: Option<&Path>) -> Result<(Vec<u8>, usize), String> {
    let (bytes, f) = save::save(song, base)?;
    let mut lib = Library::default();
    lib.factory_roots = roots.factory_roots.clone();
    lib.user_roots = roots.user_roots.clone();
    lib.search_dirs = roots.search_dirs.clone();
    let _ = Song::from_flp(&f, &mut lib, project_dir);
    let mut entries = vec![(format!("{name}.flp"), bytes)];
    for p in &lib.used {
        let Some(file) = p.file_name().map(|n| n.to_string_lossy().to_string()) else { continue };
        if entries.iter().any(|(n, _)| n.eq_ignore_ascii_case(&file)) {
            continue;
        }
        entries.push((file, std::fs::read(p).map_err(|e| format!("{}: {e}", p.display()))?));
    }
    Ok((zipfile::write(&entries), entries.len() - 1))
}

/// Unpack a project archive into `into` (made if need be) and name the project file that was in it.
pub fn unpack(archive: &Path, into: &Path) -> Result<std::path::PathBuf, String> {
    let bytes = std::fs::read(archive).map_err(|e| format!("{}: {e}", archive.display()))?;
    let files = zipfile::read(&bytes, 8 << 30)?;
    let project = files.iter().map(|(n, _)| n).filter(|n| n.to_lowercase().ends_with(".flp")).min_by_key(|n| n.matches('/').count()).cloned().ok_or("there is no project file in the archive")?;
    for (name, data) in &files {
        let to = into.join(name);
        if let Some(dir) = to.parent() {
            std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        }
        std::fs::write(&to, data).map_err(|e| format!("{}: {e}", to.display()))?;
    }
    Ok(into.join(project))
}

/// Render a song offline into interleaved stereo at `sample_rate`. In song mode the whole playlist is rendered;
/// otherwise `pattern` once. `tail` seconds are added for sounds still ringing. No audio device is touched.
pub fn render_song(song: &Song, sample_rate: f32, song_mode: bool, pattern: usize, tail: f32) -> Vec<f32> {
    render_part(song, sample_rate, song_mode, pattern, tail, None)
}

/// Render the song and keep only what one mixer track puts out after its fader.
pub fn render_track(song: &Song, sample_rate: f32, song_mode: bool, pattern: usize, tail: f32, insert: usize) -> Vec<f32> {
    render_part(song, sample_rate, song_mode, pattern, tail, Some(insert))
}

fn render_part(song: &Song, sample_rate: f32, song_mode: bool, pattern: usize, tail: f32, stem: Option<usize>) -> Vec<f32> {
    let job = RenderJob { rate: sample_rate, song_mode, pattern, tail, range: None, stems: false, stem, no_inserts: false, no_master: false };
    render_job(song, &job).mix
}

/// What to render: the song or one pattern, perhaps only a stretch of it (in ticks), with every mixer track kept apart if asked.
#[derive(Clone, Debug, Default)]
pub struct RenderJob {
    pub rate: f32,
    pub song_mode: bool,
    pub pattern: usize,
    /// Seconds allowed for sounds still ringing after the end.
    pub tail: f32,
    pub range: Option<(u32, u32)>,
    pub stems: bool,
    pub stem: Option<usize>,
    /// Leave the effect slots of the mixer tracks out.
    pub no_inserts: bool,
    /// Leave the effect slots of the master track out.
    pub no_master: bool,
}

/// A finished render: interleaved stereo, the frames before the tail, and each sounding mixer track's own sound.
pub struct Rendered {
    pub mix: Vec<f32>,
    pub main: usize,
    pub stems: Vec<(usize, Vec<f32>)>,
}

impl Rendered {
    /// Fold the tail back over the start, so the file loops without a gap; every part comes out `main` frames long.
    pub fn wrap(&mut self) {
        let main = self.main;
        let fold = |v: &mut Vec<f32>| {
            if main == 0 {
                return;
            }
            for k in main * 2..v.len() {
                let x = v[k];
                v[(k - main * 2) % (main * 2)] += x;
            }
            v.truncate(main * 2);
        };
        fold(&mut self.mix);
        for s in self.stems.iter_mut() {
            fold(&mut s.1);
        }
    }
}

pub fn render_job(song: &Song, job: &RenderJob) -> Rendered {
    let (sample_rate, song_mode, pattern, tail, stem) = (job.rate, job.song_mode, job.pattern, job.tail, job.stem);
    // hosted plug-ins are lent to the render and handed back as they were
    let hosted: Vec<_> = song.hosted().into_iter().map(|i| (i.rate(), i)).collect();
    for (_, i) in &hosted {
        i.set_offline(true);
        i.activate(sample_rate);
    }
    let mut played = song.clone();
    for (i, ins) in played.inserts.iter_mut().enumerate() {
        if (i == 0 && job.no_master) || (i != 0 && job.no_inserts) {
            ins.fx_on = false;
        }
    }
    let (h, mut p) = new_player(played, sample_rate);
    p.set_offline(true);
    p.stem = stem;
    p.stem_all = job.stems;
    h.transport.set_range(job.range);
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
    let main = out.len() / 2;
    let mut extra = 0;
    while p.busy() && extra < max_tail {
        let at = out.len();
        out.resize(at + 2 * 1024, 0.0);
        p.render(&mut out[at..], 2);
        extra += 1024;
    }
    for (was, i) in &hosted {
        i.set_offline(false);
        if let Some(r) = was {
            i.activate(*r);
        }
    }
    let frames = out.len() / 2;
    if stem.is_some() {
        p.stem_buf.resize(frames, [0.0; 2]);
        out = p.stem_buf.iter().flat_map(|f| [f[0], f[1]]).collect();
    }
    let stems = std::mem::take(&mut p.stems)
        .into_iter()
        .enumerate()
        .filter(|(_, v)| v.iter().any(|f| f[0] != 0.0 || f[1] != 0.0))
        .map(|(i, mut v)| {
            v.resize(frames, [0.0; 2]);
            (i, v.iter().flat_map(|f| [f[0], f[1]]).collect())
        })
        .collect();
    Rendered { mix: out, main, stems }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    #[test]
    fn a_render_job_keeps_tracks_apart_and_wraps_the_tail() {
        let mut r = Rendered { mix: vec![1.0, 1.0, 0.0, 0.0, 0.5, 0.25], main: 2, stems: vec![(1, vec![0.0, 0.0, 0.0, 0.0, 1.0, 1.0])] };
        r.wrap();
        assert_eq!(r.mix, vec![1.5, 1.25, 0.0, 0.0]);
        assert_eq!(r.stems[0].1, vec![1.0, 1.0, 0.0, 0.0]);
        // an empty song: nothing sounds, so no track is kept, and a picked stretch sets the length
        let song = Song::default();
        let ppq = song.ppq;
        let job = RenderJob { rate: 44_100.0, song_mode: true, pattern: 0, tail: 0.0, range: Some((0, ppq * 4)), stems: true, stem: None, no_inserts: false, no_master: false };
        let done = render_job(&song, &job);
        assert!(done.stems.is_empty());
        let want = (4.0 * 60.0 / song.tempo as f64 * 44_100.0) as usize;
        assert!(done.main >= want && done.main < want + 2048, "{} vs {want}", done.main);
    }

    #[test]
    fn a_track_render_holds_only_that_track() {
        let mut s = Song::new();
        let data: Vec<f32> = (0..24_000).map(|i| (i as f32 * 0.05).sin() * 0.5).collect();
        s.channels[0].sample = Some(Arc::new(Sample::from_mono("tone", 48_000, &data)));
        s.channels[0].insert = 1;
        s.patterns[0].toggle_step(0, 0);
        s.prepare();
        let whole = render_song(&s, 48_000.0, false, 0, 0.5);
        let one = render_track(&s, 48_000.0, false, 0, 0.5, 1);
        let two = render_track(&s, 48_000.0, false, 0, 0.5, 2);
        assert_eq!(whole.len(), one.len());
        assert_eq!(whole.len(), two.len());
        let peak = |b: &[f32]| b.iter().fold(0f32, |a, v| a.max(v.abs()));
        assert!(peak(&one) > 0.1, "the track the channel plays into is heard");
        assert!(peak(&two) == 0.0, "another track is silent");
        // all tracks in one pass: the same sound, and only the tracks that sounded
        let all = render_job(&s, &RenderJob { rate: 48_000.0, song_mode: false, pattern: 0, tail: 0.5, range: None, stems: true, stem: None, no_inserts: false, no_master: false });
        let got: Vec<usize> = all.stems.iter().map(|t| t.0).collect();
        assert_eq!(got, vec![0, 1]);
        assert_eq!(all.stems[1].1, one);
    }

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
