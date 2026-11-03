//! A stand-in for mise that appends each invocation's arguments, one line each, to the file `MISE_FIXTURE_LOG` names.
//! `trust --show` prints one section of the file `MISE_FIXTURE_TRUST_REPORT` names, with sections separated by `---` lines.
//! The nth request prints the nth section, and later requests repeat the last one.

use std::io::Write;

fn main() {
    let log = std::env::var_os("MISE_FIXTURE_LOG").expect("the log file");
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log)
        .expect("the log opens");
    let arguments: Vec<String> = std::env::args().skip(1).collect();
    writeln!(file, "{}", arguments.join(" ")).expect("the log takes a line");
    if arguments.iter().any(|argument| argument == "--show")
        && let Some(report) = std::env::var_os("MISE_FIXTURE_TRUST_REPORT")
    {
        let requests = std::fs::read_to_string(&log)
            .expect("the log reads")
            .lines()
            .filter(|line| line.ends_with("--show"))
            .count();
        let report = std::fs::read_to_string(report).expect("the trust report reads");
        let sections: Vec<&str> = report.split("---\n").collect();
        let section = sections
            .get(requests.saturating_sub(1))
            .or(sections.last())
            .expect("the trust report has a section");
        print!("{section}");
    }
}
