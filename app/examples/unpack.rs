//! Unpack the content of a user's own installer: cargo run --release --example unpack -- <installer.exe> <dest>
#[path = "../src/unpack.rs"]
mod unpack;

fn main() {
    let mut a = std::env::args().skip(1);
    let (Some(src), Some(dest)) = (a.next(), a.next()) else { return eprintln!("usage: unpack <installer> <dest>") };
    let p = unpack::Progress::default();
    match unpack::unpack(src.as_ref(), dest.as_ref(), &p) {
        Ok(n) => println!("{n} files"),
        Err(e) => eprintln!("{e}"),
    }
}
