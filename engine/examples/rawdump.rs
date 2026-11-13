//! Write a sound file's frames as raw little-endian 32-bit floats (left channel), for measuring against.
fn main() {
    let a: Vec<String> = std::env::args().collect();
    let smp = cereal_engine::Sample::load(std::path::Path::new(&a[1])).expect("sound");
    let mut out = Vec::with_capacity(smp.frames.len() * 4);
    for f in &smp.frames {
        out.extend_from_slice(&f[0].to_le_bytes());
    }
    std::fs::write(&a[2], out).expect("write");
    println!("{} frames at {}", smp.frames.len(), smp.rate);
}
