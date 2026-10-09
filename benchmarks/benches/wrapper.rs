//! The instruction and heap work of the gate's own process at the four entry points a Git call meets.
//! The tools follow no child, so the budgets cover the gate alone and not the Git it delegates to.

use gungraun::prelude::*;
use gungraun::{ExitWith, Pipe, Sandbox, Stdin};
use xtask::tool::Tool;

/// The status a refusal returns to the shell.
const PRE_PUSH_REFUSED: i32 = 1;

fn dotguard() -> std::path::PathBuf {
    std::env::var_os("DOTGUARD_BIN")
        .unwrap_or_else(|| panic!("DOTGUARD_BIN must name the release-guard dotguard binary"))
        .into()
}

/// A repository in the sandbox's working directory, so the status passthrough reaches the real Git.
fn git_init() {
    let status = Tool::Git
        .command()
        .args(["init", "--quiet", "--initial-branch", "main"])
        .status()
        .expect("start git");
    assert!(status.success(), "git init failed: {status}");
}

/// The pre-push payload of a push that rewrites a commit this clone lacks, which the gate refuses.
fn undecidable_push() {
    println!(
        "refs/heads/main {} refs/heads/main {}",
        "1".repeat(40),
        "2".repeat(40)
    );
}

#[binary_benchmark(
    config = bench_limits::wrapper("git_status").sandbox(Sandbox::new(true)),
    setup = git_init(),
)]
#[bench::passthrough()]
fn git_status() -> gungraun::Command {
    gungraun::Command::new(dotguard())
        .arg("git")
        .arg("status")
        .build()
}

#[binary_benchmark(
    config = bench_limits::wrapper("pre_push_denied")
        .exit_with(ExitWith::Code(PRE_PUSH_REFUSED)),
    setup = undecidable_push(),
)]
#[bench::undecidable_rewrite()]
fn pre_push_denied() -> gungraun::Command {
    gungraun::Command::new(dotguard())
        .arg("pre-push")
        .arg("origin")
        .stdin(Stdin::Setup(Pipe::Stdout))
        .build()
}

#[binary_benchmark(
    config = bench_limits::wrapper("commit_message_clean")
        .sandbox(Sandbox::new(true).fixtures([concat!(env!("CARGO_MANIFEST_DIR"), "/corpus")])),
)]
#[bench::clean()]
fn commit_message_clean() -> gungraun::Command {
    gungraun::Command::new(dotguard())
        .arg("commit-msg")
        .arg("corpus/clean-message.txt")
        .build()
}

#[binary_benchmark(
    config = bench_limits::wrapper("commit_message_attribution")
        .sandbox(Sandbox::new(true).fixtures([concat!(env!("CARGO_MANIFEST_DIR"), "/corpus")])),
)]
#[bench::attribution()]
fn commit_message_attribution() -> gungraun::Command {
    gungraun::Command::new(dotguard())
        .arg("commit-msg")
        .arg("corpus/commit-message.txt")
        .build()
}

binary_benchmark_group!(
    name = gate,
    benchmarks = [
        git_status,
        pre_push_denied,
        commit_message_clean,
        commit_message_attribution
    ]
);

main!(binary_benchmark_groups = gate);
