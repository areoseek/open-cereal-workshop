//! Print a stretch of an operator synth block as bytes: fmhex song.flp channel from to
fn main() {
    let a: Vec<String> = std::env::args().collect();
    let mut lib = cereal_engine::Library::new();
    let (s, _) = cereal_engine::load_flp(std::path::Path::new(&a[1]), &mut lib).unwrap();
    let g = s.channels[a[2].parse::<usize>().unwrap()].gen.as_ref().expect("an instrument");
    let d = cereal_engine::fm::Patch::unpack(&g.raw).expect("an operator synth block");
    let (from, to): (usize, usize) = (a[3].parse().unwrap(), a[4].parse().unwrap());
    for (k, row) in d[from..to.min(d.len())].chunks(16).enumerate() {
        println!("{:5} {}", from + k * 16, row.iter().map(|b| format!("{b:3}")).collect::<Vec<_>>().join(" "));
    }
}
