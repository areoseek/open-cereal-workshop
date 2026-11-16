//! Write an instrument channel's stored block to a file: genraw song.flp channel out.bin
fn main() {
    let a: Vec<String> = std::env::args().collect();
    let mut lib = cereal_engine::Library::new();
    let (s, _) = cereal_engine::load_flp(std::path::Path::new(&a[1]), &mut lib).unwrap();
    let g = s.channels[a[2].parse::<usize>().unwrap()].gen.as_ref().expect("an instrument");
    std::fs::write(&a[3], &g.raw).unwrap();
    println!("{} bytes", g.raw.len());
}
