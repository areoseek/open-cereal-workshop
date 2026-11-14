//! Write a copy of a project with every channel switched off except the ones named:
//! solo in.flp out.flp channel ...
fn main() {
    let a: Vec<String> = std::env::args().collect();
    let mut lib = cereal_engine::Library::new();
    let (mut s, flp) = cereal_engine::load_flp(std::path::Path::new(&a[1]), &mut lib).unwrap();
    let keep: Vec<usize> = a[3..].iter().map(|x| x.parse().unwrap()).collect();
    for (i, c) in s.channels.iter_mut().enumerate() {
        c.enabled = keep.contains(&i);
    }
    std::fs::write(&a[2], cereal_engine::save::to_bytes(&s, Some(&flp)).unwrap()).unwrap();
}
