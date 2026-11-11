//! Print the channels of a project with their kinds, for picking one in a test.
fn main() {
    let p = std::env::args().nth(1).expect("project file");
    let f = cereal_engine::flp::parse(&std::fs::read(&p).unwrap()).unwrap();
    for (i, c) in f.channels.iter().enumerate() {
        println!("{i} {:?} {} [{}]", c.kind, c.name, c.plugin);
    }
}
