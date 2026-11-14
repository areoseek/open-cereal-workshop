//! Print whole numbers of an operator synth block: fmpeek song.flp channel offset ...   (b1085 reads one byte)
fn main() {
    let a: Vec<String> = std::env::args().collect();
    let mut lib = cereal_engine::Library::new();
    let (s, _) = cereal_engine::load_flp(std::path::Path::new(&a[1]), &mut lib).unwrap();
    let g = s.channels[a[2].parse::<usize>().unwrap()].gen.as_ref().expect("an instrument");
    let d = cereal_engine::fm::Patch::unpack(&g.raw).expect("an operator synth block");
    let was = cereal_engine::fm::Patch::parse(&g.raw).expect("an operator synth block");
    // F1+8 counts from the first filter's settings, O2+4 from the second operator's
    let place = |o: &str| -> String {
        match o.split_once('+') {
            Some((b, off)) if b.starts_with('F') || b.starts_with('O') => {
                let at = if b.starts_with('F') { &was.filt_at[..] } else { &was.op_at[..] };
                (at[b[1..].parse::<usize>().unwrap() - 1] + off.parse::<usize>().unwrap()).to_string()
            }
            _ => o.to_string(),
        }
    };
    let asked: Vec<String> = a[3..].iter().map(|o| match o.strip_prefix('b') {
        Some(o) => format!("b{}", place(o)),
        None => place(o),
    }).collect();
    let row: Vec<String> = asked
        .iter()
        .map(|o| match o.strip_prefix('b') {
            Some(o) => format!("{o}:{}", d[o.parse::<usize>().unwrap()]),
            None => format!("{o}:{}", i32::from_le_bytes(d[o.parse::<usize>().unwrap()..][..4].try_into().unwrap())),
        })
        .collect();
    println!("{}", row.join(" "));
}
