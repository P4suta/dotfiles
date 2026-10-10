//! Cross-platform push evidence: managed SSH access to one host per OS family, gate runs through domyjob, and Git notes that pre-push and `pr-workflow ready` require.

use crate::hosts_rules::{Evidence, Family, Key, Start, Verdict, admit, gated, pin, record, start};
use crate::runtime::Real;
use crate::tool::Tool;
use anyhow::{Context, Result, bail, ensure};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use yaml_rust2::{Yaml, YamlLoader};

pub const NOTES: &str = "refs/notes/hosts";
const STATE: &str = ".local/state/hosts";
const DEFAULT_GATE: &str = "just check";
const STAGE_REPORT: &str = "DOTFILES_HOST_STAGES ";
const CODE_SHELL: &str = "/bin/sh";
const RELEASE_PINS: &str = "home/.chezmoidata/versions.toml";

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Stage {
    Code,
    Real,
}

impl Stage {
    const ALL: [Self; 2] = [Self::Code, Self::Real];

    const fn index(self) -> usize {
        match self {
            Self::Code => 0,
            Self::Real => 1,
        }
    }

    const fn name(self) -> &'static str {
        match self {
            Self::Code => "code",
            Self::Real => "real",
        }
    }
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Stages {
    code: i32,
    real: i32,
}

#[derive(Debug, clap::Subcommand)]
pub enum Action {
    /// Check tailnet presence, SSH through the agent, domyjob, and the gate's toolchain on every managed host.
    Doctor {
        #[arg(long, default_value = DEFAULT_GATE)]
        gate: String,
    },
    /// Regenerate the managed SSH aliases and pinned host keys from the tailnet.
    Sync {
        /// Choose this tailnet host for its OS family.
        #[arg(long)]
        host: Vec<String>,
        /// Remote user.
        /// Defaults to the managed one, then to the user of a hub remote.
        #[arg(long)]
        user: Option<String>,
        /// Replace this host's pinned key with the one it presents now.
        #[arg(long)]
        accept: Vec<String>,
    },
    /// Run the gate for the clean head commit on every operating system family in the continuous integration matrix and record each result as a Git note.
    Check {
        #[arg(long, default_value = DEFAULT_GATE)]
        gate: String,
    },
    /// Run the code stage in the environment built from this checkout's installed pins.
    Code {
        #[arg(long, default_value = DEFAULT_GATE)]
        gate: String,
    },
    /// Verify native provisioning and network access without an SSH agent.
    Real,
    #[command(hide = true)]
    Worker {
        #[arg(long, default_value = DEFAULT_GATE)]
        gate: String,
        #[arg(long)]
        home: Option<PathBuf>,
    },
    /// Refuse pushed GitHub branches that lack a passing note for an operating system family in their CI matrix.
    /// Reads pre-push input.
    Gate { remote: String, url: String },
}

/// Run one `hosts` subcommand in this user's home directory.
///
/// # Errors
///
/// Returns an error when the home directory stays unavailable, reading standard input fails, or the chosen subcommand fails.
pub fn run(action: Action) -> Result<()> {
    match action {
        Action::Doctor { gate } => doctor(&home()?, &gate),
        Action::Sync { host, user, accept } => sync(&home()?, &host, user, &accept),
        Action::Check { gate } => check(&home()?, &gate),
        Action::Code { gate } => code(&home()?, &gate),
        Action::Real => crate::host_checks::check(&std::env::current_dir()?, &home()?),
        Action::Worker {
            gate,
            home: native_home,
        } => worker(&native_home.map_or_else(home, Ok)?, &gate),
        Action::Gate { url, .. } => {
            let mut input = Vec::new();
            std::io::Read::read_to_end(&mut std::io::stdin(), &mut input)?;
            push_gate(Path::new("."), &url, &input)
        }
    }
}

fn home() -> Result<PathBuf> {
    std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" })
        .map(PathBuf::from)
        .context("home directory is unavailable")
}

/// The family a runner label names, such as `ubuntu-latest` or `macos-14`.
#[must_use]
pub fn label_family(label: &str) -> Option<Family> {
    let label = label.trim().to_ascii_lowercase();
    if label.starts_with("ubuntu") || label == "linux" {
        Some(Family::Linux)
    } else if label.starts_with("macos") {
        Some(Family::Mac)
    } else if label.starts_with("windows") {
        Some(Family::Windows)
    } else {
        None
    }
}

/// The family Tailscale reports for a node.
#[must_use]
pub fn os_family(os: &str) -> Option<Family> {
    match os {
        "linux" => Some(Family::Linux),
        "macOS" => Some(Family::Mac),
        "windows" => Some(Family::Windows),
        _ => None,
    }
}

fn strings(value: &Yaml) -> Vec<String> {
    match value {
        Yaml::String(text) => vec![text.clone()],
        Yaml::Array(items) => items.iter().flat_map(strings).collect(),
        _ => Vec::new(),
    }
}

fn runner_labels(job: &Yaml, name: &str) -> Result<Vec<String>> {
    let runs_on = &job["runs-on"];
    let labels = match runs_on {
        Yaml::Hash(_) => strings(&runs_on["labels"]),
        other => strings(other),
    };
    let mut resolved = Vec::new();
    for label in labels {
        let Some(expression) = label
            .trim()
            .strip_prefix("${{")
            .and_then(|rest| rest.strip_suffix("}}"))
        else {
            resolved.push(label);
            continue;
        };
        let key = expression.trim().strip_prefix("matrix.").with_context(|| {
            format!("job {name} has a runner expression the gate cannot resolve: {label}")
        })?;
        let matrix = &job["strategy"]["matrix"];
        let mut values = strings(&matrix[key]);
        if let Yaml::Array(includes) = &matrix["include"] {
            values.extend(includes.iter().flat_map(|include| strings(&include[key])));
        }
        ensure!(
            !values.is_empty(),
            "job {name} has no matrix values for {key}"
        );
        resolved.extend(values);
    }
    Ok(resolved)
}

/// The families the workflows' jobs run on, by family index.
/// Reusable-workflow calls have no runner of their own.
///
/// # Errors
///
/// Returns an error when a workflow holds invalid YAML, a runner expression resolves to nothing, or a job's runner names no known family.
pub fn required(workflows: &[(String, String)]) -> Result<[bool; 3]> {
    let mut required = [false; 3];
    for (file, text) in workflows {
        let documents = YamlLoader::load_from_str(text).with_context(|| format!("parse {file}"))?;
        let Some(Yaml::Hash(jobs)) = documents.first().map(|document| &document["jobs"]) else {
            continue;
        };
        for (name, job) in jobs {
            let name = format!("{file}:{}", name.as_str().unwrap_or("?"));
            if job["runs-on"].is_badvalue() {
                continue;
            }
            let families: Vec<_> = runner_labels(job, &name)?
                .iter()
                .filter_map(|label| label_family(label))
                .collect();
            ensure!(
                !families.is_empty(),
                "cannot tell the OS family of job {name}"
            );
            for family in families {
                required[family.index()] = true;
            }
        }
    }
    Ok(required)
}

/// One gate run, appended to the note of the commit it ran for.
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Note {
    pub host: String,
    pub os: String,
    pub tree: String,
    pub status: i32,
    pub gate: String,
    #[serde(default)]
    pub stage: Option<Stage>,
}

/// Judge one commit's note, one JSON record per line in append order.
///
/// # Errors
///
/// Returns an error when a line holds no valid note or names an unknown OS family.
pub fn evidence(notes: &str, tree: &str) -> Result<[Evidence; 3]> {
    let mut evidence = [[Evidence::Missing; 2]; 3];
    for line in notes.lines().filter(|line| !line.trim().is_empty()) {
        let note: Note = serde_json::from_str(line).context("malformed hosts note")?;
        let family = Family::ALL
            .into_iter()
            .find(|family| family.name() == note.os)
            .with_context(|| format!("hosts note names an unknown OS family: {}", note.os))?;
        let index = family.index();
        if let Some(stage) = note.stage {
            let previous = &mut evidence[index][stage.index()];
            *previous = record(*previous, note.tree == tree, note.status == 0);
        }
    }
    Ok(evidence.map(crate::hosts_rules::stages))
}

fn git(repository: &Path, arguments: &[&str]) -> Result<Vec<u8>> {
    let output = Tool::Git
        .command()
        .arg("-C")
        .arg(repository)
        .args(arguments)
        .stderr(Stdio::piped())
        .output()
        .context("start git")?;
    ensure!(
        output.status.success(),
        "git {}: {}",
        arguments.join(" "),
        String::from_utf8_lossy(&output.stderr).trim()
    );
    Ok(output.stdout)
}

fn git_text(repository: &Path, arguments: &[&str]) -> Result<String> {
    Ok(String::from_utf8(git(repository, arguments)?)?
        .trim()
        .to_owned())
}

fn workflows_at(repository: &Path, commit: &str) -> Result<Vec<(String, String)>> {
    let listing = git(
        repository,
        &[
            "ls-tree",
            "-z",
            "--name-only",
            commit,
            "--",
            ".github/workflows/",
        ],
    )?;
    let mut workflows = Vec::new();
    for path in listing
        .split(|byte| *byte == 0)
        .filter(|path| !path.is_empty())
    {
        let path = std::str::from_utf8(path)?;
        if crate::has_extension(path, "yml") || crate::has_extension(path, "yaml") {
            let text = String::from_utf8(git(repository, &["show", &format!("{commit}:{path}")])?)?;
            workflows.push((path.to_owned(), text));
        }
    }
    Ok(workflows)
}

fn note(repository: &Path, commit: &str) -> Result<String> {
    let listing = git_text(repository, &["notes", "--ref", NOTES, "list"])?;
    for line in listing.lines() {
        if let Some((blob, annotated)) = line.split_once(' ')
            && annotated == commit
        {
            return Ok(String::from_utf8(git(
                repository,
                &["cat-file", "blob", blob],
            )?)?);
        }
    }
    Ok(String::new())
}

/// The verdict on a commit's notes for every family its own workflows require.
pub(crate) fn admission(repository: &Path, commit: &str) -> Result<Verdict> {
    let commit = git_text(
        repository,
        &["rev-parse", "--verify", &format!("{commit}^{{commit}}")],
    )
    .with_context(|| format!("commit {commit} is not in this checkout; fetch it first"))?;
    let tree = git_text(repository, &["rev-parse", &format!("{commit}^{{tree}}")])?;
    let required = required(&workflows_at(repository, &commit)?)?;
    Ok(admit(
        required,
        evidence(&note(repository, &commit)?, &tree)?,
    ))
}

/// Refuses `commit` unless every required host family recorded a passing gate for it.
///
/// # Errors
///
/// Returns an error when a family recorded no result or a failed one, or Git fails while reading the notes.
pub fn require_admission(repository: &Path, commit: &str) -> Result<()> {
    match admission(repository, commit)? {
        Verdict::Admit => Ok(()),
        Verdict::Missing(family) => bail!(
            "no {} gate result for {commit}; run `dotfiles hosts check` from a clean checkout of it",
            family.name()
        ),
        Verdict::Failed(family) => bail!(
            "the {} gate failed for {commit}; fix it, commit, and run `dotfiles hosts check` again",
            family.name()
        ),
    }
}

fn github(url: &str) -> bool {
    url.contains("github.com:") || url.contains("github.com/")
}

/// Judge pre-push input: `<local ref> <local sha> <remote ref> <remote sha>` per line.
///
/// # Errors
///
/// Returns an error when the input holds invalid UTF-8 or has a malformed line.
/// Also returns an error when a gated commit lacks a passing note for a required family, or Git fails while judging it.
pub fn push_gate(repository: &Path, url: &str, input: &[u8]) -> Result<()> {
    for line in std::str::from_utf8(input)?.lines() {
        let fields: Vec<_> = line.split_whitespace().collect();
        let [_, local, remote_ref, _] = fields[..] else {
            bail!("malformed pre-push input: {line}");
        };
        let deletion = local.bytes().all(|byte| byte == b'0');
        if gated(github(url), remote_ref.starts_with("refs/heads/"), deletion) {
            require_admission(repository, local)?;
        }
    }
    Ok(())
}

fn native(home: &Path) -> Result<Real> {
    let mut native = Real::new(home)?;
    if cfg!(windows) {
        // Only the system OpenSSH reaches the 1Password agent pipe.
        let root = std::env::var_os("SystemRoot")
            .unwrap_or_else(|| platform::home::DEFAULT_SYSTEM_ROOT.into());
        let mut paths = vec![Path::new(&root).join("System32/OpenSSH")];
        paths.extend(std::env::split_paths(&native.path));
        native.path = std::env::join_paths(paths)?;
    }
    Ok(native)
}

fn command(native: &Real, tool: Tool) -> Command {
    let mut command = native.command(tool);
    if cfg!(windows) {
        command.env_remove("SSH_AUTH_SOCK");
    }
    command
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Peer {
    pub alias: String,
    pub dns: String,
    pub address: String,
    pub family: Family,
    pub online: bool,
}

#[derive(Debug)]
pub struct Tailnet {
    pub this: Option<(String, Family)>,
    pub peers: Vec<Peer>,
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct Node {
    #[serde(rename = "DNSName")]
    dns_name: String,
    #[serde(rename = "OS")]
    os: String,
    #[serde(default, rename = "TailscaleIPs")]
    tailscale_ips: Vec<String>,
    #[serde(default)]
    online: bool,
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct Status {
    #[serde(rename = "Self")]
    this: Node,
    #[serde(default)]
    peer: BTreeMap<String, Node>,
}

/// Peers of a known OS family from `tailscale status --json`, named by their `MagicDNS` label.
///
/// # Errors
///
/// Returns an error when the status fails to parse as `tailscale status --json` output.
pub fn tailnet(status: &[u8]) -> Result<Tailnet> {
    let status: Status = serde_json::from_slice(status).context("invalid tailscale status")?;
    let alias = |node: &Node| {
        node.dns_name
            .split('.')
            .next()
            .unwrap_or_default()
            .to_owned()
    };
    let mut peers: Vec<_> = status
        .peer
        .values()
        .filter_map(|node| {
            Some(Peer {
                alias: alias(node),
                dns: node.dns_name.trim_end_matches('.').to_owned(),
                address: node
                    .tailscale_ips
                    .iter()
                    .find(|ip| ip.contains('.'))?
                    .clone(),
                family: os_family(&node.os)?,
                online: node.online,
            })
        })
        .collect();
    peers.sort_by(|a, b| a.alias.cmp(&b.alias));
    Ok(Tailnet {
        this: os_family(&status.this.os).map(|family| (alias(&status.this), family)),
        peers,
    })
}

#[derive(Debug, Default, PartialEq, Eq)]
pub struct Managed {
    pub aliases: Vec<String>,
    pub user: Option<String>,
}

pub fn managed(config: &str) -> Managed {
    let mut managed = Managed::default();
    for line in config.lines() {
        let mut words = line.split_whitespace();
        match words.next() {
            Some("Host") => managed.aliases.extend(words.map(str::to_owned)),
            Some("User") => managed.user = words.next().map(str::to_owned),
            _ => {}
        }
    }
    managed
}

#[must_use]
pub fn render(aliases: &[String], user: &str, known_hosts: &Path) -> String {
    format!(
        "# Generated by `dotfiles hosts sync` from the tailnet; host keys are pinned over the tailnet.\nHost {}\n  User {user}\n  UserKnownHostsFile {}\n  StrictHostKeyChecking yes\n  ForwardAgent no\n",
        aliases.join(" "),
        known_hosts.display().to_string().replace('\\', "/")
    )
}

/// One host per OS family other than this machine's: a requested one, then the managed one, then the only online one.
///
/// # Errors
///
/// Returns an error when a requested name matches no host of a known family in the tailnet.
pub fn select(tailnet: &Tailnet, previous: &[String], requested: &[String]) -> Result<Vec<Peer>> {
    for name in requested {
        ensure!(
            tailnet.peers.iter().any(|peer| &peer.alias == name),
            "{name} is not a Linux, Mac, or Windows host in the tailnet"
        );
    }
    let mut selected = Vec::new();
    for family in Family::ALL {
        if tailnet.this.as_ref().is_some_and(|(_, own)| *own == family) {
            continue;
        }
        let candidates: Vec<_> = tailnet
            .peers
            .iter()
            .filter(|peer| peer.family == family)
            .collect();
        let pick = |names: &[String]| -> Vec<&Peer> {
            candidates
                .iter()
                .copied()
                .filter(|peer| names.contains(&peer.alias))
                .collect()
        };
        let mut chosen = pick(requested);
        if chosen.is_empty() {
            chosen = pick(previous);
        }
        if chosen.is_empty() {
            chosen = candidates
                .iter()
                .copied()
                .filter(|peer| peer.online)
                .collect();
        }
        match chosen[..] {
            [] => {}
            [peer] => selected.push(peer.clone()),
            _ => bail!(
                "several {} hosts: {}; choose one with --host",
                family.name(),
                chosen
                    .iter()
                    .map(|peer| peer.alias.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
        }
    }
    Ok(selected)
}

/// Pinned keys by the first name of each `known_hosts` line.
#[must_use]
pub fn pinned(known_hosts: &str) -> BTreeMap<String, String> {
    known_hosts
        .lines()
        .filter_map(|line| {
            let mut fields = line.split_whitespace();
            let names = fields.next()?;
            let kind = fields.next()?;
            let key = fields.next()?;
            kind.starts_with("ssh-").then(|| {
                (
                    names.split(',').next().unwrap_or_default().to_owned(),
                    format!("{kind} {key}"),
                )
            })
        })
        .collect()
}

/// The first key line of a `known_hosts` file written for one address, as `type key`.
#[must_use]
pub fn scanned(output: &str) -> Option<String> {
    output.lines().find_map(|line| {
        let mut fields = line.split_whitespace();
        let _address = fields.next()?;
        let kind = fields.next()?;
        let key = fields.next()?;
        kind.starts_with("ssh-").then(|| format!("{kind} {key}"))
    })
}

/// The user of a hub remote on one of the hosts, as in `ssh://user@host/...` or `user@host:path`.
#[must_use]
pub fn hub_user(remotes: &str, aliases: &[String]) -> Option<String> {
    remotes.split_whitespace().find_map(|url| {
        let rest = url.strip_prefix("ssh://").unwrap_or(url);
        let (user, host) = rest.split_once('@')?;
        let host = host.split([':', '/']).next()?;
        (!user.contains('/') && aliases.iter().any(|alias| alias == host)).then(|| user.to_owned())
    })
}

/// The gate as the family's login shell runs it, so a job's minimal environment still finds the profile's tools.
#[must_use]
pub fn wrap(family: Family, gate: &str) -> Vec<String> {
    let shell: &[&str] = match family {
        Family::Linux => &["sh", "-lc"],
        Family::Mac => &["zsh", "-lc"],
        Family::Windows => &["powershell.exe", "-NoProfile", "-Command"],
    };
    shell
        .iter()
        .map(|word| (*word).to_owned())
        .chain([gate.to_owned()])
        .collect()
}

/// The gate for a domyjob snapshot holding only `head.bundle`: a checkout of exactly the commit, whose project tools mise may load.
/// Trust covers only that checkout and starts before the clone, because checkout hooks already run mise there.
#[must_use]
pub fn remote_gate(family: Family, gate: &str) -> String {
    const UNIX_GIT: &str = "/usr/bin/git";
    const WINDOWS_GIT: &str = "C:/Program Files/Git/cmd/git.exe";
    let quoted = match family {
        Family::Windows => format!("'{}'", gate.replace('\'', "''")),
        Family::Linux | Family::Mac => format!("'{}'", gate.replace('\'', "'\"'\"'")),
    };
    let worker = format!(
        "mise x -- cargo run --locked --package dotfiles --bin dotfiles -- hosts worker --gate {quoted}"
    );
    match family {
        Family::Windows => format!(
            "$nativeHome = $env:USERPROFILE; $env:DOTFILES_HOST_PATH = $env:PATH; $env:DOTFILES_HOST_CARGO_HOME = if ($env:CARGO_HOME) {{ $env:CARGO_HOME }} else {{ Join-Path $nativeHome .cargo }}; $jobRoot = $PWD.Path; $env:MISE_DATA_DIR = Join-Path $env:LOCALAPPDATA mise; $env:RUSTUP_HOME = Join-Path $nativeHome .rustup; $env:HOME = Join-Path $jobRoot bootstrap; $env:USERPROFILE = $env:HOME; New-Item -ItemType Directory -Force $env:HOME | Out-Null; $env:APPDATA = Join-Path $env:HOME AppData/Roaming; $env:LOCALAPPDATA = Join-Path $env:HOME AppData/Local; $env:CARGO_HOME = Join-Path $env:HOME .cargo; $env:MISE_CARGO_HOME = $env:CARGO_HOME; New-Item -ItemType Directory -Force $env:CARGO_HOME | Out-Null; New-Item -ItemType Junction -Path (Join-Path $env:CARGO_HOME bin) -Target (Join-Path $env:DOTFILES_HOST_CARGO_HOME bin) -ErrorAction Stop | Out-Null; $env:CARGO_TARGET_DIR = Join-Path $jobRoot build; $env:MISE_CONFIG_DIR = Join-Path $env:HOME .config/mise; $env:MISE_CACHE_DIR = Join-Path $env:HOME .cache/mise; $env:MISE_STATE_DIR = Join-Path $env:HOME .local/state/mise; $env:MISE_TRUSTED_CONFIG_PATHS = Join-Path $jobRoot checkout; $env:MISE_AUTO_INSTALL = 'false'; $env:GIT_CONFIG_GLOBAL = Join-Path $env:HOME empty.toml; Set-Content -LiteralPath $env:GIT_CONFIG_GLOBAL -Value '' -NoNewline; $env:MISE_GLOBAL_CONFIG_FILE = $env:GIT_CONFIG_GLOBAL; $env:MISE_SYSTEM_CONFIG_FILE = $env:GIT_CONFIG_GLOBAL; $env:GIT_CONFIG_NOSYSTEM = '1'; & '{WINDOWS_GIT}' -c advice.detachedHead=false clone --quiet head.bundle checkout; if (-not $?) {{ exit 1 }}; Set-Location checkout; {worker} --home $nativeHome; exit $LASTEXITCODE"
        ),
        Family::Linux | Family::Mac => {
            format!(
                "nativeHome=\"$HOME\"; export DOTFILES_HOST_PATH=\"$PATH\" DOTFILES_HOST_CARGO_HOME=\"${{CARGO_HOME:-$HOME/.cargo}}\" MISE_DATA_DIR=\"$HOME/.local/share/mise\" RUSTUP_HOME=\"$HOME/.rustup\"; export HOME=\"$PWD/bootstrap\" CARGO_TARGET_DIR=\"$PWD/build\" MISE_TRUSTED_CONFIG_PATHS=\"$PWD/checkout\" MISE_AUTO_INSTALL=false; mkdir -p \"$HOME/.cargo\" && ln -s \"$DOTFILES_HOST_CARGO_HOME/bin\" \"$HOME/.cargo/bin\" && export CARGO_HOME=\"$HOME/.cargo\" MISE_CARGO_HOME=\"$HOME/.cargo\" XDG_CONFIG_HOME=\"$HOME/.config\" XDG_CACHE_HOME=\"$HOME/.cache\" XDG_DATA_HOME=\"$HOME/.local/share\" XDG_STATE_HOME=\"$HOME/.local/state\" MISE_CONFIG_DIR=\"$HOME/.config/mise\" MISE_CACHE_DIR=\"$HOME/.cache/mise\" MISE_STATE_DIR=\"$HOME/.local/state/mise\" GIT_CONFIG_GLOBAL=\"$HOME/empty.toml\" GIT_CONFIG_NOSYSTEM=1 && : > \"$GIT_CONFIG_GLOBAL\" && export MISE_GLOBAL_CONFIG_FILE=\"$GIT_CONFIG_GLOBAL\" MISE_SYSTEM_CONFIG_FILE=\"$GIT_CONFIG_GLOBAL\" && {UNIX_GIT} -c advice.detachedHead=false clone --quiet head.bundle checkout && cd checkout && {worker} --home \"$nativeHome\""
            )
        }
    }
}

fn worker(home: &Path, gate: &str) -> Result<()> {
    let code = match code(home, gate) {
        Ok(()) => 0,
        Err(error) => {
            eprintln!("code: {error:#}");
            1
        }
    };
    let real = match crate::host_checks::check(&std::env::current_dir()?, home) {
        Ok(()) => 0,
        Err(error) => {
            eprintln!("real: {error:#}");
            1
        }
    };
    println!(
        "{STAGE_REPORT}{}",
        serde_json::to_string(&Stages { code, real })?
    );
    ensure!(
        code == 0 && real == 0,
        "host stages failed: code={code}, real={real}"
    );
    Ok(())
}

fn code(home: &Path, gate: &str) -> Result<()> {
    let environment = code_environment(&std::env::current_dir()?, home)?;
    let argv = code_shell(gate);
    let status = environment
        .command(&argv[0])
        .args(&argv[1..])
        .stdin(Stdio::null())
        .status()?;
    ensure!(status.success(), "code stage failed with {status}");
    Ok(())
}

fn code_environment(
    root: &Path,
    home: &Path,
) -> Result<platform::code_environment::CodeEnvironment> {
    let mut installed = platform::code_environment::Installations::discover(home)?;
    let releases = model::data::parse(RELEASE_PINS, &fs::read_to_string(root.join(RELEASE_PINS))?)
        .map_err(anyhow::Error::msg)?;
    let pin = releases
        .mise
        .and_then(|release| release.version)
        .context("mise has no release pin")?;
    let version = platform::process::command(&installed.mise)
        .arg("--version")
        .output()?;
    ensure!(version.status.success(), "query the installed mise release");
    if std::str::from_utf8(&version.stdout)?
        .split_whitespace()
        .next()
        == Some(pin.as_str())
    {
        return platform::code_environment::CodeEnvironment::build(root, &installed);
    }
    let scope = tempfile::tempdir()?;
    let config = scope.path().join("chezmoi.toml");
    fs::write(
        &config,
        format!(
            "[data]\nprofile = {:?}\n",
            crate::profiles::native_profile()?.name()
        ),
    )?;
    let bootstrap = crate::host_checks::Scratch::new(&config)?;
    installed.mise = bootstrap
        .provision_mise(root)
        .with_context(|| format!("resolve mise release pin {pin}"))?;
    platform::code_environment::CodeEnvironment::build(root, &installed)
}

fn code_shell(gate: &str) -> Vec<String> {
    let shell: &[&str] = if cfg!(windows) {
        &["pwsh", "-NoLogo", "-NoProfile", "-Command"]
    } else {
        &[CODE_SHELL, "-c"]
    };
    shell
        .iter()
        .map(|word| (*word).to_owned())
        .chain([gate.to_owned()])
        .collect()
}

/// The exit status domyjob reports for a finished job.
/// Returns `None` when the job didn't finish or failed to launch.
#[must_use]
pub fn outcome(alias: &str, output: &str) -> Option<i32> {
    let prefix = format!("{alias}:");
    let line = output
        .lines()
        .find(|line| line.starts_with(&prefix) && line.contains(" finished "))?;
    let result = line.split_once(" finished ")?.1.trim();
    if result == "succeeded" {
        return Some(0);
    }
    result
        .strip_prefix("failed (")?
        .strip_suffix(')')?
        .parse()
        .ok()
}

fn tailnet_status(native: &Real) -> Result<Tailnet> {
    let output = command(native, Tool::Tailscale)
        .args(["status", "--json"])
        .stderr(Stdio::inherit())
        .output()
        .context("start tailscale; install it and sign in to the tailnet")?;
    ensure!(
        output.status.success(),
        "tailscale is not connected; run `tailscale up`"
    );
    tailnet(&output.stdout)
}

fn paths(home: &Path) -> (PathBuf, PathBuf) {
    let state = home.join(STATE);
    (state.join("ssh_config"), state.join("known_hosts"))
}

fn read_or_empty(path: &Path) -> Result<String> {
    match fs::read_to_string(path) {
        Ok(text) => Ok(text),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(String::new()),
        Err(error) => Err(error).with_context(|| format!("read {}", path.display())),
    }
}

fn fingerprint(native: &Real, key: &str) -> Result<String> {
    let file = tempfile::NamedTempFile::new()?;
    fs::write(file.path(), format!("{key}\n"))?;
    let output = command(native, Tool::SshKeygen)
        .arg("-lf")
        .arg(file.path())
        .output()?;
    ensure!(
        output.status.success(),
        "ssh-keygen cannot fingerprint the scanned key"
    );
    Ok(String::from_utf8(output.stdout)?
        .split_whitespace()
        .nth(1)
        .context("ssh-keygen printed no fingerprint")?
        .to_owned())
}

/// The ed25519 key the host presents to the SSH client that later connects, which `ssh-keyscan` can't always negotiate.
/// The scan offers no identity, so at most a host that admits anyone runs `true`.
fn scan(native: &Real, user: &str, address: &str) -> Result<Option<String>> {
    let directory = tempfile::tempdir()?;
    let file = directory.path().join("known_hosts");
    let known = file.display().to_string().replace('\\', "/");
    command(native, Tool::Ssh)
        .args([
            "-F",
            "none",
            "-o",
            "BatchMode=yes",
            "-o",
            "ConnectTimeout=10",
        ])
        .args([
            "-o",
            "StrictHostKeyChecking=accept-new",
            "-o",
            "HostKeyAlgorithms=ssh-ed25519",
        ])
        .args([
            "-o",
            "PreferredAuthentications=publickey",
            "-o",
            "IdentitiesOnly=yes",
        ])
        .args(["-o", "IdentityAgent=none", "-o", "IdentityFile=none"])
        .arg("-o")
        .arg(format!("UserKnownHostsFile={known}"))
        .arg("-o")
        .arg(format!("GlobalKnownHostsFile={known}"))
        .args(["-l", user, address, "true"])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .context("start ssh")?;
    Ok(scanned(&read_or_empty(&file)?))
}

pub(crate) fn sync(
    home: &Path,
    requested: &[String],
    user: Option<String>,
    accept: &[String],
) -> Result<()> {
    let native = native(home)?;
    let (config, known) = paths(home);
    let previous = managed(&read_or_empty(&config)?);
    let tailnet = tailnet_status(&native)?;
    let selected = select(&tailnet, &previous.aliases, requested)?;
    ensure!(
        !selected.is_empty(),
        "the tailnet has no other Linux, Mac, or Windows host"
    );
    let aliases: Vec<_> = selected.iter().map(|peer| peer.alias.clone()).collect();
    let remotes = git(Path::new("."), &["remote", "-v"]).unwrap_or_default();
    let user = user
        .or(previous.user)
        .or_else(|| hub_user(&String::from_utf8_lossy(&remotes), &aliases))
        .context("no hub remote names the remote user; pass --user")?;
    let pinned = pinned(&read_or_empty(&known)?);
    let mut lines = String::new();
    for peer in &selected {
        let key = scan(&native, &user, &peer.address)?.with_context(|| {
            format!(
                "{} presented no ed25519 host key over the tailnet; check its SSH server",
                peer.alias
            )
        })?;
        let state = match pinned.get(&peer.alias) {
            None => Key::New,
            Some(current) if *current == key => Key::Same,
            Some(_) => Key::Changed,
        };
        ensure!(
            pin(state, accept.contains(&peer.alias)),
            "{} presents a different host key {}; if expected, run `dotfiles hosts sync --accept {}`",
            peer.alias,
            fingerprint(&native, &key)?,
            peer.alias
        );
        writeln!(lines, "{},{},{} {key}", peer.alias, peer.dns, peer.address)?;
    }
    crate::runtime::replace(&known, lines.as_bytes(), false)?;
    crate::runtime::replace(&config, render(&aliases, &user, &known).as_bytes(), false)?;
    for peer in &selected {
        println!("{} {}: pinned", peer.family.name(), peer.alias);
    }
    Ok(())
}

/// Where each family's gate runs.
#[derive(Clone, Debug)]
enum Target {
    Local(String),
    Remote(String),
}

impl Target {
    fn name(&self) -> &str {
        match self {
            Self::Local(name) | Self::Remote(name) => name,
        }
    }
}

fn targets(home: &Path, tailnet: &Tailnet) -> Result<[Option<Target>; 3]> {
    let managed = managed(&read_or_empty(&paths(home).0)?);
    Ok(Family::ALL.map(|family| match &tailnet.this {
        Some((name, own)) if *own == family => Some(Target::Local(name.clone())),
        _ => managed
            .aliases
            .iter()
            .find(|alias| {
                tailnet
                    .peers
                    .iter()
                    .any(|peer| &peer.alias == *alias && peer.family == family)
            })
            .map(|alias| Target::Remote(alias.clone())),
    }))
}

fn domyjob(
    native: &Real,
    alias: &str,
    arguments: &[&str],
    program: &[String],
) -> Result<(Option<i32>, String)> {
    let output = command(native, Tool::Domyjob)
        .args(arguments)
        .arg(alias)
        .arg("--wait")
        .arg("--")
        .args(program)
        .stdin(Stdio::null())
        .output()
        .context("start domyjob")?;
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    Ok((outcome(alias, &text), text))
}

/// `Ok` when the host can run the gate, or the next action that would make it able to.
fn probe(
    native: &Real,
    tailnet: &Tailnet,
    family: Family,
    target: &Target,
    program: &str,
) -> Result<Result<(), String>> {
    let Target::Remote(alias) = target else {
        return Ok(if on_path(native, program) {
            Ok(())
        } else {
            Err(format!(
                "`{program}` is not on this machine's PATH; apply its dotfiles profile"
            ))
        });
    };
    let Some(peer) = tailnet.peers.iter().find(|peer| &peer.alias == alias) else {
        return Ok(Err(format!(
            "{alias} left the tailnet; run `dotfiles hosts sync`"
        )));
    };
    if !peer.online {
        return Ok(Err(format!(
            "{alias} is offline on the tailnet; wake it or start Tailscale on it"
        )));
    }
    let doctor = command(native, Tool::Domyjob)
        .args(["doctor", alias])
        .stdin(Stdio::null())
        .output()
        .context("start domyjob")?;
    if !doctor.status.success() {
        return Ok(Err(format!(
            "{alias} refused SSH or domyjob; run `ssh -o BatchMode=yes {alias} true`, approve it in 1Password, then `domyjob doctor {alias}`"
        )));
    }
    let (status, _) = domyjob(
        native,
        alias,
        &["on"],
        &wrap(family, &format!("{program} --version")),
    )?;
    Ok(if status == Some(0) {
        Ok(())
    } else {
        Err(format!(
            "{alias} has no `{program}` on its login PATH; apply its dotfiles profile"
        ))
    })
}

fn on_path(native: &Real, program: &str) -> bool {
    std::env::split_paths(&native.path).any(|directory| {
        directory.join(program).is_file()
            || (cfg!(windows)
                && ["exe", "cmd", "bat"]
                    .iter()
                    .any(|extension| directory.join(format!("{program}.{extension}")).is_file()))
    })
}

fn program(gate: &str) -> Result<&str> {
    gate.split_whitespace()
        .next()
        .context("the gate command is empty")
}

pub(crate) fn doctor(home: &Path, gate: &str) -> Result<()> {
    let native = native(home)?;
    let tailnet = tailnet_status(&native)?;
    let program = program(gate)?;
    let mut failures = 0;
    for (family, target) in Family::ALL.into_iter().zip(targets(home, &tailnet)?) {
        let Some(target) = target else {
            println!(
                "{}: no managed host; run `dotfiles hosts sync`",
                family.name()
            );
            failures += 1;
            continue;
        };
        match probe(&native, &tailnet, family, &target, program)? {
            Ok(()) => println!("{} {}: ready", family.name(), target.name()),
            Err(action) => {
                println!("{} {}: {action}", family.name(), target.name());
                failures += 1;
            }
        }
    }
    ensure!(failures == 0, "{failures} OS family host(s) are not ready");
    Ok(())
}

pub(crate) fn check(home: &Path, gate: &str) -> Result<()> {
    let root = PathBuf::from(git_text(Path::new("."), &["rev-parse", "--show-toplevel"])?);
    let clean = git(&root, &["status", "--porcelain", "--untracked-files=all"])?.is_empty();
    let commit = git_text(&root, &["rev-parse", "HEAD"])?;
    let tree = git_text(&root, &["rev-parse", "HEAD^{tree}"])?;
    let required = required(&workflows_at(&root, &commit)?)?;
    if required == [false; 3] {
        println!("The CI matrix requires no OS family; nothing to check");
        return Ok(());
    }
    let native = native(home)?;
    let mut ready = [true; 3];
    let mut actions: [Option<String>; 3] = Default::default();
    let mut hosts: [Option<Target>; 3] = Default::default();
    if clean {
        let tailnet = tailnet_status(&native)?;
        hosts = targets(home, &tailnet)?;
        let program = program(gate)?;
        for family in Family::ALL
            .into_iter()
            .filter(|family| required[family.index()])
        {
            let index = family.index();
            let verdict = match &hosts[index] {
                None => Err("no managed host; run `dotfiles hosts sync`".to_owned()),
                Some(target) => probe(&native, &tailnet, family, target, program)?,
            };
            if let Err(action) = verdict {
                ready[index] = false;
                actions[index] = Some(action);
            }
        }
    }
    match start(clean, required, ready) {
        Start::Dirty => bail!(
            "the tree has uncommitted or untracked files; notes bind to a commit, so commit first"
        ),
        Start::Unready(_) => bail!(
            "{}",
            Family::ALL
                .into_iter()
                .filter_map(|family| actions[family.index()]
                    .as_ref()
                    .map(|action| format!("{}: {action}", family.name())))
                .collect::<Vec<_>>()
                .join("\n")
        ),
        Start::Run => {}
    }
    let snapshot = tempfile::tempdir()?;
    let bundle = snapshot.path().join("head.bundle");
    git(
        &root,
        &["bundle", "create", &bundle.to_string_lossy(), "HEAD"],
    )?;
    let runs: Vec<_> = std::thread::scope(|scope| {
        let handles: Vec<_> = Family::ALL
            .into_iter()
            .filter(|family| required[family.index()])
            .filter_map(|family| hosts[family.index()].clone().map(|target| (family, target)))
            .map(|(family, target)| {
                let (native, root, snapshot) = (&native, &root, snapshot.path());
                let thread = scope.spawn(move || {
                    run_gate(native, root, snapshot, family, &target, gate).map(
                        |(status, output)| GateRun {
                            family,
                            target,
                            status,
                            output,
                        },
                    )
                });
                (family, thread)
            })
            .collect();
        handles
            .into_iter()
            .map(|(family, handle)| {
                handle.join().unwrap_or_else(|_| {
                    Err(anyhow::anyhow!(
                        "the {} gate thread panicked",
                        family.name()
                    ))
                })
            })
            .collect()
    });
    let mut failed = Vec::new();
    for run in runs {
        match run.and_then(|run| record_run(&root, &commit, &tree, gate, &run)) {
            Ok(failures) => failed.extend(failures),
            Err(error) => failed.push(format!("host gate: {error:#}")),
        }
    }
    ensure!(failed.is_empty(), "{}", failed.join("\n"));
    Ok(())
}

/// One family's gate as it ran on its host: the exit status when the job finished, and its combined output.
struct GateRun {
    family: Family,
    target: Target,
    status: Option<i32>,
    output: String,
}

/// Appends the note of one finished gate to the commit, returning the failure to report when the gate didn't pass.
fn record_run(
    root: &Path,
    commit: &str,
    tree: &str,
    gate: &str,
    run: &GateRun,
) -> Result<Vec<String>> {
    let GateRun {
        family,
        target,
        status,
        output,
    } = run;
    let Some(status) = *status else {
        eprintln!("{output}");
        return Ok(vec![format!(
            "{} {}: the job did not finish; inspect it with `domyjob ls {}` before rerunning",
            family.name(),
            target.name(),
            target.name()
        )]);
    };
    let reports: Vec<_> = output
        .lines()
        .filter_map(|line| line.strip_prefix(STAGE_REPORT))
        .collect();
    let [report] = reports.as_slice() else {
        eprintln!("{output}");
        bail!(
            "{} {}: the job returned {status} without exactly one complete stage report",
            family.name(),
            target.name()
        );
    };
    let report: Stages = serde_json::from_str(report).context("malformed host stage report")?;
    let statuses = [report.code, report.real];
    ensure!(
        (status == 0) == statuses.iter().all(|status| *status == 0),
        "the job status disagrees with its stage report"
    );
    let mut failures = Vec::new();
    for (stage, status) in Stage::ALL.into_iter().zip(statuses) {
        let note = serde_json::to_string(&Note {
            host: target.name().to_owned(),
            os: family.name().to_owned(),
            tree: tree.to_owned(),
            status,
            gate: gate.to_owned(),
            stage: Some(stage),
        })?;
        if let Err(error) = git(
            root,
            &["notes", "--ref", NOTES, "append", "-m", &note, commit],
        ) {
            failures.push(format!(
                "{} {} {}: record note: {error:#}",
                family.name(),
                target.name(),
                stage.name()
            ));
        }
        if status == 0 {
            println!(
                "{} {} {}: passed",
                family.name(),
                target.name(),
                stage.name()
            );
        } else {
            failures.push(format!(
                "{} {} {}: failed with {status}",
                family.name(),
                target.name(),
                stage.name()
            ));
        }
    }
    if !failures.is_empty() {
        eprintln!("{output}");
    }
    Ok(failures)
}

/// One family's gate on its host, with its exit status when the job finished and its combined output.
fn run_gate(
    native: &Real,
    root: &Path,
    snapshot: &Path,
    family: Family,
    target: &Target,
    gate: &str,
) -> Result<(Option<i32>, String)> {
    let combined = |output: &std::process::Output| {
        format!(
            "{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        )
    };
    match target {
        Target::Remote(alias) => {
            let output = command(native, Tool::Domyjob)
                .current_dir(snapshot)
                .args(["run", alias, "--wait", "--"])
                .args(wrap(family, &remote_gate(family, gate)))
                .stdin(Stdio::null())
                .output()
                .context("start domyjob")?;
            let text = combined(&output);
            Ok((outcome(alias, &text), text))
        }
        Target::Local(_) => {
            let output = crate::host_checks::unattended(std::env::current_exe()?)
                .args(["hosts", "worker", "--gate", gate])
                .current_dir(root)
                .stdin(Stdio::null())
                .output()
                .context("start the local gate")?;
            Ok((output.status.code(), combined(&output)))
        }
    }
}
