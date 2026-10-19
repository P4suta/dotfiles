use dotguard::{git_wrapper, postcommit, refusal::Refusal, renovate};
use std::process::ExitCode;

const USAGE_ERROR: u8 = 2;

fn main() -> ExitCode {
    let mut arguments = std::env::args().skip(1);
    if arguments.next().as_deref() != Some(platform::BUILD_HASH) {
        Refusal::new(
            "tool.stale",
            "the policy companion does not match the invoking Git guard",
            "chezmoi apply",
        )
        .evidence(
            "reapply the native runtime to install the guard and backend from the same sources",
        )
        .emit();
        return ExitCode::FAILURE;
    }
    let result = match arguments.next().as_deref() {
        Some("git") => return git_wrapper::run::<true>(&arguments.collect::<Vec<_>>()),
        Some("post-commit") => postcommit::run(),
        Some("renovate-gate") => renovate::run(arguments.next().as_deref().unwrap_or("origin")),
        Some("renovate") => renovate::dispatch(&arguments.collect::<Vec<_>>()),
        _ => return ExitCode::from(USAGE_ERROR),
    };
    ExitCode::from(u8::try_from(result).unwrap_or(1))
}
