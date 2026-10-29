use serde_json::{Value, json};
use std::io::Write;
use std::path::Path;
use std::process::Stdio;

#[path = "../../platform/tests/support/fixture_process.rs"]
mod fixture_process;

fn hook(root: &Path, command: &str, payload: &Value) -> (bool, String) {
    let mut child = fixture_process::command(env!("CARGO_BIN_EXE_dotfiles"), root)
        .arg(command)
        .current_dir(root)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(payload.to_string().as_bytes())
        .unwrap();
    let output = child.wait_with_output().unwrap();
    (
        output.status.success(),
        format!(
            "{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        ),
    )
}

fn write_event(root: &Path, path: &str) -> Value {
    json!({"hook_event_name":"PostToolUse", "cwd":root, "tool_name":"Write", "tool_input":{"file_path":path}})
}

#[test]
fn a_shell_finding_blocks_and_the_fixed_file_is_admitted() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("probe.sh");
    std::fs::write(&path, "#!/bin/sh\necho $undefined\n").unwrap();
    let (_, blocked) = hook(
        root.path(),
        "lint-hook",
        &write_event(root.path(), "probe.sh"),
    );
    assert!(blocked.contains("SC"), "{blocked}");
    assert!(blocked.contains("shellcheck"), "{blocked}");
    std::fs::write(&path, "#!/bin/sh\nprintf '%s\\n' 'clean'\n").unwrap();
    let (success, clean) = hook(
        root.path(),
        "lint-hook",
        &write_event(root.path(), "probe.sh"),
    );
    assert!(success && !clean.contains("block"), "{clean}");
}

#[test]
fn a_python_finding_blocks() {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("probe.py"), "import os\n").unwrap();
    let (_, blocked) = hook(
        root.path(),
        "lint-hook",
        &write_event(root.path(), "probe.py"),
    );
    assert!(blocked.contains("F401"), "{blocked}");
}

#[test]
fn a_workflow_finding_blocks() {
    let root = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(root.path().join(".github/workflows")).unwrap();
    std::fs::write(root.path().join(".github/workflows/check.yml"), "on: push\njobs:\n  check:\n    runs-on: ubuntu-latest\n    steps:\n      - run: echo '${{ invalid.property }}'\n").unwrap();
    let (_, blocked) = hook(
        root.path(),
        "lint-hook",
        &write_event(root.path(), ".github/workflows/check.yml"),
    );
    assert!(blocked.contains("actionlint"), "{blocked}");
    assert!(blocked.contains("expression"), "{blocked}");
}

#[test]
fn a_repository_exclusion_is_admitted() {
    let root = tempfile::tempdir().unwrap();
    std::fs::create_dir(root.path().join(".git")).unwrap();
    std::fs::write(root.path().join("ruff.toml"), "exclude = [\"probe.py\"]\n").unwrap();
    std::fs::write(root.path().join("probe.py"), "import os\n").unwrap();
    let (success, clean) = hook(
        root.path(),
        "lint-hook",
        &write_event(root.path(), "probe.py"),
    );
    assert!(success && !clean.contains("block"), "{clean}");
}

#[test]
fn scripts_assigned_in_commands_are_linted_and_unresolved_paths_have_a_next_action() {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("probe.sh"), "#!/bin/sh\necho $undefined\n").unwrap();
    for command in [
        "f=probe.sh; bash \"$f\"",
        "ROOT=.; env bash \"$ROOT/probe.sh\"",
        "xargs bash probe.sh",
    ] {
        let (success, blocked) = hook(
            root.path(),
            "claude-bash",
            &json!({"tool_name":"Bash", "cwd":root.path(), "tool_input":{"command":command}}),
        );
        assert!(
            !success && blocked.contains("shellcheck"),
            "{command}: {blocked}"
        );
    }
    let (_, blocked) = hook(
        root.path(),
        "claude-bash",
        &json!({"tool_name":"Bash", "cwd":root.path(), "tool_input":{"command":"bash \"$ROOT/probe.sh\""}}),
    );
    assert!(blocked.contains("literal path"), "{blocked}");
    assert!(!blocked.contains("could not finish"), "{blocked}");
}

#[test]
fn a_wrapped_comment_sentence_blocks_and_one_sentence_per_line_is_admitted() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("probe.rs");
    std::fs::write(
        &path,
        "/// Creates the document a New command shows, with its caret placed\n/// after the front matter so typing starts in the body.\npub fn fresh() -> u8 {\n    1\n}\n",
    )
    .unwrap();
    let (_, blocked) = hook(
        root.path(),
        "lint-hook",
        &write_event(root.path(), "probe.rs"),
    );
    assert!(blocked.contains("\"decision\":\"block\""), "{blocked}");
    assert!(blocked.contains("ocomment"), "{blocked}");
    assert!(blocked.contains("TIDY"), "{blocked}");
    assert!(blocked.contains("ocomment fix --tidy"), "{blocked}");
    std::fs::write(
        &path,
        "/// Creates the document a New command shows, with its caret placed after the front matter so typing starts in the body.\npub fn fresh() -> u8 {\n    1\n}\n",
    )
    .unwrap();
    let (success, clean) = hook(
        root.path(),
        "lint-hook",
        &write_event(root.path(), "probe.rs"),
    );
    assert!(success && !clean.contains("block"), "{clean}");
}

#[test]
fn unsupported_binary_files_stay_with_existing_gates() {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("image.png"), [0xff, 0xfe]).unwrap();
    let (success, clean) = hook(
        root.path(),
        "lint-hook",
        &write_event(root.path(), "image.png"),
    );
    assert!(success && !clean.contains("block"), "{clean}");
}

#[test]
fn repository_configs_are_respected_for_biome_taplo_and_shellcheck() {
    let root = tempfile::tempdir().unwrap();
    std::fs::create_dir(root.path().join(".git")).unwrap();
    for (config, contents, path, source) in [
        (
            "biome.json",
            "{\"files\":{\"includes\":[\"**\",\"!probe.js\"]}}",
            "probe.js",
            "var a = 1\n",
        ),
        (
            "taplo.toml",
            "exclude = [\"probe.toml\"]\n",
            "probe.toml",
            "invalid=[\n",
        ),
        (
            ".shellcheckrc",
            "disable=SC2086,SC2154\n",
            "probe.sh",
            "#!/bin/sh\necho $undefined\n",
        ),
    ] {
        std::fs::write(root.path().join(config), contents).unwrap();
        std::fs::write(root.path().join(path), source).unwrap();
        let (success, clean) = hook(root.path(), "lint-hook", &write_event(root.path(), path));
        assert!(success && !clean.contains("block"), "{path}: {clean}");
    }
}

#[test]
fn biome_uses_the_project_installation_and_configuration_directory() {
    let scope = tempfile::tempdir().unwrap();
    let root = scope.path().join("project space 日本語");
    let bin = scope.path().join("bin");
    std::fs::create_dir_all(root.join(".git")).unwrap();
    std::fs::create_dir(&bin).unwrap();
    let executable = |name: &str| bin.join(format!("{name}{}", std::env::consts::EXE_SUFFIX));
    let native = std::env::split_paths(&std::env::var_os("PATH").unwrap())
        .map(|directory| directory.join(format!("biome{}", std::env::consts::EXE_SUFFIX)))
        .find(|path| path.is_file())
        .expect("the project pins Biome for its integration tests");
    let compiled = fixture_process::command("rustc", scope.path())
        .arg(Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/biome_project.rs"))
        .args(["--edition", "2024", "-o"])
        .arg(executable("mise"))
        .status()
        .unwrap();
    assert!(compiled.success());
    std::fs::copy(executable("mise"), executable("biome")).unwrap();
    let log = scope.path().join("selected");
    let run = |path: &Path, global: bool| {
        if log.exists() {
            std::fs::remove_file(&log).unwrap();
        }
        let mut directories = vec![bin.clone()];
        directories.extend(std::env::split_paths(&std::env::var_os("PATH").unwrap()));
        let mut child = fixture_process::command(env!("CARGO_BIN_EXE_dotfiles"), scope.path())
            .arg("lint-hook")
            .current_dir(&root)
            .env("PATH", std::env::join_paths(directories).unwrap())
            .env("BIOME_FIXTURE_NATIVE", &native)
            .env("BIOME_FIXTURE_LOG", &log)
            .env(
                "BIOME_FIXTURE_GLOBAL",
                if global { "allow" } else { "refuse" },
            )
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        let event = write_event(&root, path.to_str().unwrap());
        child
            .stdin
            .take()
            .unwrap()
            .write_all(event.to_string().as_bytes())
            .unwrap();
        let output = child.wait_with_output().unwrap();
        (
            output.status,
            String::from_utf8(output.stdout).unwrap() + &String::from_utf8(output.stderr).unwrap(),
            std::fs::read_to_string(&log).ok(),
        )
    };
    for directory in [&root, &root.join("web")] {
        std::fs::create_dir_all(directory).unwrap();
        let manifest = directory.join("package.json");
        std::fs::write(
            &manifest,
            r#"{"devDependencies":{"@biomejs/biome":"2.5.1"}}"#,
        )
        .unwrap();
        let config = directory.join("biome.json");
        std::fs::write(&config, r#"{"root":true,"files":{"includes":["*.js"]},"linter":{"rules":{"recommended":true}}}"#).unwrap();
        let package = directory.join("node_modules/@biomejs/biome");
        std::fs::create_dir_all(package.join("bin")).unwrap();
        std::fs::write(
            package.join("package.json"),
            r#"{"name":"@biomejs/biome","version":"2.5.1","bin":{"biome":"bin/biome"}}"#,
        )
        .unwrap();
        let launcher = package.join("bin/biome");
        std::fs::write(&launcher, "fixture launcher\n").unwrap();
        let path = directory.join("probe space 日本語.js");
        std::fs::write(&path, "console.log(1);\n").unwrap();
        let (status, clean, selected) = run(&path, false);
        assert_eq!(status.code(), Some(0), "{clean}");
        assert!(!clean.contains("block"), "{clean}");
        assert_eq!(selected.as_deref(), Some("local"));
        std::fs::write(&path, "debugger;\n").unwrap();
        let (status, blocked, selected) = run(&path, false);
        assert_eq!(status.code(), Some(0), "{blocked}");
        let decision: Value = serde_json::from_str(blocked.lines().next().unwrap()).unwrap();
        assert_eq!(decision["decision"], "block");
        assert!(blocked.contains("noDebugger"), "{blocked}");
        assert_eq!(selected.as_deref(), Some("local"));
        std::fs::write(&path, "console.log(1);\n").unwrap();
        std::fs::remove_file(&launcher).unwrap();
        let (status, missing, selected) = run(&path, true);
        assert_eq!(status.code(), Some(0), "{missing}");
        assert!(missing.contains("@biomejs/biome"), "{missing}");
        assert!(missing.contains("block"), "{missing}");
        assert_eq!(selected, None);
        std::fs::remove_file(&manifest).unwrap();
        let (status, fallback, selected) = run(&path, true);
        assert_eq!(status.code(), Some(0), "{fallback}");
        assert!(!fallback.contains("block"), "{fallback}");
        assert_eq!(selected.as_deref(), Some("global"));
        std::fs::remove_file(&config).unwrap();
    }
}

#[test]
fn patches_lint_only_written_destinations_and_ignore_deleted_files() {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("unrelated.py"), "import os\n").unwrap();
    std::fs::write(
        root.path().join("new.sh"),
        "#!/bin/sh\nprintf '%s\\n' 'clean'\n",
    )
    .unwrap();
    let (success, clean) = hook(
        root.path(),
        "lint-hook",
        &json!({"tool_name":"functions.apply_patch", "cwd":root.path(), "tool_input":"*** Begin Patch\n*** Update File: old.sh\n*** Move to: new.sh\n@@\n*** Delete File: deleted.py\n*** End Patch"}),
    );
    assert!(success && !clean.contains("block"), "{clean}");
}

#[derive(Debug)]
struct ActionlintFixture<'a> {
    scope: &'a Path,
    root: &'a Path,
    bin: &'a Path,
    native: &'a Path,
    project: &'a Path,
}

impl ActionlintFixture<'_> {
    fn run(
        &self,
        tools: &Value,
        program: &Path,
        which: &str,
        allow_global: bool,
        target: &Path,
    ) -> (String, String) {
        let metadata = self.scope.join("metadata.json");
        let log = self.scope.join("selected");
        std::fs::write(&metadata, tools.to_string()).unwrap();
        if log.exists() {
            std::fs::remove_file(&log).unwrap();
        }
        let mut directories = vec![self.bin.to_path_buf()];
        directories.extend(std::env::split_paths(&std::env::var_os("PATH").unwrap()));
        let mut child = fixture_process::command(env!("CARGO_BIN_EXE_dotfiles"), self.scope)
            .arg("lint-hook")
            .current_dir(self.root)
            .env("PATH", std::env::join_paths(directories).unwrap())
            .env("ACTIONLINT_FIXTURE_NATIVE", self.native)
            .env("ACTIONLINT_FIXTURE_LOG", &log)
            .env("ACTIONLINT_FIXTURE_CWD", target.parent().unwrap())
            .env("ACTIONLINT_FIXTURE_ROOT", self.root)
            .env("ACTIONLINT_FIXTURE_METADATA", &metadata)
            .env("ACTIONLINT_FIXTURE_EXECUTABLE", program)
            .env("ACTIONLINT_FIXTURE_PROJECT", self.project)
            .env("ACTIONLINT_FIXTURE_WHICH", which)
            .env(
                "ACTIONLINT_FIXTURE_GLOBAL",
                if allow_global { "allow" } else { "refuse" },
            )
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        child
            .stdin
            .take()
            .unwrap()
            .write_all(
                write_event(self.root, target.to_str().unwrap())
                    .to_string()
                    .as_bytes(),
            )
            .unwrap();
        let output = child.wait_with_output().unwrap();
        assert_eq!(output.status.code(), Some(0));
        (
            String::from_utf8(output.stdout).unwrap() + &String::from_utf8(output.stderr).unwrap(),
            std::fs::read_to_string(&log).unwrap_or_default(),
        )
    }
}

fn actionlint_refuses_invalid_metadata(
    fixture: &ActionlintFixture<'_>,
    entry: &Value,
    action: &Path,
    configuration: &Path,
) {
    for (field, value) in [
        ("installed", json!(false)),
        ("active", json!(false)),
        ("requested_version", json!("latest")),
        ("source", json!({"type":"environment","path":configuration})),
    ] {
        let mut invalid = entry.clone();
        invalid[field] = value;
        let (blocked, selected) = fixture.run(
            &json!({"github:kjanat/actionlint":[invalid]}),
            fixture.project,
            "ok",
            true,
            action,
        );
        assert!(blocked.contains("block"), "{field}: {blocked}");
        assert_eq!(selected, "metadata\n");
    }
    for invalid in [
        json!("invalid metadata"),
        json!({"github:kjanat/actionlint":[]}),
        json!({"github:kjanat/actionlint":[entry.clone(),entry.clone()]}),
        json!({"actionlint":[entry.clone()],"github:kjanat/actionlint":[entry.clone()]}),
        json!({"github:kjanat/actionlint":[{"installed":true}]}),
    ] {
        let (blocked, selected) = fixture.run(&invalid, fixture.project, "ok", true, action);
        assert!(blocked.contains("block"), "{blocked}");
        assert_eq!(selected, "metadata\n");
    }
}

fn actionlint_executables(
    scope: &Path,
    bin: &Path,
    project: &Path,
    global: &Path,
) -> std::path::PathBuf {
    let native = std::env::split_paths(&std::env::var_os("PATH").unwrap())
        .map(|directory| directory.join(format!("actionlint{}", std::env::consts::EXE_SUFFIX)))
        .find(|path| path.is_file())
        .expect("the project pins Actionlint for its integration tests");
    let compiled = bin.join(format!("mise{}", std::env::consts::EXE_SUFFIX));
    let status = fixture_process::command("rustc", scope)
        .arg(Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/actionlint_project.rs"))
        .args(["--edition", "2024", "-o"])
        .arg(&compiled)
        .status()
        .unwrap();
    assert!(status.success());
    for destination in [project, global] {
        std::fs::copy(&compiled, destination).unwrap();
    }
    native
}

#[test]
fn actionlint_uses_only_the_declared_project_installation() {
    let scope = tempfile::tempdir().unwrap();
    let root = scope.path().join("project space 日本語");
    let bin = scope.path().join("bin");
    let installation = scope.path().join("installed space 日本語");
    let action = root.join(".github/actions/probe/action.yml");
    for directory in [
        root.join(".git"),
        action.parent().unwrap().into(),
        bin.clone(),
        installation.clone(),
    ] {
        std::fs::create_dir_all(directory).unwrap();
    }
    let executable = |directory: &Path, name: &str| {
        directory.join(format!("{name}{}", std::env::consts::EXE_SUFFIX))
    };
    let project = executable(&installation, "actionlint");
    let global = executable(&bin, "actionlint");
    let native = actionlint_executables(scope.path(), &bin, &project, &global);
    let configuration = root.join("mise.toml");
    std::fs::write(
        &configuration,
        "[tools]\n\"github:kjanat/actionlint\" = \"1.17.0\"\n",
    )
    .unwrap();
    std::fs::write(
        root.join(".github/actionlint.yaml"),
        "self-hosted-runner:\n  labels: []\n",
    )
    .unwrap();
    std::fs::write(&action, "name: Example\ndescription: Example action\nruns:\n  using: composite\n  steps:\n    - shell: bash\n      run: printf '%s\\n' clean\n").unwrap();
    let entry = json!({
        "version":"1.17.0", "requested_version":"1.17.0", "install_path":installation,
        "source":{"type":"mise.toml","path":configuration}, "installed":true,"active":true
    });
    let fixture = ActionlintFixture {
        scope: scope.path(),
        root: &root,
        bin: &bin,
        native: &native,
        project: &project,
    };
    let run = |tools: &Value, program: &Path, which: &str, allow_global: bool| {
        fixture.run(tools, program, which, allow_global, &action)
    };
    let tools = json!({"github:kjanat/actionlint":[entry.clone()]});
    let (clean, selected) = run(&tools, &project, "ok", false);
    assert!(!clean.contains("block"), "{clean}");
    assert_eq!(selected, "metadata\nwhich\nproject\n");
    let workflow = root.join(".github/workflows/probe.yml");
    std::fs::create_dir_all(workflow.parent().unwrap()).unwrap();
    std::fs::write(&workflow, "on: push\njobs:\n  check:\n    runs-on: ubuntu-latest\n    steps:\n      - run: echo '${{ invalid.property }}'\n").unwrap();
    let (blocked, selected) = fixture.run(&tools, &project, "ok", false, &workflow);
    assert!(
        blocked.contains("block") && blocked.contains("expression"),
        "{blocked}"
    );
    assert_eq!(selected, "metadata\nwhich\nproject\n");
    actionlint_refuses_invalid_metadata(&fixture, &entry, &action, &configuration);
    for (program, which) in [
        (project.as_path(), "fail"),
        (global.as_path(), "ok"),
        (Path::new("actionlint"), "ok"),
        (installation.join("missing").as_path(), "ok"),
    ] {
        let (blocked, selected) = run(&tools, program, which, true);
        assert!(blocked.contains("block"), "{blocked}");
        assert_eq!(selected, "metadata\nwhich\n");
    }
    let (clean, selected) = run(&json!({}), &project, "ok", true);
    assert!(!clean.contains("block"), "{clean}");
    assert_eq!(selected, "metadata\nglobal\n");
    let inherited_configuration = scope.path().join("mise.toml");
    std::fs::write(
        &inherited_configuration,
        "[tools]\nactionlint = \"1.17.0\"\n",
    )
    .unwrap();
    let mut inherited = entry;
    inherited["source"]["path"] = json!(inherited_configuration);
    let (clean, selected) = run(
        &json!({"github:kjanat/actionlint":[inherited]}),
        &project,
        "ok",
        true,
    );
    assert!(!clean.contains("block"), "{clean}");
    assert_eq!(selected, "metadata\nglobal\n");
}

#[test]
fn clean_action_metadata_is_checked_through_actionlint() {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("action.yml"), "name: Example\ndescription: Example action\nruns:\n  using: composite\n  steps:\n    - shell: bash\n      run: printf '%s\\n' clean\n").unwrap();
    let (success, clean) = hook(
        root.path(),
        "lint-hook",
        &write_event(root.path(), "action.yml"),
    );
    assert!(success && !clean.contains("block"), "{clean}");
}

#[test]
fn assignments_inside_scripts_and_all_invocation_forms_are_inspected() {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("probe.sh"), "#!/bin/sh\necho $undefined\n").unwrap();
    std::fs::write(
        root.path().join("wrapper.sh"),
        "#!/bin/sh\nf=probe.sh\nsh \"${f}\"\n",
    )
    .unwrap();
    for command in [
        "sh wrapper.sh",
        "f=probe.sh; sh \"${f}\"",
        "./probe.sh",
        "source probe.sh",
        ". probe.sh",
        "env sh probe.sh",
        "xargs -n 1 env sh probe.sh",
    ] {
        let (success, blocked) = hook(
            root.path(),
            "claude-bash",
            &json!({"tool_name":"Bash", "cwd":root.path(), "tool_input":{"command":command}}),
        );
        assert!(
            !success && blocked.contains("shellcheck"),
            "{command}: {blocked}"
        );
    }
}

#[test]
fn configuration_outside_a_repository_cannot_disable_strict_defaults() {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(
        root.path().join("biome.json"),
        "{\"files\":{\"includes\":[\"**\",\"!probe.js\"]}}",
    )
    .unwrap();
    std::fs::write(root.path().join("probe.js"), "var a = 1\n").unwrap();
    let (_, blocked) = hook(
        root.path(),
        "lint-hook",
        &write_event(root.path(), "probe.js"),
    );
    assert!(blocked.contains("block"), "{blocked}");
}

#[test]
fn opencode_blocks_a_shell_finding_and_admits_its_fix() {
    let root = tempfile::tempdir().unwrap();
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/lint-adapter.ts");
    for (source, expected) in [
        ("#!/bin/sh\necho $undefined\n", "blocked"),
        ("#!/bin/sh\nprintf '%s\\n' 'clean'\n", "clean"),
    ] {
        std::fs::write(root.path().join("probe.sh"), source).unwrap();
        let output = fixture_process::command(platform::tool::Tool::Bun.executable(), root.path())
            .arg(&fixture)
            .arg(root.path())
            .arg(env!("CARGO_BIN_EXE_dotfiles"))
            .arg(expected)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
}

#[cfg(target_os = "macos")]
#[test]
fn the_system_bash_reports_bash4_features_with_a_hook_reproducer() {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(
        root.path().join("probe.sh"),
        "#!/bin/bash\nmapfile values\nprintf '%s\\n' \"${values[@]}\"\n",
    )
    .unwrap();
    let (_, blocked) = hook(
        root.path(),
        "lint-hook",
        &write_event(root.path(), "probe.sh"),
    );
    assert!(blocked.contains("bash4.Mapfile: 1 finding(s)"), "{blocked}");
    assert!(blocked.contains("claude-bash"), "{blocked}");
}

#[test]
fn xargs_without_a_literal_script_has_an_actionable_refusal() {
    let root = tempfile::tempdir().unwrap();
    for command in [
        "printf '%s\\n' probe.sh | xargs bash",
        "xargs -I '{}' sh '{}'",
        "xargs -I FILE sh FILE",
    ] {
        let (success, blocked) = hook(
            root.path(),
            "claude-bash",
            &json!({"tool_name":"Bash", "cwd":root.path(), "tool_input":{"command":command}}),
        );
        assert!(
            !success && blocked.contains("literal path"),
            "{command}: {blocked}"
        );
        assert!(
            !blocked.contains("could not finish"),
            "{command}: {blocked}"
        );
    }
}

#[test]
fn a_reproducing_command_preserves_spaces_and_apostrophes() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("owner's probe.py");
    std::fs::write(&path, "import os\n").unwrap();
    let report = dotfiles::lint_hook::file(&path, None).unwrap();
    let command = report
        .findings
        .iter()
        .flat_map(|finding| finding.lines())
        .find_map(|line| line.strip_prefix("Reproduce: "))
        .unwrap();
    let mut shell = if cfg!(windows) {
        platform::tool::Tool::Pwsh.command()
    } else {
        platform::tool::Tool::Bash.command()
    };
    if cfg!(windows) {
        shell.args(["-NoProfile", "-Command"]);
    } else {
        shell.arg("-c");
    }
    let output = shell.arg(command).output().unwrap();
    assert!(!output.status.success(), "{command}");
    assert!(
        String::from_utf8_lossy(&output.stdout).contains("F401"),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn a_powershell_command_that_invokes_bash_still_lints_the_script() {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("probe.sh"), "#!/bin/sh\necho $undefined\n").unwrap();
    let (success, blocked) = hook(
        root.path(),
        "claude-bash",
        &json!({"tool_name":"PowerShell", "cwd":root.path(), "tool_input":{"command":"bash probe.sh"}}),
    );
    assert!(!success && blocked.contains("SC2086"), "{blocked}");
}

#[test]
fn a_filename_cannot_override_linter_options() {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("ignored.toml"), "exclude = [\"**\"]\n").unwrap();
    std::fs::write(root.path().join("--config=ignored.toml"), "invalid=[\n").unwrap();
    let (_, blocked) = hook(
        root.path(),
        "lint-hook",
        &write_event(root.path(), "--config=ignored.toml"),
    );
    assert!(
        blocked.contains("block") && blocked.contains("invalid TOML"),
        "{blocked}"
    );
}

#[test]
fn malformed_hook_fields_never_silently_admit_a_write() {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(
        root.path().join("probe.sh"),
        "#!/bin/sh\nprintf '%s\\n' 'clean'\n",
    )
    .unwrap();
    for event in [
        json!({}),
        json!({"tool_name":"Write", "cwd":7, "tool_input":{"file_path":"probe.sh"}}),
    ] {
        let (_, blocked) = hook(root.path(), "lint-hook", &event);
        assert!(
            blocked.contains("block") && blocked.contains("invalid input"),
            "{blocked}"
        );
    }
}
