//! Write a copy of a project with whole numbers of one operator synth block changed:
//! fmset in.flp out.flp channel offset=value ...   (offsets into the unpacked block)
fn main() {
    let a: Vec<String> = std::env::args().collect();
    let mut lib = cereal_engine::Library::new();
    let (mut s, flp) = cereal_engine::load_flp(std::path::Path::new(&a[1]), &mut lib).unwrap();
    let n: usize = a[3].parse().unwrap();
    let sets: Vec<(usize, i32)> = a[4..].iter().map(|x| x.split_once('=').unwrap()).map(|(o, v)| (o.parse().unwrap(), v.parse().unwrap())).collect();
    let g = s.channels[n].gen.as_mut().expect("an instrument");
    let (raw, patch) = cereal_engine::fm::Patch::edited(&g.raw, |d| {
        for (o, v) in &sets {
            d[*o..*o + 4].copy_from_slice(&v.to_le_bytes());
        }
    })
    .expect("a block that still reads");
    g.raw = raw;
    g.fm = Some(std::sync::Arc::new(patch));
    std::fs::write(&a[2], cereal_engine::save::to_bytes(&s, Some(&flp)).unwrap()).unwrap();
}
