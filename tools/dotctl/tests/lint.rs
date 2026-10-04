//! Every lint kind, on a file that should pass and a file that should fail.
//!
//! A validator that never fails is worse than none, and these ran green against the whole repo on their first try, so the failing half is the half that carries the information.
//! The tests drive the built binary rather than the modules, because the exit code is what lefthook reads.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

/// Repo root: the tests need the real `.yamllint` and `PSScriptAnalyzerSettings.psd1`, and `dotctl` resolves both from there.
fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .expect("crate lives at <repo>/tools/dotctl")
        .to_path_buf()
}

fn lint(kind: &str, files: &[&Path]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_dotctl"))
        .current_dir(repo_root())
        .arg("lint")
        .arg(kind)
        .args(files)
        .output()
        .expect("dotctl runs")
}

/// Writes one fixture and returns its path; the directory owns the file.
fn fixture(dir: &tempfile::TempDir, name: &str, content: &str) -> PathBuf {
    let path = dir.path().join(name);
    std::fs::write(&path, content).expect("fixture written");
    path
}

#[track_caller]
fn assert_verdict(kind: &str, name: &str, content: &str, should_pass: bool) {
    let dir = tempfile::tempdir().expect("temp dir");
    let path = fixture(&dir, name, content);
    let out = lint(kind, &[&path]);
    let passed = out.status.success();
    assert_eq!(
        passed,
        should_pass,
        "{kind} on {name}: expected pass={should_pass}, got pass={passed}\n\
         stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr),
    );
}

#[test]
fn toml_separates_valid_from_broken() {
    assert_verdict("toml", "ok.toml", "key = \"value\"\n", true);
    assert_verdict("toml", "bad.toml", "key = \n", false);
}

#[test]
fn json_separates_valid_from_broken() {
    assert_verdict("json", "ok.json", "{\"a\": 1}\n", true);
    assert_verdict("json", "bad.json", "{\"a\": }\n", false);
}

#[test]
fn gitconfig_separates_valid_from_broken() {
    assert_verdict("gitconfig", "ok.gitconfig", "[user]\n\tname = x\n", true);
    assert_verdict("gitconfig", "bad.gitconfig", "[user\n\tname = x\n", false);
}

#[test]
fn yaml_separates_valid_from_broken() {
    assert_verdict("yaml", "ok.yaml", "---\na: 1\n", true);
    assert_verdict("yaml", "bad.yaml", "---\na: [1, 2\n", false);
}

#[test]
fn typos_separates_clean_from_misspelled() {
    assert_verdict("typos", "ok.md", "a sentence with no mistakes\n", true);
    // Split so the fixture's own misspelling is not a misspelling in this file, which typos also checks.
    // An extend-words entry would hide the real thing just as effectively.
    let misspelled = concat!("t", "eh", " quick brown fox\n");
    assert_verdict("typos", "bad.md", misspelled, false);
}

#[test]
fn shellcheck_separates_clean_from_suspect() {
    assert_verdict("shellcheck", "ok.sh", "#!/bin/sh\necho hello\n", true);
    assert_verdict(
        "shellcheck",
        "bad.sh",
        "#!/bin/sh\necho $undefined\n",
        false,
    );
}

#[test]
fn powershell_reports_analyzer_findings() {
    assert_verdict("ps1", "ok.ps1", "Write-Output 'hello'\n", true);
    // PSAvoidUsingWriteHost, a Warning, and not on the repo's exclude list.
    assert_verdict("ps1", "bad.ps1", "Write-Host 'hello'\n", false);
}

/// The catch-all kind: a template that does not render is a failure even when no per-format validator claims its extension.
#[test]
fn template_requires_a_successful_render() {
    assert_verdict("template", "ok.tmpl", "plain {{ \"text\" }}\n", true);
    assert_verdict("template", "bad.tmpl", "{{ .does.not.exist }}\n", false);
}

/// Templates are rendered before validation, so a template that renders to broken TOML fails the TOML kind, not the template kind.
#[test]
fn render_happens_before_validation() {
    assert_verdict("toml", "ok.toml.tmpl", "key = {{ \"1\" }}\n", true);
    assert_verdict("toml", "bad.toml.tmpl", "key = {{ \"\" }}\n", false);
}

/// A staged file can be deleted in the same commit, so a path that no longer exists is skipped rather than failing the run.
#[test]
fn missing_paths_are_skipped() {
    let out = lint("toml", &[Path::new("no/such/file.toml")]);
    assert!(out.status.success());
}
