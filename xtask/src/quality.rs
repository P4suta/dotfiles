use crate::tool::Tool;
use anyhow::{Context, Result, ensure};
use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

pub(crate) fn command(root: &Path, tool: Tool, arguments: &[&str]) -> Result<()> {
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

/// Lints the GitHub workflows with actionlint and audits them with zizmor.
/// zizmor runs offline, so the check never waits on the GitHub API.
///
/// # Errors
///
/// Returns an error when either checker can't start or reports a finding.
pub fn workflows(root: &Path) -> Result<()> {
    let findings = workflow_findings(
        &fs::read_to_string(root.join(".github/workflows/ci.yml"))?,
        &fs::read_to_string(root.join("policy/github.toml"))?,
    )?;
    ensure!(findings.is_empty(), "{}", findings.join("\n"));
    run(Tool::Actionlint.command(), root, "actionlint", &[])?;
    run(
        Tool::Zizmor.command(),
        root,
        "zizmor",
        &["--offline", ".github/workflows"],
    )
}

/// Refusals for a CI workflow that the settings in `policy/github.toml` can't rely on.
/// The workflow must run for pull requests and the merge queue, and each required status must name a job that needs every other job and always runs.
/// No step may call `cargo run`, since the justfile owns every command.
///
/// # Errors
///
/// Returns an error when the workflow fails to parse as YAML or the policy as TOML.
pub fn workflow_findings(workflow: &str, policy: &str) -> Result<Vec<String>> {
    use yaml_rust2::{Yaml, YamlLoader};
    let document = YamlLoader::load_from_str(workflow)?
        .into_iter()
        .next()
        .context("ci.yml holds no document")?;
    let policy: toml::Table = toml::from_str(policy)?;
    let mut found = Vec::new();
    for event in ["pull_request", "merge_group"] {
        if document["on"][event].is_badvalue() {
            found.push(format!("ci.yml does not run on {event}"));
        }
    }
    let jobs = document["jobs"].as_hash().context("ci.yml holds no jobs")?;
    let required: Vec<&str> = policy
        .get("ruleset")
        .and_then(|ruleset| ruleset.get("main"))
        .and_then(|main| main.get("required_status_checks"))
        .and_then(toml::Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(toml::Value::as_str)
        .collect();
    for status in required {
        let Some((id, job)) = jobs
            .iter()
            .find(|(_, job)| job["name"].as_str() == Some(status))
        else {
            found.push(format!(
                "policy/github.toml requires {status}, which no job of ci.yml reports"
            ));
            continue;
        };
        let needs: Vec<&str> = match &job["needs"] {
            Yaml::Array(items) => items.iter().filter_map(Yaml::as_str).collect(),
            Yaml::String(item) => vec![item.as_str()],
            _ => Vec::new(),
        };
        for (other, _) in jobs {
            if other != id
                && let Some(other) = other.as_str()
                && !needs.contains(&other)
            {
                found.push(format!(
                    "the {status} job does not wait for the {other} job"
                ));
            }
        }
        if job["if"].as_str() != Some("always()") {
            found.push(format!(
                "the {status} job must run with `if: always()` so a skipped job never hides a failure"
            ));
        }
    }
    for (id, job) in jobs {
        for step in job["steps"].as_vec().into_iter().flatten() {
            if step["run"]
                .as_str()
                .is_some_and(|run| run.contains("cargo run"))
            {
                found.push(format!(
                    "the {} job calls `cargo run`; call a just recipe instead",
                    id.as_str().unwrap_or_default()
                ));
            }
        }
    }
    Ok(found)
}

/// Refuses a dependency of the root workspace that `deny.toml` rejects: an advisory, a yanked release, an unlisted license, or an unknown source.
///
/// # Errors
///
/// Returns an error when cargo-deny can't start or reports a finding.
pub fn dependencies(root: &Path) -> Result<()> {
    run(
        Tool::CargoDeny.command(),
        root,
        "cargo-deny",
        &["--locked", "check"],
    )
}

/// Whether a rendered command launcher or Git hook execs a program located through `$HOME` at run time.
#[must_use]
pub fn launcher_resolves_through_home(name: &str, contents: &str) -> bool {
    (name.starts_with(".local/bin/") || name.contains("/hooks/"))
        && contents.lines().any(|line| {
            line.trim_start().starts_with("exec ")
                && (line.contains("$HOME") || line.contains("${HOME}"))
        })
}

/// Git for Windows ships the POSIX shell beside its `mingw64/libexec/git-core` directory.
#[must_use]
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

pub(crate) fn rendered(dump: &Value, directory: &Path) -> Result<()> {
    let targets = dump.as_object().context("profile dump must be a mapping")?;
    fs::create_dir(directory)?;
    for (index, (name, target)) in targets.iter().enumerate() {
        if target["type"] != "file" && target["type"] != "script" {
            continue;
        }
        let contents = target["contents"]
            .as_str()
            .context("rendered content must be text")?;
        let script = target["type"] == "script";
        let file = |extension: &str| directory.join(format!("{index}.{extension}"));
        if crate::has_extension(name, "json") {
            serde_json::from_str::<Value>(contents)
                .with_context(|| format!("invalid rendered JSON: {name}"))?;
        }
        if crate::has_extension(name, "toml") {
            toml_file(directory, &file("toml"), name, contents)?;
        }
        if crate::has_extension(name, "nu") {
            let text = if name.ends_with("/config.nu") {
                let env = targets
                    .get(".config/nushell/env.nu")
                    .and_then(|value| value["contents"].as_str())
                    .context("Nushell configuration requires its rendered environment")?;
                format!("{env}\n{contents}")
            } else {
                contents.to_owned()
            };
            nushell_file(directory, &file("nu"), name, text)?;
        }
        if contents.starts_with("#!/bin/sh\n") {
            shell_file(directory, &file("sh"), name, contents, script)?;
        }
        if crate::has_extension(name, "ps1") {
            powershell_file(directory, &file("ps1"), name, contents, script)?;
        }
    }
    Ok(())
}

fn toml_file(directory: &Path, file: &Path, name: &str, contents: &str) -> Result<()> {
    fs::write(file, contents)?;
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
    Ok(())
}

/// Parses a rendered Nushell file with each generated module it sources stubbed out.
fn nushell_file(directory: &Path, file: &Path, name: &str, mut text: String) -> Result<()> {
    let generated = directory.join("generated");
    fs::create_dir_all(&generated)?;
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
    fs::write(file, text)?;
    let output = Tool::Nu
        .command()
        .current_dir(directory)
        .args([
            "--no-config-file",
            "--commands",
            "if not (nu-check --debug $env.DOTFILES_NU_PARSE_FILE) { exit 1 }",
        ])
        .env("DOTFILES_NU_PARSE_FILE", file)
        .output()?;
    ensure!(
        output.status.success(),
        "invalid rendered Nushell {name}: {}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    Ok(())
}

fn shell_file(
    directory: &Path,
    file: &Path,
    name: &str,
    contents: &str,
    script: bool,
) -> Result<()> {
    ensure!(
        !launcher_resolves_through_home(name, contents),
        "launcher {name} finds its program through the runtime HOME, which tools that run Git with another HOME replace; render the installed path"
    );
    fs::write(file, contents)?;
    let path = file.to_str().context("invalid fixture path")?;
    run(posix_shell()?, directory, "sh", &["-n", path])?;
    command(directory, Tool::Shellcheck, &["--severity=style", path])?;
    if script {
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
    Ok(())
}

fn powershell_file(
    directory: &Path,
    file: &Path,
    name: &str,
    contents: &str,
    script: bool,
) -> Result<()> {
    if script {
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
    fs::write(file, contents)?;
    let status = Tool::Pwsh.command().current_dir(directory).args(["-NoProfile", "-NonInteractive", "-Command",
        "$tokens=$null; $errors=$null; [System.Management.Automation.Language.Parser]::ParseFile($env:DOTFILES_PARSE_FILE,[ref]$tokens,[ref]$errors) > $null; if($errors.Count){$errors | Out-String | Write-Error; exit 1}"])
        .env("DOTFILES_PARSE_FILE", file).stdin(Stdio::null()).status()?;
    ensure!(status.success(), "invalid rendered PowerShell: {name}");
    Ok(())
}

/// Installs the locked Bun dependencies and runs their `check` script.
///
/// # Errors
///
/// Returns an error when Bun can't start or either Bun command fails.
pub fn adapters(root: &Path) -> Result<()> {
    command(
        root,
        Tool::Bun,
        &["install", "--frozen-lockfile", "--ignore-scripts"],
    )?;
    command(root, Tool::Bun, &["run", "check"])
}

/// Installs the pinned Ansible collections into a temporary directory and syntax-checks `ops/site.yml`.
///
/// # Errors
///
/// Returns an error when creating the temporary directory fails, an Ansible command can't start, the collection installation fails, or the syntax check fails.
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

/// Runs the public source check, then scans a snapshot of the source files with Gitleaks.
///
/// # Errors
///
/// Returns an error when the public source check fails, copying the snapshot fails, Gitleaks can't start, or Gitleaks reports a failure.
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

/// `OComment` language overrides decide how a prose rule rewrites each file.
/// Patterns must name an existing file or select by extension, as in `**/*.md.tmpl`.
///
/// # Errors
///
/// Returns an error when reading or parsing `.ocomment.toml` fails, or an override pattern names neither an existing file nor an extension.
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
#[must_use]
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
///
/// # Errors
///
/// Returns an error when `just --summary` can't start, fails, prints non-UTF-8 output, or lists no recipe, or reading a directory fails.
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
                let markdown = crate::has_extension(&relative, "md");
                for (line, recipe) in recipe_references(&text, markdown) {
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

/// chezmoi source attributes, which name a target and so never belong inside `.chezmoitemplates/`.
const SOURCE_ATTRIBUTES: [&str; 18] = [
    "after_",
    "before_",
    "create_",
    "dot_",
    "empty_",
    "encrypted_",
    "exact_",
    "executable_",
    "external_",
    "literal_",
    "modify_",
    "once_",
    "onchange_",
    "private_",
    "readonly_",
    "remove_",
    "run_",
    "symlink_",
];

/// Refusals for one `.chezmoitemplates/` file, named by its path below that directory.
/// A file belongs under `partials/`, or under `native/<topic>/<profile>` with an optional extension, and carries no source attribute.
#[must_use]
pub fn template_path_findings(relative: &str) -> Vec<String> {
    let mut found = Vec::new();
    let segments: Vec<&str> = relative.split('/').collect();
    if let Some(segment) = segments.iter().find(|segment| {
        SOURCE_ATTRIBUTES
            .iter()
            .any(|attribute| segment.starts_with(attribute))
    }) {
        found.push(format!(
            ".chezmoitemplates/{relative}: `{segment}` carries a chezmoi source attribute; name the body after its topic"
        ));
    }
    match segments.as_slice() {
        ["partials", _, ..] => {}
        ["native", topic, body] if !topic.is_empty() => {
            let profile = body.split('.').next().unwrap_or_default();
            if !crate::profile_rules::Profile::ALL
                .iter()
                .any(|known| known.name() == profile)
            {
                found.push(format!(
                    ".chezmoitemplates/{relative}: a native body is named after its profile, such as native/{topic}/mac"
                ));
            }
        }
        _ => found.push(format!(
            ".chezmoitemplates/{relative}: only partials/ and native/<topic>/<profile> may hold templates"
        )),
    }
    found
}

/// The `.chezmoitemplates/` paths that a source text names, with a `%s` profile placeholder expanded to every profile.
fn template_references(text: &str, into: &mut std::collections::BTreeSet<String>) {
    for prefix in ["partials/", "native/"] {
        for (index, _) in text.match_indices(prefix) {
            if text[..index]
                .chars()
                .next_back()
                .is_some_and(|before| before.is_alphanumeric() || before == '_' || before == '-')
            {
                continue;
            }
            let reference: String = text[index..].chars().take_while(|c| *c != '"').collect();
            if reference.contains("%s") {
                for profile in crate::profile_rules::Profile::ALL {
                    into.insert(reference.replace("%s", profile.name()));
                }
            } else {
                into.insert(reference);
            }
        }
    }
}

/// Refusals for the shared templates: a misplaced or attribute-named file, a template that no source includes, and native bodies with identical bytes.
/// Also refuses the retired `fail "Target is unavailable for this profile"` guard, since `.chezmoiignore` already keeps a target from other profiles.
///
/// # Errors
///
/// Returns an error when reading the source state fails.
pub fn template_findings(root: &Path) -> Result<Vec<String>> {
    let source = crate::source_state(root);
    let templates = source.join(".chezmoitemplates");
    let mut bodies = std::collections::BTreeMap::new();
    let mut referenced = std::collections::BTreeSet::new();
    let mut found = Vec::new();
    let mut stack = vec![source.clone()];
    while let Some(directory) = stack.pop() {
        for entry in fs::read_dir(&directory)? {
            let path = entry?.path();
            if path.is_dir() {
                stack.push(path);
                continue;
            }
            let bytes = fs::read(&path)?;
            let text = String::from_utf8_lossy(&bytes);
            template_references(&text, &mut referenced);
            if let Ok(relative) = path.strip_prefix(&templates) {
                let relative = relative.to_string_lossy().replace('\\', "/");
                found.extend(template_path_findings(&relative));
                bodies.insert(relative, bytes);
            } else if text.contains("Target is unavailable for this profile") {
                found.push(format!(
                    "{}: a profile guard inside a template; list the target in home/.chezmoidata/targets.toml instead",
                    path.strip_prefix(root)
                        .unwrap_or(&path)
                        .to_string_lossy()
                        .replace('\\', "/")
                ));
            }
        }
    }
    for path in bodies.keys() {
        if !referenced.contains(path) {
            found.push(format!(
                ".chezmoitemplates/{path}: no source template includes it; delete it or include it"
            ));
        }
    }
    let native: Vec<_> = bodies
        .iter()
        .filter(|(path, _)| path.starts_with("native/"))
        .collect();
    for (index, (left, bytes)) in native.iter().enumerate() {
        for (right, other) in &native[index + 1..] {
            if bytes == other {
                found.push(format!(
                    ".chezmoitemplates/{left} and {right} hold the same bytes; select one body for both profiles"
                ));
            }
        }
    }
    found.sort();
    Ok(found)
}

/// The chezmoi version that `mise.toml` pins, or why it names none.
pub(crate) fn mise_chezmoi_pin(mise: &str) -> Result<String> {
    let table: toml::Table = toml::from_str(mise)?;
    let pin = &table
        .get("tools")
        .and_then(|tools| tools.get("chezmoi"))
        .context("mise.toml pins no chezmoi version")?;
    pin.as_str()
        .or_else(|| pin.get("version").and_then(toml::Value::as_str))
        .map(str::to_owned)
        .context("mise.toml names chezmoi without a version string")
}

/// Refuses when the source state's `.chezmoiversion` and the `mise.toml` chezmoi pin name different versions.
///
/// # Errors
///
/// Returns an error when reading `mise.toml` or `.chezmoiversion` fails, `mise.toml` pins no chezmoi version string, or the two versions differ.
pub fn chezmoi_version(root: &Path) -> Result<()> {
    let pin = mise_chezmoi_pin(&fs::read_to_string(root.join("mise.toml"))?)?;
    let required = fs::read_to_string(crate::source_state(root).join(".chezmoiversion"))
        .context("the source state needs .chezmoiversion")?;
    ensure!(
        required.trim() == pin,
        "home/.chezmoiversion requires chezmoi {} but mise.toml pins {pin}; set both to the same version",
        required.trim()
    );
    Ok(())
}

/// The value `key` takes in the `[*]` section of an `.editorconfig`.
pub fn editorconfig_default<'a>(text: &'a str, key: &str) -> Option<&'a str> {
    let mut every_file = false;
    for line in text.lines().map(str::trim) {
        if line.starts_with('[') {
            every_file = line == "[*]";
        } else if every_file
            && let Some((name, value)) = line.split_once('=')
            && name.trim() == key
        {
            return Some(value.trim());
        }
    }
    None
}

/// The line ending the `* text=auto` rule of a `.gitattributes` declares.
pub fn gitattributes_default_eol(text: &str) -> Option<&str> {
    text.lines()
        .map(str::trim)
        .filter(|line| line.split_whitespace().next() == Some("*"))
        .flat_map(str::split_whitespace)
        .find_map(|attribute| attribute.strip_prefix("eol="))
}

/// Refuses an `.editorconfig` whose default line ending, charset, or final newline disagrees with `.gitattributes` and the LF and UTF-8 policy.
///
/// # Errors
///
/// Returns an error when reading `.editorconfig` or `.gitattributes` fails, `.gitattributes` declares no default line ending, or `.editorconfig` disagrees with it.
pub fn editorconfig(root: &Path) -> Result<()> {
    let editor = fs::read_to_string(root.join(".editorconfig"))
        .context("the repository needs an .editorconfig")?;
    let attributes = fs::read_to_string(root.join(".gitattributes"))?;
    let eol = gitattributes_default_eol(&attributes)
        .context(".gitattributes declares no default eol for `*`")?;
    ensure!(
        editorconfig_default(&editor, "end_of_line") == Some(eol)
            && editorconfig_default(&editor, "charset") == Some("utf-8")
            && editorconfig_default(&editor, "insert_final_newline") == Some("true"),
        ".editorconfig must set end_of_line = {eol}, charset = utf-8, and insert_final_newline = true for [*], as .gitattributes and the line ending gate require"
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    #[test]
    fn editorconfig_and_gitattributes_name_the_same_line_ending() {
        let editor = "root = true\n\n[*.md]\nend_of_line = crlf\n\n[*]\nend_of_line = lf\n";
        assert_eq!(
            super::editorconfig_default(editor, "end_of_line"),
            Some("lf")
        );
        assert_eq!(
            super::gitattributes_default_eol("# note\n* text=auto eol=lf\n*.bat eol=crlf\n"),
            Some("lf")
        );
        assert_eq!(super::gitattributes_default_eol("*.sh eol=lf\n"), None);
    }

    use super::*;

    #[test]
    fn the_chezmoi_pin_reads_a_plain_or_a_table_version() {
        assert_eq!(
            mise_chezmoi_pin("[tools]\nchezmoi = \"2.73.0\"\n").unwrap(),
            "2.73.0"
        );
        assert_eq!(
            mise_chezmoi_pin("[tools]\nchezmoi = { version = \"2.74.1\" }\n").unwrap(),
            "2.74.1"
        );
        assert!(mise_chezmoi_pin("[tools]\njust = \"1\"\n").is_err());
    }
}
