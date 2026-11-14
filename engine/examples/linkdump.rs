//! Every controller link of one project, with its mapping formula: `linkdump song.flp`.
fn main() {
    let path = std::env::args().nth(1).expect("file");
    let f = cereal_engine::flp::parse(&std::fs::read(&path).expect("read")).expect("parse");
    let base = cereal_engine::flp::mixer_base(f.version_tuple());
    for (k, l) in f.links.iter().enumerate() {
        let text = f.link_formulas.iter().find(|(i, _)| *i == k).map(|(_, t)| t.as_str()).unwrap_or("");
        let src = f.channels.get(l.source as usize).map(|c| c.name.clone()).unwrap_or_default();
        let raw = (l.dest >> 6) as i64 - base as i64;
        println!("{k:3} kind {} source {} {:?} extra {:#x} param {:#06x} dest {:#06x} (insert {} slot {}) formula {:?}", l.kind, l.source, src, l.extra, l.param, l.dest, raw, l.dest & 63, text);
    }
}
