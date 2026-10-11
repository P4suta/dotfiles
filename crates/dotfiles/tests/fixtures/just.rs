//! The install tasks record only successful fixture installations.

fn main() {
    let task = std::env::args().nth(1).unwrap();
    let binary = match task.as_str() {
        "install-review-guard" => "coderabbit",
        "install-pr-workflow" => "pr-workflow",
        "install-skill-ops" => "skill-ops",
        other => panic!("unexpected install task: {other}"),
    };
    let log = std::env::var("FIXTURE_HOOK_LOG").unwrap();
    let mut events = std::fs::read_to_string(&log).unwrap_or_default();
    events.push_str(&format!("{task}\n"));
    std::fs::write(log, events).unwrap();
    if std::env::var("FIXTURE_INSTALL_FAILURE").as_deref() == Ok(task.as_str()) {
        std::process::exit(1);
    }
    let home = std::env::var(if cfg!(windows) { "USERPROFILE" } else { "HOME" }).unwrap();
    let record = std::path::Path::new(&home)
        .join(".local/state/dotfiles/installed")
        .join(binary);
    std::fs::write(record, std::env::var("FIXTURE_BUILD_HASH").unwrap()).unwrap();
}
