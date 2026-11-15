//! Print a stretch of an operator synth block as bytes: fmhex song.flp channel from to
fn main() {
    let a: Vec<String> = std::env::args().collect();
    let mut lib = cereal_engine::Library::new();
    let (s, _) = cereal_engine::load_flp(std::path::Path::new(&a[1]), &mut lib).unwrap();
    let g = s.channels[a[2].parse::<usize>().unwrap()].gen.as_ref().expect("an instrument");
    let d = cereal_engine::fm::Patch::unpack(&g.raw).expect("an operator synth block");
    if let Some(v) = a[3].strip_prefix("f=") {
        // where a number sits, read as a float at any byte
        let v: f32 = v.parse().unwrap();
        let hits: Vec<usize> = (0..d.len().saturating_sub(3)).filter(|&i| (f32::from_le_bytes(d[i..i + 4].try_into().unwrap()) - v).abs() < 2e-4).collect();
        println!("{hits:?}");
        return;
    }
    if let Some(v) = a[3].strip_prefix("i=") {
        let v: i32 = v.parse().unwrap();
        let hits: Vec<usize> = (0..d.len().saturating_sub(3)).filter(|&i| i32::from_le_bytes(d[i..i + 4].try_into().unwrap()) == v).collect();
        println!("{hits:?}");
        return;
    }
    let (from, to): (usize, usize) = (a[3].parse().unwrap(), a[4].parse().unwrap());
    for (k, row) in d[from..to.min(d.len())].chunks(16).enumerate() {
        println!("{:5} {}", from + k * 16, row.iter().map(|b| format!("{b:3}")).collect::<Vec<_>>().join(" "));
    }
}
