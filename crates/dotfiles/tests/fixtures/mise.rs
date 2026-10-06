//! A stand-in for mise that appends each invocation's arguments, one line each, to the file `MISE_FIXTURE_LOG` names.

use std::io::Write;

fn main() {
    let log = std::env::var_os("MISE_FIXTURE_LOG").expect("the log file");
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(log)
        .expect("the log opens");
    let arguments: Vec<String> = std::env::args().skip(1).collect();
    writeln!(file, "{}", arguments.join(" ")).expect("the log takes a line");
}
