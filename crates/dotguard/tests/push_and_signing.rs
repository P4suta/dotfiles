//! The pre-push and post-commit gates in real repositories: each refusal reaches standard error as one complete record.

use dotguard::refusal::Refusal;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

#[path = "support/fixture_git.rs"]
mod fixture_git;

const ZERO: &str = "0000000000000000000000000000000000000000";
const RENOVATE_MANIFEST: &str = "[dependencies]\nfixture = \"1.0.1\"\n";
const RENOVATE_REMOTE: &str = "https://example.invalid/fixture/repository.git";
const MISE_BACKENDS: [(&str, &str, &str, &str); 4] = [
    ("nu", "github-releases", "nushell/nushell", "0.116.0"),
    ("conda:zsh", "conda", "conda-forge/zsh", "5.9"),
    ("pypi:ansible-core", "pypi", "ansible-core", "2.21.4"),
    ("cargo-deny", "crate", "cargo-deny", "0.20.2"),
];
const RENOVATE_MISE_MANIFEST: &str = "[tools]\nnu = \"0.116.0\"\n";
const MISE_CI_EXPRESSION: &str = "${{ steps.mise.outputs.version }}";
const MISE_LITERAL_FILE: &str = "home/.chezmoidata/versions.toml";
const MISE_LITERAL_MANIFEST: &str =
    "# renovate: datasource=github-releases depName=jdx/mise\nversion = \"2026.10.0\"\n";

struct Repository {
    scope: PathBuf,
}

impl Drop for Repository {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.scope);
    }
}

impl Repository {
    fn new(name: &str) -> Self {
        let scope =
            std::env::temp_dir().join(format!("dotguard-gates-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&scope);
        std::fs::create_dir_all(scope.join("home")).unwrap();
        std::fs::write(
            scope.join("gitconfig"),
            "[user]\n\tname = Fixture\n\temail = fixture@example.invalid\n",
        )
        .unwrap();
        let repository = Self { scope };
        repository.git(&["init", "-q", "-b", "main"]);
        repository
    }

    fn path(&self) -> PathBuf {
        self.scope.join("home")
    }

    fn command(&self, program: impl AsRef<std::ffi::OsStr>) -> Command {
        let mut command = fixture_git::command(program, &self.scope.join("gitconfig"));
        for (name, _) in std::env::vars_os() {
            let text = name.to_string_lossy();
            if text.starts_with("ALLOW_") || text.starts_with("DOTGUARD_") {
                command.env_remove(name);
            }
        }
        command
            .current_dir(self.path())
            .env("HOME", self.path())
            .env("USERPROFILE", self.path());
        command
    }

    fn git(&self, arguments: &[&str]) -> String {
        let output = self.command("git").args(arguments).output().unwrap();
        assert!(
            output.status.success(),
            "git {arguments:?}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8(output.stdout).unwrap().trim().to_owned()
    }

    /// Records an unsigned commit with plumbing, so no hook runs before the gate under test.
    /// Only the gate has signing configured, so the fixture commits stay unsigned.
    fn commit(&self, file: &str, parents: &[&str]) -> String {
        std::fs::write(self.path().join(file), file).unwrap();
        self.git(&["add", file]);
        let tree = self.git(&["write-tree"]);
        let mut arguments = vec!["commit-tree", &tree, "-m", file];
        for parent in parents {
            arguments.extend(["-p", parent]);
        }
        self.git(&arguments)
    }

    /// Runs the gate with `config` added to the repository's configuration through the environment.
    fn dotguard(&self, arguments: &[&str], input: &str, config: &[(&str, &str)]) -> Output {
        self.dotguard_with(arguments, input, config, &[])
    }

    fn dotguard_with(
        &self,
        arguments: &[&str],
        input: &str,
        config: &[(&str, &str)],
        variables: &[(&str, &str)],
    ) -> Output {
        self.binary_with(
            Path::new(env!("CARGO_BIN_EXE_dotguard")),
            arguments,
            input,
            config,
            variables,
        )
    }

    fn binary_with(
        &self,
        binary: &Path,
        arguments: &[&str],
        input: &str,
        config: &[(&str, &str)],
        variables: &[(&str, &str)],
    ) -> Output {
        let mut command = self.command(binary);
        command
            .envs(variables.iter().copied())
            .env("GIT_CONFIG_COUNT", config.len().to_string());
        for (index, (key, value)) in config.iter().enumerate() {
            command
                .env(format!("GIT_CONFIG_KEY_{index}"), key)
                .env(format!("GIT_CONFIG_VALUE_{index}"), value);
        }
        let mut child = command
            .args(arguments)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        child
            .stdin
            .take()
            .unwrap()
            .write_all(input.as_bytes())
            .unwrap();
        child.wait_with_output().unwrap()
    }
}

/// The record a refused gate printed last on standard error.
fn refusal(output: &Output) -> Refusal {
    assert_eq!(output.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&output.stderr);
    let refusal = Refusal::find(&stderr).expect("a structured refusal");
    assert!(refusal.is_complete(), "{refusal:?}");
    assert!(
        stderr.trim_end().ends_with(&refusal.line()),
        "the record must be the last line: {stderr}"
    );
    refusal
}

/// Signing required, as the pre-push gate reads it.
const SIGNED: [(&str, &str); 1] = [("commit.gpgsign", "true")];

/// A repository whose `origin` already carries `base`.
fn published(name: &str) -> (Repository, String) {
    let repository = Repository::new(name);
    let base = repository.commit("base", &[]);
    repository.git(&["update-ref", "refs/remotes/origin/main", &base]);
    (repository, base)
}

fn renovate_fixture(name: &str) -> (Repository, String, String) {
    let repository = Repository::new(name);
    let tools = repository.path().join(".local/bin");
    std::fs::create_dir_all(&tools).unwrap();
    let docker = tools.join(format!("docker{}", std::env::consts::EXE_SUFFIX));
    let compiler = fixture_git::command("rustc", &repository.scope.join("gitconfig"))
        .args(["--edition=2024", "--crate-name", "renovate_fixture"])
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/fixtures/renovate_tool.rs"
        ))
        .arg("-o")
        .arg(&docker)
        .output()
        .unwrap();
    assert!(
        compiler.status.success(),
        "{}",
        String::from_utf8_lossy(&compiler.stderr)
    );
    std::fs::copy(
        &docker,
        tools.join(format!("id{}", std::env::consts::EXE_SUFFIX)),
    )
    .unwrap();
    let base = repository.commit("base", &[]);
    std::fs::write(repository.path().join("Cargo.toml"), RENOVATE_MANIFEST).unwrap();
    std::fs::write(repository.path().join("mise.toml"), RENOVATE_MISE_MANIFEST).unwrap();
    let literal_file = repository.path().join(MISE_LITERAL_FILE);
    std::fs::create_dir_all(literal_file.parent().unwrap()).unwrap();
    std::fs::write(literal_file, MISE_LITERAL_MANIFEST).unwrap();
    repository.git(&["add", "Cargo.toml", "mise.toml", MISE_LITERAL_FILE]);
    let tree = repository.git(&["write-tree"]);
    let offered = repository.git(&["commit-tree", &tree, "-p", &base, "-m", "dependency"]);
    repository.git(&["update-ref", "HEAD", &offered]);
    (repository, base, offered)
}

fn renovate_variables(repository: &Repository) -> Vec<(&'static str, String)> {
    vec![
        (
            "RENOVATE_FIXTURE_ARGS",
            repository
                .scope
                .join("arguments")
                .to_string_lossy()
                .into_owned(),
        ),
        ("RENOVATE_FIXTURE_MANIFEST", RENOVATE_MANIFEST.to_owned()),
        (
            "RENOVATE_LOCAL_REPORT_FILE",
            repository
                .scope
                .join("manual-report.json")
                .to_string_lossy()
                .into_owned(),
        ),
        ("RENOVATE_LOCAL_SKIP", String::new()),
        ("RENOVATE_GITHUB_COM_TOKEN", String::new()),
    ]
}

fn copied_guard(repository: &Repository) -> PathBuf {
    let directory = repository.scope.join("guard");
    std::fs::create_dir_all(&directory).unwrap();
    let guard = directory.join(format!("dotguard{}", std::env::consts::EXE_SUFFIX));
    std::fs::copy(env!("CARGO_BIN_EXE_dotguard"), &guard).unwrap();
    guard
}

#[test]
fn renovate_backend_requires_its_executable_sibling_and_never_uses_a_path_fallback() {
    let (repository, _, _) = renovate_fixture("renovate-missing-backend");
    let guard = copied_guard(&repository);
    let backend_name = format!(
        "{}{}",
        dotguard::RENOVATE_BINARY,
        std::env::consts::EXE_SUFFIX
    );
    std::fs::copy(
        env!("CARGO_BIN_EXE_dotguard-renovate"),
        repository.path().join(".local/bin").join(&backend_name),
    )
    .unwrap();
    let variables = renovate_variables(&repository);
    let values: Vec<_> = variables
        .iter()
        .map(|(name, value)| (*name, value.as_str()))
        .collect();
    let missing = repository.binary_with(&guard, &["renovate", "run"], "", &[], &values);
    assert_eq!(refusal(&missing).rule, "tool.failure");
    assert!(!repository.scope.join("arguments").exists());

    std::fs::create_dir(guard.with_file_name(backend_name)).unwrap();
    let unstartable = repository.binary_with(&guard, &["renovate", "run"], "", &[], &values);
    assert_eq!(refusal(&unstartable).rule, "tool.failure");
    assert!(!repository.scope.join("arguments").exists());
}

#[test]
fn renovate_backend_refuses_a_different_or_missing_source_hash_before_dispatch() {
    let (repository, _, _) = renovate_fixture("renovate-mismatched-backend");
    let guard = copied_guard(&repository);
    std::fs::copy(
        repository
            .path()
            .join(format!(".local/bin/docker{}", std::env::consts::EXE_SUFFIX)),
        guard.with_file_name(format!(
            "{}{}",
            dotguard::RENOVATE_BINARY,
            std::env::consts::EXE_SUFFIX
        )),
    )
    .unwrap();
    let mut variables = renovate_variables(&repository);
    variables.push((
        "RENOVATE_FIXTURE_BACKEND",
        env!("CARGO_BIN_EXE_dotguard-renovate").to_owned(),
    ));
    let values: Vec<_> = variables
        .iter()
        .map(|(name, value)| (*name, value.as_str()))
        .collect();
    let mismatch = repository.binary_with(&guard, &["renovate", "run"], "", &[], &values);
    assert_eq!(refusal(&mismatch).rule, "tool.stale");
    assert!(!repository.scope.join("arguments").exists());
    for arguments in [&[][..], &["different-source-build", "renovate", "run"][..]] {
        let refused = repository.binary_with(
            Path::new(env!("CARGO_BIN_EXE_dotguard-renovate")),
            arguments,
            "",
            &[],
            &values,
        );
        let record = refusal(&refused);
        assert_eq!(record.rule, "tool.stale");
        eprintln!("{}", record.line());
        assert!(!repository.scope.join("arguments").exists());
    }
}

#[test]
fn git_commands_require_the_matching_policy_before_running_git() {
    let (repository, _, _) = renovate_fixture("git-matching-policy");
    let guard = copied_guard(&repository);
    let head = repository.git(&["rev-parse", "HEAD"]);
    let arguments = ["git", "commit", "--allow-empty", "-m", "must not run"];
    let missing = repository.binary_with(&guard, &arguments, "", &[], &[]);
    assert_eq!(refusal(&missing).rule, "tool.failure");
    let ordinary = repository.binary_with(&guard, &["git", "status", "--short"], "", &[], &[]);
    assert!(ordinary.status.success());
    std::fs::copy(
        repository
            .path()
            .join(format!(".local/bin/docker{}", std::env::consts::EXE_SUFFIX)),
        guard.with_file_name(format!(
            "{}{}",
            dotguard::RENOVATE_BINARY,
            std::env::consts::EXE_SUFFIX
        )),
    )
    .unwrap();
    let mismatched = repository.binary_with(
        &guard,
        &arguments,
        "",
        &[],
        &[(
            "RENOVATE_FIXTURE_BACKEND",
            env!("CARGO_BIN_EXE_dotguard-renovate"),
        )],
    );
    assert_eq!(refusal(&mismatched).rule, "tool.stale");
    assert_eq!(repository.git(&["rev-parse", "HEAD"]), head);
}

#[test]
fn renovate_preserves_manual_arguments_and_keeps_hook_arguments_out_of_the_container() {
    let (repository, base, offered) = renovate_fixture("renovate-arguments");
    let variables = renovate_variables(&repository);
    let values: Vec<_> = variables
        .iter()
        .map(|(name, value)| (*name, value.as_str()))
        .collect();
    let manual = repository.dotguard_with(
        &[
            "renovate",
            "run",
            "--dry-run=lookup",
            "--enabled-managers=cargo",
        ],
        "",
        &[],
        &values,
    );
    assert!(
        manual.status.success(),
        "{}",
        String::from_utf8_lossy(&manual.stderr)
    );
    assert_eq!(
        std::fs::read_to_string(repository.scope.join("arguments")).unwrap(),
        "--dry-run=lookup\n--enabled-managers=cargo"
    );
    let input = format!("refs/heads/main {offered} refs/heads/main {base}\n");
    let gate = repository.dotguard_with(
        &["renovate-gate", "origin", RENOVATE_REMOTE],
        &input,
        &[],
        &values,
    );
    assert!(
        gate.status.success(),
        "{}",
        String::from_utf8_lossy(&gate.stderr)
    );
    assert_eq!(
        std::fs::read_to_string(repository.scope.join("arguments")).unwrap(),
        ""
    );

    let mut failed_values = values;
    failed_values.push(("RENOVATE_FIXTURE_FAIL", "1"));
    let failed = repository.dotguard_with(
        &["renovate-gate", "origin", RENOVATE_REMOTE],
        &input,
        &[],
        &failed_values,
    );
    let rejected = refusal(&failed);
    assert_eq!(rejected.rule, "push.evidence");
    assert!(rejected.cause.contains("did not produce verified evidence"));
    eprintln!("{}", rejected.line());
}

#[test]
fn renovate_snapshot_isolates_hook_git_environment_and_preserves_the_source_repository() {
    let (repository, base, offered) = renovate_fixture("renovate-hook-environment");
    let root = repository.path();
    std::fs::write(
        root.join("Cargo.toml"),
        "[dependencies]\nfixture = \"2.0.0\"\n",
    )
    .unwrap();
    repository.git(&["add", "Cargo.toml"]);
    std::fs::write(root.join("Cargo.toml"), "working tree stays unchanged\n").unwrap();
    let before_head = repository.git(&["rev-parse", "HEAD"]);
    let before_index = std::fs::read(root.join(".git/index")).unwrap();
    let before_status = repository.git(&["--no-optional-locks", "status", "--porcelain"]);
    let before_worktree = std::fs::read(root.join("Cargo.toml")).unwrap();
    let mut variables = renovate_variables(&repository);
    variables.extend([
        ("GIT_DIR", root.join(".git").to_string_lossy().into_owned()),
        ("GIT_WORK_TREE", root.to_string_lossy().into_owned()),
        (
            "GIT_INDEX_FILE",
            root.join(".git/index").to_string_lossy().into_owned(),
        ),
    ]);
    let values: Vec<_> = variables
        .iter()
        .map(|(name, value)| (*name, value.as_str()))
        .collect();
    let output = repository.dotguard_with(
        &["renovate-gate", "origin"],
        &format!("refs/heads/main {offered} refs/heads/main {base}\n"),
        &[],
        &values,
    );
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(repository.git(&["rev-parse", "HEAD"]), before_head);
    assert_eq!(
        std::fs::read(root.join(".git/index")).unwrap(),
        before_index
    );
    assert_eq!(
        repository.git(&["--no-optional-locks", "status", "--porcelain"]),
        before_status
    );
    assert_eq!(
        std::fs::read(root.join("Cargo.toml")).unwrap(),
        before_worktree
    );
}

#[test]
fn renovate_deleting_a_manifest_does_not_start_a_lookup() {
    let (repository, _, initial) = renovate_fixture("renovate-deleted-manifest");
    let workflow = ".github/workflows/mutants.yml";
    let base = changed_file(&repository, workflow, "name: Mutants\n", &initial);
    repository.git(&["rm", workflow]);
    let tree = repository.git(&["write-tree"]);
    let offered = repository.git(&["commit-tree", &tree, "-p", &base, "-m", "remove workflow"]);
    repository.git(&["update-ref", "HEAD", &offered]);
    let output = report_gate(&repository, &base, &offered, &serde_json::json!({}));
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(!repository.scope.join("arguments").exists());
}

#[test]
fn renovate_deletions_leave_changed_manifests_required() {
    let (repository, _, initial) = renovate_fixture("renovate-deletion-and-change");
    let workflow = ".github/workflows/mutants.yml";
    let base = changed_file(&repository, workflow, "name: Mutants\n", &initial);
    repository.git(&["rm", workflow]);
    let offered = changed_file(
        &repository,
        "Cargo.toml",
        "[dependencies]\nfixture = \"1.0.0\"\n",
        &base,
    );
    let mut report = current_renovate_report();
    let dependency = &mut report["repositories"]["local"]["packageFiles"]["cargo"][0]["deps"][0];
    dependency["currentValue"] = "1.0.0".into();
    dependency["updates"] =
        serde_json::json!([{"updateType":"patch","newValue":"1.0.1","isBreaking":false}]);
    let output = report_gate(&repository, &base, &offered, &report);
    assert_eq!(refusal(&output).rule, "push.dependencies");
    assert!(repository.scope.join("arguments").exists());
}

fn current_renovate_report() -> serde_json::Value {
    serde_json::json!({"repositories":{"local":{"packageFiles":{"cargo":[{
        "packageFile":"Cargo.toml",
        "deps":[{"depName":"fixture","currentValue":"1.0.1","datasource":"crate","updates":[]}]
    }]}}}})
}

#[test]
fn renovate_fetches_managed_credentials_without_exposing_them_or_overriding_the_environment() {
    let (repository, base, offered) = renovate_fixture("renovate-managed-credentials");
    let tools = repository.path().join(".local/bin");
    for name in ["chezmoi", "doppler"] {
        std::fs::copy(
            tools.join(format!("docker{}", std::env::consts::EXE_SUFFIX)),
            tools.join(format!("{name}{}", std::env::consts::EXE_SUFFIX)),
        )
        .unwrap();
    }
    let report = repository.scope.join("credential-report.json");
    std::fs::write(
        &report,
        serde_json::to_vec(&current_renovate_report()).unwrap(),
    )
    .unwrap();
    let called = repository.scope.join("doppler-called");
    for (scope, explicit, secret, fail, expected, fetched) in [
        (
            "managed-project/prd",
            "",
            "synthetic-doppler-token",
            "0",
            "synthetic-doppler-token",
            true,
        ),
        (
            "managed-project/prd",
            "synthetic-environment-token",
            "unused-token",
            "1",
            "synthetic-environment-token",
            false,
        ),
        ("managed-project/prd", "", "unused-token", "1", "", true),
        (
            "managed-project/prd",
            "",
            "${unresolved.reference}",
            "0",
            "",
            true,
        ),
        ("managed-project/prd", "", "", "0", "", true),
        (
            "managed-project/prd/other",
            "",
            "unused-token",
            "0",
            "",
            false,
        ),
    ] {
        if called.exists() {
            std::fs::remove_file(&called).unwrap();
        }
        let mut variables = renovate_variables(&repository);
        variables.retain(|(name, _)| *name != "RENOVATE_GITHUB_COM_TOKEN");
        variables.extend([
            ("MISE_NOT_FOUND_AUTO_INSTALL", "true".to_owned()),
            ("MISE_AUTO_PRUNE", "true".to_owned()),
            ("RENOVATE_GITHUB_COM_TOKEN", explicit.to_owned()),
            ("RENOVATE_FIXTURE_DOPPLER_SCOPE", scope.to_owned()),
            ("RENOVATE_FIXTURE_DOPPLER_TOKEN", secret.to_owned()),
            ("RENOVATE_FIXTURE_DOPPLER_FAIL", fail.to_owned()),
            ("RENOVATE_FIXTURE_EXPECT_GITHUB_TOKEN", expected.to_owned()),
            (
                "RENOVATE_FIXTURE_DOPPLER_CALL",
                called.to_string_lossy().into_owned(),
            ),
            (
                "RENOVATE_FIXTURE_REPORT",
                report.to_string_lossy().into_owned(),
            ),
        ]);
        let values: Vec<_> = variables
            .iter()
            .map(|(name, value)| (*name, value.as_str()))
            .collect();
        let output = repository.dotguard_with(
            &["renovate-gate", "origin", RENOVATE_REMOTE],
            &format!("refs/heads/main {offered} refs/heads/main {base}\n"),
            &[],
            &values,
        );
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(called.exists(), fetched, "{scope}");
        for leaked in [
            "synthetic-doppler-token",
            "synthetic-environment-token",
            "synthetic-credential-error",
            "${unresolved.reference}",
        ] {
            assert!(!String::from_utf8_lossy(&output.stdout).contains(leaked));
            assert!(!String::from_utf8_lossy(&output.stderr).contains(leaked));
        }
    }
}

#[test]
fn renovate_accepts_warning_problems_without_hiding_updates_or_errors() {
    let (repository, base, offered) = renovate_fixture("renovate-problem-severity");
    for global in [false, true] {
        let mut report = current_renovate_report();
        let problems = if global {
            &mut report["problems"]
        } else {
            &mut report["repositories"]["local"]["problems"]
        };
        *problems = serde_json::json!([{"level":40,"msg":"A feature is deprecated"}]);
        let output = report_gate(&repository, &base, &offered, &report);
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );

        let mut outdated = report.clone();
        outdated["repositories"]["local"]["packageFiles"]["cargo"][0]["deps"][0]["updates"] =
            serde_json::json!([{"updateType":"patch","newValue":"1.0.2"}]);
        assert_eq!(
            refusal(&report_gate(&repository, &base, &offered, &outdated)).rule,
            "push.dependencies"
        );

        for level in [
            serde_json::json!(50),
            serde_json::json!(60),
            serde_json::json!(41),
            serde_json::json!("40"),
            serde_json::Value::Null,
        ] {
            let mut failed = report.clone();
            let problems = if global {
                &mut failed["problems"]
            } else {
                &mut failed["repositories"]["local"]["problems"]
            };
            problems[0]["level"] = level;
            assert_eq!(
                refusal(&report_gate(&repository, &base, &offered, &failed)).rule,
                "push.evidence"
            );
        }
    }
}

#[test]
fn renovate_accepts_unversioned_skips_independently_of_the_manager() {
    let (repository, base, offered) = renovate_fixture("renovate-unversioned");
    for manager in ["bundler", "npm", "cargo", "nuget", "future-manager"] {
        for reason in ["invalid-value", "unspecified-version"] {
            let dependency = serde_json::json!({"depName":"fixture","skipReason":reason,"updates":[],"warnings":[]});
            let report = serde_json::json!({"repositories":{"local":{"packageFiles":{manager:[{"packageFile":"Cargo.toml","deps":[dependency]}]}}}});
            let output = report_gate(&repository, &base, &offered, &report);
            assert!(
                output.status.success(),
                "{manager}/{reason}: {}",
                String::from_utf8_lossy(&output.stderr)
            );
        }
    }
}

#[test]
fn renovate_never_treats_a_declared_version_or_lookup_failure_as_unversioned() {
    let (repository, base, offered) = renovate_fixture("renovate-unversioned-refusals");
    for (field, value) in [
        ("currentValue", serde_json::json!("invalid")),
        ("currentValue", serde_json::json!("")),
        ("currentDigest", serde_json::json!("sha256:123")),
        ("lockedVersion", serde_json::json!("1.0.1")),
        ("lockedVersion", serde_json::json!(false)),
        ("warnings", serde_json::json!([{"message":"lookup failed"}])),
        (
            "updates",
            serde_json::json!([{"updateType":"patch","newValue":"1.0.2"}]),
        ),
        ("skipReason", serde_json::json!("github-token-required")),
        ("skipReason", serde_json::json!("unsupported-datasource")),
    ] {
        let mut dependency = serde_json::json!({"depName":"fixture","skipReason":"invalid-value","updates":[],"warnings":[]});
        dependency[field] = value;
        let report = serde_json::json!({"repositories":{"local":{"packageFiles":{"cargo":[{"packageFile":"Cargo.toml","deps":[dependency]}]}}}});
        assert_eq!(
            refusal(&report_gate(&repository, &base, &offered, &report)).rule,
            "push.evidence",
            "{field}"
        );
    }
}

#[test]
fn renovate_report_refusals_identify_the_failed_dependency_without_its_payload() {
    let (repository, base, offered) = renovate_fixture("renovate-report-diagnostics");
    let mut report = current_renovate_report();
    let dependency = &mut report["repositories"]["local"]["packageFiles"]["cargo"][0]["deps"][0];
    dependency["skipReason"] = serde_json::json!("github-token-required");
    dependency["warnings"] = serde_json::json!([{"message":"Bearer private-token"}]);
    let rejected = refusal(&report_gate(&repository, &base, &offered, &report));
    assert_eq!(rejected.rule, "push.evidence");
    assert!(rejected.evidence.iter().any(|line| line.contains("cargo")
        && line.contains("Cargo.toml")
        && line.contains("fixture")
        && line.contains("github-token-required")));
    assert!(
        rejected
            .evidence
            .iter()
            .all(|line| !line.contains("private-token"))
    );
}

#[test]
fn renovate_inspects_saved_reports_without_starting_a_container() {
    let (repository, _, _) = renovate_fixture("renovate-inspect");
    let file = repository.scope.join("saved-report.json");
    std::fs::write(&file, include_bytes!("fixtures/renovate-local-44.52.json")).unwrap();
    let inspected = repository.dotguard_with(
        &["renovate", "inspect", file.to_str().unwrap()],
        "",
        &[],
        &[],
    );
    assert!(
        inspected.status.success(),
        "{}",
        String::from_utf8_lossy(&inspected.stderr)
    );
    let output = String::from_utf8(inspected.stdout).unwrap();
    assert!(output.contains("1 nonfatal notices"));
    assert!(output.contains("bootsnap 1.17.0 -> 1.26.0"));
    assert!(!repository.scope.join("arguments").exists());
    std::fs::write(
        &file,
        br#"{"problems":[{"level":50}],"repositories":{"local":{"packageFiles":{}}}}"#,
    )
    .unwrap();
    let failed = repository.dotguard_with(
        &["renovate", "inspect", file.to_str().unwrap()],
        "",
        &[],
        &[],
    );
    assert_eq!(refusal(&failed).rule, "push.evidence");
    assert!(!repository.scope.join("arguments").exists());
}
fn report_gate(
    repository: &Repository,
    base: &str,
    offered: &str,
    report: &serde_json::Value,
) -> Output {
    let file = repository.scope.join("fixture-report.json");
    std::fs::write(&file, serde_json::to_vec(report).unwrap()).unwrap();
    let mut variables = renovate_variables(repository);
    variables.push((
        "RENOVATE_FIXTURE_MANIFEST",
        repository.git(&["show", &format!("{offered}:Cargo.toml")]) + "\n",
    ));
    variables.push((
        "RENOVATE_FIXTURE_REPORT",
        file.to_string_lossy().into_owned(),
    ));
    let values: Vec<_> = variables
        .iter()
        .map(|(name, value)| (*name, value.as_str()))
        .collect();
    repository.dotguard_with(
        &["renovate-gate", "origin", RENOVATE_REMOTE],
        &format!("refs/heads/main {offered} refs/heads/main {base}\n"),
        &[],
        &values,
    )
}

const WORKSPACE_MANIFEST: &str = "[workspace]\nmembers = [\"member\"]\nexclude = [\"dependency\"]\nresolver = \"3\"\n[workspace.dependencies]\nrenamed = { package = \"actual-fixture\", version = \"1.0.1\" }\n[patch.crates-io]\nactual-fixture = { path = \"dependency\" }\n";
const MEMBER_MANIFEST: &str = "[package]\nname = \"member\"\nversion = \"0.1.0\"\nedition = \"2024\"\n[dependencies]\nrenamed = { workspace = true }\n";

fn cargo_workspace_fixture(name: &str) -> (Repository, String) {
    let (repository, _, parent) = renovate_fixture(name);
    for directory in ["member/src", "dependency/src"] {
        std::fs::create_dir_all(repository.path().join(directory)).unwrap();
        std::fs::write(repository.path().join(directory).join("lib.rs"), "").unwrap();
    }
    std::fs::write(repository.path().join("Cargo.toml"), WORKSPACE_MANIFEST).unwrap();
    std::fs::write(repository.path().join("member/Cargo.toml"), MEMBER_MANIFEST).unwrap();
    std::fs::write(
        repository.path().join("dependency/Cargo.toml"),
        "[package]\nname = \"actual-fixture\"\nversion = \"1.0.1\"\nedition = \"2024\"\n[features]\nextra = []\n",
    ).unwrap();
    let generated = repository
        .command(env!("CARGO"))
        .args(["generate-lockfile", "--offline"])
        .output()
        .unwrap();
    assert!(
        generated.status.success(),
        "{}",
        String::from_utf8_lossy(&generated.stderr)
    );
    repository.git(&["add", "--all"]);
    let tree = repository.git(&["write-tree"]);
    let base = repository.git(&["commit-tree", &tree, "-p", &parent, "-m", "workspace"]);
    (repository, base)
}

fn workspace_report() -> serde_json::Value {
    serde_json::json!({"repositories":{"local":{"packageFiles":{"cargo":[
        {"packageFile":"Cargo.toml","lockFiles":["Cargo.lock"],"deps":[{
            "depName":"renamed","packageName":"actual-fixture","datasource":"crate",
            "depType":"workspace.dependencies","currentValue":"1.0.1","updates":[{"updateType":"patch","newValue":"1.0.2"}],"warnings":[]
        }]},
        {"packageFile":"member/Cargo.toml","lockFiles":["Cargo.lock"],"deps":[{
            "depName":"renamed","datasource":"crate","depType":"dependencies",
            "currentValue":"","skipReason":"inherited-dependency","updates":[],"warnings":[]
        }]}
    ]}}}})
}

fn workspace_offer(repository: &Repository, base: &str, manifest: &str) -> String {
    std::fs::write(repository.path().join("member/Cargo.toml"), manifest).unwrap();
    repository.git(&["add", "--all"]);
    let tree = repository.git(&["write-tree"]);
    repository.git(&["commit-tree", &tree, "-p", base, "-m", "member"])
}

fn local_workspace_fixture(name: &str) -> (Repository, String, serde_json::Value) {
    let (repository, parent) = cargo_workspace_fixture(name);
    std::fs::write(
        repository.path().join("Cargo.toml"),
        WORKSPACE_MANIFEST.replace(
            "package = \"actual-fixture\", version = \"1.0.1\"",
            "package = \"actual-fixture\", path = \"dependency\"",
        ),
    )
    .unwrap();
    repository.git(&["add", "Cargo.toml"]);
    let tree = repository.git(&["write-tree"]);
    let base = repository.git(&["commit-tree", &tree, "-p", &parent, "-m", "local provider"]);
    let mut report = workspace_report();
    report["repositories"]["local"]["packageFiles"]["cargo"][0]["deps"][0] = serde_json::json!({
        "depName":"renamed", "packageName":"actual-fixture", "datasource":"crate",
        "depType":"workspace.dependencies", "currentValue":"",
        "skipReason":"path-dependency", "updates":[], "warnings":[]
    });
    (repository, base, report)
}

#[test]
fn renovate_verifies_inherited_local_cargo_dependencies() {
    let (repository, base, report) = local_workspace_fixture("renovate-inherited-local");
    let offered = workspace_offer(&repository, &base, &format!("{MEMBER_MANIFEST}# comment\n"));
    let output = report_gate(&repository, &base, &offered, &report);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn renovate_verifies_versioned_local_cargo_dependencies() {
    for (version, name) in [
        ("1.0.1", "local-publication-version"),
        ("^1.0.1", "local-compatible-version"),
        ("=1.0.1", "local-exact-version"),
    ] {
        let (repository, base, report) = local_workspace_fixture(name);
        let path = repository.path().join("Cargo.toml");
        std::fs::write(
            &path,
            std::fs::read_to_string(&path).unwrap().replace(
                "path = \"dependency\"",
                &format!("path = \"dependency\", version = {version:?}"),
            ),
        )
        .unwrap();
        let offered = workspace_offer(&repository, &base, MEMBER_MANIFEST);
        let output = report_gate(&repository, &base, &offered, &report);
        assert!(
            output.status.success(),
            "{version}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
}

#[test]
fn renovate_rejects_forged_inherited_local_cargo_evidence() {
    let (repository, base, original) = local_workspace_fixture("renovate-local-refusals");
    let offered = workspace_offer(&repository, &base, &format!("{MEMBER_MANIFEST}# comment\n"));
    for (field, value) in [
        ("packageName", serde_json::json!("different-package")),
        (
            "registryUrls",
            serde_json::json!(["https://wrong.invalid/index"]),
        ),
        ("depName", serde_json::json!("absent")),
        ("lockedVersion", serde_json::json!("1.0.1")),
        (
            "updates",
            serde_json::json!([{"updateType":"patch","newValue":"1.0.2"}]),
        ),
    ] {
        let mut report = original.clone();
        report["repositories"]["local"]["packageFiles"]["cargo"][1]["deps"][0][field] = value;
        assert_eq!(
            refusal(&report_gate(&repository, &base, &offered, &report)).rule,
            "push.evidence",
            "{field}"
        );
    }
    let mut report = original.clone();
    report["repositories"]["local"]["packageFiles"]["cargo"][1]["lockFiles"] =
        serde_json::json!(["member/Cargo.lock"]);
    assert_eq!(
        refusal(&report_gate(&repository, &base, &offered, &report)).rule,
        "push.evidence"
    );
}

#[test]
fn renovate_rejects_inherited_local_cargo_sources_outside_the_snapshot() {
    let (repository, base, report) = local_workspace_fixture("renovate-local-escape");
    let outside = repository.scope.join("external");
    std::fs::create_dir_all(outside.join("src")).unwrap();
    for name in ["Cargo.toml", "src/lib.rs"] {
        std::fs::copy(
            repository.path().join("dependency").join(name),
            outside.join(name),
        )
        .unwrap();
    }
    let path = repository.path().join("Cargo.toml");
    std::fs::write(
        &path,
        std::fs::read_to_string(&path).unwrap().replace(
            "path = \"dependency\"",
            &format!("path = {:?}", outside.to_string_lossy()),
        ),
    )
    .unwrap();
    let offered = workspace_offer(&repository, &base, MEMBER_MANIFEST);
    let rejected = refusal(&report_gate(&repository, &base, &offered, &report));
    assert_eq!(rejected.rule, "push.evidence");
    assert!(
        rejected
            .evidence
            .iter()
            .any(|line| line.contains("escapes the committed snapshot"))
    );
}

#[test]
fn renovate_verifies_inherited_cargo_and_ignores_member_comments() {
    let (repository, base) = cargo_workspace_fixture("renovate-inherited-comments");
    std::fs::write(
        repository.path().join("member/Cargo.toml"),
        format!("{MEMBER_MANIFEST}# comment only\n"),
    )
    .unwrap();
    repository.git(&["add", "member/Cargo.toml"]);
    let tree = repository.git(&["write-tree"]);
    let offered = repository.git(&["commit-tree", &tree, "-p", &base, "-m", "comment"]);
    let output = report_gate(&repository, &base, &offered, &workspace_report());
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn renovate_attributes_changed_inherited_bindings_to_the_workspace_provider() {
    for (name, manifest, kind, target) in [
        (
            "features",
            MEMBER_MANIFEST.replace(
                "workspace = true",
                "workspace = true, features = [\"extra\"]",
            ),
            "dependencies",
            None,
        ),
        (
            "kind",
            MEMBER_MANIFEST.replace("[dependencies]", "[dev-dependencies]"),
            "dev-dependencies",
            None,
        ),
        (
            "target",
            MEMBER_MANIFEST.replace("[dependencies]", "[target.'cfg(unix)'.dependencies]"),
            "dependencies",
            Some("cfg(unix)"),
        ),
    ] {
        let (repository, base) = cargo_workspace_fixture(&format!("renovate-inherited-{name}"));
        let offered = workspace_offer(&repository, &base, &manifest);
        let mut report = workspace_report();
        let reference = &mut report["repositories"]["local"]["packageFiles"]["cargo"][1]["deps"][0];
        reference["depType"] = serde_json::json!(kind);
        if let Some(target) = target {
            reference["target"] = serde_json::json!(target);
        }
        assert_eq!(
            refusal(&report_gate(&repository, &base, &offered, &report)).rule,
            "push.dependencies",
            "{name}"
        );
    }
}

#[test]
fn renovate_blocks_new_references_and_new_member_manifests() {
    let (repository, parent) = cargo_workspace_fixture("renovate-inherited-new-reference");
    let empty = MEMBER_MANIFEST.replace("renamed = { workspace = true }\n", "");
    let base = workspace_offer(&repository, &parent, &empty);
    let offered = workspace_offer(&repository, &base, MEMBER_MANIFEST);
    assert_eq!(
        refusal(&report_gate(
            &repository,
            &base,
            &offered,
            &workspace_report()
        ))
        .rule,
        "push.dependencies"
    );

    let (repository, base) = cargo_workspace_fixture("renovate-inherited-new-manifest");
    std::fs::create_dir_all(repository.path().join("new-member/src")).unwrap();
    std::fs::write(repository.path().join("new-member/src/lib.rs"), "").unwrap();
    std::fs::write(
        repository.path().join("new-member/Cargo.toml"),
        MEMBER_MANIFEST.replace("name = \"member\"", "name = \"new-member\""),
    )
    .unwrap();
    std::fs::write(
        repository.path().join("Cargo.toml"),
        WORKSPACE_MANIFEST.replace("[\"member\"]", "[\"member\", \"new-member\"]"),
    )
    .unwrap();
    let offered = workspace_offer(&repository, &base, MEMBER_MANIFEST);
    let mut report = workspace_report();
    report["repositories"]["local"]["packageFiles"]["cargo"][1]["packageFile"] =
        serde_json::json!("new-member/Cargo.toml");
    assert_eq!(
        refusal(&report_gate(&repository, &base, &offered, &report)).rule,
        "push.dependencies"
    );
}

#[test]
fn renovate_keeps_workspace_provider_versions_subject_to_freshness() {
    let (repository, base) = cargo_workspace_fixture("renovate-inherited-version");
    std::fs::write(
        repository.path().join("Cargo.toml"),
        WORKSPACE_MANIFEST.replace("1.0.1", "1.0.0"),
    )
    .unwrap();
    let offered = workspace_offer(&repository, &base, MEMBER_MANIFEST);
    let mut report = workspace_report();
    report["repositories"]["local"]["packageFiles"]["cargo"][0]["deps"][0]["currentValue"] =
        serde_json::json!("1.0.0");
    assert_eq!(
        refusal(&report_gate(&repository, &base, &offered, &report)).rule,
        "push.dependencies"
    );
}

#[test]
fn renovate_preserves_workspace_update_eligibility_and_requires_evidence_for_inspection() {
    let (repository, base) = cargo_workspace_fixture("renovate-inherited-eligibility");
    let offered = workspace_offer(
        &repository,
        &base,
        &MEMBER_MANIFEST.replace(
            "workspace = true",
            "workspace = true, features = [\"extra\"]",
        ),
    );
    for (field, value) in [
        ("pendingChecks", serde_json::json!(true)),
        ("isBreaking", serde_json::json!(true)),
        ("updateType", serde_json::json!("major")),
    ] {
        let mut report = workspace_report();
        report["repositories"]["local"]["packageFiles"]["cargo"][0]["deps"][0]["updates"][0]
            [field] = value;
        let output = report_gate(&repository, &base, &offered, &report);
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    let path = repository.scope.join("inherited-report.json");
    std::fs::write(&path, serde_json::to_vec(&workspace_report()).unwrap()).unwrap();
    assert_eq!(
        refusal(&repository.dotguard(&["renovate", "inspect", path.to_str().unwrap()], "", &[]))
            .rule,
        "push.evidence"
    );
}

#[test]
fn renovate_refuses_misbound_and_failed_inherited_evidence() {
    let (repository, base) = cargo_workspace_fixture("renovate-inherited-refusals");
    let offered = workspace_offer(&repository, &base, &format!("{MEMBER_MANIFEST}# comment\n"));
    let original = workspace_report();
    for (field, value) in [
        ("currentValue", serde_json::json!("1.0.1")),
        ("currentValue", serde_json::Value::Null),
        ("lockedVersion", serde_json::json!("1.0.1")),
        ("currentVersion", serde_json::json!("1.0.1")),
        ("currentDigest", serde_json::json!("")),
        ("depType", serde_json::json!("workspace.dependencies")),
        ("depName", serde_json::json!("absent")),
        ("packageName", serde_json::json!("different-package")),
        (
            "registryUrls",
            serde_json::json!(["https://wrong.invalid/index"]),
        ),
        ("warnings", serde_json::json!([{"message":"private-token"}])),
        (
            "updates",
            serde_json::json!([{"updateType":"patch","newValue":"1.0.2"}]),
        ),
        ("target", serde_json::json!("cfg(windows)")),
        ("registryUrls", serde_json::json!(false)),
    ] {
        let mut report = original.clone();
        report["repositories"]["local"]["packageFiles"]["cargo"][1]["deps"][0][field] = value;
        let rejected = refusal(&report_gate(&repository, &base, &offered, &report));
        assert_eq!(rejected.rule, "push.evidence", "{field}");
        assert!(
            rejected
                .evidence
                .iter()
                .all(|evidence| !evidence.contains("private-token"))
        );
    }
    for member in [false, true] {
        for field in ["packageFile", "lockFiles"] {
            let mut report = original.clone();
            let file =
                &mut report["repositories"]["local"]["packageFiles"]["cargo"][usize::from(member)];
            file[field] = if field == "packageFile" {
                serde_json::json!("missing/Cargo.toml")
            } else {
                serde_json::json!(["member/Cargo.lock"])
            };
            assert_eq!(
                refusal(&report_gate(&repository, &base, &offered, &report)).rule,
                "push.evidence"
            );
        }
    }
    for (field, value) in [
        ("currentValue", serde_json::json!("1.0.0")),
        ("packageName", serde_json::json!("different-package")),
        (
            "registryUrls",
            serde_json::json!(["https://wrong.invalid/index"]),
        ),
        ("skipReason", serde_json::json!("github-token-required")),
        ("warnings", serde_json::json!([{"message":"lookup failed"}])),
    ] {
        let mut report = original.clone();
        report["repositories"]["local"]["packageFiles"]["cargo"][0]["deps"][0][field] = value;
        assert_eq!(
            refusal(&report_gate(&repository, &base, &offered, &report)).rule,
            "push.evidence"
        );
    }
    for duplicate in [false, true] {
        let mut report = original.clone();
        let deps = report["repositories"]["local"]["packageFiles"]["cargo"][0]["deps"]
            .as_array_mut()
            .unwrap();
        if duplicate {
            deps.push(deps[0].clone());
        } else {
            deps.clear();
        }
        assert_eq!(
            refusal(&report_gate(&repository, &base, &offered, &report)).rule,
            "push.evidence"
        );
    }
}

#[test]
fn renovate_metadata_never_replaces_committed_output_filenames() {
    let (repository, parent) = cargo_workspace_fixture("renovate-inherited-collision");
    for filename in [".gate-index", "metadata.json", "cargo-metadata.json"] {
        std::fs::write(repository.path().join(filename), "committed source").unwrap();
    }
    let base = workspace_offer(&repository, &parent, MEMBER_MANIFEST);
    let offered = workspace_offer(&repository, &base, &format!("{MEMBER_MANIFEST}# comment\n"));
    let output = report_gate(&repository, &base, &offered, &workspace_report());
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    for filename in [".gate-index", "metadata.json", "cargo-metadata.json"] {
        assert_eq!(
            std::fs::read_to_string(repository.path().join(filename)).unwrap(),
            "committed source"
        );
    }
}

#[test]
fn renovate_refuses_failed_metadata_in_either_committed_snapshot() {
    for baseline in [false, true] {
        let (repository, parent) =
            cargo_workspace_fixture(&format!("renovate-inherited-metadata-{baseline}"));
        std::fs::write(repository.path().join("member/Cargo.toml"), "malformed = [").unwrap();
        let broken = workspace_offer(&repository, &parent, "malformed = [");
        let (base, offered) = if baseline {
            let offered = workspace_offer(&repository, &broken, MEMBER_MANIFEST);
            (broken, offered)
        } else {
            (parent, broken)
        };
        assert_eq!(
            refusal(&report_gate(
                &repository,
                &base,
                &offered,
                &workspace_report()
            ))
            .rule,
            "push.evidence"
        );
    }
}

fn backend_report(
    (name, datasource, package, version): (&str, &str, &str, &str),
) -> serde_json::Value {
    serde_json::json!({"repositories":{"local":{"packageFiles":{
        "mise":[{"packageFile":"mise.toml","deps":[{
            "depName":name,"lockedVersion":version,"skipReason":"unsupported-datasource","updates":[]
        }]}],
        "regex":[{"packageFile":"mise.toml","deps":[{
            "depName":name,"currentValue":version,"currentVersion":version,
            "datasource":datasource,"packageName":package,"updates":[],"warnings":[]
        }]}]
    }}}})
}

fn regex_counterpart(report: &mut serde_json::Value) -> &mut serde_json::Value {
    &mut report["repositories"]["local"]["packageFiles"]["regex"][0]["deps"][0]
}

fn mise_ci_report() -> serde_json::Value {
    let alias = serde_json::json!({
        "depName":"jdx/mise","packageName":"jdx/mise","datasource":"github-release-attachments",
        "depType":"uses-with","currentValue":MISE_CI_EXPRESSION,"skipReason":"invalid-value","updates":[],"warnings":[]
    });
    serde_json::json!({"repositories":{"local":{"packageFiles":{
        "github-actions":[{"packageFile":".github/workflows/ci.yml","deps":[alias.clone(),alias.clone(),alias.clone(),alias.clone(),alias]}],
        "regex":[{"packageFile":MISE_LITERAL_FILE,"deps":[{
            "depName":"jdx/mise","packageName":"jdx/mise","datasource":"github-releases",
            "currentValue":"2026.10.0","currentVersion":"2026.10.0","replaceString":MISE_LITERAL_MANIFEST.trim_end(),"updates":[],"warnings":[]
        }]}]
    }}}})
}

fn mise_ci_alias(report: &mut serde_json::Value) -> &mut serde_json::Value {
    &mut report["repositories"]["local"]["packageFiles"]["github-actions"][0]["deps"][0]
}

fn changed_file(repository: &Repository, file: &str, contents: &str, parent: &str) -> String {
    let path = repository.path().join(file);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, contents).unwrap();
    repository.git(&["add", file]);
    let tree = repository.git(&["write-tree"]);
    let offered = repository.git(&["commit-tree", &tree, "-p", parent, "-m", "pin change"]);
    repository.git(&["update-ref", "HEAD", &offered]);
    offered
}

fn outdated_mise_report() -> serde_json::Value {
    let mut report = mise_ci_report();
    regex_counterpart(&mut report)["updates"] = serde_json::json!([{
        "updateType":"patch","newValue":"2026.10.6","isBreaking":false
    }]);
    report
}

#[test]
fn renovate_blocks_a_version_only_change_with_its_unchanged_annotation() {
    let (repository, _, base) = renovate_fixture("renovate-version-only");
    let contents = MISE_LITERAL_MANIFEST.replace("2026.10.0", "2026.10.1");
    let offered = changed_file(&repository, MISE_LITERAL_FILE, &contents, &base);
    let mut report = outdated_mise_report();
    let counterpart = regex_counterpart(&mut report);
    counterpart["currentValue"] = "2026.10.1".into();
    counterpart["currentVersion"] = "2026.10.1".into();
    counterpart["replaceString"] = contents.trim_end().into();
    let rejected = refusal(&report_gate(&repository, &base, &offered, &report));
    assert_eq!(rejected.rule, "push.dependencies");
}

#[test]
fn renovate_never_borrows_another_pins_or_files_matching_version() {
    let (repository, _, initial) = renovate_fixture("renovate-pin-ownership");
    let other =
        "# renovate: datasource=github-releases depName=other/tool\nversion = \"2026.9.0\"\n";
    let before = format!("{MISE_LITERAL_MANIFEST}{other}");
    let base = changed_file(&repository, MISE_LITERAL_FILE, &before, &initial);
    let after = before.replace("2026.9.0", "2026.10.0");
    let offered = changed_file(&repository, MISE_LITERAL_FILE, &after, &base);
    let report = outdated_mise_report();
    let output = report_gate(&repository, &base, &offered, &report);
    assert!(
        output.status.success(),
        "unrelated pin: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let other_file = format!("{RENOVATE_MISE_MANIFEST}other = \"2026.10.0\"\n");
    let next = changed_file(&repository, "mise.toml", &other_file, &offered);
    let output = report_gate(&repository, &offered, &next, &report);
    assert!(
        output.status.success(),
        "unrelated file: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(String::from_utf8_lossy(&output.stderr).contains("dependency manifests changed"));
}

#[test]
fn renovate_requires_the_verified_literal_mise_source_for_its_exact_ci_aliases() {
    let (repository, base, offered) = renovate_fixture("renovate-ci-alias");
    let mut report = mise_ci_report();
    let output = report_gate(&repository, &base, &offered, &report);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    regex_counterpart(&mut report)["updates"] = serde_json::json!([{
        "updateType":"patch","newValue":"2026.10.6","isBreaking":false
    }]);
    let rejected = refusal(&report_gate(&repository, &base, &offered, &report));
    assert_eq!(rejected.rule, "push.dependencies");
    assert!(
        rejected
            .evidence
            .iter()
            .any(|line| line.contains(MISE_LITERAL_FILE))
    );
}

#[test]
fn renovate_refuses_other_ci_aliases_and_missing_ambiguous_or_failed_literal_sources() {
    let (repository, base, offered) = renovate_fixture("renovate-ci-alias-refusals");
    let mut reports = Vec::new();
    for (field, value) in [
        ("currentValue", serde_json::json!("${{ env.MISE_VERSION }}")),
        ("datasource", serde_json::json!("github-tags")),
        ("depName", serde_json::json!("other/tool")),
        ("packageName", serde_json::json!("other/tool")),
        ("depType", serde_json::json!("action")),
        ("skipReason", serde_json::json!("internal-error")),
        ("warnings", serde_json::json!([{"message":"failed lookup"}])),
        ("updates", serde_json::json!([{"updateType":"major"}])),
    ] {
        let mut report = mise_ci_report();
        mise_ci_alias(&mut report)[field] = value;
        reports.push((field, report));
    }
    for (field, value) in [
        ("depName", serde_json::json!("other/tool")),
        ("datasource", serde_json::json!("github-tags")),
        ("packageName", serde_json::json!("other/tool")),
        ("currentValue", serde_json::json!("")),
        ("currentVersion", serde_json::json!("")),
        ("skipReason", serde_json::json!("github-token-required")),
        ("enabled", serde_json::json!(false)),
        ("warnings", serde_json::json!([{"message":"failed lookup"}])),
        ("updates", serde_json::json!([null])),
        (
            "updates",
            serde_json::json!([{"updateType":"major","pendingChecks":"true"}]),
        ),
    ] {
        let mut report = mise_ci_report();
        regex_counterpart(&mut report)[field] = value;
        reports.push((field, report));
    }
    let mut missing = mise_ci_report();
    missing["repositories"]["local"]["packageFiles"]
        .as_object_mut()
        .unwrap()
        .remove("regex");
    reports.push(("missing-source", missing));
    let mut duplicate = mise_ci_report();
    let counterpart = regex_counterpart(&mut duplicate).clone();
    duplicate["repositories"]["local"]["packageFiles"]["regex"][0]["deps"]
        .as_array_mut()
        .unwrap()
        .push(counterpart);
    reports.push(("ambiguous-source", duplicate));
    for manager in ["regex", "github-actions"] {
        let mut wrong = mise_ci_report();
        wrong["repositories"]["local"]["packageFiles"][manager][0]["packageFile"] =
            "other/file.toml".into();
        reports.push(("wrong-file", wrong));
    }
    let mut wrong_manager = mise_ci_report();
    let managers = wrong_manager["repositories"]["local"]["packageFiles"]
        .as_object_mut()
        .unwrap();
    let alias = managers.remove("github-actions").unwrap();
    managers.insert("cargo".to_owned(), alias);
    reports.push(("wrong-manager", wrong_manager));
    for (name, report) in reports {
        assert_eq!(
            refusal(&report_gate(&repository, &base, &offered, &report)).rule,
            "push.evidence",
            "{name}"
        );
    }
}

#[test]
fn renovate_accepts_only_verified_backend_duplicates_and_still_checks_their_updates() {
    let (repository, base, offered) = renovate_fixture("renovate-backend-coverage");
    let mut rejected = Vec::new();
    for backend in MISE_BACKENDS {
        let output = report_gate(&repository, &base, &offered, &backend_report(backend));
        if !output.status.success() {
            rejected.push(backend.0);
        }
    }
    assert_eq!(
        rejected,
        Vec::<&str>::new(),
        "verified counterpart rejected"
    );
    let mut outdated = backend_report(MISE_BACKENDS[0]);
    regex_counterpart(&mut outdated)["updates"] = serde_json::json!([{
        "updateType":"patch","newValue":"0.116.1","isBreaking":false
    }]);
    let output = report_gate(&repository, &base, &offered, &outdated);
    assert_eq!(refusal(&output).rule, "push.dependencies");
}

#[test]
fn renovate_refuses_missing_mismatched_or_failed_backend_counterparts() {
    let (repository, base, offered) = renovate_fixture("renovate-bad-backend-coverage");
    let mut reports = Vec::new();
    for (field, value) in [
        ("currentValue", serde_json::json!("0.115.0")),
        ("currentValue", serde_json::json!("")),
        ("datasource", serde_json::json!("crate")),
        ("packageName", serde_json::json!("wrong/package")),
        ("currentVersion", serde_json::json!("")),
        ("currentVersion", serde_json::Value::Null),
        ("skipReason", serde_json::json!("github-token-required")),
        ("skipReason", serde_json::json!("ignored")),
        ("enabled", serde_json::json!(false)),
        ("enabled", serde_json::json!("false")),
        ("warnings", serde_json::json!([{"message":"lookup failed"}])),
        ("warnings", serde_json::json!({})),
        ("warnings", serde_json::json!(false)),
        ("updates", serde_json::json!([null])),
        ("updates", serde_json::json!([{}])),
        (
            "updates",
            serde_json::json!([{"updateType":"major","isBreaking":"true"}]),
        ),
    ] {
        let mut report = backend_report(MISE_BACKENDS[0]);
        regex_counterpart(&mut report)[field] = value;
        reports.push((field, report));
    }
    let mut missing = backend_report(MISE_BACKENDS[0]);
    missing["repositories"]["local"]["packageFiles"]
        .as_object_mut()
        .unwrap()
        .remove("regex");
    reports.push(("missing", missing));
    let mut duplicate = backend_report(MISE_BACKENDS[0]);
    let counterpart = regex_counterpart(&mut duplicate).clone();
    duplicate["repositories"]["local"]["packageFiles"]["regex"][0]["deps"]
        .as_array_mut()
        .unwrap()
        .push(counterpart);
    reports.push(("duplicate", duplicate));
    let mut wrong_file = backend_report(MISE_BACKENDS[0]);
    wrong_file["repositories"]["local"]["packageFiles"]["regex"][0]["packageFile"] =
        "other/mise.toml".into();
    reports.push(("wrong-file", wrong_file));
    reports.push((
        "unknown-backend",
        backend_report(("unknown", "crate", "unknown", "1.0.0")),
    ));
    for (name, report) in reports {
        let output = report_gate(&repository, &base, &offered, &report);
        assert_eq!(refusal(&output).rule, "push.evidence", "{name}");
    }
}

fn hosted_runner_report(name: &str) -> serde_json::Value {
    serde_json::json!({"repositories":{"local":{"packageFiles":{"github-actions":[{
        "packageFile":".github/workflows/ci.yml","deps":[{
            "depName":name,"datasource":"github-runners","depType":"github-runner",
            "currentValue":"latest","skipReason":"invalid-version","updates":[]
        }]
    }]}}}})
}

#[test]
fn renovate_admits_only_exact_hosted_channels() {
    let (repository, base, offered) = renovate_fixture("renovate-hosted-channels");
    let mut rejected = Vec::new();
    for name in ["ubuntu", "windows"] {
        let output = report_gate(&repository, &base, &offered, &hosted_runner_report(name));
        if !output.status.success() {
            rejected.push(name);
        }
    }
    assert_eq!(
        rejected,
        Vec::<&str>::new(),
        "intentional hosted channels rejected"
    );
    for (field, value) in [
        ("depName", serde_json::json!("macos")),
        ("currentValue", serde_json::json!("24.04")),
        ("datasource", serde_json::json!("github-tags")),
        ("depType", serde_json::json!("action")),
        ("depType", serde_json::json!("workflow")),
        ("warnings", serde_json::json!([{"message":"lookup failed"}])),
        ("warnings", serde_json::json!({})),
        ("warnings", serde_json::json!(false)),
        ("updates", serde_json::json!([{"updateType":"major"}])),
    ] {
        let mut report = hosted_runner_report("ubuntu");
        report["repositories"]["local"]["packageFiles"]["github-actions"][0]["deps"][0][field] =
            value;
        let output = report_gate(&repository, &base, &offered, &report);
        assert_eq!(refusal(&output).rule, "push.evidence", "{field}");
    }
}

fn matrix_input_report(value: &str) -> serde_json::Value {
    serde_json::json!({"repositories":{"local":{"packageFiles":{"github-actions":[{
        "packageFile":".github/workflows/ci.yml","deps":[{
            "depName":"rust","depType":"uses-with",
            "currentValue":value,"skipReason":"invalid-value","updates":[]
        }]
    }]}}}})
}

#[test]
fn renovate_admits_only_matrix_expression_inputs() {
    let (repository, base, offered) = renovate_fixture("renovate-matrix-inputs");
    let mut rejected = Vec::new();
    for value in ["${{ matrix.rust }}", "${{matrix.tool-chain}}"] {
        let output = report_gate(&repository, &base, &offered, &matrix_input_report(value));
        if !output.status.success() {
            rejected.push(value);
        }
    }
    assert_eq!(
        rejected,
        Vec::<&str>::new(),
        "matrix expression inputs rejected"
    );
    for (field, value) in [
        (
            "currentValue",
            serde_json::json!("${{ steps.mise.outputs.version }}"),
        ),
        (
            "currentValue",
            serde_json::json!("${{ matrix.rust }} extra"),
        ),
        ("currentValue", serde_json::json!("stable")),
        ("depType", serde_json::json!("action")),
        ("skipReason", serde_json::json!("invalid-version")),
        ("warnings", serde_json::json!([{"message":"lookup failed"}])),
        ("updates", serde_json::json!([{"updateType":"major"}])),
    ] {
        let mut report = matrix_input_report("${{ matrix.rust }}");
        report["repositories"]["local"]["packageFiles"]["github-actions"][0]["deps"][0][field] =
            value;
        let output = report_gate(&repository, &base, &offered, &report);
        assert_eq!(refusal(&output).rule, "push.evidence", "{field}");
    }
    let mut wrong_manager = matrix_input_report("${{ matrix.rust }}");
    let managers = wrong_manager["repositories"]["local"]["packageFiles"]
        .as_object_mut()
        .unwrap();
    let inputs = managers.remove("github-actions").unwrap();
    managers.insert("cargo".to_owned(), inputs);
    assert_eq!(
        refusal(&report_gate(&repository, &base, &offered, &wrong_manager)).rule,
        "push.evidence"
    );
}

#[test]
fn renovate_admits_supported_java_matrix_diagnostics() {
    let (repository, base, offered) = renovate_fixture("renovate-java-matrix-inputs");
    let mut report = matrix_input_report("${{ matrix.java }}");
    let dependency =
        &mut report["repositories"]["local"]["packageFiles"]["github-actions"][0]["deps"][0];
    dependency["depName"] = "java-jdk".into();
    dependency["packageName"] = "java-jdk".into();
    dependency["datasource"] = "java-version".into();
    dependency["skipReason"] = "unsupported-version".into();
    let output = report_gate(&repository, &base, &offered, &report);
    assert!(output.status.success(), "{output:?}");
    for (field, value) in [
        ("currentValue", serde_json::json!("latest")),
        ("currentValue", serde_json::json!("21.x")),
        ("datasource", serde_json::json!("docker")),
        ("depName", serde_json::json!("unknown")),
        ("packageName", serde_json::json!("java-jre")),
        ("warnings", serde_json::json!([{"message":"lookup failed"}])),
        ("updates", serde_json::json!([{"updateType":"major"}])),
    ] {
        let mut invalid = report.clone();
        invalid["repositories"]["local"]["packageFiles"]["github-actions"][0]["deps"][0][field] =
            value;
        assert_eq!(
            refusal(&report_gate(&repository, &base, &offered, &invalid)).rule,
            "push.evidence",
            "{field}"
        );
    }
}

fn container_reference_report(value: &str) -> serde_json::Value {
    serde_json::json!({"repositories":{"local":{"packageFiles":{"github-actions":[{
        "packageFile":".github/workflows/release.yml","deps":[{
            "skipReason":"contains-variable","replaceString":value,
            "datasource":"docker","depType":"container","updates":[],
            "autoReplaceStringTemplate":"{{depName}}{{#if newValue}}:{{newValue}}{{/if}}"
        }]
    }]}}}})
}

#[test]
fn renovate_admits_only_supported_nameless_container_references() {
    let (repository, base, offered) = renovate_fixture("renovate-container-references");
    for value in [
        "${{ matrix.image }}",
        "${{ matrix.container && matrix.container.image || null }}",
    ] {
        let report = container_reference_report(value);
        let output = report_gate(&repository, &base, &offered, &report);
        assert!(output.status.success(), "{value}: {output:?}");
    }
    let report =
        container_reference_report("${{ matrix.container && matrix.container.image || null }}");
    for (field, value) in [
        ("replaceString", serde_json::json!("rust:1.95")),
        (
            "replaceString",
            serde_json::json!("${{ matrix.image }}:latest"),
        ),
        ("replaceString", serde_json::json!("${{ env.IMAGE }}")),
        (
            "replaceString",
            serde_json::json!("${{ matrix.image || 'rust:old' }}"),
        ),
        (
            "replaceString",
            serde_json::json!("${{ matrix.container && matrix.other.image || null }}"),
        ),
        (
            "replaceString",
            serde_json::json!("${{ matrix.container && matrix.container.image || 'rust:old' }}"),
        ),
        ("replaceString", serde_json::json!("${{ matrix. }}")),
        ("replaceString", serde_json::json!("${{matrix..image}}")),
        ("replaceString", serde_json::Value::Null),
        ("datasource", serde_json::json!("github-tags")),
        ("depType", serde_json::json!("action")),
        ("skipReason", serde_json::json!("internal-error")),
        ("depName", serde_json::json!("")),
        ("depName", serde_json::Value::Null),
        ("currentValue", serde_json::json!("1.95")),
        ("currentDigest", serde_json::json!("sha256:missing")),
        ("lockedVersion", serde_json::json!("1.95")),
        ("currentVersion", serde_json::json!("1.95")),
        ("autoReplaceStringTemplate", serde_json::json!(false)),
        ("warnings", serde_json::json!([{"message":"lookup failed"}])),
        ("warnings", serde_json::json!({})),
        ("updates", serde_json::json!([{"updateType":"major"}])),
        ("updates", serde_json::Value::Null),
        ("skipStage", serde_json::json!("lookup")),
        ("unknownField", serde_json::json!(true)),
    ] {
        let mut invalid = report.clone();
        invalid["repositories"]["local"]["packageFiles"]["github-actions"][0]["deps"][0][field] =
            value;
        assert_eq!(
            refusal(&report_gate(&repository, &base, &offered, &invalid)).rule,
            "push.evidence",
            "{field}"
        );
    }
    let mut wrong_manager = report.clone();
    let managers = wrong_manager["repositories"]["local"]["packageFiles"]
        .as_object_mut()
        .unwrap();
    let files = managers.remove("github-actions").unwrap();
    managers.insert("dockerfile".to_owned(), files);
    assert_eq!(
        refusal(&report_gate(&repository, &base, &offered, &wrong_manager)).rule,
        "push.evidence"
    );
    let mut malformed_file = report.clone();
    malformed_file["repositories"]["local"]["packageFiles"]["github-actions"][0]["lockFiles"] =
        serde_json::json!(false);
    assert_eq!(
        refusal(&report_gate(&repository, &base, &offered, &malformed_file)).rule,
        "push.evidence"
    );
    for field in [
        "skipReason",
        "replaceString",
        "datasource",
        "depType",
        "updates",
    ] {
        let mut invalid = report.clone();
        invalid["repositories"]["local"]["packageFiles"]["github-actions"][0]["deps"][0]
            .as_object_mut()
            .unwrap()
            .remove(field);
        assert_eq!(
            refusal(&report_gate(&repository, &base, &offered, &invalid)).rule,
            "push.evidence",
            "{field}"
        );
    }
}

#[test]
fn renovate_refuses_wrong_warning_types_even_for_excluded_dependencies() {
    let (repository, base, offered) = renovate_fixture("renovate-warning-types");
    let mut accepted = Vec::new();
    for reason in [None, Some("path-dependency")] {
        for warnings in [serde_json::json!({}), serde_json::json!(false)] {
            let mut report = current_renovate_report();
            let dependency =
                &mut report["repositories"]["local"]["packageFiles"]["cargo"][0]["deps"][0];
            dependency["warnings"] = warnings;
            if let Some(reason) = reason {
                dependency["skipReason"] = reason.into();
            }
            let output = report_gate(&repository, &base, &offered, &report);
            if output.status.success() {
                accepted.push(reason);
            } else {
                assert_eq!(refusal(&output).rule, "push.evidence");
            }
        }
    }
    assert_eq!(
        accepted,
        Vec::<Option<&str>>::new(),
        "wrong warning types accepted"
    );
}

#[test]
fn renovate_honors_pending_checks_and_still_blocks_eligible_updates() {
    let (repository, base, offered) = renovate_fixture("renovate-pending-checks");
    let mut pending = current_renovate_report();
    pending["repositories"]["local"]["packageFiles"]["cargo"][0]["deps"][0]["updates"] = serde_json::json!([
        {"updateType":"patch","newValue":"1.0.3","pendingChecks":true}
    ]);
    let output = report_gate(&repository, &base, &offered, &pending);
    assert!(
        output.status.success(),
        "pending update must wait: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let mut eligible = current_renovate_report();
    eligible["repositories"]["local"]["packageFiles"]["cargo"][0]["deps"][0]["updates"] = serde_json::json!([
        {"updateType":"patch","newValue":"1.0.2","pendingChecks":false}
    ]);
    let eligible_output = report_gate(&repository, &base, &offered, &eligible);
    assert_eq!(refusal(&eligible_output).rule, "push.dependencies");
    eprintln!("{}", String::from_utf8_lossy(&eligible_output.stderr));
    pending["repositories"]["local"]["packageFiles"]["cargo"][0]["deps"][0]["updates"]
        .as_array_mut()
        .unwrap()
        .push(serde_json::json!({"updateType":"patch","newValue":"1.0.2"}));
    let rejected = refusal(&report_gate(&repository, &base, &offered, &pending));
    assert_eq!(rejected.rule, "push.dependencies");
    assert!(rejected.evidence.iter().any(|line| line.contains("1.0.2")));
    assert!(rejected.evidence.iter().all(|line| !line.contains("1.0.3")));
    for field in ["skipReason", "warnings"] {
        let mut failed = current_renovate_report();
        let dependency =
            &mut failed["repositories"]["local"]["packageFiles"]["cargo"][0]["deps"][0];
        dependency["updates"] =
            serde_json::json!([{"updateType":"patch","newValue":"1.0.3","pendingChecks":true}]);
        dependency[field] = if field == "skipReason" {
            serde_json::json!("github-token-required")
        } else {
            serde_json::json!([{"message":"lookup failed"}])
        };
        assert_eq!(
            refusal(&report_gate(&repository, &base, &offered, &failed)).rule,
            "push.evidence"
        );
    }
}

#[test]
fn renovate_refuses_malformed_pending_flags_before_filtering() {
    let (repository, base, offered) = renovate_fixture("renovate-pending-types");
    let mut accepted = Vec::new();
    for value in [
        serde_json::json!("true"),
        serde_json::Value::Null,
        serde_json::json!(1),
        serde_json::json!({}),
        serde_json::json!([]),
    ] {
        let mut report = current_renovate_report();
        report["repositories"]["local"]["packageFiles"]["cargo"][0]["deps"][0]["updates"] = serde_json::json!([
            {"updateType":"major","pendingChecks":value}
        ]);
        let output = report_gate(&repository, &base, &offered, &report);
        if output.status.success() {
            accepted.push(value);
        } else {
            assert_eq!(refusal(&output).rule, "push.evidence");
        }
    }
    assert_eq!(
        accepted,
        Vec::<serde_json::Value>::new(),
        "malformed pending flags accepted"
    );
}

#[test]
fn renovate_refuses_incomplete_reports_even_when_no_updates_are_listed() {
    let (repository, base, offered) = renovate_fixture("renovate-incomplete-report");
    let mut reports = Vec::new();
    for reason in [
        "github-token-required",
        "internal-error",
        "invalid-version",
        "unsupported-datasource",
        "unknown-registry",
    ] {
        let mut report = current_renovate_report();
        report["repositories"]["local"]["packageFiles"]["cargo"][0]["deps"][0]["skipReason"] =
            reason.into();
        reports.push((reason, report));
    }
    let mut warning = current_renovate_report();
    warning["repositories"]["local"]["packageFiles"]["cargo"][0]["deps"][0]["warnings"] = serde_json::json!([{"topic":"fixture","message":"Failed to look up crate package fixture"}]);
    reports.push(("lookup-warning", warning));
    let mut problem = current_renovate_report();
    problem["repositories"]["local"]["problems"] =
        serde_json::json!({"level":40,"msg":"GitHub token is required for some dependencies"});
    reports.push(("repository-problem", problem));
    let mut missing = current_renovate_report();
    missing["repositories"]["local"]["packageFiles"]["cargo"][0]["deps"][0]
        .as_object_mut()
        .unwrap()
        .remove("updates");
    reports.push(("missing-updates", missing));
    let mut missing = current_renovate_report();
    missing["repositories"]["local"]["packageFiles"]["cargo"][0]
        .as_object_mut()
        .unwrap()
        .remove("deps");
    reports.push(("missing-dependencies", missing));
    let mut accepted = Vec::new();
    for (name, report) in reports {
        let output = report_gate(&repository, &base, &offered, &report);
        if output.status.success() {
            accepted.push(name);
        } else {
            let rejected = refusal(&output);
            assert_eq!(rejected.rule, "push.evidence", "{name}");
            assert!(
                rejected.cause.contains("report could not be validated"),
                "{name}"
            );
        }
    }
    assert_eq!(
        accepted,
        Vec::<&str>::new(),
        "incomplete reports accepted as current"
    );
}

#[test]
fn renovate_refuses_malformed_update_elements_before_filtering() {
    let (repository, base, offered) = renovate_fixture("renovate-malformed-updates");
    let updates = [
        ("null", serde_json::Value::Null),
        ("empty", serde_json::json!({})),
        ("empty-type", serde_json::json!({"updateType":""})),
        ("wrong-type", serde_json::json!({"updateType":[]})),
        ("wrong-bucket", serde_json::json!({"bucket":false})),
        (
            "string-breaking",
            serde_json::json!({"updateType":"major","isBreaking":"true"}),
        ),
        (
            "null-breaking",
            serde_json::json!({"updateType":"major","isBreaking":null}),
        ),
        (
            "numeric-breaking",
            serde_json::json!({"updateType":"major","isBreaking":1}),
        ),
        (
            "wrong-value",
            serde_json::json!({"updateType":"major","newValue":{}}),
        ),
        (
            "wrong-digest",
            serde_json::json!({"updateType":"major","newDigest":[]}),
        ),
    ];
    let mut accepted = Vec::new();
    for (name, update) in updates {
        let mut report = current_renovate_report();
        report["repositories"]["local"]["packageFiles"]["cargo"][0]["deps"][0]["updates"] =
            serde_json::json!([update]);
        let output = report_gate(&repository, &base, &offered, &report);
        if output.status.success() {
            accepted.push(name);
        } else {
            assert_eq!(refusal(&output).rule, "push.evidence", "{name}");
        }
    }
    for field in ["currentValue", "currentDigest", "replaceString"] {
        let mut report = current_renovate_report();
        let dependency =
            &mut report["repositories"]["local"]["packageFiles"]["cargo"][0]["deps"][0];
        dependency[field] = serde_json::json!({});
        dependency["updates"] = serde_json::json!([{"updateType":"major"}]);
        let output = report_gate(&repository, &base, &offered, &report);
        if output.status.success() {
            accepted.push(field);
        } else {
            assert_eq!(refusal(&output).rule, "push.evidence", "{field}");
        }
    }
    assert_eq!(
        accepted,
        Vec::<&str>::new(),
        "malformed updates accepted as current"
    );
}

#[test]
fn renovate_accepts_current_and_explicitly_excluded_dependencies_and_preserves_breaking_updates() {
    let (repository, base, offered) = renovate_fixture("renovate-valid-report");
    let mut reports = vec![("current", current_renovate_report())];
    for reason in [
        "path-dependency",
        "internal-package",
        "ignored",
        "disabled",
        "is-pinned",
    ] {
        let mut report = current_renovate_report();
        report["repositories"]["local"]["packageFiles"]["cargo"][0]["deps"][0]["skipReason"] =
            reason.into();
        reports.push((reason, report));
    }
    for update_type in ["major", "minor"] {
        let mut report = current_renovate_report();
        report["repositories"]["local"]["packageFiles"]["cargo"][0]["deps"][0]["updates"] =
            serde_json::json!([{"updateType":update_type,"newValue":"2.0.0","isBreaking":true}]);
        reports.push((update_type, report));
    }
    let mut optional = current_renovate_report();
    optional["repositories"]["local"]["packageFiles"]["cargo"][0]["deps"][0]["updates"] =
        serde_json::json!([{"bucket":"major"}]);
    reports.push(("optional-fields-and-bucket", optional));
    let mut nullable = current_renovate_report();
    let dependency = &mut nullable["repositories"]["local"]["packageFiles"]["cargo"][0]["deps"][0];
    dependency["currentValue"] = serde_json::Value::Null;
    dependency["updates"] =
        serde_json::json!([{"updateType":"major","newValue":null,"newDigest":null}]);
    reports.push(("nullable-values", nullable));
    for (name, report) in reports {
        let output = report_gate(&repository, &base, &offered, &report);
        assert!(
            output.status.success(),
            "{name}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
}

#[test]
fn a_rewrite_is_refused_with_the_integration_step_and_a_bypass_for_exactly_that_update() {
    let (repository, base) = published("rewrite");
    let published = repository.commit("published", &[&base]);
    repository.git(&["update-ref", "refs/remotes/origin/main", &published]);
    let local = repository.commit("local", &[&base]);
    let refused = refusal(&repository.dotguard(
        &["pre-push", "origin"],
        &format!("refs/heads/main {local} refs/heads/main {published}\n"),
        &SIGNED,
    ));
    assert_eq!(refused.rule, "push.history");
    assert_eq!(
        refused.next.as_deref().unwrap_or_default(),
        "git pull --rebase origin main"
    );
    assert_eq!(
        refused.bypass.as_deref(),
        Some("ALLOW_FORCE=1 git push --force-with-lease origin refs/heads/main:refs/heads/main")
    );
}

/// The stack tooling pushes a rewrite while it holds the lease, and never a deletion.
#[test]
fn only_the_leased_stack_tooling_pushes_a_rewrite() {
    let (repository, base) = published("stack-lease");
    let published = repository.commit("published", &[&base]);
    repository.git(&["update-ref", "refs/remotes/origin/main", &published]);
    let local = repository.commit("local", &[&base]);
    let rewrite = format!("refs/heads/main {local} refs/heads/main {published}\n");
    let token = [("DOTGUARD_STACK", "token")];
    let unleased = repository.dotguard_with(&["pre-push", "origin"], &rewrite, &[], &token);
    assert_eq!(refusal(&unleased).rule, "push.history");
    std::fs::write(repository.path().join(".git/dotguard-stack.lease"), "token").unwrap();
    let leased = repository.dotguard_with(&["pre-push", "origin"], &rewrite, &[], &token);
    assert!(
        leased.status.success(),
        "{}",
        String::from_utf8_lossy(&leased.stderr)
    );
    let deletion = format!("(delete) {ZERO} refs/heads/topic {base}\n");
    let deleted = repository.dotguard_with(&["pre-push", "origin"], &deletion, &[], &token);
    assert_eq!(refusal(&deleted).rule, "push.history");
}

#[test]
fn a_deletion_is_refused_with_the_forge_step_and_a_waiver_that_repeats_it() {
    let (repository, base) = published("delete");
    let refused = refusal(&repository.dotguard(
        &["pre-push", "origin"],
        &format!("(delete) {ZERO} refs/heads/topic {base}\n"),
        &SIGNED,
    ));
    assert_eq!(refused.rule, "push.history");
    assert_eq!(
        refused.next.as_deref().unwrap_or_default(),
        "gh api -X DELETE 'repos/{owner}/{repo}/git/refs/heads/topic'"
    );
    assert_eq!(
        refused.bypass.as_deref(),
        Some("ALLOW_FORCE=1 git push origin :refs/heads/topic")
    );
}

#[test]
fn one_linear_unsigned_branch_is_rebased_onto_the_parent_of_its_oldest_unsigned_commit() {
    let (repository, base) = published("linear");
    let first = repository.commit("first", &[&base]);
    let second = repository.commit("second", &[&first]);
    repository.git(&["update-ref", "refs/heads/topic", &second]);
    let refused = refusal(&repository.dotguard(
        &["pre-push", "origin"],
        &format!("refs/heads/topic {second} refs/heads/topic {ZERO}\n"),
        &SIGNED,
    ));
    assert_eq!(refused.rule, "push.signature");
    assert_eq!(
        refused.next.as_deref().unwrap_or_default(),
        format!("git rebase --exec 'git commit --amend --no-edit -S' {base} topic")
    );
    assert_eq!(refused.evidence.len(), 2);
}

/// An amend moves only the checked-out branch, so the refusal lists a tag at the checked-out commit instead of amending it.
#[test]
fn an_unsigned_tag_at_the_checked_out_commit_is_listed_instead_of_amended() {
    let (repository, base) = published("tag");
    let tip = repository.commit("tip", &[&base]);
    repository.git(&["update-ref", "HEAD", &tip]);
    repository.git(&["tag", "v1", &tip]);
    let refused = refusal(&repository.dotguard(
        &["pre-push", "origin"],
        &format!("refs/tags/v1 {tip} refs/tags/v1 {ZERO}\n"),
        &SIGNED,
    ));
    assert_eq!(refused.rule, "push.signature");
    assert_eq!(
        refused.next.as_deref().unwrap_or_default(),
        format!("git log '--format=%h %G? %s' {tip} --not --remotes=origin")
    );
    let refused = refusal(&repository.dotguard(
        &["pre-push", "origin"],
        &format!("refs/heads/main {tip} refs/heads/main {base}\n"),
        &SIGNED,
    ));
    assert_eq!(
        refused.next.as_deref().unwrap_or_default(),
        "git commit --amend -S --no-edit"
    );
}

#[test]
fn several_refs_or_a_merge_are_listed_instead_of_rebased() {
    let (repository, base) = published("several");
    let left = repository.commit("left", &[&base]);
    let right = repository.commit("right", &[&base]);
    let refused = refusal(&repository.dotguard(
        &["pre-push", "origin"],
        &format!(
            "refs/heads/left {left} refs/heads/left {ZERO}\nrefs/heads/right {right} refs/heads/right {ZERO}\n"
        ),
        &SIGNED,
    ));
    assert_eq!(refused.rule, "push.signature");
    assert_eq!(
        refused.next.as_deref().unwrap_or_default(),
        format!("git log '--format=%h %G? %s' {left} --not --remotes=origin")
    );
    assert_eq!(refused.evidence.len(), 2);

    let merge = repository.commit("merge", &[&left, &right]);
    let refused = refusal(&repository.dotguard(
        &["pre-push", "origin"],
        &format!("refs/heads/main {merge} refs/heads/main {base}\n"),
        &SIGNED,
    ));
    assert_eq!(
        refused.next.as_deref().unwrap_or_default(),
        format!("git log '--format=%h %G? %s' {merge} --not --remotes=origin")
    );
    assert_eq!(refused.evidence.len(), 3);
}

impl Repository {
    /// Runs the post-commit gate with signing configured to a program that doesn't exist, so every signature fails.
    fn failed_signature(&self) -> Refusal {
        let missing = self.path().join("missing");
        let missing = missing.to_str().unwrap();
        let refused = refusal(&self.dotguard(
            &["post-commit"],
            "",
            &[
                ("commit.gpgsign", "true"),
                ("gpg.format", "ssh"),
                ("user.signingkey", missing),
                ("gpg.ssh.program", missing),
            ],
        ));
        assert_eq!(refused.rule, "commit.signature");
        assert_eq!(refused.bypass, None);
        refused
    }
}

#[test]
fn a_failed_signature_rolls_the_commit_back_and_names_the_commit_to_repeat() {
    let repository = Repository::new("signing");
    let base = repository.commit("base", &[]);
    let unsigned = repository.commit("unsigned", &[&base]);
    repository.git(&["update-ref", "HEAD", &unsigned]);
    let refused = repository.failed_signature();
    assert_eq!(
        refused.next.as_deref().unwrap_or_default(),
        format!("git commit --reuse-message={unsigned}")
    );
    assert_eq!(refused.bypass, None);
    assert_eq!(repository.git(&["rev-parse", "HEAD"]), base);
}

/// Git removes `MERGE_HEAD` before the hook runs, so the rollback writes the second parent back.
#[test]
fn a_rolled_back_merge_keeps_its_second_parent_for_the_next_commit() {
    let repository = Repository::new("merge-rollback");
    let base = repository.commit("base", &[]);
    let left = repository.commit("left", &[&base]);
    let right = repository.commit("right", &[&base]);
    repository.git(&["update-ref", "HEAD", &left]);
    let merge = repository.commit("merge", &[&left, &right]);
    repository.git(&["update-ref", "-m", "commit (merge): merge", "HEAD", &merge]);
    repository.failed_signature();
    assert_eq!(repository.git(&["rev-parse", "HEAD"]), left);
    assert_eq!(repository.git(&["rev-parse", "MERGE_HEAD"]), right);
    assert_eq!(
        repository.git(&["write-tree"]),
        repository.git(&["rev-parse", &format!("{merge}^{{tree}}")])
    );
}

/// An amend replaced the commit `HEAD` named before, so the rollback restores that commit instead of its parent.
#[test]
fn a_rolled_back_amend_restores_the_commit_it_replaced() {
    let repository = Repository::new("amend-rollback");
    let base = repository.commit("base", &[]);
    let original = repository.commit("original", &[&base]);
    repository.git(&["update-ref", "-m", "commit: original", "HEAD", &original]);
    let amended = repository.commit("amended", &[&base]);
    repository.git(&[
        "update-ref",
        "-m",
        "commit (amend): amended",
        "HEAD",
        &amended,
    ]);
    repository.failed_signature();
    assert_eq!(repository.git(&["rev-parse", "HEAD"]), original);
    assert_eq!(
        repository.git(&["write-tree"]),
        repository.git(&["rev-parse", &format!("{amended}^{{tree}}")])
    );
}

#[test]
fn every_sign_lock_skip_lands_in_the_bypass_log() {
    let repository = Repository::new("sign-lock");
    let base = repository.commit("base", &[]);
    repository.git(&["update-ref", "HEAD", &base]);
    let skipped = repository.dotguard_with(
        &["post-commit"],
        "",
        &SIGNED,
        &[("GIT_HOOK_FORCE_SIGN_LOCK", "1")],
    );
    assert!(skipped.status.success());
    let log =
        std::fs::read_to_string(repository.path().join(".local/state/git-bypass.log")).unwrap();
    assert!(
        log.lines()
            .any(|line| line.contains("\tskipped\tsign-lock\t")
                && line.contains("reason=the caller set GIT_HOOK_FORCE_SIGN_LOCK")
                && line.contains(&base)),
        "{log}"
    );
}
