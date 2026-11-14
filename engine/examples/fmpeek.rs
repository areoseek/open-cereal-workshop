//! Print whole numbers of an operator synth block: fmpeek song.flp channel offset ...   (b1085 reads one byte)
fn main() {
    let a: Vec<String> = std::env::args().collect();
    let mut lib = cereal_engine::Library::new();
    let (s, _) = cereal_engine::load_flp(std::path::Path::new(&a[1]), &mut lib).unwrap();
    let g = s.channels[a[2].parse::<usize>().unwrap()].gen.as_ref().expect("an instrument");
    let d = cereal_engine::fm::Patch::unpack(&g.raw).expect("an operator synth block");
    let row: Vec<String> = a[3..]
        .iter()
        .map(|o| match o.strip_prefix('b') {
            Some(o) => format!("{o}:{}", d[o.parse::<usize>().unwrap()]),
            None => format!("{o}:{}", i32::from_le_bytes(d[o.parse::<usize>().unwrap()..][..4].try_into().unwrap())),
        })
        .collect();
    println!("{}", row.join(" "));
}
