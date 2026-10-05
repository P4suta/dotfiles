use crate::prose_rules::{
    Ledger, Owner, Reply, Standard, exemption_valid, ledger, occurrences, reply,
    repository_standard, rewritten_by_this_hook, tightened,
};
use crate::tool::Tool;
use anyhow::{Context, Result, bail, ensure};
use dotguard::lang;
use pulldown_cmark::{Event, Options, Parser, Tag, TagEnd};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

pub const POLICY: &str = "policy/prose.toml";
pub const SOURCE_CONFIG: &str = "dot_config/prose";
const INSTALL: &str = "mise run install:prose";

#[derive(Clone, Copy, Debug, PartialEq, Eq, clap::ValueEnum)]
pub enum Channel {
    Document,
    Commit,
    Pr,
    Issue,
    Comment,
    Reply,
}

#[derive(
    Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, clap::ValueEnum, Deserialize, Serialize,
)]
#[serde(rename_all = "kebab-case")]
pub enum Scope {
    Documents,
    Comments,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, clap::ValueEnum)]
pub enum Client {
    Claude,
    Codex,
    Opencode,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Finding {
    pub path: String,
    pub line: usize,
    pub column: usize,
    pub rule: String,
    pub message: String,
    pub sentence: String,
}

impl fmt::Display for Finding {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "{}:{}:{}: {}: {}\n    {}",
            self.path, self.line, self.column, self.rule, self.message, self.sentence
        )
    }
}

pub fn render(findings: &[Finding]) -> String {
    findings
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join("\n")
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Lock {
    vale: String,
    package: Vec<Package>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Package {
    name: String,
    version: String,
    source: String,
    archive_sha256: String,
    tree_sha256: String,
}

#[derive(Deserialize, Serialize, Default)]
#[serde(deny_unknown_fields)]
struct Tools {
    vale: Option<PathBuf>,
    bun: Option<PathBuf>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct TokenRules {
    rules: BTreeMap<String, TokenRule>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct TokenRule {
    message: String,
    tokens: Vec<String>,
}

/// The configuration, the pinned linters, and the textlint installation one check runs with.
pub struct Bundle {
    pub config: PathBuf,
    pub textlint: PathBuf,
    vale: Option<PathBuf>,
    bun: Option<PathBuf>,
    installs_textlint: bool,
}

fn home() -> Result<PathBuf> {
    std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" })
        .map(PathBuf::from)
        .context("the user directory is unavailable")
}

pub fn runtime_directory() -> Result<PathBuf> {
    match std::env::var_os("PROSE_RUNTIME") {
        Some(directory) => Ok(PathBuf::from(directory)),
        None => Ok(home()?.join(".local/share/prose")),
    }
}

impl Bundle {
    /// The configuration in a dotfiles checkout, with the linters its `mise.toml` pins.
    pub fn source(root: &Path) -> Result<Self> {
        let config = root.join(SOURCE_CONFIG);
        let lock = fs::read(config.join("textlint/bun.lock"))
            .context("read the pinned textlint lockfile")?;
        let digest = hex(&Sha256::digest(&lock));
        Ok(Self {
            textlint: std::env::temp_dir()
                .join(format!("dotfiles-prose-textlint-{}", &digest[..16])),
            config,
            vale: None,
            bun: None,
            installs_textlint: true,
        })
    }

    /// The configuration and linters that `mise run install:prose` placed in the user directory.
    pub fn installed() -> Result<Self> {
        let config = match std::env::var_os("PROSE_CONFIG") {
            Some(directory) => PathBuf::from(directory),
            None => home()?.join(".config/prose"),
        };
        let runtime = runtime_directory()?;
        let tools: Tools = match fs::read_to_string(runtime.join("tools.toml")) {
            Ok(text) => toml::from_str(&text).context("read the installed prose tool paths")?,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Tools::default(),
            Err(error) => return Err(error.into()),
        };
        Ok(Self {
            config,
            textlint: runtime.join("textlint"),
            vale: tools.vale,
            bun: tools.bun,
            installs_textlint: false,
        })
    }

    fn vale(&self) -> Command {
        self.vale
            .as_ref()
            .map_or_else(|| Tool::Vale.command(), crate::tool::external)
    }

    fn bun(&self) -> Command {
        self.bun
            .as_ref()
            .map_or_else(|| Tool::Bun.command(), crate::tool::external)
    }

    /// Refuses a missing configuration, a changed style package, or another Vale version.
    pub fn verify(&self) -> Result<()> {
        let text = fs::read_to_string(self.config.join("packages.toml")).with_context(|| {
            format!(
                "the prose configuration at {} is missing; run `{INSTALL}` in the dotfiles source",
                self.config.display()
            )
        })?;
        let lock: Lock = toml::from_str(&text).context("read the pinned prose packages")?;
        for name in [
            "vale.ini",
            "styles/Dotfiles/Attribution.yml",
            "japanese.toml",
        ] {
            ensure!(
                self.config.join(name).is_file(),
                "{} is missing from the prose configuration; run `{INSTALL}` in the dotfiles source",
                name
            );
        }
        for package in &lock.package {
            ensure!(
                !package.version.is_empty()
                    && package.source.starts_with("https://")
                    && package.archive_sha256.len() == 64,
                "the pin for the {} style package is incomplete in packages.toml",
                package.name
            );
            let directory = self.config.join("styles").join(&package.name);
            ensure!(
                directory.is_dir(),
                "the {} style package is missing; run `{INSTALL}` in the dotfiles source",
                package.name
            );
            ensure!(
                tree_digest(&directory)? == package.tree_sha256,
                "the {} style package differs from its pinned digest; restore styles/{} from the dotfiles source with `{INSTALL}`",
                package.name,
                package.name
            );
        }
        let output = self.vale().arg("--version").output().with_context(|| {
            format!(
                "Vale {} cannot start; run `mise install` and `{INSTALL}` in the dotfiles source",
                lock.vale
            )
        })?;
        let reported = String::from_utf8_lossy(&output.stdout);
        ensure!(
            output.status.success()
                && reported.split_whitespace().last() == Some(lock.vale.as_str()),
            "the prose checker requires Vale {} but found `{}`; run `mise install` and `{INSTALL}` in the dotfiles source",
            lock.vale,
            reported.trim()
        );
        Ok(())
    }
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

/// A digest over every file below `directory`, keyed by its relative path.
pub fn tree_digest(directory: &Path) -> Result<String> {
    let mut files = Vec::new();
    let mut stack = vec![directory.to_path_buf()];
    while let Some(current) = stack.pop() {
        for entry in fs::read_dir(&current)? {
            let path = entry?.path();
            if path.is_dir() {
                stack.push(path);
            } else {
                let relative = path
                    .strip_prefix(directory)?
                    .to_string_lossy()
                    .replace('\\', "/");
                files.push((relative, path));
            }
        }
    }
    files.sort();
    let mut hasher = Sha256::new();
    for (relative, path) in files {
        let bytes = fs::read(path)?;
        hasher.update(relative.as_bytes());
        hasher.update([0]);
        hasher.update((bytes.len() as u64).to_le_bytes());
        hasher.update(&bytes);
    }
    Ok(hex(&hasher.finalize()))
}

/// Blanks code, markup, and metadata with spaces and keeps every line break, so line numbers stay valid.
pub fn masked(text: &str) -> String {
    let mut bytes = text.as_bytes().to_vec();
    let mut blank = |range: std::ops::Range<usize>| {
        for byte in &mut bytes[range] {
            if *byte != b'\n' && *byte != b'\r' {
                *byte = b' ';
            }
        }
    };
    let options = Options::ENABLE_TABLES | Options::ENABLE_YAML_STYLE_METADATA_BLOCKS;
    let mut open: Option<usize> = None;
    for (event, range) in Parser::new_ext(text, options).into_offset_iter() {
        match event {
            Event::Start(
                Tag::CodeBlock(_) | Tag::MetadataBlock(_) | Tag::Table(_) | Tag::HtmlBlock,
            ) if open.is_none() => {
                open = Some(range.start);
            }
            Event::End(
                TagEnd::CodeBlock | TagEnd::MetadataBlock(_) | TagEnd::Table | TagEnd::HtmlBlock,
            ) => {
                if let Some(start) = open.take() {
                    blank(start..range.end);
                }
            }
            Event::Code(_)
            | Event::InlineHtml(_)
            | Event::Html(_)
            | Event::InlineMath(_)
            | Event::DisplayMath(_) => {
                blank(range);
            }
            Event::Start(Tag::Link { .. }) if text[range.clone()].starts_with('<') => {
                blank(range);
            }
            _ => {}
        }
    }
    String::from_utf8(bytes).unwrap_or_else(|_| text.to_owned())
}

const ABBREVIATIONS: [&str; 16] = [
    "e.g", "i.e", "etc", "vs", "cf", "approx", "no", "fig", "mr", "mrs", "ms", "dr", "st", "inc",
    "ltd", "u.s",
];

/// Character columns, counted from one, where a second sentence starts on the same line.
pub fn sentence_starts(line: &str) -> Vec<usize> {
    let chars: Vec<char> = line.chars().collect();
    let mut starts = Vec::new();
    for (index, &character) in chars.iter().enumerate() {
        if matches!(character, '。' | '！' | '？') {
            let rest = chars[index + 1..]
                .iter()
                .position(|c| !matches!(c, '」' | '』' | '）' | ')' | '"'));
            if let Some(offset) = rest {
                let next = chars[index + 1 + offset];
                if !next.is_whitespace() {
                    starts.push(index + offset + 2);
                }
            }
            continue;
        }
        if !matches!(character, '.' | '!' | '?') || index == 0 {
            continue;
        }
        let before = chars[index - 1];
        if !(before.is_alphanumeric() || matches!(before, ')' | ']' | '"' | '\'' | '’' | '”')) {
            continue;
        }
        let word: String = chars[..index]
            .iter()
            .rev()
            .take_while(|c| c.is_alphanumeric() || **c == '.')
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect();
        let lower = word.to_lowercase();
        if ABBREVIATIONS.contains(&lower.as_str())
            || (!word.is_empty() && word.chars().all(|c| c.is_ascii_digit() || c == '.'))
            || (word.chars().count() == 1 && word.chars().all(char::is_uppercase))
        {
            continue;
        }
        let mut cursor = index + 1;
        while cursor < chars.len() && matches!(chars[cursor], ')' | ']' | '"' | '\'' | '’' | '”')
        {
            cursor += 1;
        }
        let spaces = chars[cursor..].iter().take_while(|c| **c == ' ').count();
        if spaces == 0 {
            continue;
        }
        cursor += spaces;
        if chars.get(cursor).is_some_and(|c| c.is_uppercase()) {
            starts.push(cursor + 1);
        }
    }
    starts
}

/// The sentence of `line` that contains the one-based character `column`.
fn sentence_at(line: &str, column: usize) -> String {
    let mut start = 1;
    let mut end = line.chars().count() + 1;
    for boundary in sentence_starts(line) {
        if boundary <= column {
            start = boundary;
        } else {
            end = boundary;
            break;
        }
    }
    line.chars()
        .skip(start - 1)
        .take(end - start)
        .collect::<String>()
        .trim()
        .to_owned()
}

/// One text a check reads, with the source line each of its lines came from.
pub struct Input {
    pub path: String,
    pub text: String,
    pub markdown_document: bool,
    pub lines: Vec<usize>,
}

impl Input {
    pub fn new(path: &str, text: &str, markdown_document: bool) -> Self {
        Self {
            path: path.to_owned(),
            lines: (1..=text.lines().count().max(1)).collect(),
            text: text.to_owned(),
            markdown_document,
        }
    }

    fn finding(&self, line: usize, column: usize, rule: &str, message: &str) -> Finding {
        let source = self
            .text
            .lines()
            .nth(line.saturating_sub(1))
            .unwrap_or_default();
        Finding {
            path: self.path.clone(),
            line: self
                .lines
                .get(line.saturating_sub(1))
                .copied()
                .unwrap_or(line),
            column,
            rule: rule.to_owned(),
            message: message.to_owned(),
            sentence: sentence_at(source, column),
        }
    }
}

fn sentence_per_line(input: &Input) -> Vec<Finding> {
    let visible = masked(&input.text);
    let mut findings = Vec::new();
    for (index, line) in visible.lines().enumerate() {
        for column in sentence_starts(line) {
            findings.push(input.finding(
                index + 1,
                column,
                "Dotfiles.SentencePerLine",
                "Start each sentence on its own line.",
            ));
        }
    }
    findings
}

fn language(input: &Input, mode: lang::Mode) -> Vec<Finding> {
    let visible = masked(&input.text);
    lang::scan(&visible, mode)
        .into_iter()
        .map(|hit| {
            input.finding(
                hit.line,
                hit.col,
                "Dotfiles.English",
                &format!("Write persisted text in English; found {} text.", hit.kind),
            )
        })
        .collect()
}

#[derive(Deserialize)]
struct Alert {
    #[serde(rename = "Check")]
    check: String,
    #[serde(rename = "Line")]
    line: usize,
    #[serde(rename = "Span")]
    span: Vec<usize>,
    #[serde(rename = "Message")]
    message: String,
}

/// Runs the pinned Vale over every input in one process.
fn vale(bundle: &Bundle, inputs: &[Input]) -> Result<Vec<Finding>> {
    if inputs.is_empty() {
        return Ok(Vec::new());
    }
    let directory = tempfile::tempdir()?;
    let mut names = Vec::new();
    for (index, input) in inputs.iter().enumerate() {
        let name = format!(
            "{index}.{}",
            if input.markdown_document {
                "md"
            } else {
                "prose"
            }
        );
        fs::write(directory.path().join(&name), &input.text)?;
        names.push(name);
    }
    let config = crate::canonical(&bundle.config.join("vale.ini"))?;
    let output = bundle
        .vale()
        .current_dir(directory.path())
        .arg("--config")
        .arg(&config)
        .args(["--output", "JSON", "--no-exit", "--no-global"])
        .args(&names)
        .stdin(Stdio::null())
        .output()
        .with_context(|| {
            format!("start Vale; run `mise install` and `{INSTALL}` in the dotfiles source")
        })?;
    ensure!(
        output.status.success(),
        "Vale failed: {}",
        String::from_utf8_lossy(&output.stderr).trim()
    );
    let report: BTreeMap<String, Vec<Alert>> = if output.stdout.iter().all(u8::is_ascii_whitespace)
    {
        BTreeMap::new()
    } else {
        serde_json::from_slice(&output.stdout).context("read the Vale report")?
    };
    let mut findings = Vec::new();
    for (file, alerts) in report {
        let index: usize = Path::new(&file)
            .file_stem()
            .and_then(|stem| stem.to_str())
            .and_then(|stem| stem.parse().ok())
            .with_context(|| format!("Vale reported an unknown file {file}"))?;
        let input = inputs
            .get(index)
            .context("Vale reported an unknown input")?;
        for alert in alerts {
            let column = alert.span.first().copied().unwrap_or(1);
            findings.push(input.finding(alert.line, column, &alert.check, &alert.message));
        }
    }
    Ok(findings)
}

fn token_rules(bundle: &Bundle, input: &Input) -> Result<Vec<Finding>> {
    let rules: TokenRules =
        toml::from_str(&fs::read_to_string(bundle.config.join("japanese.toml"))?)
            .context("read the Japanese repository rules")?;
    let visible = masked(&input.text);
    let mut findings = Vec::new();
    for (index, line) in visible.lines().enumerate() {
        for (name, rule) in &rules.rules {
            for token in &rule.tokens {
                for (offset, _) in line.match_indices(token.as_str()) {
                    let column = line[..offset].chars().count() + 1;
                    findings.push(input.finding(
                        index + 1,
                        column,
                        name,
                        &format!("{} ({token})", rule.message),
                    ));
                }
            }
        }
    }
    Ok(findings)
}

/// Installs the pinned textlint packages into `target` unless the same lockfile already produced it.
pub fn install_textlint(package: &Path, target: &Path, bun: Command) -> Result<()> {
    let lock = fs::read(package.join("bun.lock"))?;
    if fs::read(target.join("bun.lock")).is_ok_and(|current| current == lock)
        && target
            .join("node_modules/textlint/bin/textlint.js")
            .is_file()
    {
        return Ok(());
    }
    let parent = target
        .parent()
        .context("the textlint directory has no parent")?;
    fs::create_dir_all(parent)?;
    let staging = tempfile::tempdir_in(parent)?;
    fs::copy(
        package.join("package.json"),
        staging.path().join("package.json"),
    )?;
    fs::write(staging.path().join("bun.lock"), &lock)?;
    let mut bun = bun;
    let status = bun
        .current_dir(staging.path())
        .args(["install", "--frozen-lockfile", "--ignore-scripts"])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .status()
        .context("start bun to install the pinned textlint packages")?;
    ensure!(
        status.success(),
        "the pinned textlint installation failed with {status}"
    );
    let staged = staging.keep();
    if target.exists() {
        let replaced = parent.join(format!(
            ".{}-replaced-{}",
            target
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("textlint"),
            std::process::id()
        ));
        fs::rename(target, &replaced)?;
        fs::rename(&staged, target)?;
        let _ = fs::remove_dir_all(replaced);
    } else if fs::rename(&staged, target).is_err() {
        let _ = fs::remove_dir_all(&staged);
        ensure!(
            target
                .join("node_modules/textlint/bin/textlint.js")
                .is_file(),
            "another process left an incomplete textlint installation at {}",
            target.display()
        );
    }
    Ok(())
}

#[derive(Deserialize)]
struct TextlintFile {
    messages: Vec<TextlintMessage>,
}

#[derive(Deserialize)]
struct TextlintMessage {
    #[serde(rename = "ruleId")]
    rule: String,
    message: String,
    line: usize,
    column: usize,
}

fn textlint(bundle: &Bundle, input: &Input) -> Result<Vec<Finding>> {
    if bundle.installs_textlint {
        install_textlint(
            &bundle.config.join("textlint"),
            &bundle.textlint,
            bundle.bun(),
        )?;
    }
    let script = bundle
        .textlint
        .join("node_modules/textlint/bin/textlint.js");
    ensure!(
        script.is_file(),
        "textlint is not installed at {}; run `{INSTALL}` in the dotfiles source",
        bundle.textlint.display()
    );
    let directory = tempfile::tempdir()?;
    let file = directory.path().join("reply.md");
    fs::write(&file, &input.text)?;
    let output = bundle
        .bun()
        .current_dir(&bundle.textlint)
        .arg(&script)
        .arg("--config")
        .arg(bundle.config.join("textlint/textlintrc.json"))
        .arg("--rules-base-directory")
        .arg(bundle.textlint.join("node_modules"))
        .args(["--format", "json"])
        .arg(&file)
        .stdin(Stdio::null())
        .output()
        .with_context(|| {
            format!("start bun for textlint; run `{INSTALL}` in the dotfiles source")
        })?;
    let report: Vec<TextlintFile> = serde_json::from_slice(&output.stdout).with_context(|| {
        format!(
            "textlint failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        )
    })?;
    Ok(report
        .into_iter()
        .flat_map(|file| file.messages)
        .map(|message| {
            input.finding(
                message.line,
                message.column,
                &format!("textlint.{}", message.rule),
                message.message.lines().next().unwrap_or_default(),
            )
        })
        .collect())
}

/// Removes a Conventional Commits prefix such as `feat(scope)!: `, which carries structure rather than prose.
pub fn without_type(subject: &str) -> &str {
    let Some((prefix, rest)) = subject.split_once(": ") else {
        return subject;
    };
    let head = prefix.trim_end_matches('!');
    let kind = head.split_once('(').map_or(head, |(kind, _)| kind);
    if !kind.is_empty()
        && kind.bytes().all(|byte| byte.is_ascii_lowercase())
        && !prefix.contains(' ')
    {
        rest
    } else {
        subject
    }
}

fn hexadecimal(text: &str) -> bool {
    text.len() >= 7 && text.bytes().all(|byte| byte.is_ascii_hexdigit())
}

/// A subject that Git writes around another commit's subject: an autosquash marker, a revert, a reapply, or a merge.
fn quoted_subject(subject: &str) -> bool {
    let quoted = |prefix: &str| {
        subject
            .strip_prefix(prefix)
            .is_some_and(|rest| rest.len() > 1 && rest.starts_with('"') && rest.ends_with('"'))
    };
    ["fixup! ", "squash! ", "amend! "]
        .iter()
        .any(|marker| subject.starts_with(marker))
        || quoted("Revert ")
        || quoted("Reapply ")
        || [
            "Merge branch '",
            "Merge branches '",
            "Merge remote-tracking branch '",
            "Merge tag '",
            "Merge commit '",
        ]
        .iter()
        .any(|prefix| subject.starts_with(prefix))
}

/// A commit as Git names it in a generated sentence: an abbreviated hash, or with `revert --reference` a hash followed by the subject and short date in parentheses.
fn commit_reference(text: &str) -> bool {
    let date = |date: &str| {
        date.len() == 10
            && date.bytes().enumerate().all(|(index, byte)| {
                if index == 4 || index == 7 {
                    byte == b'-'
                } else {
                    byte.is_ascii_digit()
                }
            })
    };
    hexadecimal(text)
        || text.split_once(" (").is_some_and(|(hash, rest)| {
            hexadecimal(hash)
                && rest
                    .strip_suffix(')')
                    .and_then(|rest| rest.rsplit_once(", "))
                    .is_some_and(|(_, stamp)| date(stamp))
        })
}

/// The sentences Git writes into a revert or a recorded cherry-pick, which name commits rather than describe the change.
fn generated_lines(lines: &mut Vec<&str>) {
    let commit = |rest: &str, end: &str| rest.strip_suffix(end).is_some_and(commit_reference);
    let mut index = 0;
    while index < lines.len() {
        let line = lines[index];
        let rest = line.strip_prefix("This reverts commit ");
        if rest.is_some_and(|rest| commit(rest, "."))
            || line
                .strip_prefix("(cherry picked from commit ")
                .is_some_and(|rest| commit(rest, ")"))
        {
            lines.remove(index);
        } else if rest.is_some_and(|rest| commit(rest, ", reversing"))
            && lines.get(index + 1).is_some_and(|next| {
                next.strip_prefix("changes made to ")
                    .is_some_and(|rest| commit(rest, "."))
            })
        {
            lines.drain(index..index + 2);
        } else {
            index += 1;
        }
    }
}

/// The message without Git commentary, the verbose diff, trailers, Git-generated subjects and sentences, or the type prefix.
/// An `amend!` commit carries the replacement message in its body, and the checker reads that body as a message of its own.
pub fn commit_message(raw: &str) -> String {
    let mut lines: Vec<&str> = raw
        .lines()
        .take_while(|line| !line.starts_with("# ------------------------ >8 "))
        .filter(|line| !line.starts_with('#'))
        .collect();
    while lines.first().is_some_and(|line| line.trim().is_empty()) {
        lines.remove(0);
    }
    if lines
        .first()
        .is_some_and(|subject| subject.starts_with("amend! "))
    {
        let body: Vec<&str> = lines[1..].to_vec();
        return commit_message(&body.join("\n"));
    }
    let quoted = lines.first().is_some_and(|subject| quoted_subject(subject));
    if quoted {
        lines.remove(0);
    }
    generated_lines(&mut lines);
    while lines.first().is_some_and(|line| line.trim().is_empty()) {
        lines.remove(0);
    }
    while lines.last().is_some_and(|line| line.trim().is_empty()) {
        lines.pop();
    }
    let paragraph = lines
        .iter()
        .rposition(|line| line.trim().is_empty())
        .map_or(lines.len(), |index| index + 1);
    if paragraph > 0
        && paragraph < lines.len()
        && lines[paragraph..].iter().all(|line| {
            line.split_once(": ").is_some_and(|(key, value)| {
                !key.is_empty()
                    && key
                        .bytes()
                        .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
                    && !value.trim().is_empty()
            })
        })
    {
        lines.truncate(paragraph);
        while lines.last().is_some_and(|line| line.trim().is_empty()) {
            lines.pop();
        }
    }
    let mut text = String::new();
    for (index, line) in lines.iter().enumerate() {
        text.push_str(if index == 0 && !quoted {
            without_type(line)
        } else {
            line
        });
        text.push('\n');
    }
    text
}

/// A pull request or issue as one text: the title, then the body without CodeRabbit requests.
pub fn publication(title: Option<&str>, body: &str) -> String {
    let mut text = String::new();
    if let Some(title) = title.filter(|title| !title.trim().starts_with("@coderabbitai")) {
        text.push_str(without_type(title));
        text.push_str("\n\n");
    }
    for line in body.lines() {
        if !line.trim().starts_with("@coderabbitai") {
            text.push_str(line);
        }
        text.push('\n');
    }
    text
}

pub fn japanese(text: &str) -> bool {
    text.chars().any(|c| matches!(c as u32, 0x3040..=0x30FF))
}

/// Checks one text for one channel and returns every finding.
pub fn check(bundle: &Bundle, channel: Channel, path: &str, text: &str) -> Result<Vec<Finding>> {
    bundle.verify()?;
    let mut findings = Vec::new();
    match channel {
        Channel::Comment => bail!(
            "comments are read from source files through OComment; pass the source paths instead"
        ),
        Channel::Reply if japanese(text) => {
            let input = Input::new(path, text, false);
            findings.extend(language(&input, lang::Mode::Japanese));
            findings.extend(token_rules(bundle, &input)?);
            findings.extend(textlint(bundle, &input)?);
        }
        Channel::Reply => {
            let input = Input::new(path, text, false);
            findings.extend(language(&input, lang::Mode::English));
            findings.extend(vale(bundle, std::slice::from_ref(&input))?);
        }
        Channel::Document | Channel::Commit | Channel::Pr | Channel::Issue => {
            let text = match channel {
                Channel::Commit => commit_message(text),
                Channel::Pr | Channel::Issue => publication(None, text),
                _ => text.to_owned(),
            };
            let input = Input::new(path, &text, channel == Channel::Document);
            findings.extend(language(&input, lang::Mode::English));
            findings.extend(sentence_per_line(&input));
            findings.extend(vale(bundle, std::slice::from_ref(&input))?);
        }
    }
    findings.sort_by(|a, b| (a.line, a.column, &a.rule).cmp(&(b.line, b.column, &b.rule)));
    Ok(findings)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Publication {
    PullRequest,
    Issue,
}

/// Checks a pull request or issue title and body as `pr-workflow` publishes them.
pub fn check_publication(
    bundle: &Bundle,
    kind: Publication,
    title: &str,
    body: &str,
) -> Result<()> {
    bundle.verify()?;
    let (name, rerun) = match kind {
        Publication::PullRequest => ("pull request", "pr-workflow check"),
        Publication::Issue => ("issue", "pr-workflow issue check"),
    };
    let input = Input::new(name, &publication(Some(title), body), false);
    let mut findings = language(&input, lang::Mode::English);
    findings.extend(sentence_per_line(&input));
    findings.extend(vale(bundle, std::slice::from_ref(&input))?);
    ensure!(
        findings.is_empty(),
        "the {name} fails the prose checker:\n{}\nRewrite these sentences, then rerun `{rerun}`.",
        render(&findings)
    );
    Ok(())
}

#[derive(Deserialize)]
struct Scanned {
    path: String,
    report: Option<ScannedReport>,
}

#[derive(Deserialize)]
struct ScannedReport {
    comments: Vec<ScannedComment>,
}

#[derive(Deserialize)]
struct ScannedComment {
    line: usize,
    column: usize,
    end_line: usize,
    kind: String,
    text: String,
}

/// The prose of one comment line without its delimiters.
pub fn comment_prose(line: &str) -> String {
    let mut text = line.trim();
    for suffix in ["*/", "-->"] {
        text = text.strip_suffix(suffix).unwrap_or(text).trim_end();
    }
    for prefix in [
        "//!", "///", "//", "/**", "/*", "<!--", "--", ";;", ";", "*",
    ] {
        if let Some(rest) = text.strip_prefix(prefix) {
            text = rest;
            break;
        }
    }
    text = text.trim_start_matches('#');
    text.strip_prefix(' ').unwrap_or(text).trim_end().to_owned()
}

/// Reads the comments of `paths` through OComment.
pub fn comment_inputs(root: &Path, paths: &[String]) -> Result<Vec<Input>> {
    if paths.is_empty() {
        return Ok(Vec::new());
    }
    let mut command = Tool::Ocomment.command();
    command
        .current_dir(root)
        .args(["scan", "--format", "jsonl", "--quiet"]);
    if root.join(".ocomment.toml").is_file() {
        command.arg("--config").arg(root.join(".ocomment.toml"));
    }
    let output = command
        .args(paths)
        .stdin(Stdio::null())
        .output()
        .context("OComment is required to read comments; install it with the dotfiles `ocomment` setup step (`cargo run --manifest-path xtask/Cargo.toml -- setup ocomment --live`)")?;
    ensure!(
        output.status.code().is_some_and(|code| code < 2),
        "OComment could not scan the comments: {}",
        String::from_utf8_lossy(&output.stderr).trim()
    );
    scanned_comments(&String::from_utf8(output.stdout)?)
}

/// Joins adjacent comment lines of an OComment JSON Lines scan into paragraphs, one input per file.
pub fn scanned_comments(jsonl: &str) -> Result<Vec<Input>> {
    let mut inputs = Vec::new();
    for line in jsonl.lines().filter(|line| !line.trim().is_empty()) {
        let scanned: Scanned = serde_json::from_str(line).context("read the OComment scan")?;
        let Some(report) = scanned.report else {
            continue;
        };
        let mut text = String::new();
        let mut lines = Vec::new();
        let mut previous_end = None;
        for comment in report.comments.iter().filter(|comment| {
            matches!(
                comment.kind.as_str(),
                "line" | "block" | "doc-line" | "doc-block"
            )
        }) {
            let adjacent = previous_end == Some((comment.line.saturating_sub(1), comment.column));
            if !adjacent && !text.is_empty() {
                text.push('\n');
                lines.push(comment.line);
            }
            for (offset, part) in comment.text.lines().enumerate() {
                text.push_str(&comment_prose(part));
                text.push('\n');
                lines.push(comment.line + offset);
            }
            previous_end = Some((comment.end_line, comment.column));
        }
        if !text.trim().is_empty() {
            inputs.push(Input {
                path: scanned.path.replace('\\', "/"),
                text,
                markdown_document: false,
                lines,
            });
        }
    }
    Ok(inputs)
}

/// Checks comments for language, line layout, and style.
pub fn check_inputs(bundle: &Bundle, inputs: &[Input]) -> Result<Vec<Finding>> {
    let mut findings: Vec<Finding> = inputs
        .iter()
        .flat_map(|input| {
            let mut found = language(input, lang::Mode::English);
            found.extend(sentence_per_line(input));
            found
        })
        .collect();
    findings.extend(vale(bundle, inputs)?);
    Ok(findings)
}

/// Checks the comments of source files for language and style.
pub fn check_comments(bundle: &Bundle, root: &Path, paths: &[String]) -> Result<Vec<Finding>> {
    bundle.verify()?;
    check_inputs(bundle, &comment_inputs(root, paths)?)
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Policy {
    #[serde(default)]
    pub exempt: Vec<Exempt>,
    pub legacy: LegacyLedger,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Exempt {
    pub scope: Scope,
    pub paths: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub rules: Vec<String>,
    pub reason: String,
}

/// Legacy findings per file and rule, each named by the fingerprint of its sentence.
pub type Fingerprints = BTreeMap<String, BTreeMap<String, Vec<String>>>;

/// A short digest of the sentence a finding reports, which an edited sentence no longer matches.
pub fn fingerprint(sentence: &str) -> String {
    hex(&Sha256::digest(sentence.trim().as_bytes()))[..16].to_owned()
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct LegacyLedger {
    pub reason: String,
    #[serde(default)]
    pub documents: Fingerprints,
    #[serde(default)]
    pub comments: Fingerprints,
}

impl LegacyLedger {
    fn entries(&mut self, scope: Scope) -> &mut Fingerprints {
        match scope {
            Scope::Documents => &mut self.documents,
            Scope::Comments => &mut self.comments,
        }
    }
}

fn matches_path(pattern: &str, path: &str) -> bool {
    pattern
        .strip_suffix('/')
        .map_or(pattern == path, |directory| {
            path.starts_with(&format!("{directory}/"))
        })
}

pub fn read_policy(root: &Path) -> Result<Policy> {
    toml::from_str(
        &fs::read_to_string(root.join(POLICY)).with_context(|| format!("read {POLICY}"))?,
    )
    .with_context(|| format!("{POLICY} is invalid"))
}

fn tracked(root: &Path) -> Result<Vec<String>> {
    let output = Tool::Git
        .command()
        .current_dir(root)
        .args(["ls-files", "-z"])
        .output()
        .context("list tracked files")?;
    ensure!(output.status.success(), "git ls-files failed");
    Ok(String::from_utf8(output.stdout)?
        .split('\0')
        .filter(|path| !path.is_empty())
        .map(str::to_owned)
        .collect())
}

/// Markdown documents: `*.md`, `*.md.tmpl`, and the files `.ocomment.toml` declares Markdown.
fn documents(root: &Path, files: &[String]) -> Result<BTreeSet<String>> {
    let mut declared = BTreeSet::new();
    if let Ok(text) = fs::read_to_string(root.join(".ocomment.toml")) {
        let config: toml::Value = toml::from_str(&text)?;
        for entry in config
            .get("overrides")
            .and_then(toml::Value::as_array)
            .into_iter()
            .flatten()
        {
            if entry.get("language").and_then(toml::Value::as_str) == Some("markdown") {
                for path in entry
                    .get("paths")
                    .and_then(toml::Value::as_array)
                    .into_iter()
                    .flatten()
                {
                    if let Some(path) = path.as_str().filter(|path| !path.contains('*')) {
                        declared.insert(path.to_owned());
                    }
                }
            }
        }
    }
    Ok(files
        .iter()
        .filter(|path| {
            path.ends_with(".md") || path.ends_with(".md.tmpl") || declared.contains(*path)
        })
        .cloned()
        .collect())
}

/// Every finding in the repository for one scope before any exemption or ledger entry applies.
fn scan(
    root: &Path,
    scope: Scope,
    bundle: &Bundle,
    policy: &Policy,
    used: &mut [bool],
) -> Result<Vec<Finding>> {
    bundle.verify()?;
    let files = tracked(root)?;
    let documents = documents(root, &files)?;
    let mut selected = Vec::new();
    for path in files {
        let in_scope = match scope {
            Scope::Documents => documents.contains(&path),
            Scope::Comments => !documents.contains(&path),
        };
        if !in_scope || !root.join(&path).is_file() {
            continue;
        }
        let mut skipped = false;
        for (index, exempt) in policy.exempt.iter().enumerate() {
            if exempt.scope == scope
                && exempt.rules.is_empty()
                && exempt
                    .paths
                    .iter()
                    .any(|pattern| matches_path(pattern, &path))
            {
                used[index] = true;
                skipped = true;
            }
        }
        if !skipped {
            selected.push(path);
        }
    }
    match scope {
        Scope::Comments => check_comments(bundle, root, &selected),
        Scope::Documents => {
            let mut inputs = Vec::new();
            let mut findings = Vec::new();
            for path in &selected {
                let text =
                    fs::read_to_string(root.join(path)).with_context(|| format!("read {path}"))?;
                let input = Input::new(path, &text, true);
                findings.extend(language(&input, lang::Mode::English));
                findings.extend(sentence_per_line(&input));
                inputs.push(input);
            }
            findings.extend(vale(bundle, &inputs)?);
            Ok(findings)
        }
    }
}

/// The ledger entries that would cover `findings`, each list sorted.
pub fn record(findings: &[Finding]) -> Fingerprints {
    let mut entries = Fingerprints::new();
    for finding in findings {
        entries
            .entry(finding.path.clone())
            .or_default()
            .entry(finding.rule.clone())
            .or_default()
            .push(fingerprint(&finding.sentence));
    }
    for rules in entries.values_mut() {
        for list in rules.values_mut() {
            list.sort();
        }
    }
    entries
}

/// Findings that no rule-scoped exemption covers.
fn unexempted(
    scope: Scope,
    findings: Vec<Finding>,
    policy: &Policy,
    used: &mut [bool],
) -> Vec<Finding> {
    findings
        .into_iter()
        .filter(|finding| {
            let mut covered = false;
            for (index, exempt) in policy.exempt.iter().enumerate() {
                if exempt.scope == scope
                    && exempt.rules.contains(&finding.rule)
                    && exempt
                        .paths
                        .iter()
                        .any(|pattern| matches_path(pattern, &finding.path))
                {
                    used[index] = true;
                    covered = true;
                }
            }
            !covered
        })
        .collect()
}

/// Judges findings with the exemptions and the legacy ledger and lists every refusal with its next action.
pub fn judge(
    scope: Scope,
    findings: Vec<Finding>,
    policy: &mut Policy,
    mut used: Vec<bool>,
) -> Vec<String> {
    let findings = unexempted(scope, findings, policy, &mut used);
    let mut refusals = Vec::new();
    for (exempt, used) in policy.exempt.iter().zip(&used) {
        if exempt.scope == scope && !exemption_valid(!exempt.reason.trim().is_empty(), *used) {
            refusals.push(format!(
                "{POLICY}: the exemption for {:?} needs a reason and must match a finding; give it a reason or delete it",
                exempt.paths
            ));
        }
    }
    if policy.legacy.reason.trim().is_empty() {
        refusals.push(format!(
            "{POLICY}: the legacy ledger needs a reason; state why the counted prose remains"
        ));
    }
    let found = record(&findings);
    let allowed = policy.legacy.entries(scope).clone();
    let keys: BTreeSet<(String, String)> = found
        .iter()
        .chain(&allowed)
        .flat_map(|(path, rules)| rules.keys().map(|rule| (path.clone(), rule.clone())))
        .collect();
    let none = Vec::new();
    for (path, rule) in keys {
        let lookup = |entries: &Fingerprints| -> Vec<String> {
            entries
                .get(&path)
                .and_then(|rules| rules.get(&rule))
                .unwrap_or(&none)
                .clone()
        };
        let (present, recorded) = (lookup(&found), lookup(&allowed));
        let fingerprints: BTreeSet<&String> = present.iter().chain(&recorded).collect();
        let mut new = Vec::new();
        let mut stale = Vec::new();
        for key in fingerprints {
            match ledger(occurrences(&present, key), occurrences(&recorded, key)) {
                Ledger::Within => {}
                Ledger::Exceeded => new.push(key.clone()),
                Ledger::Stale => stale.push(key.clone()),
            }
        }
        if !new.is_empty() {
            let listed: Vec<Finding> = findings
                .iter()
                .filter(|finding| {
                    finding.path == path
                        && finding.rule == rule
                        && new.contains(&fingerprint(&finding.sentence))
                })
                .cloned()
                .collect();
            refusals.push(format!(
                "{}\n{path}: the legacy ledger does not record these {rule} findings; rewrite the sentences above and rerun `just prose`",
                render(&listed)
            ));
        }
        if !stale.is_empty() {
            refusals.push(format!(
                "{path}: the legacy ledger records {rule} findings that no longer occur ({}); run `just prose-tighten` to remove them",
                stale.join(", ")
            ));
        }
    }
    refusals
}

/// Checks the repository for one scope with `policy/prose.toml`.
pub fn repository(root: &Path, scope: Scope, bundle: &Bundle) -> Result<()> {
    let mut policy = read_policy(root)?;
    let mut used = vec![false; policy.exempt.len()];
    let findings = scan(root, scope, bundle, &policy, &mut used)?;
    let refusals = judge(scope, findings, &mut policy, used);
    ensure!(
        refusals.is_empty(),
        "the {scope:?} prose check failed:\n\n{}",
        refusals.join("\n\n")
    );
    println!("Validated repository {scope:?} prose against {POLICY}");
    Ok(())
}

/// Lists the findings of one scope that no exemption covers.
pub fn findings(root: &Path, scope: Scope, bundle: &Bundle) -> Result<Vec<Finding>> {
    let policy = read_policy(root)?;
    let mut used = vec![false; policy.exempt.len()];
    let findings = scan(root, scope, bundle, &policy, &mut used)?;
    Ok(unexempted(scope, findings, &policy, &mut used))
}

/// Removes ledger entries whose findings no longer occur, and never adds one.
pub fn tighten(root: &Path, scope: Scope, bundle: &Bundle) -> Result<()> {
    let found = record(&findings(root, scope, bundle)?);
    let mut policy = read_policy(root)?;
    let ledger = policy.legacy.entries(scope);
    for (path, rules) in ledger.iter_mut() {
        for (rule, recorded) in rules.iter_mut() {
            let present = found
                .get(path)
                .and_then(|rules| rules.get(rule))
                .cloned()
                .unwrap_or_default();
            let keys: BTreeSet<String> = recorded.iter().cloned().collect();
            let mut kept = Vec::new();
            for key in keys {
                let count = tightened(occurrences(recorded, &key), occurrences(&present, &key));
                kept.extend(std::iter::repeat_n(key, count as usize));
            }
            *recorded = kept;
        }
        rules.retain(|_, recorded| !recorded.is_empty());
    }
    ledger.retain(|_, rules| !rules.is_empty());
    fs::write(root.join(POLICY), toml::to_string(&policy)?)?;
    println!("Tightened the {scope:?} legacy ledger in {POLICY}");
    Ok(())
}

/// The final reply text from a Stop hook input, falling back to the transcript.
pub fn reply_text(input: &Value) -> Result<String> {
    if let Some(text) = input["last_assistant_message"].as_str() {
        return Ok(text.to_owned());
    }
    let Some(path) = input["transcript_path"].as_str() else {
        return Ok(String::new());
    };
    let transcript = fs::read_to_string(path).context("read the session transcript")?;
    let mut parts = Vec::new();
    for line in transcript.lines().rev() {
        let Ok(entry) = serde_json::from_str::<Value>(line) else {
            continue;
        };
        let content = &entry["message"]["content"];
        match entry["type"].as_str() {
            Some("assistant") => {
                for block in content.as_array().into_iter().flatten().rev() {
                    if block["type"] == "text"
                        && let Some(text) = block["text"].as_str()
                    {
                        parts.push(text.to_owned());
                    }
                }
            }
            Some("user") => {
                let prompt = content.is_string()
                    || content
                        .as_array()
                        .is_some_and(|blocks| blocks.iter().any(|block| block["type"] == "text"));
                if prompt {
                    break;
                }
            }
            _ => {}
        }
    }
    parts.reverse();
    Ok(parts.join("\n\n"))
}

/// The file that records this hook's rewrite request for one session, when the client names the session.
fn rewrite_marker(input: &Value) -> Option<PathBuf> {
    let session = input["session_id"].as_str()?;
    let valid = !session.is_empty()
        && session.len() <= 128
        && session
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'));
    valid.then(|| Some(runtime_directory().ok()?.join("replies").join(session)))?
}

/// The hook response for a finished turn, which passes it, requests one rewrite, or ends with the findings.
pub fn stop(input: &Value, bundle: Result<Bundle>) -> Value {
    let marker = rewrite_marker(input);
    let continued = rewritten_by_this_hook(
        input["stop_hook_active"] == true,
        marker.as_ref().map(|path| path.is_file()),
    );
    let checked = bundle.and_then(|bundle| {
        let text = reply_text(input)?;
        if text.trim().is_empty() {
            return Ok(Vec::new());
        }
        check(&bundle, Channel::Reply, "reply", &text)
    });
    let reason = match &checked {
        Ok(findings) if findings.is_empty() => String::new(),
        Ok(findings) => format!(
            "The final reply fails the prose checker:\n{}\nRewrite the reply so that `prose check --channel reply` accepts it.",
            render(findings)
        ),
        Err(error) => format!("The prose checker cannot judge the final reply: {error:#}"),
    };
    let ended = |reason: &str, message: &str| serde_json::json!({"continue": false, "stopReason": reason, "systemMessage": message});
    let action = reply(reason.is_empty(), continued);
    if action != Reply::Rewrite
        && let Some(path) = &marker
    {
        let _ = fs::remove_file(path);
    }
    match action {
        Reply::Allow => serde_json::json!({}),
        Reply::Rewrite => {
            let recorded = marker.as_ref().map_or(Ok(()), |path| {
                fs::create_dir_all(path.parent().unwrap_or(path)).and_then(|()| fs::write(path, ""))
            });
            match recorded {
                Ok(()) => serde_json::json!({"decision": "block", "reason": reason}),
                Err(error) => ended(
                    &reason,
                    &format!(
                        "The prose checker cannot record its rewrite request ({error}), so it ends the turn instead of requesting one."
                    ),
                ),
            }
        }
        Reply::EndUnchecked => ended(
            &reason,
            "The final reply still fails the prose checker after one rewrite.",
        ),
    }
}

/// The GitHub owner that a remote address names, in HTTPS, SSH, or scp-like form.
/// `resolve` maps an SSH host to the host name it connects to, so an SSH host entry for github.com names its owner too.
pub fn github_owner(url: &str, resolve: impl Fn(&str) -> Option<String>) -> Option<String> {
    let (authority, path, ssh) = if let Some((scheme, rest)) = url.split_once("://") {
        let (authority, path) = rest.split_once('/')?;
        let ssh = matches!(scheme, "ssh" | "git+ssh" | "ssh+git");
        if !ssh && !matches!(scheme, "https" | "http") {
            return None;
        }
        (authority, path, ssh)
    } else {
        let (authority, path) = url.split_once(':')?;
        if authority.len() < 2 || authority.contains('/') || authority.contains('\\') {
            return None;
        }
        (authority, path, true)
    };
    let host = authority
        .rsplit_once('@')
        .map_or(authority, |(_, host)| host);
    let host = host.split_once(':').map_or(host, |(host, _)| host);
    let github = |host: &str| host.eq_ignore_ascii_case("github.com");
    if !(github(host) || ssh && resolve(host).is_some_and(|name| github(&name))) {
        return None;
    }
    let owner = path.trim_start_matches('/').split('/').next()?;
    (!owner.is_empty()).then(|| owner.to_ascii_lowercase())
}

/// Which standard a repository with these remotes takes, as [`repository_standard`] decides.
pub fn remotes_standard(
    declared: Option<bool>,
    remotes: &[(String, String)],
    personal: &[String],
    resolve: impl Fn(&str) -> Option<String>,
) -> Standard {
    let owner = |url: &str| match github_owner(url, &resolve) {
        Some(owner)
            if personal
                .iter()
                .any(|login| login.eq_ignore_ascii_case(&owner)) =>
        {
            Owner::Personal
        }
        Some(_) => Owner::Foreign,
        None => Owner::Unresolved,
    };
    let origin = remotes
        .iter()
        .find(|(name, _)| name == "origin")
        .map(|(_, url)| owner(url));
    let others: Vec<Owner> = remotes
        .iter()
        .filter(|(name, _)| name != "origin")
        .map(|(_, url)| owner(url))
        .collect();
    repository_standard(
        declared,
        origin,
        others.contains(&Owner::Personal),
        others.contains(&Owner::Foreign),
    )
}

/// The host name that OpenSSH connects to for `host`, as the SSH configuration resolves it.
fn ssh_host_name(host: &str) -> Option<String> {
    let output = Tool::Ssh
        .hook_command()
        .args(["-G", "--", host])
        .stdin(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    String::from_utf8(output.stdout)
        .ok()?
        .lines()
        .find_map(|line| {
            line.strip_prefix("hostname ")
                .map(|name| name.trim().to_owned())
        })
}

/// The personal owner logins of the global workflow policy.
pub fn personal_owners() -> Result<Vec<String>> {
    let policy: Value = serde_json::from_str(include_str!(
        "../../dot_agents/skills/pull-request/assets/workflow-policy.json"
    ))?;
    let owners: Vec<String> = policy["personal_owners"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|owner| owner["login"].as_str().map(str::to_owned))
        .collect();
    ensure!(
        !owners.is_empty(),
        "the global workflow policy names no personal owner"
    );
    Ok(owners)
}

/// The commit-msg gate every repository on a host runs: it checks the message when the repository takes the personal standard.
pub fn commit_gate(
    repository: &Path,
    message: &Path,
    bundle: impl FnOnce() -> Result<Bundle>,
) -> Result<()> {
    let output = Tool::Git
        .hook_command()
        .current_dir(repository)
        .args(["remote", "-v"])
        .output()
        .context("list the repository remotes")?;
    ensure!(
        output.status.success(),
        "git remote failed in {}",
        repository.display()
    );
    let remotes: Vec<(String, String)> = String::from_utf8(output.stdout)?
        .lines()
        .filter_map(|line| {
            let mut fields = line.split_whitespace();
            Some((fields.next()?.to_owned(), fields.next()?.to_owned()))
        })
        .collect();
    let setting = Tool::Git
        .hook_command()
        .current_dir(repository)
        .args(["config", "--type=bool", "--get", "prose.standard"])
        .output()
        .context("read the prose.standard setting")?;
    let declared = match setting.status.code() {
        Some(0) => Some(String::from_utf8(setting.stdout)?.trim() == "true"),
        Some(1) => None,
        _ => bail!(
            "prose.standard is not a boolean in {}: {}\nRun `git config prose.standard true` or `git config prose.standard false`.",
            repository.display(),
            String::from_utf8_lossy(&setting.stderr).trim()
        ),
    };
    match remotes_standard(declared, &remotes, &personal_owners()?, ssh_host_name) {
        Standard::Applies => {}
        Standard::Skips => return Ok(()),
        Standard::Undetermined => {
            let cause = remotes
                .iter()
                .find(|(name, _)| name == "origin")
                .map_or_else(
                    || {
                        "the repository has no origin remote and no personal GitHub remote"
                            .to_owned()
                    },
                    |(_, url)| format!("origin {url} names no GitHub owner"),
                );
            eprintln!(
                "The commit message was not checked for the writing standard: {cause}.\nRun `git config prose.standard true` when this repository publishes to a personal destination, or `git config prose.standard false` when it keeps its own rules."
            );
            return Ok(());
        }
    }
    let text = fs::read_to_string(message)
        .with_context(|| format!("read the commit message {}", message.display()))?;
    let findings = check(&bundle()?, Channel::Commit, "commit message", &text)?;
    ensure!(
        findings.is_empty(),
        "the commit message fails the prose checker:\n{}\nRewrite these sentences and commit again; `prose check --channel commit MESSAGE_FILE` reports the same findings.",
        render(&findings)
    );
    Ok(())
}

const HOOK_PREFIX: &str = "prose reply --client ";

/// Registers the reply check as the client's Stop hook and drops earlier registrations of it.
pub fn merge_hooks(mut value: Value, client: Client) -> Result<Value> {
    let name = match client {
        Client::Claude => "claude",
        Client::Codex => "codex",
        Client::Opencode => {
            bail!("OpenCode reads the reply check from its plugin, not from hook settings")
        }
    };
    let hooks = value
        .as_object_mut()
        .context("client settings must be a JSON object")?
        .entry("hooks")
        .or_insert_with(|| serde_json::json!({}))
        .as_object_mut()
        .context("native hooks must be an object")?;
    let entries = hooks
        .entry("Stop")
        .or_insert_with(|| serde_json::json!([]))
        .as_array_mut()
        .context("the native Stop hook must be an array")?;
    let mut retained = Vec::new();
    for mut entry in std::mem::take(entries) {
        let handlers = entry["hooks"]
            .as_array_mut()
            .context("a native hook group must contain handlers")?;
        let before = handlers.len();
        handlers.retain(|handler| {
            !handler["command"]
                .as_str()
                .is_some_and(|command| command.starts_with(HOOK_PREFIX))
        });
        if !handlers.is_empty() || handlers.len() == before {
            retained.push(entry);
        }
    }
    retained.push(serde_json::json!({"hooks": [{"type": "command", "command": format!("{HOOK_PREFIX}{name}"), "timeout": 120}]}));
    *entries = retained;
    Ok(value)
}

/// Locates a tool the source checkout pins, for the installed checker to run outside that checkout.
fn pinned_path(source: &Path, tool: &str) -> Result<PathBuf> {
    let output = Tool::Mise
        .command()
        .current_dir(source)
        .args(["which", tool])
        .output()
        .with_context(|| format!("locate the pinned {tool} with mise"))?;
    ensure!(
        output.status.success(),
        "mise cannot locate the pinned {tool}; run `mise install` in the dotfiles source"
    );
    Ok(PathBuf::from(String::from_utf8(output.stdout)?.trim()))
}

/// Installs the reply checker, its linters, and the Claude Code and Codex Stop hooks.
pub fn install(source: &Path, binary: &Path, user_directory: &Path) -> Result<()> {
    ensure!(binary.is_file(), "the built prose executable is missing");
    let directory = user_directory.join(".local/bin");
    fs::create_dir_all(&directory)?;
    let temporary = tempfile::NamedTempFile::new_in(&directory)?;
    fs::copy(binary, temporary.path())?;
    temporary.as_file().sync_all()?;
    temporary.persist(directory.join(format!("prose{}", std::env::consts::EXE_SUFFIX)))?;
    let runtime = runtime_directory()?;
    fs::create_dir_all(&runtime)?;
    let tools = Tools {
        vale: Some(pinned_path(source, "vale")?),
        bun: Some(pinned_path(source, "bun")?),
    };
    fs::write(runtime.join("tools.toml"), toml::to_string(&tools)?)?;
    install_textlint(
        &source.join(SOURCE_CONFIG).join("textlint"),
        &runtime.join("textlint"),
        tools
            .bun
            .as_ref()
            .map_or_else(|| Tool::Bun.command(), crate::tool::external),
    )?;
    Bundle::installed()?.verify()?;
    for (client, path) in [
        (Client::Codex, user_directory.join(".codex/hooks.json")),
        (Client::Claude, user_directory.join(".claude/settings.json")),
    ] {
        let value: Value = if path.try_exists()? {
            crate::skill_ops::read_json(&path)?
        } else {
            serde_json::json!({})
        };
        let next = merge_hooks(value.clone(), client)?;
        if next != value {
            crate::skill_ops::write_json(&path, &next, true)?;
        }
    }
    println!(
        "Installed the prose checker, its pinned linters, and the Claude Code and Codex reply hooks"
    );
    println!(
        "Verify the clients' hook trust and the OpenCode plugin activation before relying on the reply check"
    );
    Ok(())
}
