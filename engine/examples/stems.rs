//! Splits a sound file into its four parts: stems <in> <out folder> [weights folder]
use cereal_engine::{sample::Sample, stems, wav};
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicUsize};

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let dir = args.get(3).cloned().or_else(|| std::env::var("CEREAL_STEM_MODELS").ok()).expect("weights folder");
    let nets: Vec<stems::Net> =
        stems::FILES.iter().map(|f| stems::Net::read(&std::fs::read(Path::new(&dir).join(f)).expect("weight file")).expect("weights")).collect();
    let nets: [stems::Net; 4] = nets.try_into().ok().unwrap();
    let s = Sample::load(Path::new(&args[1])).expect("sample");
    let t = std::time::Instant::now();
    let (done, total, stop) = (AtomicUsize::new(0), AtomicUsize::new(0), AtomicBool::new(false));
    let parts = stems::split(&s, &nets, &done, &total, &stop).unwrap();
    eprintln!("{:.1} s of sound in {:.1} s", s.seconds(), t.elapsed().as_secs_f32());
    std::fs::create_dir_all(&args[2]).unwrap();
    for (p, name) in parts.iter().zip(stems::PARTS) {
        let flat: Vec<f32> = p.frames.iter().flatten().copied().collect();
        let f = std::fs::File::create(Path::new(&args[2]).join(format!("{name}.wav"))).unwrap();
        wav::write(std::io::BufWriter::new(f), &flat, p.rate, 2, 16).unwrap();
    }
}
