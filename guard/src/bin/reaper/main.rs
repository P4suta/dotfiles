//! reaper — the two launchd maintenance agents, one binary.
//!
//! Installed as both `~/.local/bin/dev-reaper` and `~/.local/bin/session-reaper` (the same file, busybox-style), so the launchd plists that invoke those names never notice the migration from bash.
//! Invoked as `reaper` it wants the agent named explicitly: `reaper dev|session [args…]`.
//!
//! Both agents are ports of their bash predecessors, behavior for behavior — including the macOS workarounds the originals documented at length, which mostly evaporate here: RFC 3339 timestamps are parsed instead of routed through `date -j -f`, the single-instance lock is still an atomic `mkdir` (there is no `flock(1)`, and a lock directory works everywhere), and the activity map is simply a `HashMap` now that the language is not bash 3.2.

use devreaper::DevReaper;
use sessionreaper::SessionReaper;

mod config;
mod devreaper;
mod sessionreaper;
mod util;

fn main() -> std::process::ExitCode {
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
