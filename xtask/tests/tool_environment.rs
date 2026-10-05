use dotfiles_xtask::tool::{GIT_LOCAL_ENVIRONMENT, Tool};
use std::collections::BTreeSet;
use std::ffi::OsStr;

#[test]
fn the_cleared_variables_are_exactly_gits_repository_local_variables() {
    let output = Tool::Git
        .command()
        .args(["rev-parse", "--local-env-vars"])
        .output()
        .expect("run git");
    assert!(output.status.success());
    let listed: BTreeSet<String> = String::from_utf8(output.stdout)
        .expect("utf-8")
        .lines()
        .map(str::to_owned)
        .collect();
    let cleared: BTreeSet<String> = GIT_LOCAL_ENVIRONMENT.map(str::to_owned).into();
    assert_eq!(cleared, listed);
}

#[test]
fn tool_processes_never_inherit_a_hooks_repository() {
    let command = Tool::Cargo.command();
    let removed: BTreeSet<&OsStr> = command
        .get_envs()
        .filter(|(_, value)| value.is_none())
        .map(|(name, _)| name)
        .collect();
    for name in GIT_LOCAL_ENVIRONMENT {
        assert!(removed.contains(OsStr::new(name)), "{name} is inherited");
    }
}

#[test]
fn hook_gates_keep_the_commit_in_progress() {
    let command = Tool::Lefthook.hook_command();
    assert_eq!(command.get_envs().count(), 0);
}
