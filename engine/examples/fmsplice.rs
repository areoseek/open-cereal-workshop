//! Build one operator synth block out of the parts of two, to find which part carries a behaviour:
//! fmsplice a.flp b.flp channel out.flp mask
//! The mask has ten letters, a or b, for: the fixed head, operators 1 to 6, filters 1 and 2, filter 3 with the rest.
fn main() {
    let a: Vec<String> = std::env::args().collect();
    let mut lib = cereal_engine::Library::new();
    let n: usize = a[3].parse().unwrap();
    let (mut s, flp) = cereal_engine::load_flp(std::path::Path::new(&a[1]), &mut lib).unwrap();
    let (other, _) = cereal_engine::load_flp(std::path::Path::new(&a[2]), &mut lib).unwrap();
    let cuts = |raw: &[u8]| -> (Vec<u8>, Vec<usize>) {
        let p = cereal_engine::fm::Patch::parse(raw).expect("an operator synth block");
        let d = cereal_engine::fm::Patch::unpack(raw).unwrap();
        let mut c = vec![0];
        c.extend_from_slice(&p.op_at);
        c.extend_from_slice(&p.filt_at);
        c.push(d.len());
        (d, c)
    };
    let g = s.channels[n].gen.as_mut().expect("an instrument");
    let (da, ca) = cuts(&g.raw);
    let (db, cb) = cuts(&other.channels[n].gen.as_ref().expect("an instrument").raw);
    let mut out = vec![];
    for (k, m) in a[5].chars().enumerate() {
        let (d, c) = if m == 'a' { (&da, &ca) } else { (&db, &cb) };
        out.extend_from_slice(&d[c[k]..c[k + 1]]);
    }
    let (raw, patch) = cereal_engine::fm::Patch::edited(&g.raw, |d| *d = out).expect("a block that still reads");
    g.raw = raw;
    g.fm = Some(std::sync::Arc::new(patch));
    std::fs::write(&a[4], cereal_engine::save::to_bytes(&s, Some(&flp)).unwrap()).unwrap();
}
