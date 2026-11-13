//! Render a project file to a WAV without opening the app.
//!
//!   cargo run --release -p cereal-engine --example flp_render -- song.flp out.wav [--pattern N] [--track N] [--tail seconds] [--dry]
//!
//! Samples are looked up through CEREAL_FACTORY_DATA and CEREAL_SAMPLE_DIRS.

use std::path::Path;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.len() < 2 {
        eprintln!("usage: flp_render <in.flp> <out.wav> [--pattern N]");
        std::process::exit(2);
    }
    let pattern = args.iter().position(|a| a == "--pattern").and_then(|i| args.get(i + 1)).and_then(|v| v.parse::<usize>().ok());
    let mut lib = cereal_engine::Library::new();
    let (song, flp) = match cereal_engine::load_flp(Path::new(&args[0]), &mut lib) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("{e}");
            std::process::exit(1);
        }
    };
    // --dry renders with every effect slot switched off, for comparing a unit against its input
    let mut song = song;
    if args.iter().any(|a| a == "--dry") {
        for ins in song.inserts.iter_mut() {
            for s in ins.slots.iter_mut() {
                s.enabled = false;
            }
        }
    }
    let loaded = song.channels.iter().filter(|c| c.sample.is_some()).count();
    eprintln!("{} bpm, {} channels ({} with samples), {} patterns, {} clips, {} bars", song.tempo, song.channels.len(), loaded, song.patterns.len(), song.clips.len(), song.length() / song.bar());
    for m in &lib.missing {
        eprintln!("missing: {m}");
    }
    let sr = 44_100.0;
    let track = args.iter().position(|a| a == "--track").and_then(|i| args.get(i + 1)).and_then(|v| v.parse::<usize>().ok());
    let tail = args.iter().position(|a| a == "--tail").and_then(|i| args.get(i + 1)).and_then(|v| v.parse::<f32>().ok()).unwrap_or(4.0);
    let buf = match (pattern, track) {
        (Some(p), None) => cereal_engine::render_song(&song, sr, false, p.saturating_sub(1), tail),
        // an empty playlist renders the pattern, as the original does
        (None, None) if song.clips.is_empty() => cereal_engine::render_song(&song, sr, false, 0, tail),
        // saved in pattern mode with nothing to hear in the playlist: the pattern that was open
        (None, None) if !song.song_mode && song.clips.iter().all(|c| matches!(c.src, cereal_engine::song::ClipSrc::Pattern(p) if song.patterns.get(p).is_none_or(|p| p.notes.is_empty()))) => {
            cereal_engine::render_song(&song, sr, false, (flp.current_pattern as usize).saturating_sub(1), tail)
        }
        (None, None) => cereal_engine::render_song(&song, sr, true, 0, tail),
        (p, Some(t)) => cereal_engine::render_track(&song, sr, p.is_none(), p.unwrap_or(1).saturating_sub(1), tail, t),
    };
    let peak = buf.iter().fold(0f32, |m, x| m.max(x.abs()));
    let rms = (buf.iter().map(|x| (*x as f64).powi(2)).sum::<f64>() / buf.len().max(1) as f64).sqrt();
    eprintln!("{:.1} s, peak {:.3}, rms {:.4}", buf.len() as f32 / 2.0 / sr, peak, rms);
    let file = std::fs::File::create(&args[1]).expect("create output");
    cereal_engine::wav::write_pcm16(std::io::BufWriter::new(file), &buf, sr as u32, 2).expect("write wav");
}
