//! Print each channel's polyphony settings: chanpoly song.flp
fn main() {
    let a: Vec<String> = std::env::args().collect();
    let flp = cereal_engine::flp::parse(&std::fs::read(&a[1]).unwrap()).unwrap();
    for c in &flp.channels {
        println!("chan {} poly {:?} full_porta {}", c.index, c.poly, c.full_porta);
    }
}
