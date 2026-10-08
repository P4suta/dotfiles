use std::fs;
use std::path::Path;

#[path = "support/fixture_process.rs"]
mod fixture_process;

#[path = "../../dotguard/tests/support/fixture_git.rs"]
mod fixture_git;

#[test]
fn a_fixture_refuses_the_machine_incident_store_before_starting_a_process() {
    let home = platform::home::home().expect("the machine home");
    let state = home.join(fixture_process::MACHINE_STATE);
    if state.is_dir() {
        assert!(std::panic::catch_unwind(|| fixture_process::command("git", &state)).is_err());
    }
    assert!(
        std::panic::catch_unwind(|| { fixture_process::command("git", &std::env::temp_dir()) })
            .is_err()
    );
}

#[test]
fn fixture_git_cannot_inherit_another_incident_store() {
    let fixture = tempfile::tempdir().expect("fixture");
    let global = fixture.path().join("gitconfig");
    let ordinary = fixture_git::command("git", &global);
    let command = fixture_git::inheriting(
        "git",
        &global,
        [("SKILL_OPS_STATE".into(), "another-store".into())],
    );
    let state = command
        .get_envs()
        .find_map(|(name, value)| (name == fixture_process::INCIDENT_STATE).then_some(value))
        .flatten()
        .expect("fixture incident store");
    assert_eq!(
        Path::new(state),
        fixture.path().canonicalize().unwrap().join("state")
    );
    assert!(
        ordinary
            .get_envs()
            .any(|(name, value)| name == fixture_process::INCIDENT_STATE && value == Some(state))
    );
}

fn bypasses(source: &str) -> Vec<&str> {
    if !source.contains(concat!("CARGO_BIN_", "EXE_")) {
        return Vec::new();
    }
    source
        .lines()
        .filter(|line| {
            [
                concat!("process::", "command("),
                concat!("tool::", "external("),
                concat!("Command::", "new("),
            ]
            .iter()
            .any(|constructor| {
                line.split_once(constructor)
                    .is_some_and(|(before, argument)| {
                        if before
                            .chars()
                            .last()
                            .is_some_and(|letter| letter.is_ascii_alphanumeric() || letter == '_')
                        {
                            return false;
                        }
                        let argument = argument.trim_start().trim_start_matches('&');
                        ["env!(", "binary)", "program)", "executable)"]
                            .iter()
                            .any(|start| argument.starts_with(start))
                    })
            })
        })
        .collect()
}

#[test]
fn the_check_refuses_direct_and_aliased_managed_processes() {
    let binary = concat!("env!(\"CARGO_BIN_", "EXE_dotfiles\")");
    for argument in [binary, "binary", "&binary", "program", "executable"] {
        let source = format!("let binary = {binary};\nplatform::process::command({argument})");
        assert_eq!(bypasses(&source).len(), 1);
    }
    assert!(bypasses(&format!("fixture_process::command({binary}, scope)")).is_empty());
    assert!(bypasses("platform::process::command(binary)").is_empty());
}

#[test]
fn managed_test_processes_use_the_fixture_state_constructor() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .unwrap();
    let mut suites = vec![root.join("xtask/tests")];
    for entry in fs::read_dir(root.join("crates")).unwrap() {
        let tests = entry.unwrap().path().join("tests");
        if tests.is_dir() {
            suites.push(tests);
        }
    }
    let mut found = Vec::new();
    for suite in suites {
        for entry in fs::read_dir(suite).unwrap() {
            let path = entry.unwrap().path();
            if path.extension().is_some_and(|extension| extension == "rs") {
                for line in bypasses(&fs::read_to_string(&path).unwrap()) {
                    found.push(format!("{}: {line}", path.display()));
                }
            }
        }
    }
    assert!(
        found.is_empty(),
        "use fixture_process::command:\n{}",
        found.join("\n")
    );
}
