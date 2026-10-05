//! Cross-platform push evidence: managed SSH access to one host per OS family, gate runs through domyjob, and Git notes that pre-push and `pr-workflow ready` require.

use crate::hosts_rules::{Evidence, Family, Key, Start, Verdict, admit, gated, pin, record, start};
use crate::runtime::Native;
use crate::tool::Tool;
use anyhow::{Context, Result, bail, ensure};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use yaml_rust2::{Yaml, YamlLoader};

pub const NOTES: &str = "refs/notes/hosts";
const STATE: &str = ".local/state/hosts";
const DEFAULT_GATE: &str = "just check";

#[derive(clap::Subcommand)]
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
        /// Remote user; defaults to the managed one, then to a hub remote's.
        #[arg(long)]
        user: Option<String>,
        /// Replace this host's pinned key with the one it presents now.
        #[arg(long)]
        accept: Vec<String>,
    },
    /// Run the gate for the clean HEAD on every OS family in the CI matrix and record each result as a Git note.
    Check {
        #[arg(long, default_value = DEFAULT_GATE)]
        gate: String,
    },
    /// Refuse pushed GitHub branches that lack a passing note for an OS family in their CI matrix; reads pre-push input.
    Gate { remote: String, url: String },
}

pub fn run(action: Action) -> Result<()> {
    match action {
        Action::Doctor { gate } => doctor(&home()?, &gate),
        Action::Sync { host, user, accept } => sync(&home()?, &host, user, &accept),
        Action::Check { gate } => check(&home()?, &gate),
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

/// The families the workflows' jobs run on, by family index; reusable-workflow calls have no runner of their own.
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
}

/// Judge one commit's note, one JSON record per line in append order.
pub fn evidence(notes: &str, tree: &str) -> Result<[Evidence; 3]> {
    let mut evidence = [Evidence::Missing; 3];
    for line in notes.lines().filter(|line| !line.trim().is_empty()) {
        let note: Note = serde_json::from_str(line).context("malformed hosts note")?;
        let family = Family::ALL
            .into_iter()
            .find(|family| family.name() == note.os)
            .with_context(|| format!("hosts note names an unknown OS family: {}", note.os))?;
        let index = family.index();
        evidence[index] = record(evidence[index], note.tree == tree, note.status == 0);
    }
    Ok(evidence)
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
        if path.ends_with(".yml") || path.ends_with(".yaml") {
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
pub fn admission(repository: &Path, commit: &str) -> Result<Verdict> {
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

pub fn require_admission(repository: &Path, commit: &str) -> Result<()> {
    match admission(repository, commit)? {
        Verdict::Admit => Ok(()),
        Verdict::Missing(family) => bail!(
            "no {} gate result for {commit}; run `dotfiles-xtask hosts check` from a clean checkout of it",
            family.name()
        ),
        Verdict::Failed(family) => bail!(
            "the {} gate failed for {commit}; fix it, commit, and run `dotfiles-xtask hosts check` again",
            family.name()
        ),
    }
}

fn github(url: &str) -> bool {
    url.contains("github.com:") || url.contains("github.com/")
}

/// Judge pre-push input: `<local ref> <local sha> <remote ref> <remote sha>` per line.
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

fn native(home: &Path) -> Result<Native> {
    let mut native = Native::new(home)?;
    if cfg!(windows) {
        // Only the system OpenSSH reaches the 1Password agent pipe.
        let root = std::env::var_os("SystemRoot").unwrap_or_else(|| "C:\\Windows".into());
        let mut paths = vec![Path::new(&root).join("System32/OpenSSH")];
        paths.extend(std::env::split_paths(&native.path));
        native.path = std::env::join_paths(paths)?;
    }
    Ok(native)
}

fn command(native: &Native, tool: Tool) -> Command {
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

/// Peers of a known OS family from `tailscale status --json`, named by their MagicDNS label.
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

pub fn render(aliases: &[String], user: &str, known_hosts: &Path) -> String {
    format!(
        "# Generated by `dotfiles-xtask hosts sync` from the tailnet; host keys are pinned over the tailnet.\nHost {}\n  User {user}\n  UserKnownHostsFile {}\n  StrictHostKeyChecking yes\n  ForwardAgent no\n",
        aliases.join(" "),
        known_hosts.display().to_string().replace('\\', "/")
    )
}

/// One host per OS family other than this machine's: a requested one, then the managed one, then the only online one.
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
pub fn hub_user(remotes: &str, aliases: &[String]) -> Option<String> {
    remotes.split_whitespace().find_map(|url| {
        let rest = url.strip_prefix("ssh://").unwrap_or(url);
        let (user, host) = rest.split_once('@')?;
        let host = host.split([':', '/']).next()?;
        (!user.contains('/') && aliases.iter().any(|alias| alias == host)).then(|| user.to_owned())
    })
}

/// The gate as the family's login shell runs it, so a job's minimal environment still finds the profile's tools.
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
pub fn remote_gate(family: Family, gate: &str) -> String {
    const CLONE: &str = "git -c advice.detachedHead=false clone --quiet head.bundle checkout";
    match family {
        Family::Windows => format!(
            "{CLONE}; if (-not $?) {{ exit 1 }}; Set-Location checkout; $env:MISE_TRUSTED_CONFIG_PATHS = $PWD.Path; {gate}; exit $LASTEXITCODE"
        ),
        Family::Linux | Family::Mac => {
            format!("{CLONE} && cd checkout && export MISE_TRUSTED_CONFIG_PATHS=\"$PWD\" && {gate}")
        }
    }
}

/// The exit status domyjob reports for a finished job; `None` when the job did not finish or failed to launch.
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

fn tailnet_status(native: &Native) -> Result<Tailnet> {
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

fn fingerprint(native: &Native, key: &str) -> Result<String> {
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

/// The ed25519 key the host presents to the SSH client that later connects, which `ssh-keyscan` cannot always negotiate; no identity is offered, so at most a host that admits anyone runs `true`.
fn scan(native: &Native, user: &str, address: &str) -> Result<Option<String>> {
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

pub fn sync(
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
            "{} presents a different host key {}; if expected, run `dotfiles-xtask hosts sync --accept {}`",
            peer.alias,
            fingerprint(&native, &key)?,
            peer.alias
        );
        lines.push_str(&format!(
            "{},{},{} {key}\n",
            peer.alias, peer.dns, peer.address
        ));
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
    native: &Native,
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
    native: &Native,
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
            "{alias} left the tailnet; run `dotfiles-xtask hosts sync`"
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

fn on_path(native: &Native, program: &str) -> bool {
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

pub fn doctor(home: &Path, gate: &str) -> Result<()> {
    let native = native(home)?;
    let tailnet = tailnet_status(&native)?;
    let program = program(gate)?;
    let mut failures = 0;
    for (family, target) in Family::ALL.into_iter().zip(targets(home, &tailnet)?) {
        let Some(target) = target else {
            println!(
                "{}: no managed host; run `dotfiles-xtask hosts sync`",
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

pub fn check(home: &Path, gate: &str) -> Result<()> {
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
                None => Err("no managed host; run `dotfiles-xtask hosts sync`".to_owned()),
                Some(target) => probe(&native, &tailnet, family, target, program)?,
            };
            if let Err(action) = verdict {
                ready[index] = false;
                actions[index] = Some(action);
                break;
            }
        }
    }
    match start(clean, required, ready) {
        Start::Dirty => bail!(
            "the tree has uncommitted or untracked files; notes bind to a commit, so commit first"
        ),
        Start::Unready(family) => bail!(
            "{}: {}",
            family.name(),
            actions[family.index()].as_deref().unwrap_or("not ready")
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
                scope.spawn(move || -> Result<(Family, Target, Option<i32>, String)> {
                    let argv = wrap(family, gate);
                    let (status, output) = match &target {
                        Target::Remote(alias) => {
                            let mut command = command(native, Tool::Domyjob);
                            command.current_dir(snapshot);
                            let output = command
                                .args(["run", alias, "--wait", "--"])
                                .args(wrap(family, &remote_gate(family, gate)))
                                .stdin(Stdio::null())
                                .output()
                                .context("start domyjob")?;
                            let text = format!(
                                "{}{}",
                                String::from_utf8_lossy(&output.stdout),
                                String::from_utf8_lossy(&output.stderr)
                            );
                            (outcome(alias, &text), text)
                        }
                        Target::Local(_) => {
                            let output = crate::tool::external(&argv[0])
                                .args(&argv[1..])
                                .current_dir(root)
                                .stdin(Stdio::null())
                                .output()
                                .context("start the local gate")?;
                            (
                                output.status.code(),
                                format!(
                                    "{}{}",
                                    String::from_utf8_lossy(&output.stdout),
                                    String::from_utf8_lossy(&output.stderr)
                                ),
                            )
                        }
                    };
                    Ok((family, target, status, output))
                })
            })
            .collect();
        handles
            .into_iter()
            .map(|handle| handle.join().expect("gate thread panicked"))
            .collect()
    });
    let mut failed = Vec::new();
    for run in runs {
        let (family, target, status, output) = run?;
        let Some(status) = status else {
            eprintln!("{output}");
            failed.push(format!(
                "{} {}: the job did not finish; inspect it with `domyjob ls {}` before rerunning",
                family.name(),
                target.name(),
                target.name()
            ));
            continue;
        };
        let note = serde_json::to_string(&Note {
            host: target.name().to_owned(),
            os: family.name().to_owned(),
            tree: tree.clone(),
            status,
            gate: gate.to_owned(),
        })?;
        git(
            &root,
            &["notes", "--ref", NOTES, "append", "-m", &note, &commit],
        )?;
        if status == 0 {
            println!("{} {}: passed", family.name(), target.name());
        } else {
            eprintln!("{output}");
            failed.push(format!(
                "{} {}: failed with {status}",
                family.name(),
                target.name()
            ));
        }
    }
    ensure!(failed.is_empty(), "{}", failed.join("\n"));
    Ok(())
}
