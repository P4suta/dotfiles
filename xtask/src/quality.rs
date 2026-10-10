use crate::tool::Tool;
use anyhow::{Context, Result, ensure};
use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

/// Git for Windows' exec path sits three levels inside its installation: `mingw64/libexec/git-core`.
const GIT_EXEC_PATH_DEPTH: usize = 3;

/// The word that starts a recipe reference.
const JUST: &str = "just ";
const DENY_CONFIG: &str = "deny.toml";
const RUSTSEC_URL: &str = "https://github.com/RustSec/advisory-db";
/// The directory the pinned cargo-deny derives from its default advisory URL.
const RUSTSEC_DATABASE: &str = "advisory-db-3157b0e258782691";
const ADVISORY_ROOT: &str = "advisories";
const ADVISORY_CLONE_DEPTH: &str = "1";
const TEMP_ENVIRONMENT: [&str; 3] = ["TMPDIR", "TMP", "TEMP"];

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
    let mut checks = crate::checks::Checks::default();
    checks.run("workflows.policy", || {
        let findings = workflow_findings(
            &fs::read_to_string(root.join(".github/workflows/ci.yml"))?,
            &fs::read_to_string(root.join("policy/github.toml"))?,
        )?;
        ensure!(findings.is_empty(), "{}", findings.join("\n"));
        Ok(())
    });
    checks.run("workflows.actionlint", || {
        run(Tool::Actionlint.command(), root, "actionlint", &[])
    });
    checks.run("workflows.zizmor", || {
        run(
            Tool::Zizmor.command(),
            root,
            "zizmor",
            &["--offline", ".github/workflows"],
        )
    });
    checks.finish()
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
        let guard = job["steps"].as_vec().into_iter().flatten().any(|step| {
            step["env"]["RESULTS"].as_str() == Some("${{ join(needs.*.result, ' ') }}")
                && step["run"].as_str().is_some_and(|run| {
                    run.contains("success) ;;")
                        && !run.contains("skipped")
                        && run.contains("exit 1")
                })
        });
        if !guard {
            found.push(format!(
                "the {status} job must refuse every result other than success"
            ));
        }
    }
    for (id, job) in jobs {
        if !job["continue-on-error"].is_badvalue()
            && job["continue-on-error"].as_bool() != Some(false)
        {
            found.push(format!(
                "the {} job suppresses failure with continue-on-error",
                id.as_str().unwrap_or_default()
            ));
        }
        for step in job["steps"].as_vec().into_iter().flatten() {
            if !step["continue-on-error"].is_badvalue()
                && step["continue-on-error"].as_bool() != Some(false)
            {
                found.push(format!(
                    "the {} job suppresses a step failure with continue-on-error",
                    id.as_str().unwrap_or_default()
                ));
            }
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
    let scope = tempfile::tempdir()?;
    let cache = scope.path().join(ADVISORY_ROOT);
    let database = cache.join(RUSTSEC_DATABASE);
    let mut policy: toml::Table = toml::from_str(&fs::read_to_string(root.join(DENY_CONFIG))?)?;
    let advisories = policy
        .entry("advisories")
        .or_insert_with(|| toml::Value::Table(toml::Table::new()))
        .as_table_mut()
        .context("advisories must be a table")?;
    if let Some(urls) = advisories.get("db-urls") {
        let urls = urls.as_array().context("advisory URLs must be an array")?;
        ensure!(
            urls.is_empty() || urls.len() == 1 && urls[0].as_str() == Some(RUSTSEC_URL),
            "the isolated advisory checker requires the configured RustSec database"
        );
    }
    advisories.insert(
        "db-path".into(),
        toml::Value::String(cache.to_str().context("advisory path is not UTF-8")?.into()),
    );
    let config = scope.path().join(DENY_CONFIG);
    fs::write(&config, toml::to_string(&policy)?)?;
    run(
        Tool::Git.command(),
        root,
        "git",
        &[
            "clone",
            "--depth",
            ADVISORY_CLONE_DEPTH,
            "--",
            RUSTSEC_URL,
            database.to_str().context("advisory path is not UTF-8")?,
        ],
    )?;
    let checker = || {
        let mut command = Tool::CargoDeny.command();
        command.envs(TEMP_ENVIRONMENT.map(|key| (key, scope.path())));
        command
    };
    run(
        checker(),
        root,
        "cargo-deny",
        &["--locked", "fetch", "index", "std-replacement"],
    )?;
    run(
        checker(),
        root,
        "cargo-deny",
        &[
            "--locked",
            "--offline",
            "--config",
            config
                .to_str()
                .context("deny configuration path is not UTF-8")?,
            "check",
        ],
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
    Some(
        exec_path
            .ancestors()
            .nth(GIT_EXEC_PATH_DEPTH)?
            .join("usr/bin/sh.exe"),
    )
}

/// Native Windows has no `sh` on PATH, and its System32 `bash.exe` enters WSL, so Git for Windows' shell parses launchers.
fn posix_shell() -> Result<Command> {
    if !cfg!(windows) {
        return Ok(Tool::Sh.command());
    }
    Ok(crate::tool::external(posix_shell_path()?))
}

/// Git for Windows' `sh.exe`, beside its `bash.exe`.
fn posix_shell_path() -> Result<PathBuf> {
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
    Ok(shell)
}

/// The word of a `# shellcheck shell=` directive in the leading comments of a shell text, or an empty word.
#[must_use]
pub fn shell_directive(contents: &str) -> &str {
    contents
        .lines()
        .map(str::trim)
        .take_while(|line| line.is_empty() || line.starts_with('#'))
        .find_map(|line| {
            line.strip_prefix('#')?
                .trim_start()
                .strip_prefix("shellcheck ")?
                .split_whitespace()
                .find_map(|setting| setting.strip_prefix("shell="))
        })
        .unwrap_or_default()
}

/// The shell dialect of a rendered target, when the text names a shell script.
#[must_use]
pub fn shell_selection(name: &str, contents: &str) -> Option<crate::shell_target_rules::Selection> {
    let file_name = name.rsplit('/').next().unwrap_or(name);
    let first_line = contents.lines().next().unwrap_or_default();
    crate::shell_target_rules::select(
        file_name.as_bytes(),
        first_line.as_bytes(),
        shell_directive(contents).as_bytes(),
    )
}

/// The key the gate plants at the top level of a governed target, which no consumer declares.
pub const PLANTED_KEY: &str = "dotfiles-planted-unknown-key";

/// The schema errors a refusal quotes for one target.
const QUOTED_ERRORS: usize = 5;

/// A consumer's vendored schema and the rendered targets it governs, with the targets every profile rendered so far.
pub struct Contract {
    pub schema: String,
    pub targets: Vec<String>,
    digest_matches: bool,
    validator: jsonschema::Validator,
    pub witnesses: Vec<String>,
}

impl std::fmt::Debug for Contract {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("Contract")
            .field("schema", &self.schema)
            .field("targets", &self.targets)
            .field("witnesses", &self.witnesses)
            .finish_non_exhaustive()
    }
}

impl Contract {
    /// Compiles a vendored schema offline, recording whether its bytes keep the manifest's digest.
    ///
    /// # Errors
    ///
    /// Returns an error when reading, parsing, or compiling the schema fails.
    pub fn load(
        root: &Path,
        vendored: &crate::obligations::Vendored,
        targets: &[String],
        resources: &[crate::obligations::Vendored],
        close_root: bool,
        undeclared: &[String],
    ) -> Result<Self> {
        let schema = &vendored.schema;
        let read = |vendored: &crate::obligations::Vendored| -> Result<(bool, Value)> {
            let path = &vendored.schema;
            let (digest_matches, bytes) = crate::obligations::vendored_bytes(root, vendored)
                .with_context(|| format!("read {path}"))?;
            let document =
                serde_json::from_slice(&bytes).with_context(|| format!("parse {path}"))?;
            Ok((digest_matches, document))
        };
        let (mut digest_matches, mut document) = read(vendored)?;
        if close_root {
            let root_schema = document
                .as_object_mut()
                .with_context(|| format!("{schema} is no schema object"))?;
            ensure!(
                root_schema.get("additionalProperties") != Some(&Value::Bool(false)),
                "{schema} already refuses unknown top-level keys; remove close_root from {}",
                crate::obligations::MANIFEST
            );
            root_schema.insert("additionalProperties".to_owned(), Value::Bool(false));
            let declared = root_schema
                .entry("properties")
                .or_insert_with(|| Value::Object(serde_json::Map::new()))
                .as_object_mut()
                .with_context(|| format!("{schema} holds no properties object"))?;
            for key in undeclared {
                ensure!(
                    declared.insert(key.clone(), Value::Bool(true)).is_none(),
                    "{schema} now declares {key}; remove it from undeclared in {}",
                    crate::obligations::MANIFEST
                );
            }
        } else {
            ensure!(
                undeclared.is_empty(),
                "{schema} names undeclared keys without close_root in {}",
                crate::obligations::MANIFEST
            );
        }
        let mut referenced = Vec::new();
        for resource in resources {
            let (matches, contents) = read(resource)?;
            digest_matches &= matches;
            let id = contents["$id"]
                .as_str()
                .with_context(|| format!("{} declares no $id", resource.schema))?
                .to_owned();
            referenced.push((id, contents));
        }
        let mut options = jsonschema::options();
        let identifiers: std::collections::BTreeSet<&str> = referenced
            .iter()
            .map(|(id, _)| id.as_str())
            .chain(document["$id"].as_str())
            .collect();
        known_references(&document, &identifiers)?;
        for (_, contents) in &referenced {
            known_references(contents, &identifiers)?;
        }
        for (id, contents) in referenced {
            options.with_document(id, contents);
        }
        let validator = options
            .build(&document)
            .map_err(|error| anyhow::anyhow!("compile {schema}: {error}"))?;
        Ok(Self {
            schema: schema.to_owned(),
            targets: targets.to_vec(),
            digest_matches,
            validator,
            witnesses: Vec::new(),
        })
    }

    /// The refusal for one rendered target, or `None` when it conforms and the schema refuses a planted key.
    #[must_use]
    pub fn refusal(&self, profile: &str, target: &str, instance: &Value) -> Option<String> {
        let mut planted = instance.clone();
        if let Some(object) = planted.as_object_mut() {
            object.insert(PLANTED_KEY.to_owned(), Value::Bool(true));
        }
        let facts = crate::schema_rules::Target {
            conforms: self.validator.is_valid(instance),
            planted_refused: !self.validator.is_valid(&planted),
        };
        let schema = &self.schema;
        match crate::schema_rules::verdict(facts) {
            crate::schema_rules::Verdict::Conforms => None,
            crate::schema_rules::Verdict::Violates => {
                let errors: Vec<String> = self
                    .validator
                    .validate(instance)
                    .err()
                    .into_iter()
                    .flatten()
                    .take(QUOTED_ERRORS)
                    .map(|error| format!("{} at `{}`", error, error.instance_path))
                    .collect();
                Some(format!(
                    "the {profile} profile renders {target}, which breaks {schema}: {}",
                    errors.join("; ")
                ))
            }
            crate::schema_rules::Verdict::Open => Some(format!(
                "{schema} admits the unknown key {PLANTED_KEY} in {target} of the {profile} profile, so a misspelled key would apply silently"
            )),
        }
    }

    /// The refusal for the schema over every profile, or `None` when its bytes hold and a target saw it.
    #[must_use]
    pub fn standing_refusal(&self) -> Option<String> {
        let schema = &self.schema;
        let witnesses = u32::try_from(self.witnesses.len()).unwrap_or(u32::MAX);
        match crate::schema_rules::standing(self.digest_matches, witnesses) {
            crate::schema_rules::Standing::Holds => None,
            crate::schema_rules::Standing::Drifted => Some(format!(
                "{schema} differs from the SHA-256 that {} records; vendor the pinned release again",
                crate::obligations::MANIFEST
            )),
            crate::schema_rules::Standing::Unwitnessed => Some(format!(
                "no profile renders {}, so {schema} checks nothing",
                self.targets.join(", ")
            )),
        }
    }
}

fn known_references(
    document: &Value,
    identifiers: &std::collections::BTreeSet<&str>,
) -> Result<()> {
    match document {
        Value::Array(items) => {
            for item in items {
                known_references(item, identifiers)?;
            }
        }
        Value::Object(properties) => {
            for keyword in ["$ref", "$dynamicRef", "$recursiveRef"] {
                if let Some(reference) = properties.get(keyword).and_then(Value::as_str) {
                    let external = reference.split('#').next().unwrap_or_default();
                    ensure!(
                        external.is_empty() || identifiers.contains(external),
                        "unvendored schema reference {reference}; network retrieval is disabled"
                    );
                }
            }
            for (key, value) in properties {
                if !matches!(key.as_str(), "default" | "examples" | "enum" | "const") {
                    known_references(value, identifiers)?;
                }
            }
        }
        _ => {}
    }
    Ok(())
}

/// The contracts of every `json-schema` obligation in the manifest.
///
/// # Errors
///
/// Returns an error when a schema fails to load.
pub fn contracts(root: &Path, manifest: &crate::obligations::Manifest) -> Result<Vec<Contract>> {
    manifest
        .obligations
        .iter()
        .filter_map(|obligation| match obligation {
            crate::obligations::Obligation::JsonSchema {
                schema,
                sha256,
                encoded_sha256,
                targets,
                resources,
                close_root,
                undeclared,
            } => Some(Contract::load(
                root,
                &crate::obligations::Vendored {
                    schema: schema.clone(),
                    sha256: sha256.clone(),
                    encoded_sha256: encoded_sha256.clone(),
                },
                targets,
                resources,
                *close_root,
                undeclared,
            )),
            _ => None,
        })
        .collect()
}

/// The rendered text of a governed target as a JSON value, read as TOML for a `.toml` target.
fn instance(name: &str, contents: &str) -> Result<Value> {
    if crate::has_extension(name, "toml") {
        toml::from_str(contents).with_context(|| format!("invalid rendered TOML: {name}"))
    } else {
        serde_json::from_str(contents).with_context(|| format!("invalid rendered JSON: {name}"))
    }
}

/// Validates every governed target of one profile's dump under its consumer's schema and records it as a witness.
/// Exempt as semantic-boundary: a schema checks a permission rule's syntax and a format string's type.
/// The consumer decides whether the rule takes effect and how the format renders.
/// Evidence: `xtask/tests/schemas.rs::a_planted_unknown_key_in_a_mapped_target_fails_and_names_the_target_and_profile`.
/// Reconsider when the cell-grid comparison renders each format string through its consumer.
///
/// # Errors
///
/// Returns an error naming the target and the profile when a target breaks its schema or the schema admits a planted key.
pub fn conform(profile: &str, dump: &Value, contracts: &mut [Contract]) -> Result<()> {
    let mut refusals = Vec::new();
    for contract in contracts.iter_mut() {
        for target in contract.targets.clone() {
            let Some(contents) = dump[&target]["contents"].as_str() else {
                continue;
            };
            if let Some(refusal) = contract.refusal(profile, &target, &instance(&target, contents)?)
            {
                refusals.push(refusal);
            }
            contract.witnesses.push(format!("{profile}:{target}"));
        }
    }
    ensure!(refusals.is_empty(), "{}", refusals.join("\n"));
    Ok(())
}

/// Checks every rendered target of one profile and returns the shell targets it checked.
pub(crate) fn rendered(root: &Path, dump: &Value, directory: &Path) -> Result<Vec<String>> {
    let targets = dump.as_object().context("profile dump must be a mapping")?;
    fs::create_dir(directory)?;
    let mut shells = Vec::new();
    let mut powershell = Vec::new();
    for (index, (name, target)) in targets.iter().enumerate() {
        if target["type"] != "file" && target["type"] != "script" {
            continue;
        }
        let contents = target["contents"]
            .as_str()
            .context("rendered content must be text")?;
        let script = target["type"] == "script";
        if let Some(fault) = crate::lint_rules::fault(contents.as_bytes()) {
            anyhow::bail!("rendered {name} breaks the encoding: {}", fault.key());
        }
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
        if let Some(selection) = shell_selection(name, contents) {
            shell_file(
                root,
                directory,
                &file("sh"),
                name,
                contents,
                script,
                selection,
            )?;
            shells.push(name.clone());
        }
        if crate::has_extension(name, "ps1") {
            powershell_file(directory, &file("ps1"), name, contents, script)?;
            powershell.push((name.clone(), contents.to_owned()));
        }
    }
    if !powershell.is_empty() {
        powershell.extend(runtime_scripts(root)?);
        let findings = crate::lint::analyze_definitions(root, &powershell)?;
        ensure!(
            findings.is_empty(),
            "PSScriptAnalyzer {} refuses these scripts: {}",
            platform::script_analyzer::VERSION,
            findings
                .iter()
                .map(|(name, rule)| format!("{name} ({rule})"))
                .collect::<Vec<_>>()
                .join(", ")
        );
    }
    Ok(shells)
}

/// The rules `shellcheck` reports over one shell text with the committed settings.
///
/// # Errors
///
/// Returns an error when the text names no shell script or `shellcheck` can't start.
pub fn shellcheck_findings(root: &Path, name: &str, text: &str) -> Result<Vec<String>> {
    let selection =
        shell_selection(name, text).with_context(|| format!("{name} is no shell script"))?;
    let scratch = tempfile::tempdir()?;
    let file = scratch.path().join("probe.sh");
    fs::write(&file, text)?;
    let mut command = Tool::Shellcheck.command();
    command
        .current_dir(scratch.path())
        .args(["--severity=style", "--format=gcc", "--rcfile"])
        .arg(root.join(SHELLCHECK_SETTINGS));
    if selection.needs_shell_flag() {
        command.arg(format!("--shell={}", selection.dialect.name()));
    }
    // A relative path keeps a Windows drive colon out of the `path:line:column` records.
    let output = command
        .arg("probe.sh")
        .output()
        .context("start ShellCheck")?;
    Ok(
        crate::lint::parse_shellcheck(&String::from_utf8_lossy(&output.stdout))
            .into_iter()
            .map(|(_, code)| code)
            .collect(),
    )
}

/// Refusals for every lint obligation whose probe draws no finding of its rule, so a gate that stopped finding anything can't pass.
///
/// # Errors
///
/// Returns an error when a linter can't start.
pub fn lint_probes(root: &Path, manifest: &crate::obligations::Manifest) -> Result<Vec<String>> {
    use crate::obligations::{LintTool, Obligation};
    let mut refusals = Vec::new();
    for obligation in &manifest.obligations {
        let Obligation::Lint {
            tool,
            probe,
            finding,
            ..
        } = obligation
        else {
            continue;
        };
        let found = match tool {
            LintTool::Shellcheck => shellcheck_findings(root, "probe", probe)?,
            LintTool::Psscriptanalyzer => {
                crate::lint::analyze_definitions(root, &[("probe".to_owned(), probe.clone())])?
                    .into_iter()
                    .map(|(_, rule)| rule)
                    .collect()
            }
        };
        if !found.contains(finding) {
            refusals.push(format!(
                "the {} probe drew {found:?} instead of {finding}, so the gate would pass a script it must refuse",
                tool.settings()
            ));
        }
    }
    Ok(refusals)
}

/// The directory of the PowerShell scripts the Windows runtime embeds.
pub const RUNTIME_SCRIPTS: &str = "crates/dotfiles/assets";

/// Every PowerShell script the Windows runtime embeds, named by its checkout path.
fn runtime_scripts(root: &Path) -> Result<Vec<(String, String)>> {
    let mut found = Vec::new();
    for entry in fs::read_dir(root.join(RUNTIME_SCRIPTS))? {
        let path = entry?.path();
        if path.extension().is_some_and(|extension| extension == "ps1") {
            let name = format!(
                "{RUNTIME_SCRIPTS}/{}",
                path.file_name().unwrap_or_default().to_string_lossy()
            );
            found.push((name, fs::read_to_string(&path)?));
        }
    }
    found.sort();
    Ok(found)
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
    root: &Path,
    directory: &Path,
    file: &Path,
    name: &str,
    contents: &str,
    script: bool,
    selection: crate::shell_target_rules::Selection,
) -> Result<()> {
    use crate::shell_target_rules::{Checker, checker};
    ensure!(
        !launcher_resolves_through_home(name, contents),
        "launcher {name} finds its program through the runtime HOME, which tools that run Git with another HOME replace; render the installed path"
    );
    fs::write(file, contents)?;
    let path = file.to_str().context("invalid fixture path")?;
    let dialect = selection.dialect;
    match checker(dialect) {
        Checker::Shellcheck => {
            run(
                shell_parser(dialect)?,
                directory,
                dialect.name(),
                &["-n", path],
            )
            .with_context(|| format!("rendered {name} doesn't parse as {}", dialect.name()))?;
            shellcheck(root, directory, path, selection)
                .with_context(|| format!("ShellCheck refuses rendered {name}"))?;
        }
        // Exempt as disproportionate: shellcheck reads no zsh per SC1071, so `zsh -n` and the zsh startup exec-count ceiling stand in.
        Checker::ZshParse => zsh_parse(directory, name, path)
            .with_context(|| format!("rendered {name} doesn't parse as zsh"))?,
    }
    if script && !crate::launchers::bootstraps(name) {
        ensure!(
            single_exec(contents),
            "procedural shell setup must move to Rust: {name}"
        );
    }
    Ok(())
}

/// The shell targets that only a `#!/bin/sh` first line once selected, which the profile gate must check in some profile.
pub const SHELL_WITNESSES: [&str; 4] = [
    ".bashrc",
    ".profile",
    ".claude/bashenv.sh",
    ".config/shell/ssh-agent.sh",
];

/// The `shellcheck` settings every rendered shell target and the lint obligation share.
pub const SHELLCHECK_SETTINGS: &str = "policy/shellcheckrc";

/// The parser of a dialect's syntax check.
fn shell_parser(dialect: crate::shell_target_rules::Dialect) -> Result<Command> {
    use crate::shell_target_rules::Dialect;
    match dialect {
        Dialect::Sh => posix_shell(),
        Dialect::Bash if cfg!(windows) => {
            let shell = posix_shell_path()?;
            Ok(crate::tool::external(shell.with_file_name("bash.exe")))
        }
        Dialect::Bash => Ok(Tool::Bash.command()),
        Dialect::Zsh => Ok(Tool::Zsh.command()),
    }
}

/// Runs `shellcheck` at style severity with the committed settings, naming the dialect when the text names none.
///
/// # Errors
///
/// Returns an error when `shellcheck` can't start or reports a finding.
pub fn shellcheck(
    root: &Path,
    directory: &Path,
    path: &str,
    selection: crate::shell_target_rules::Selection,
) -> Result<()> {
    let settings = root.join(SHELLCHECK_SETTINGS);
    let mut arguments = vec![
        "--severity=style".to_owned(),
        "--rcfile".to_owned(),
        settings.to_string_lossy().into_owned(),
    ];
    if selection.needs_shell_flag() {
        arguments.push(format!("--shell={}", selection.dialect.name()));
    }
    arguments.push(path.to_owned());
    let arguments: Vec<&str> = arguments.iter().map(String::as_str).collect();
    command(directory, Tool::Shellcheck, &arguments)
}

/// Parses a zsh startup file with macOS's system shell or Linux's pinned shell.
/// Windows leaves the parse to the Mac and Linux gates when it has no zsh.
fn zsh_parse(directory: &Path, name: &str, path: &str) -> Result<()> {
    let installed = Path::new(Tool::Zsh.program()).is_file();
    if !installed && cfg!(windows) {
        println!("zsh is absent on this host, so the Mac gate parses {name} with zsh -n");
        return Ok(());
    }
    run(Tool::Zsh.command(), directory, "zsh", &["-n", path])
}

/// Whether a shell launcher holds only one `exec`, after an optional `set -eu`.
#[must_use]
pub fn single_exec(contents: &str) -> bool {
    let mut lines = contents
        .lines()
        .filter(|line| !line.starts_with('#') && !line.trim().is_empty())
        .peekable();
    if lines.peek() == Some(&"set -eu") {
        lines.next();
    }
    lines.next().is_some_and(|line| line.starts_with("exec ")) && lines.next().is_none()
}

fn powershell_file(
    directory: &Path,
    file: &Path,
    name: &str,
    contents: &str,
    script: bool,
) -> Result<()> {
    if script && !crate::launchers::bootstraps(name) {
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
            match lines.as_slice() {
                [] => true,
                [call, exit] => call.starts_with("& ") && exit == "exit $LASTEXITCODE",
                _ => false,
            },
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
        let starts = markdown && line.trim_start().trim_start_matches("$ ").starts_with(JUST);
        for (index, _) in line.match_indices(JUST) {
            let before = &line[..index];
            let command = starts && before.trim_start().trim_start_matches("$ ").is_empty()
                || before.ends_with(['`', '\'', '"', '('])
                || before.ends_with("run: ")
                || before.ends_with("see ");
            let recipe: String = line[index + JUST.len()..]
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

    #[cfg(target_os = "linux")]
    #[test]
    fn the_pinned_zsh_rejects_invalid_startup_syntax() {
        let directory = tempfile::tempdir().unwrap();
        let path = "startup.zsh";
        fs::write(directory.path().join(path), "if true; then\nfi\n").unwrap();
        zsh_parse(directory.path(), path, path).unwrap();
        fs::write(directory.path().join(path), "if true; then\n").unwrap();
        let error = zsh_parse(directory.path(), path, path).unwrap_err();
        assert_eq!(error.to_string(), "zsh failed with exit status: 1");
    }

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
