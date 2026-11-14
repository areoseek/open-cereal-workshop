//! Write a copy of a project with channel 0 sent to mixer track 1 and that track's equaliser set:
//! `inseq in.flp out.flp band:db:place:width ...` (band 0..2; place and width as 0..65536).
fn main() {
    let a: Vec<String> = std::env::args().collect();
    let mut lib = cereal_engine::Library::new();
    let (mut s, flp) = cereal_engine::load_flp(std::path::Path::new(&a[1]), &mut lib).unwrap();
    s.channels[0].insert = 1;
    for spec in &a[3..] {
        let v: Vec<f32> = spec.split(':').map(|x| x.parse().expect("number")).collect();
        let b = &mut s.inserts[1].eq[v[0] as usize];
        b.gain_db = v[1];
        b.freq = v[2] / 65536.0;
        b.q = v[3] / 65536.0;
    }
    std::fs::write(&a[2], cereal_engine::save::to_bytes(&s, Some(&flp)).unwrap()).unwrap();
}
