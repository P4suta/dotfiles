//! A stand-in for lefthook that fails to load its configuration.

fn main() {
    if std::env::args().nth(1).as_deref() == Some("dump") {
        eprintln!("fixture: invalid lefthook configuration");
        std::process::exit(1);
    }
}
