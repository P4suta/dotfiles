//! The lint stages: each linter the repository pins, run over repository source files and held to a shrink-only baseline under `policy/lint-baselines/`.
//! `check --staged` runs every stage over the staged files, and the prose gate runs every stage over tracked and nonignored new files.

use crate::lint_rules::{self, Standing};
use crate::tool::Tool;
use anyhow::{Context, Result, ensure};
use std::collections::BTreeMap;
use std::fs;
use std::path::Path;
use std::process::Command;

/// The directory of the baselines, relative to the checkout.
pub const BASELINES: &str = "policy/lint-baselines";

/// Paths per linter process, which keeps a Windows command line under its length limit.
const CHUNK: usize = 200;

/// A linter record's parts: its path, line, column, and the rest.
const LOCATED_PARTS: usize = 4;

/// The bytes that hold a shebang line.
const HEAD_BYTES: usize = 32;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Stage {
    Encoding,
    Rustfmt,
    Typos,
    Taplo,
    Yamllint,
    Shellcheck,
    ScriptAnalyzer,
}

impl Stage {
    pub const ALL: [Self; 7] = [
        Self::Encoding,
        Self::Rustfmt,
        Self::Typos,
        Self::Taplo,
        Self::Yamllint,
        Self::Shellcheck,
        Self::ScriptAnalyzer,
    ];

    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Encoding => "encoding",
            Self::Rustfmt => "rustfmt",
            Self::Typos => "typos",
            Self::Taplo => "taplo",
            Self::Yamllint => "yamllint",
            Self::Shellcheck => "shellcheck",
            Self::ScriptAnalyzer => "psscriptanalyzer",
        }
    }

    /// The configuration at the checkout root that the stage reads.
    #[must_use]
    pub const fn configuration(self) -> Option<&'static str> {
        match self {
            Self::Encoding => Some(".editorconfig"),
            Self::Typos => Some(".typos.toml"),
            Self::Yamllint => Some(".yamllint"),
            Self::Rustfmt | Self::Taplo | Self::Shellcheck | Self::ScriptAnalyzer => None,
        }
    }

    /// The baseline file, relative to the checkout.
    /// The suffix keeps taplo from reading the taplo baseline as its own configuration.
    #[must_use]
    pub fn baseline(self) -> String {
        format!("{BASELINES}/{}-findings.toml", self.name())
    }

    /// Whether the stage reads `path`, which starts with the bytes of `head`.
    /// chezmoi templates hold template syntax that no linter parses, so the source-specific stages skip them.
    #[must_use]
    pub fn covers(self, path: &str, head: &[u8]) -> bool {
        let extension = Path::new(path)
            .extension()
            .map(|extension| extension.to_string_lossy().to_ascii_lowercase())
            .unwrap_or_default();
        let extension = extension.as_str();
        match self {
            Self::Encoding | Self::Typos => true,
            Self::Rustfmt => extension == "rs",
            Self::Taplo => extension == "toml",
            Self::Yamllint => matches!(extension, "yml" | "yaml") || path == ".yamllint",
            Self::Shellcheck => {
                extension != "tmpl"
                    && (matches!(extension, "sh" | "bash")
                        || [
                            &b"#!/bin/sh"[..],
                            b"#!/bin/bash",
                            b"#!/usr/bin/env sh",
                            b"#!/usr/bin/env bash",
                        ]
                        .iter()
                        .any(|shebang| head.starts_with(shebang)))
            }
            Self::ScriptAnalyzer => matches!(extension, "ps1" | "psm1" | "psd1"),
        }
    }
}

/// Finding counts by path, then by rule.
pub type Counts = BTreeMap<String, BTreeMap<String, u32>>;

fn count(found: impl IntoIterator<Item = (String, String)>) -> Counts {
    let mut counts = Counts::new();
    for (path, rule) in found {
        *counts.entry(path).or_default().entry(rule).or_default() += 1;
    }
    counts
}

/// The text files among `paths`, which Git neither treats as binary nor exempts from text handling.
fn text_files(root: &Path, paths: &[String]) -> Result<Vec<String>> {
    let mut text = Vec::new();
    for chunk in paths.chunks(CHUNK) {
        let output = Tool::Git
            .command()
            .current_dir(root)
            .args([
                "ls-files",
                "--cached",
                "--others",
                "--exclude-standard",
                "--eol",
                "-z",
                "--",
            ])
            .args(chunk)
            .output()
            .context("start git")?;
        ensure!(output.status.success(), "git cannot list line endings");
        for record in output.stdout.split(|byte| *byte == 0) {
            let record = String::from_utf8_lossy(record);
            let Some((info, path)) = record.split_once('\t') else {
                continue;
            };
            let mut fields = info.split_whitespace();
            let _index = fields.next();
            let work = fields.next().unwrap_or_default();
            let attributes = fields.collect::<Vec<_>>().join(" ");
            if work != "w/-text" && !attributes.contains("-text") && !attributes.contains("binary")
            {
                text.push(path.to_owned());
            }
        }
    }
    Ok(text)
}

/// Runs each chunk and accepts a failed exit only when its output holds parsable findings.
fn chunked(
    root: &Path,
    command: impl Fn() -> Command,
    paths: &[String],
    parse: impl Fn(&str) -> Vec<(String, String)>,
) -> Result<String> {
    let mut text = String::new();
    for chunk in paths.chunks(CHUNK) {
        let mut command = command();
        let output = command
            .current_dir(root)
            .args(chunk)
            .output()
            .context("start a linter")?;
        let run = format!(
            "{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        ensure!(
            lint_rules::completed(output.status.success(), !parse(&run).is_empty()),
            "{} failed with {} without parsable findings:\n{}",
            command.get_program().to_string_lossy(),
            output.status,
            run.trim()
        );
        text.push_str(&run);
    }
    Ok(text)
}

/// The checkout-relative spelling of a path a linter printed, absolute or relative, with forward slashes.
fn relative(root: &Path, printed: &str) -> String {
    let printed = printed
        .trim()
        .trim_start_matches(r"\\?\")
        .replace('\\', "/");
    let root = root
        .to_string_lossy()
        .trim_start_matches(r"\\?\")
        .replace('\\', "/");
    let root = root.trim_end_matches('/');
    printed
        .strip_prefix(root)
        .map_or(printed.as_str(), |rest| rest.trim_start_matches('/'))
        .to_owned()
}

/// Splits a `path:line:column: rest` record.
fn located(line: &str) -> Option<(&str, &str)> {
    let mut parts = line.splitn(LOCATED_PARTS, ':');
    let path = parts.next()?;
    let row = parts.next()?;
    let column = parts.next()?;
    let rest = parts.next()?;
    (row.chars().all(|c| c.is_ascii_digit())
        && column.chars().all(|c| c.is_ascii_digit())
        && !row.is_empty())
    .then_some((path, rest))
}

#[must_use]
pub fn parse_typos(output: &str) -> Vec<(String, String)> {
    output
        .lines()
        .filter_map(|line| {
            let (path, rest) = located(line)?;
            let word = rest.split('`').nth(1)?;
            Some((path.to_owned(), word.to_owned()))
        })
        .collect()
}

#[must_use]
pub fn parse_yamllint(output: &str) -> Vec<(String, String)> {
    output
        .lines()
        .filter_map(|line| {
            let (path, rest) = located(line)?;
            let rule = rest.trim_end().strip_suffix(')')?.rsplit_once('(')?.1;
            Some((path.to_owned(), rule.to_owned()))
        })
        .collect()
}

#[must_use]
pub fn parse_shellcheck(output: &str) -> Vec<(String, String)> {
    output
        .lines()
        .filter_map(|line| {
            let (path, rest) = located(line)?;
            let code = rest.trim_end().strip_suffix(']')?.rsplit_once('[')?.1;
            Some((path.to_owned(), code.to_owned()))
        })
        .collect()
}

/// Paths that taplo names in its error records, with the rule each record breaks.
#[must_use]
pub fn parse_taplo(root: &Path, output: &str, rule: &str) -> Vec<(String, String)> {
    let mut paths: Vec<_> = output
        .lines()
        .filter_map(|line| {
            let path = line
                .split_once("path=\"")
                .and_then(|(_, quoted)| quoted.split('"').next())
                .or_else(|| {
                    let location = line.trim_start().strip_prefix("┌─ ")?;
                    let (location, column) = location.rsplit_once(':')?;
                    let (path, row) = location.rsplit_once(':')?;
                    (!row.is_empty()
                        && !column.is_empty()
                        && row.bytes().all(|byte| byte.is_ascii_digit())
                        && column.bytes().all(|byte| byte.is_ascii_digit()))
                    .then_some(path)
                })?;
            Some((relative(root, path), rule.to_owned()))
        })
        .collect();
    paths.sort_unstable();
    paths.dedup();
    paths
}

/// Paths that `rustfmt --check` would reformat, once each.
#[must_use]
pub fn parse_rustfmt(root: &Path, output: &str) -> Vec<(String, String)> {
    let mut paths: Vec<String> = output
        .lines()
        .filter_map(|line| {
            let rest = line
                .strip_prefix("Diff in ")?
                .trim_end()
                .trim_end_matches(':');
            let rest = rest.split(" at line ").next()?;
            let path = match rest.rsplit_once(':') {
                Some((path, row)) if row.chars().all(|c| c.is_ascii_digit()) => path,
                _ => rest,
            };
            Some(relative(root, path))
        })
        .collect();
    paths.sort();
    paths.dedup();
    paths
        .into_iter()
        .map(|path| (path, "format".to_owned()))
        .collect()
}

/// The `PSScriptAnalyzer` settings every PowerShell gate and the lint obligation share.
pub const SCRIPT_ANALYZER_SETTINGS: &str = "policy/PSScriptAnalyzerSettings.psd1";

/// The pinned `PSScriptAnalyzer` module, saved into the user cache on first use and refused when its assembly differs from the pin.
fn analyzer_module() -> Result<std::path::PathBuf> {
    let base = platform::home::home()
        .context("the home is unavailable")?
        .join(".cache/dotfiles/psscriptanalyzer");
    platform::script_analyzer::save(&base)?;
    platform::script_analyzer::verified(&base)
}

/// Runs the pinned analyzer with the committed settings over each `(label, file)` input, reading each file as a script definition when `definitions` holds, and returns each finding as its label and rule.
fn analyze(
    root: &Path,
    inputs: &[(String, String)],
    definitions: bool,
) -> Result<Vec<(String, String)>> {
    let module = analyzer_module()?;
    let script = "$ErrorActionPreference = 'Stop'
Import-Module -Name $env:DOTFILES_ANALYZER_MODULE -Force
if ((Get-Module PSScriptAnalyzer).Version.ToString() -ne $env:DOTFILES_ANALYZER_VERSION) {
  throw 'The loaded analyzer version differs from its pin'
}
foreach ($line in ($env:DOTFILES_ANALYZER_INPUTS -split \"`n\")) {
  if (-not $line) { continue }
  $label, $path = $line -split \"`t\", 2
  if ($env:DOTFILES_ANALYZER_DEFINITIONS -eq '1') {
    $text = Get-Content -Raw -LiteralPath $path
    $findings = Invoke-ScriptAnalyzer -ScriptDefinition $text -Settings $env:DOTFILES_ANALYZER_SETTINGS
  } else {
    $findings = Invoke-ScriptAnalyzer -Path $path -Settings $env:DOTFILES_ANALYZER_SETTINGS
  }
  foreach ($finding in $findings) {
    \"$label`t$($finding.RuleName)\"
  }
}";
    let listed: Vec<String> = inputs
        .iter()
        .map(|(label, path)| format!("{label}\t{path}"))
        .collect();
    let cache = platform::process::PowerShellCache::new()?;
    let mut command = Tool::Pwsh.command();
    command
        .current_dir(root)
        .args(["-NoProfile", "-NonInteractive", "-Command", script])
        .env("DOTFILES_ANALYZER_MODULE", &module)
        .env(
            "DOTFILES_ANALYZER_VERSION",
            platform::script_analyzer::VERSION,
        )
        .env(
            "DOTFILES_ANALYZER_SETTINGS",
            root.join(SCRIPT_ANALYZER_SETTINGS),
        )
        .env("DOTFILES_ANALYZER_INPUTS", listed.join("\n"))
        .env(
            "DOTFILES_ANALYZER_DEFINITIONS",
            if definitions { "1" } else { "0" },
        )
        .stdin(std::process::Stdio::null());
    let output = cache.apply(&mut command).output().context("start pwsh")?;
    ensure!(
        output.status.success(),
        "PSScriptAnalyzer {} failed: {}",
        platform::script_analyzer::VERSION,
        String::from_utf8_lossy(&output.stderr)
    );
    Ok(String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter_map(|line| line.split_once('\t'))
        .map(|(label, rule)| (label.to_owned(), rule.trim().to_owned()))
        .collect())
}

/// The analyzer's findings over script texts, each named by its label and passed with `-ScriptDefinition`.
///
/// # Errors
///
/// Returns an error when the pinned analyzer fails to save or verify, or pwsh fails.
pub fn analyze_definitions(
    root: &Path,
    texts: &[(String, String)],
) -> Result<Vec<(String, String)>> {
    let scratch = tempfile::tempdir()?;
    let mut inputs = Vec::new();
    for (index, (label, text)) in texts.iter().enumerate() {
        let file = scratch.path().join(format!("{index}.ps1"));
        fs::write(&file, text)?;
        inputs.push((label.clone(), file.to_string_lossy().into_owned()));
    }
    analyze(root, &inputs, true)
}

fn script_analyzer(root: &Path, paths: &[String]) -> Result<Vec<(String, String)>> {
    let inputs: Vec<(String, String)> = paths
        .iter()
        .map(|path| (path.clone(), path.clone()))
        .collect();
    analyze(root, &inputs, false)
}

/// The tracked symbolic links among `paths`, which a checkout without link support writes as a file holding the target.
fn symlinks(root: &Path, paths: &[String]) -> Result<Vec<String>> {
    let mut links = Vec::new();
    for chunk in paths.chunks(CHUNK) {
        let output = Tool::Git
            .command()
            .current_dir(root)
            .args(["ls-files", "--stage", "-z", "--"])
            .args(chunk)
            .output()
            .context("start git")?;
        ensure!(output.status.success(), "git cannot list the index");
        links.extend(
            output
                .stdout
                .split(|byte| *byte == 0)
                .map(String::from_utf8_lossy)
                .filter(|record| record.starts_with("120000 "))
                .filter_map(|record| record.split_once('\t').map(|(_, path)| path.to_owned())),
        );
    }
    Ok(links)
}

fn head(root: &Path, path: &str) -> Vec<u8> {
    fs::read(root.join(path))
        .map(|mut bytes| {
            bytes.truncate(HEAD_BYTES);
            bytes
        })
        .unwrap_or_default()
}

/// The findings of `stage` over the existing files among `paths`.
///
/// # Errors
///
/// Returns an error when a linter can't start, or when Git can't list the text files.
pub fn findings(root: &Path, stage: Stage, paths: &[String]) -> Result<Counts> {
    if stage == Stage::Encoding {
        crate::profile_snapshot::validate_source_speech(root, paths)?;
    }
    let links = symlinks(root, paths)?;
    let files: Vec<String> = paths
        .iter()
        .filter(|path| {
            // A baseline quotes the findings it admits, so no stage reads one.
            root.join(path).is_file()
                && !root.join(path).is_symlink()
                && !links.contains(path)
                && !path.starts_with(BASELINES)
        })
        .filter(|path| stage.covers(path, &head(root, path)))
        .cloned()
        .collect();
    if files.is_empty() {
        return Ok(Counts::new());
    }
    let found = match stage {
        Stage::Encoding => {
            let mut found = Vec::new();
            for path in text_files(root, &files)? {
                if let Some(fault) = lint_rules::fault(&fs::read(root.join(&path))?) {
                    found.push((path, fault.key().to_owned()));
                }
            }
            found
        }
        Stage::Rustfmt => parse_rustfmt(
            root,
            &chunked(
                root,
                || {
                    let mut command = Tool::Rustfmt.command();
                    command.args(["--check", "--edition", "2024", "--color", "never", "--"]);
                    command
                },
                &files,
                |output| parse_rustfmt(root, output),
            )?,
        ),
        Stage::Typos => parse_typos(&chunked(
            root,
            || {
                let mut command = Tool::Typos.command();
                command.args([
                    "--force-exclude",
                    "--format",
                    "brief",
                    "--config",
                    ".typos.toml",
                    "--",
                ]);
                command
            },
            &files,
            parse_typos,
        )?),
        Stage::Taplo => taplo_findings(root, &files)?,
        Stage::Yamllint => parse_yamllint(&chunked(
            root,
            || {
                let mut command = Tool::Yamllint.command();
                command.args(["--format", "parsable", "--config-file", ".yamllint", "--"]);
                command
            },
            &files,
            parse_yamllint,
        )?),
        Stage::Shellcheck => parse_shellcheck(&chunked(
            root,
            || {
                let mut command = Tool::Shellcheck.command();
                command.args(["--format", "gcc", "--"]);
                command
            },
            &files,
            parse_shellcheck,
        )?),
        Stage::ScriptAnalyzer => script_analyzer(root, &files)?,
    };
    Ok(count(found))
}

/// Check syntax before formatting the valid files.
fn taplo_findings(root: &Path, files: &[String]) -> Result<Vec<(String, String)>> {
    let canonical_root = crate::canonical(root)?;
    let root = canonical_root.as_path();
    let mut found = parse_taplo(
        root,
        &chunked(
            root,
            || {
                let mut command = Tool::Taplo.command();
                command.args(["lint", "--no-schema", "--colors", "never", "--"]);
                command
            },
            files,
            |output| parse_taplo(root, output, "syntax"),
        )?,
        "syntax",
    );
    let valid: Vec<String> = files
        .iter()
        .filter(|path| !found.iter().any(|(invalid, _)| *path == invalid))
        .cloned()
        .collect();
    found.extend(parse_taplo(
        root,
        &chunked(
            root,
            || {
                let mut command = Tool::Taplo.command();
                command.args(["fmt", "--check", "--colors", "never", "--"]);
                command
            },
            &valid,
            |output| parse_taplo(root, output, "format"),
        )?,
        "format",
    ));
    found.retain(|(path, _)| !path.is_empty() && files.contains(path));
    Ok(found)
}

/// Reads a baseline's admitted counts from its TOML text.
///
/// # Errors
///
/// Returns an error when the text holds anything but a findings table.
pub fn parse_baseline(text: &str) -> Result<Counts> {
    #[derive(serde::Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Baseline {
        #[serde(default)]
        findings: Counts,
    }
    Ok(toml::from_str::<Baseline>(text)?.findings)
}

/// The baseline text that admits exactly `counts`.
///
/// # Errors
///
/// Returns an error when the counts don't serialize as TOML.
pub fn render_baseline(stage: Stage, counts: &Counts) -> Result<String> {
    let mut table = toml::Table::new();
    let findings: toml::Table = counts
        .iter()
        .map(|(path, rules)| {
            (
                path.clone(),
                toml::Value::Table(
                    rules
                        .iter()
                        .map(|(rule, count)| {
                            (rule.clone(), toml::Value::Integer(i64::from(*count)))
                        })
                        .collect(),
                ),
            )
        })
        .collect();
    table.insert("findings".into(), toml::Value::Table(findings));
    Ok(format!(
        "# The count of {} findings per path and rule that existed when the stage started.\n# The gate refuses a finding beyond these counts and an entry that admits more than its path holds, so this file only shrinks.\n\n{}",
        stage.name(),
        toml::to_string(&table)?
    ))
}

fn admitted_in(counts: &Counts, path: &str, rule: &str) -> u32 {
    counts
        .get(path)
        .and_then(|rules| rules.get(rule))
        .copied()
        .unwrap_or(0)
}

/// Refusals for `found` in the paths of `scope` that differ from the counts the baseline admits.
#[must_use]
pub fn compare(stage: Stage, scope: &[String], admitted: &Counts, found: &Counts) -> Vec<String> {
    let mut refusals = Vec::new();
    for path in scope {
        let mut rules: Vec<&String> = admitted
            .get(path)
            .into_iter()
            .chain(found.get(path))
            .flat_map(BTreeMap::keys)
            .collect();
        rules.sort();
        rules.dedup();
        for rule in rules {
            let allowed = admitted_in(admitted, path, rule);
            let count = admitted_in(found, path, rule);
            match lint_rules::standing(allowed, count) {
                Standing::Within => {}
                Standing::Exceeded => refusals.push(format!(
                    "{}: {path} holds {count} {rule} findings, and the baseline admits {allowed}",
                    stage.name()
                )),
                Standing::Shrunk => refusals.push(format!(
                    "{}: {path} now holds {count} {rule} findings; lower its entry in {} from {allowed}",
                    stage.name(),
                    stage.baseline()
                )),
            }
        }
    }
    refusals
}

/// Refusals for a baseline edit that admits more than the baseline at the reference did.
#[must_use]
pub fn growth(stage: Stage, before: &Counts, after: &Counts) -> Vec<String> {
    let mut refusals = Vec::new();
    for (path, rules) in after {
        for (rule, count) in rules {
            let previous = admitted_in(before, path, rule);
            if !lint_rules::shrinks(previous, *count) {
                refusals.push(format!(
                    "{}: the baseline raised {path} {rule} from {previous} to {count}; fix the finding instead",
                    stage.name()
                ));
            }
        }
    }
    refusals
}

/// The baseline of `stage` at `reference`, or `None` when the stage had none there.
fn baseline_at(root: &Path, stage: Stage, reference: &str) -> Result<Option<Counts>> {
    let output = Tool::Git
        .command()
        .current_dir(root)
        .args(["show", &format!("{reference}:{}", stage.baseline())])
        .output()
        .context("start git")?;
    if !output.status.success() {
        return Ok(None);
    }
    Ok(Some(parse_baseline(&String::from_utf8_lossy(
        &output.stdout,
    ))?))
}

fn baseline(root: &Path, stage: Stage) -> Result<Counts> {
    match fs::read_to_string(root.join(stage.baseline())) {
        Ok(text) => parse_baseline(&text).with_context(|| format!("parse {}", stage.baseline())),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Counts::new()),
        Err(error) => Err(error.into()),
    }
}

/// Refusals for every baseline that admits more than it did at `reference`.
/// A stage without a baseline at the reference starts its baseline in this change.
///
/// # Errors
///
/// Returns an error when Git can't start or a baseline doesn't parse.
pub fn baseline_growth(root: &Path, reference: &str) -> Result<Vec<String>> {
    let mut refusals = Vec::new();
    for stage in Stage::ALL {
        if let Some(before) = baseline_at(root, stage, reference)? {
            refusals.extend(growth(stage, &before, &baseline(root, stage)?));
        }
    }
    Ok(refusals)
}

/// Every tracked path, as Git lists it.
///
/// # Errors
///
/// Returns an error when Git can't list the tracked files.
pub fn tracked(root: &Path) -> Result<Vec<String>> {
    list_paths(root, false)
}

/// Tracked paths and nonignored new paths, sorted without duplicates.
///
/// # Errors
///
/// Returns an error if Git can't list source paths.
pub fn source_files(root: &Path) -> Result<Vec<String>> {
    list_paths(root, true)
}

fn list_paths(root: &Path, include_new: bool) -> Result<Vec<String>> {
    let mut command = Tool::Git.command();
    command
        .current_dir(root)
        .args(["ls-files", "--cached", "-z"]);
    if include_new {
        command.args(["--others", "--exclude-standard"]);
    }
    let output = command.output().context("start git")?;
    ensure!(output.status.success(), "git cannot list source files");
    let mut paths: Vec<String> = output
        .stdout
        .split(|byte| *byte == 0)
        .filter(|name| !name.is_empty())
        .map(|name| String::from_utf8_lossy(name).into_owned())
        .collect();
    paths.sort();
    paths.dedup();
    Ok(paths)
}

/// Runs every stage over `scope` and refuses a finding beyond its baseline, an entry that admits more than its path holds, and a baseline that grew since `reference`.
/// Over every tracked file, it also refuses an entry for a path Git no longer tracks.
///
/// # Errors
///
/// Returns an error listing every refusal, or when a linter or Git fails.
pub fn check(root: &Path, scope: &[String], every_file: bool, reference: &str) -> Result<()> {
    let growth = baseline_growth(root, reference)
        .unwrap_or_else(|error| vec![format!("baseline comparison: {error:#}")]);
    check_content(root, scope, every_file, growth)
}

/// Checks content in an isolated staged snapshot with baseline growth measured in its source checkout.
///
/// # Errors
///
/// Returns every lint refusal, including baseline growth, and any process or file error.
pub fn check_content(
    root: &Path,
    scope: &[String],
    every_file: bool,
    mut refusals: Vec<String>,
) -> Result<()> {
    let mut checks = crate::checks::Checks::default();
    for stage in Stage::ALL {
        checks.run(stage.name(), || {
            let admitted = baseline(root, stage)?;
            let found = findings(root, stage, scope)?;
            refusals.extend(compare(stage, scope, &admitted, &found));
            if every_file {
                refusals.extend(
                    admitted
                        .keys()
                        .filter(|path| !scope.contains(path))
                        .map(|path| {
                            format!(
                                "{}: {} names {path}, which Git no longer tracks; remove its entry",
                                stage.name(),
                                stage.baseline()
                            )
                        }),
                );
            }
            Ok(())
        });
    }
    if let Err(error) = checks.finish() {
        refusals.push(error.to_string());
    }
    ensure!(refusals.is_empty(), "{}", refusals.join("\n"));
    println!(
        "Ran the lint stages over {} files: {}",
        scope.len(),
        Stage::ALL.map(Stage::name).join(", ")
    );
    Ok(())
}

/// Rewrites every baseline to admit exactly the findings in the tracked files, which `check` then holds to only shrink.
///
/// # Errors
///
/// Returns an error when a linter or Git fails, or when it can't write a baseline.
pub fn write_baselines(root: &Path) -> Result<()> {
    let scope = tracked(root)?;
    fs::create_dir_all(root.join(BASELINES))?;
    for stage in Stage::ALL {
        let found = findings(root, stage, &scope)?;
        fs::write(root.join(stage.baseline()), render_baseline(stage, &found)?)?;
        println!("Wrote {} with {} paths", stage.baseline(), found.len());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture_linter(root: &Path) -> impl Fn() -> Command {
        let script = root.join("linter.ps1");
        fs::write(
            &script,
            r"param([Parameter(ValueFromRemainingArguments=$true)][string[]]$Paths)
if ($Paths -contains 'clean.yml') { exit 0 }
if ($Paths -contains 'broken.yml') {
    [Console]::Out.WriteLine('summary without findings')
    [Console]::Error.WriteLine('No version is set for shim: python')
    exit 1
}
if ($Paths -contains 'silent.yml') { exit 1 }
$finding = 'a.yml:7:21: [error] too many spaces after colon (colons)'
if ($Paths -contains 'stderr.yml') {
    [Console]::Error.WriteLine($finding)
} else {
    [Console]::Out.WriteLine($finding)
}
exit 1
",
        )
        .unwrap();
        let cache = platform::process::PowerShellCache::new().unwrap();
        move || {
            let mut command = Tool::Pwsh.command();
            command
                .args(["-NoProfile", "-NonInteractive", "-File"])
                .arg(&script);
            cache.apply(&mut command);
            command
        }
    }

    #[test]
    fn successful_runs_and_failed_runs_with_findings_keep_their_results() {
        let root = tempfile::tempdir().unwrap();
        let command = fixture_linter(root.path());
        for path in ["a.yml", "stderr.yml"] {
            let output =
                chunked(root.path(), &command, &[path.to_owned()], parse_yamllint).unwrap();
            assert_eq!(
                parse_yamllint(&output),
                [("a.yml".to_owned(), "colons".to_owned())]
            );
        }
        let output = chunked(
            root.path(),
            &command,
            &["clean.yml".to_owned()],
            parse_yamllint,
        )
        .unwrap();
        assert_eq!(parse_yamllint(&output), Vec::<(String, String)>::new());
    }

    #[test]
    fn findings_from_an_earlier_chunk_cannot_hide_a_later_failure() {
        let root = tempfile::tempdir().unwrap();
        let command = fixture_linter(root.path());
        let mut paths = vec!["a.yml".to_owned(); CHUNK];
        paths.push("broken.yml".to_owned());
        let error = chunked(root.path(), command, &paths, parse_yamllint)
            .unwrap_err()
            .to_string();
        assert!(
            error.contains("No version is set for shim: python"),
            "{error}"
        );
        assert!(error.contains("summary without findings"), "{error}");
    }

    #[test]
    fn a_silent_failure_and_a_missing_executable_are_errors() {
        let root = tempfile::tempdir().unwrap();
        assert!(
            chunked(
                root.path(),
                fixture_linter(root.path()),
                &["silent.yml".to_owned()],
                parse_yamllint,
            )
            .is_err()
        );
        let error = chunked(
            root.path(),
            || platform::process::command(root.path().join("missing-linter")),
            &["a.yml".to_owned()],
            parse_yamllint,
        )
        .unwrap_err();
        assert!(error.to_string().contains("start a linter"));
    }

    #[test]
    fn a_failed_linter_without_findings_reports_its_stderr() {
        let root = tempfile::tempdir().unwrap();
        let error = chunked(
            root.path(),
            || {
                let mut command = Tool::Git.command();
                command.arg("--invalid-linter-option");
                command
            },
            &["a.yml".to_owned()],
            parse_yamllint,
        )
        .unwrap_err()
        .to_string();
        assert!(error.contains("git"), "{error}");
        assert!(error.contains("invalid-linter-option"), "{error}");
    }

    #[test]
    fn taplo_keeps_syntax_findings_without_trying_to_format_invalid_files() {
        let root = tempfile::tempdir().unwrap();
        fs::write(root.path().join("template.toml"), "{{ value }}\n").unwrap();
        fs::write(root.path().join("format.toml"), "value=1\n").unwrap();
        fs::write(root.path().join("clean.toml"), "value = 1\n").unwrap();
        let template = "template.toml".to_owned();
        assert_eq!(
            count(taplo_findings(root.path(), std::slice::from_ref(&template)).unwrap()),
            count([(template.clone(), "syntax".into())])
        );
        assert_eq!(
            count(
                taplo_findings(
                    root.path(),
                    &[template.clone(), "format.toml".into(), "clean.toml".into()]
                )
                .unwrap()
            ),
            count([
                (template, "syntax".into()),
                ("format.toml".into(), "format".into())
            ])
        );
    }

    #[test]
    fn taplo_syntax_locations_keep_native_paths_and_ignore_summary_lines() {
        for root in ["/repo", "C:/repo", r"C:\repo"] {
            assert_eq!(
                parse_taplo(
                    Path::new(root),
                    &format!(
                        "error: invalid TOML\n  ┌─ {root}/a/b.toml:12:3\n  ┌─ {root}/a/b.toml:13:4\nERROR operation failed\n"
                    ),
                    "syntax"
                ),
                [("a/b.toml".to_owned(), "syntax".to_owned())]
            );
        }
        assert_eq!(
            parse_taplo(Path::new("/repo"), "ERROR operation failed\n", "syntax"),
            Vec::<(String, String)>::new()
        );
        for location in [
            "a.toml",
            "a.toml::1",
            "a.toml:1:",
            "a.toml:row:1",
            "a.toml:1:column",
        ] {
            assert_eq!(
                parse_taplo(Path::new("/repo"), &format!("  ┌─ {location}\n"), "syntax"),
                Vec::<(String, String)>::new()
            );
        }
    }

    #[test]
    fn each_linter_record_names_its_path_and_rule() {
        // The test joins the misspelling at run time, so the typos stage finds nothing in this file.
        let word = ["f", "pr"].concat();
        assert_eq!(
            parse_typos(&format!(
                "src/a.rs:931:19: error: `{word}` should be `for`, `far`\n"
            )),
            [("src/a.rs".to_owned(), word)]
        );
        assert_eq!(
            parse_yamllint("a.yml:7:21: [error] too many spaces after colon (colons)\n"),
            [("a.yml".to_owned(), "colons".to_owned())]
        );
        assert_eq!(
            parse_shellcheck("x.sh:3:8: note: Double quote to prevent globbing. [SC2086]\n"),
            [("x.sh".to_owned(), "SC2086".to_owned())]
        );
        let root = Path::new("C:/repo");
        assert_eq!(
            parse_taplo(
                root,
                "ERROR taplo:format_files: the file is not properly formatted path=\"C:/repo/a/b.toml\"\n",
                "format"
            ),
            [("a/b.toml".to_owned(), "format".to_owned())]
        );
        assert_eq!(
            parse_rustfmt(
                root,
                "Diff in C:\\repo\\src\\x.rs:12:\nDiff in C:\\repo\\src\\x.rs:40:\n"
            ),
            [("src/x.rs".to_owned(), "format".to_owned())]
        );
        assert_eq!(
            parse_typos("summary without a location\n"),
            Vec::<(String, String)>::new()
        );
    }

    #[test]
    fn a_new_finding_or_a_stale_entry_is_refused_and_an_exact_baseline_passes() {
        let scope = vec!["a.yml".to_owned()];
        let admitted = count([("a.yml".into(), "colons".into())]);
        assert_eq!(
            compare(Stage::Yamllint, &scope, &admitted, &admitted),
            Vec::<String>::new()
        );
        let more = count([
            ("a.yml".into(), "colons".into()),
            ("a.yml".into(), "colons".into()),
        ]);
        assert_eq!(compare(Stage::Yamllint, &scope, &admitted, &more).len(), 1);
        assert_eq!(
            compare(Stage::Yamllint, &scope, &admitted, &Counts::new()).len(),
            1
        );
        assert_eq!(
            compare(Stage::Yamllint, &[], &admitted, &Counts::new()),
            Vec::<String>::new()
        );
    }

    #[test]
    fn a_baseline_may_lose_an_entry_but_never_gain_one() {
        let before = count([("a.yml".into(), "colons".into())]);
        assert_eq!(
            growth(Stage::Yamllint, &before, &Counts::new()),
            Vec::<String>::new()
        );
        assert_eq!(
            growth(
                Stage::Yamllint,
                &before,
                &count([("b.yml".into(), "colons".into())])
            )
            .len(),
            1
        );
    }

    #[test]
    fn a_rendered_baseline_reads_back_as_the_same_counts() {
        let counts = count([
            ("a dir/x.toml".into(), "format".into()),
            ("y.sh".into(), "SC2086".into()),
            ("y.sh".into(), "SC2086".into()),
        ]);
        let text = render_baseline(Stage::Shellcheck, &counts).unwrap();
        assert_eq!(parse_baseline(&text).unwrap(), counts);
        assert!(parse_baseline("[other]\n").is_err());
    }

    #[test]
    fn the_shell_stage_reads_scripts_by_extension_or_shebang_but_never_templates() {
        assert!(Stage::Shellcheck.covers("bin/tool", b"#!/bin/sh\nexec x\n"));
        assert!(Stage::Shellcheck.covers("a.sh", b""));
        assert!(!Stage::Shellcheck.covers("a.sh.tmpl", b"#!/bin/sh\n"));
        assert!(!Stage::Shellcheck.covers("notes.txt", b"plain"));
    }

    #[test]
    fn every_lint_configuration_at_the_root_names_the_stage_or_gate_that_reads_it() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
        for stage in Stage::ALL {
            if let Some(configuration) = stage.configuration() {
                assert!(root.join(configuration).is_file(), "{configuration}");
            }
        }
        let read_elsewhere = [
            ".ocomment.toml",
            "clippy.toml",
            "deny.toml",
            ".gitattributes",
        ];
        for entry in fs::read_dir(&root).unwrap() {
            let name = entry.unwrap().file_name().to_string_lossy().into_owned();
            let lint = [
                ".typos",
                ".yamllint",
                ".editorconfig",
                ".ocomment",
                "clippy",
                "deny",
                ".markdownlint",
                ".shellcheckrc",
                "PSScriptAnalyzer",
                ".taplo",
                "rustfmt",
            ]
            .iter()
            .any(|prefix| name.starts_with(prefix));
            if lint {
                assert!(
                    Stage::ALL
                        .iter()
                        .any(|stage| stage.configuration() == Some(name.as_str()))
                        || read_elsewhere.contains(&name.as_str()),
                    "{name} configures a linter that no stage runs"
                );
            }
        }
    }
}
