//! Write a copy of a project with channel settings changed: `chset in.flp out.flp chan cut=49 cutby=49 gate=2`
//! (the cut group, the group it is cut by, and the gate length in steps).
fn main() {
    let a: Vec<String> = std::env::args().collect();
    let mut lib = cereal_engine::Library::new();
    let (mut s, flp) = cereal_engine::load_flp(std::path::Path::new(&a[1]), &mut lib).unwrap();
    let f = &mut s.channels[a[3].parse::<usize>().unwrap()].smp;
    for kv in &a[4..] {
        let (k, v) = kv.split_once('=').expect("key=value");
        match k {
            "cut" => f.cut = v.parse().unwrap(),
            "cutby" => f.cut_by = v.parse().unwrap(),
            "gate" => f.gate_steps = v.parse().unwrap(),
            "mono" => f.mono = v != "0",
            "porta" => f.porta = v != "0",
            _ => panic!("unknown setting {k}"),
        }
    }
    std::fs::write(&a[2], cereal_engine::save::to_bytes(&s, Some(&flp)).unwrap()).unwrap();
}
