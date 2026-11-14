//! Copy one channel's instrument block into a channel of another project:
//! graft from.flp from_channel into.flp into_channel out.flp
fn main() {
    let a: Vec<String> = std::env::args().collect();
    let mut lib = cereal_engine::Library::new();
    let (from, _) = cereal_engine::load_flp(std::path::Path::new(&a[1]), &mut lib).unwrap();
    let (mut s, flp) = cereal_engine::load_flp(std::path::Path::new(&a[3]), &mut lib).unwrap();
    let raw = from.channels[a[2].parse::<usize>().unwrap()].gen.as_ref().expect("an instrument").raw.clone();
    let g = s.channels[a[4].parse::<usize>().unwrap()].gen.as_mut().expect("an instrument");
    g.fm = cereal_engine::fm::Patch::parse(&raw).map(std::sync::Arc::new);
    g.raw = raw;
    std::fs::write(&a[5], cereal_engine::save::to_bytes(&s, Some(&flp)).unwrap()).unwrap();
}
