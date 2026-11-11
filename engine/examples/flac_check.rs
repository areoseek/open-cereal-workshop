//! Re-encode any audio file our reader takes as FLAC, to check the writer against other decoders.
fn main() {
    let a: Vec<String> = std::env::args().collect();
    let s = cereal_engine::Sample::load(std::path::Path::new(&a[1])).expect("load");
    let flat: Vec<f32> = s.frames.iter().flat_map(|f| [f[0], f[1]]).collect();
    let bits = a.get(3).and_then(|b| b.parse().ok()).unwrap_or(16);
    let f = std::fs::File::create(&a[2]).expect("create");
    cereal_engine::flac::write(std::io::BufWriter::new(f), &flat, s.rate, 2, bits).expect("write");
}
