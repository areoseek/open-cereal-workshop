//! Render a project to a WAV file without touching an audio device.
//!
//!     cargo run --release -p cereal-engine --example render -- song.flp out.wav [sample folder ...]

use std::path::{Path, PathBuf};

use cereal_engine::{load_flp, render_song, wav, Library};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.len() < 2 {
        eprintln!("usage: render <project.flp> <out.wav> [sample folder ...]");
        std::process::exit(2);
    }
    let mut lib = Library::new();
    for dir in &args[2..] {
        lib.search_dirs.push(PathBuf::from(dir));
    }
    lib.rescan();
    let (song, _) = match load_flp(Path::new(&args[0]), &mut lib) {
        Ok(x) => x,
        Err(e) => {
            eprintln!("{e}");
            std::process::exit(1);
        }
    };
    let missing = song.channels.iter().filter(|c| !c.sample_path.is_empty() && c.sample.is_none()).count();
    let started = std::time::Instant::now();
    let rate = 44_100.0;
    let buf = render_song(&song, rate, true, 0, 4.0);
    let secs = buf.len() as f32 / 2.0 / rate;
    let peak = buf.iter().fold(0f32, |m, x| m.max(x.abs()));
    let rms = (buf.iter().map(|x| (x * x) as f64).sum::<f64>() / buf.len().max(1) as f64).sqrt();
    let bad = buf.iter().filter(|x| !x.is_finite()).count();
    let file = std::fs::File::create(&args[1]).expect("cannot create the output file");
    wav::write_pcm16(std::io::BufWriter::new(file), &buf, rate as u32, 2).expect("cannot write the output file");
    println!(
        "{:.1} s of audio in {:.1} s, peak {:.3}, rms {:.4}, {} bad samples, {} channels, {} missing samples",
        secs,
        started.elapsed().as_secs_f32(),
        peak,
        rms,
        bad,
        song.channels.len(),
        missing
    );
}
