//! One binary for the two launchd maintenance agents.
//!
//! The same file installs as `~/.local/bin/dev-reaper` and `~/.local/bin/session-reaper`, and dispatches on the name that launchd invokes.
//! Invoked as `reaper`, it takes the agent name first: `reaper dev|session [args…]`.

use devreaper::DevReaper;
use sessionreaper::SessionReaper;

mod config;
mod devreaper;
mod sessionreaper;
mod util;

fn main() -> std::process::ExitCode {
    // The agents reap Unix processes through ps, grep, launchd, and systemd, which Windows lacks.
    if cfg!(not(unix)) {
        eprintln!("reaper maintains Unix processes and does not run on Windows");
        return std::process::ExitCode::from(2);
    }
    let argv0 = std::env::args().next().map_or_else(
        || "reaper".to_owned(),
        |a| a.rsplit('/').next().unwrap_or("reaper").to_owned(),
    );
    let args: Vec<String> = std::env::args().skip(1).collect();

    match argv0.as_str() {
        "dev-reaper" => code(DevReaper::dispatch(&args)),
        "session-reaper" => code(SessionReaper::dispatch(&args)),
        _ => {
            match (
                args.first().map(String::as_str),
                args.get(1).map(String::as_str),
            ) {
                (Some("dev"), Some(sub)) => code(DevReaper::run(sub)),
                (Some("session"), Some(sub)) => code(SessionReaper::run(sub)),
                _ => {
                    eprintln!(
                        "usage: reaper dev|session {{reap|cleanup}}  (or invoke as dev-reaper / session-reaper)"
                    );
                    code(2)
                }
            }
        }
    }
}

fn code(n: i32) -> std::process::ExitCode {
    std::process::ExitCode::from(u8::try_from(n).unwrap_or(1))
}
