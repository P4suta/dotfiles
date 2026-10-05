use crate::tool::Tool;
use anyhow::{Context, Result, ensure};
use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

pub fn command(root: &Path, tool: Tool, arguments: &[&str]) -> Result<()> {
    run(tool.command(), root, tool.program(), arguments)
}

fn run(mut command: Command, root: &Path, name: &str, arguments: &[&str]) -> Result<()> {
    let status = command
        .current_dir(root)
        .args(arguments)
        .status()
        .with_context(|| format!("start required checker {name}"))?;
    ensure!(status.success(), "{name} failed with {status}");
    Ok(())
}

/// Whether a rendered command launcher or Git hook execs a program located through `$HOME` at run time.
pub fn launcher_resolves_through_home(name: &str, contents: &str) -> bool {
    (name.starts_with(".local/bin/") || name.contains("/hooks/"))
        && contents.lines().any(|line| {
            line.trim_start().starts_with("exec ")
                && (line.contains("$HOME") || line.contains("${HOME}"))
        })
}

/// Git for Windows ships the POSIX shell beside its `mingw64/libexec/git-core` directory.
pub fn git_for_windows_shell(exec_path: &Path) -> Option<PathBuf> {
    Some(exec_path.ancestors().nth(3)?.join("usr/bin/sh.exe"))
}

/// Native Windows has no `sh` on PATH, and its System32 `bash.exe` enters WSL, so Git for Windows' shell parses launchers.
fn posix_shell() -> Result<Command> {
    if !cfg!(windows) {
        return Ok(Tool::Sh.command());
    }
    let output = Tool::Git
        .command()
        .arg("--exec-path")
        .output()
        .context("locate Git for Windows")?;
    ensure!(output.status.success(), "git --exec-path failed");
    let exec_path = PathBuf::from(String::from_utf8(output.stdout)?.trim());
    let shell = git_for_windows_shell(&exec_path).context("unexpected Git for Windows layout")?;
    ensure!(
        shell.is_file(),
        "Git for Windows shell is missing at {}",
        shell.display()
    );
    Ok(crate::tool::external(shell))
}

pub fn rendered(dump: &Value, directory: &Path) -> Result<()> {
    let targets = dump.as_object().context("profile dump must be a mapping")?;
    fs::create_dir(directory)?;
    for (index, (name, target)) in targets.iter().enumerate() {
        if target["type"] != "file" && target["type"] != "script" {
            continue;
        }
        let contents = target["contents"]
            .as_str()
            .context("rendered content must be text")?;
        if name.ends_with(".json") {
            serde_json::from_str::<Value>(contents)
                .with_context(|| format!("invalid rendered JSON: {name}"))?;
        }
        if name.ends_with(".toml") {
            let file = directory.join(format!("{index}.toml"));
            fs::write(&file, contents)?;
            let output = Tool::Taplo
                .command()
                .current_dir(directory)
                .args(["lint", "--no-schema", "--no-auto-config"])
                .arg(file)
                .output()?;
            ensure!(
                output.status.success(),
                "invalid rendered TOML {name}: {}",
                String::from_utf8_lossy(&output.stderr)
            );
        }
        if name.ends_with(".nu") {
            let generated = directory.join("generated");
            fs::create_dir_all(&generated)?;
            let mut text = if name.ends_with("/config.nu") {
                let env = targets
                    .get(".config/nushell/env.nu")
                    .and_then(|value| value["contents"].as_str())
                    .context("Nushell configuration requires its rendered environment")?;
                format!("{env}\n{contents}")
            } else {
                contents.to_owned()
            };
            for module in [
                "mise",
                "starship",
                "television",
                "atuin",
                "zoxide",
                "ls-colors",
            ] {
                let path = generated.join(format!("{module}.nu"));
                fs::write(&path, "")?;
                let source = format!("~/.config/nushell/generated/{module}.nu");
                text = text.replace(&source, &format!("r#'{}'#", path.display()));
            }
            let file = directory.join(format!("{index}.nu"));
            fs::write(&file, text)?;
            let output = Tool::Nu
                .command()
                .current_dir(directory)
                .args([
                    "--no-config-file",
                    "--commands",
                    "if not (nu-check --debug $env.DOTFILES_NU_PARSE_FILE) { exit 1 }",
                ])
                .env("DOTFILES_NU_PARSE_FILE", &file)
                .output()?;
            ensure!(
                output.status.success(),
                "invalid rendered Nushell {name}: {}{}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
        }
        if contents.starts_with("#!/bin/sh\n") {
            ensure!(
                !launcher_resolves_through_home(name, contents),
                "launcher {name} finds its program through the runtime HOME, which tools that run Git with another HOME replace; render the installed path"
            );
            let file = directory.join(format!("{index}.sh"));
            fs::write(&file, contents)?;
            run(
                posix_shell()?,
                directory,
                "sh",
                &["-n", file.to_str().context("invalid fixture path")?],
            )?;
            command(
                directory,
                Tool::Shellcheck,
                &[
                    "--severity=style",
                    file.to_str().context("invalid fixture path")?,
                ],
            )?;
            if target["type"] == "script" {
                ensure!(
                    contents
                        .lines()
                        .filter(|line| !line.starts_with('#') && !line.trim().is_empty())
                        .count()
                        == 1
                        && contents.lines().any(|line| line.starts_with("exec ")),
                    "procedural shell setup must move to Rust: {name}"
                );
            }
        }
        if name.ends_with(".ps1") {
            if target["type"] == "script" {
                let mut lines = Vec::new();
                let mut continued = String::new();
                for line in contents.lines().map(str::trim) {
                    if line.is_empty() || line.starts_with('#') {
                        continue;
                    }
                    if let Some(line) = line.strip_suffix('`') {
                        continued.push_str(line);
                        continued.push(' ');
                        continue;
                    }
                    continued.push_str(line);
                    lines.push(std::mem::take(&mut continued));
                }
                ensure!(
                    continued.is_empty(),
                    "unfinished PowerShell launcher: {name}"
                );
                if lines
                    .first()
                    .is_some_and(|line| line == "$ErrorActionPreference = 'Stop'")
                {
                    lines.remove(0);
                }
                ensure!(
                    lines.is_empty()
                        || (lines.len() == 2
                            && lines[0].starts_with("& ")
                            && lines[1] == "exit $LASTEXITCODE"),
                    "procedural PowerShell setup must move to Rust: {name}"
                );
            }
            let file = directory.join(format!("{index}.ps1"));
            fs::write(&file, contents)?;
            let status = Tool::Pwsh.command().current_dir(directory).args(["-NoProfile", "-NonInteractive", "-Command",
                "$tokens=$null; $errors=$null; [System.Management.Automation.Language.Parser]::ParseFile($env:DOTFILES_PARSE_FILE,[ref]$tokens,[ref]$errors) > $null; if($errors.Count){$errors | Out-String | Write-Error; exit 1}"])
                .env("DOTFILES_PARSE_FILE", &file).stdin(Stdio::null()).status()?;
            ensure!(status.success(), "invalid rendered PowerShell: {name}");
        }
    }
    Ok(())
}

pub fn adapters(root: &Path) -> Result<()> {
    command(
        root,
        Tool::Bun,
        &["install", "--frozen-lockfile", "--ignore-scripts"],
    )?;
    command(root, Tool::Bun, &["run", "check"])
}

pub fn ops(root: &Path) -> Result<()> {
    let owned = tempfile::tempdir()?;
    let collections = owned.path().join("collections");
    let command = |tool: Tool| {
        let mut command = tool.command();
        command
            .current_dir(root.join("ops"))
            .env("ANSIBLE_HOME", owned.path())
            .env("ANSIBLE_COLLECTIONS_PATH", &collections);
        command
    };
    let status = command(Tool::AnsibleGalaxy)
        .args([
            "collection",
            "install",
            "--no-deps",
            "--requirements-file",
            "requirements.yml",
            "--collections-path",
        ])
        .arg(&collections)
        .status()?;
    ensure!(
        status.success(),
        "pinned Ansible collection installation failed"
    );
    let status = command(Tool::AnsiblePlaybook)
        .args(["--syntax-check", "site.yml"])
        .status()?;
    ensure!(status.success(), "Ansible syntax check failed");
    Ok(())
}

pub fn secrets(root: &Path) -> Result<()> {
    crate::profiles::check_public(root)?;
    let snapshot = tempfile::tempdir()?;
    let mut files = Vec::new();
    crate::profiles::source_files(root, &mut files)?;
    for file in files {
        let target = snapshot.path().join(file.strip_prefix(root)?);
        fs::create_dir_all(target.parent().context("snapshot parent is missing")?)?;
        fs::copy(file, target)?;
    }
    let report = tempfile::NamedTempFile::new()?;
    let output = Tool::Gitleaks
        .command()
        .current_dir(root)
        .args([
            "dir",
            "--redact",
            "--no-banner",
            "--report-format",
            "json",
            "--report-path",
        ])
        .arg(report.path())
        .arg(snapshot.path())
        .output()?;
    if !output.status.success() {
        if let Ok(findings) = serde_json::from_slice::<Vec<Value>>(&fs::read(report.path())?) {
            for finding in findings {
                eprintln!(
                    "Secret scanner finding: {}:{} / {}",
                    finding["File"], finding["StartLine"], finding["RuleID"]
                );
            }
        }
        anyhow::bail!(
            "required secret scanner failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    println!("Secret scan passed on the publication source snapshot");
    Ok(())
}

#[derive(serde::Deserialize)]
struct CommentConfig {
    #[serde(default)]
    overrides: Vec<CommentOverride>,
}

#[derive(serde::Deserialize)]
struct CommentOverride {
    paths: Vec<String>,
    language: String,
}

/// OComment language overrides decide how a prose rule rewrites each file.
/// Patterns must name an existing file or select by extension, as in `**/*.md.tmpl`.
pub fn comment_scopes(root: &Path) -> Result<()> {
    let config: CommentConfig = toml::from_str(&fs::read_to_string(root.join(".ocomment.toml"))?)
        .context("read .ocomment.toml overrides")?;
    let mut refused = Vec::new();
    for entry in &config.overrides {
        for path in &entry.paths {
            let accepted = if path.contains('*') {
                path.strip_prefix("**/*.").is_some_and(|extension| {
                    !extension.is_empty() && !extension.contains(['*', '/', '?', '['])
                })
            } else {
                root.join(path).is_file()
            };
            if !accepted {
                refused.push(format!("{} ({})", path, entry.language));
            }
        }
    }
    ensure!(
        refused.is_empty(),
        "OComment overrides must name an existing file or an extension pattern such as **/*.md.tmpl: {}",
        refused.join(", ")
    );
    Ok(())
}

/// `just` invocations a reader of the repository should run, written as code, quoted, or after `run:` or `see`, or at the start of a Markdown line.
pub fn recipe_references(text: &str, markdown: bool) -> Vec<(usize, String)> {
    let mut found = Vec::new();
    for (number, line) in text.lines().enumerate() {
        let starts = markdown
            && line
                .trim_start()
                .trim_start_matches("$ ")
                .starts_with("just ");
        for (index, _) in line.match_indices("just ") {
            let before = &line[..index];
            let command = starts && before.trim_start().trim_start_matches("$ ").is_empty()
                || before.ends_with(['`', '\'', '"', '('])
                || before.ends_with("run: ")
                || before.ends_with("see ");
            let recipe: String = line[index + 5..]
                .chars()
                .take_while(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || *c == '-')
                .collect();
            if command && !recipe.is_empty() && !recipe.starts_with('-') {
                found.push((number + 1, recipe));
            }
        }
    }
    found
}

/// Recipe references that name no recipe in the justfile, so a hint or document sends the reader to a missing command.
pub fn unknown_recipes(root: &Path) -> Result<Vec<String>> {
    let summary = Tool::Just
        .command()
        .current_dir(root)
        .args(["--summary", "--justfile"])
        .arg(root.join("justfile"))
        .output()?;
    ensure!(summary.status.success(), "just --summary failed");
    let recipes: std::collections::BTreeSet<String> = String::from_utf8(summary.stdout)?
        .split_whitespace()
        .map(str::to_owned)
        .collect();
    ensure!(!recipes.is_empty(), "the justfile defines no recipes");
    let mut unknown = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(directory) = stack.pop() {
        for entry in fs::read_dir(&directory)? {
            let path = entry?.path();
            let relative = path
                .strip_prefix(root)?
                .to_string_lossy()
                .replace('\\', "/");
            if path.is_dir() {
                if !matches!(
                    relative.as_str(),
                    ".git" | "target" | "node_modules" | "docs/adr" | "xtask/tests"
                ) && !relative.ends_with("/target")
                {
                    stack.push(path);
                }
            } else if let Ok(text) = fs::read_to_string(&path) {
                for (line, recipe) in recipe_references(&text, relative.ends_with(".md")) {
                    if !recipes.contains(&recipe) {
                        unknown.push(format!("{relative}:{line}: just {recipe}"));
                    }
                }
            }
        }
    }
    unknown.sort();
    Ok(unknown)
}

/// Profile leaves that no template includes.
pub fn unreferenced_leaves(root: &Path) -> Result<Vec<String>> {
    let leaves_root = root.join(".chezmoitemplates/profiles");
    let mut leaves = Vec::new();
    let mut stack = vec![leaves_root.clone()];
    while let Some(directory) = stack.pop() {
        for entry in fs::read_dir(&directory)? {
            let path = entry?.path();
            if path.is_dir() {
                stack.push(path);
            } else {
                let relative = path.strip_prefix(root.join(".chezmoitemplates"))?;
                leaves.push(relative.to_string_lossy().replace('\\', "/"));
            }
        }
    }
    let mut referenced = std::collections::BTreeSet::new();
    let mut sources = vec![root.to_path_buf()];
    while let Some(directory) = sources.pop() {
        for entry in fs::read_dir(&directory)? {
            let path = entry?.path();
            let name = path
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or_default();
            if path.is_dir() {
                if !matches!(name, ".git" | "target" | "node_modules") {
                    sources.push(path);
                }
            } else if let Ok(text) = fs::read_to_string(&path) {
                for (index, _) in text.match_indices("profiles/") {
                    let tail = &text[index..];
                    let reference: String = tail.chars().take_while(|c| *c != '"').collect();
                    referenced.insert(reference);
                }
            }
        }
    }
    leaves.retain(|leaf| !referenced.contains(leaf));
    leaves.sort();
    Ok(leaves)
}
