//! Convert an FL Studio project to our own format:
//!   cargo run -p cereal-engine --example flp2cereal -- in.flp out.cereal
use cereal_engine::flp;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let (Some(inp), Some(outp)) = (args.first(), args.get(1)) else {
        eprintln!("usage: flp2cereal <in.flp> <out.cereal>");
        std::process::exit(2);
    };
    let bytes = std::fs::read(inp).expect("read .flp");
    let flp = flp::parse(&bytes).expect("parse .flp");
    let project = flp.to_project();
    std::fs::write(outp, project.to_text()).expect("write .cereal");
    println!(
        "{inp} -> {outp}: title={:?} bpm={} channels={} notes folded={}",
        project.title, project.bpm, project.channels.len(), flp.notes.len()
    );
}
