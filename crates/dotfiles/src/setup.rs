use crate::profile_rules::{Mode, Profile, permitted};
use crate::runtime::{Real, Runner, args, checked, optional, replace};
use crate::tool::Tool;
use anyhow::{Context, Result, ensure};
use serde_json::Value;
use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};

pub use platform::step::Step;

#[derive(Debug)]
pub struct TemplateData {
    pub source: PathBuf,
    pub home: PathBuf,
    pub profile: Profile,
    pub data: Value,
}

impl TemplateData {
    /// The JSON pointer to `pointer` under this profile's `platforms` entry.
    #[must_use]
    pub fn platform(&self, pointer: &str) -> String {
        format!("/platforms/{}{pointer}", self.profile.name())
    }
}

pub(crate) fn load(source: &Path, config: &Path, home: &Path) -> Result<TemplateData> {
    let scope = tempfile::tempdir()?;
    let output = crate::profiles::chezmoi(source, scope.path(), config, home)
        .args(["data", "--format", "json"])
        .output()?;
    ensure!(
        output.status.success(),
        "machine-local profile could not be read"
    );
    let data: Value = serde_json::from_slice(&output.stdout)?;
    let profile = crate::profiles::configured_profile(&data)?;
    Ok(TemplateData {
        source: source.to_path_buf(),
        home: home.to_path_buf(),
        profile,
        data,
    })
}

/// Runs one setup `step` on this host with the machine-local `config`.
///
/// # Errors
///
/// Returns an error when `config` names a relative path or lies inside `source`, or when loading the home or configuration fails.
/// Returns an error when the caller leaves `live` unset or the configured profile belongs to another host.
/// Returns an error when the step fails.
pub fn run(source: &Path, config: &Path, step: Step, live: bool) -> Result<()> {
    ensure!(
        config.is_absolute() && !crate::canonical(config)?.starts_with(source),
        "setup requires machine-local config outside the public source"
    );
    let home = std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" })
        .context("native home is unavailable")?;
    let context = load(source, config, Path::new(&home))?;
    ensure!(
        permitted(
            Mode::Full,
            context.profile == crate::profiles::native_profile()?,
            false,
            live
        ),
        "setup requires --live and this host's machine-local native profile"
    );
    let mut native = Real::new(&context.home)?;
    native.directory = Some(context.home.clone());
    native
        .environment
        .push(("KERL_BUILD_DOCS".into(), "no".into()));
    let mut kerl = "--without-javac --without-wx --without-odbc".to_owned();
    if context.profile == Profile::Mac
        && native.available(Tool::Brew)
        && let Ok(prefix) = checked(&mut native, Tool::Brew, &args(&["--prefix", "openssl@3"]))
    {
        let prefix = String::from_utf8(prefix)?;
        if Path::new(prefix.trim()).is_dir() {
            kerl.push_str(" --with-ssl=");
            kerl.push_str(prefix.trim());
        }
    }
    native
        .environment
        .push(("KERL_CONFIGURE_OPTIONS".into(), kerl.into()));
    if let Some(jobs) = context
        .data
        .pointer(&context.platform("/mise/jobs"))
        .and_then(Value::as_u64)
    {
        native
            .environment
            .push(("MISE_JOBS".into(), jobs.to_string().into()));
    }
    execute(&context, &mut native, step)
}

/// Packaged tools the setup steps of the rendered scripts require.
#[must_use]
pub fn scripted_requirements(profile: Profile, scripts: &str) -> Vec<Tool> {
    let mut tools: Vec<Tool> = scripted_steps(scripts)
        .into_iter()
        .flat_map(|step| step.requires(profile).iter().copied())
        .collect();
    tools.sort();
    tools.dedup();
    tools
}

/// Packaged tools that installed entry points run, read from rendered files.
/// An agent launcher reads the secrets the machine-local configuration selects through Doppler, so it needs Doppler wherever it renders.
/// A global Git hook runs the Lefthook configuration of the repository it fires in, so it needs Lefthook wherever it renders.
#[must_use]
pub fn entry_requirements(rendered: &str) -> Vec<Tool> {
    let mut tools = Vec::new();
    for words in rendered
        .lines()
        .map(|line| line.split_whitespace().collect::<Vec<_>>())
    {
        let launcher = words.iter().position(|word| {
            let program = word.trim_matches(['\'', '"', '^']);
            let name = program.rsplit(['/', '\\']).next().unwrap_or(program);
            name == "dotfiles" || name == "dotfiles.exe"
        });
        if let Some(start) = launcher {
            if words[start..].contains(&"agent") {
                tools.push(Tool::Doppler);
            }
            if words[start..].contains(&"hook") {
                tools.push(Tool::Lefthook);
            }
        }
    }
    tools.sort();
    tools.dedup();
    tools
}

/// A way a run order of the setup steps breaks their declarations.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Violation {
    /// The step runs before a prerequisite that the profile runs.
    Early(Step, Step),
    /// The profile runs the prerequisite of a step, and the order leaves it out.
    Missing(Step, Step),
    /// The order runs the step more than once.
    Repeated(Step),
    /// A step that runs before the files comes after one that runs after them.
    Phase(Step, Step),
}

/// Every way `sequence` breaks the declarations on `profile`, checked at every occurrence of each step.
/// The order must run each step once, after every prerequisite the profile runs, and every before-phase step before every after-phase step.
#[must_use]
pub fn ordering_violations(profile: Profile, sequence: &[Step]) -> Vec<Violation> {
    let mut violations = Vec::new();
    for (index, step) in sequence.iter().enumerate() {
        if sequence.iter().take(index).any(|earlier| earlier == step) {
            violations.push(Violation::Repeated(*step));
        }
        for prerequisite in step.prerequisites() {
            match sequence
                .iter()
                .position(|candidate| *candidate == prerequisite)
            {
                Some(first) if first > index => {
                    violations.push(Violation::Early(*step, prerequisite));
                }
                None if prerequisite.runs_on(profile) => {
                    violations.push(Violation::Missing(*step, prerequisite));
                }
                _ => {}
            }
        }
        if step.phase() == platform::step::Phase::Before
            && let Some(after) = sequence
                .iter()
                .take(index)
                .find(|earlier| earlier.phase() == platform::step::Phase::After)
        {
            violations.push(Violation::Phase(*step, *after));
        }
    }
    violations
}

/// Setup steps a rendered script runs, in script order, from `dotfiles ... setup STEP` invocations.
#[must_use]
pub fn scripted_steps(scripts: &str) -> Vec<Step> {
    use clap::ValueEnum;
    let mut steps = Vec::new();
    for words in scripts
        .lines()
        .map(|line| line.split_whitespace().collect::<Vec<_>>())
    {
        for (word, next) in words.iter().zip(words.iter().skip(1)) {
            if *word == "setup"
                && let Ok(step) = Step::from_str(next, false)
                && !steps.contains(&step)
            {
                steps.push(step);
            }
        }
    }
    steps
}

/// Requirements checked for every scripted step before apply changes anything.
pub(crate) fn preflight(
    context: &TemplateData,
    runner: &mut impl Runner,
    steps: &[Step],
) -> Result<()> {
    let mut unmet = Vec::new();
    for &step in steps {
        if step == Step::Domyjob && native_domyjob(runner)? {
            continue;
        }
        if let Some(source) = owner_source(step)
            && !owner_checkout(context, source)?.join(".git").exists()
        {
            let mut arguments = args(&[
                "-c",
                "core.sshCommand=ssh -o BatchMode=yes",
                "ls-remote",
                "--heads",
            ]);
            arguments.push(source.clone_url().into());
            if !runner.run(Tool::Git, &arguments)?.success {
                unmet.push(format!(
                    "{step:?} clones {} with the owner's GitHub credentials, which this session cannot use non-interactively",
                    source.repository()
                ));
            }
        }
    }
    ensure!(
        unmet.is_empty(),
        "setup cannot complete on this host; run from a session that satisfies it or name the script in setup.ignore:\n{}",
        unmet.join("\n")
    );
    Ok(())
}

fn owner_source(step: Step) -> Option<OwnerSource> {
    match step {
        Step::Ocomment => Some(OwnerSource::Ocomment),
        Step::Domyjob => Some(OwnerSource::Domyjob),
        Step::Fleet => Some(OwnerSource::Fleet),
        _ => None,
    }
}

fn owner_checkout(context: &TemplateData, source: OwnerSource) -> Result<PathBuf> {
    let projects = string(&context.data, "/paths/projects")?;
    ensure!(
        !projects.is_empty(),
        "machine-local project root is missing"
    );
    Ok(Path::new(&projects)
        .join("github.com/P4suta")
        .join(source.repository()))
}

fn string(data: &Value, pointer: &str) -> Result<String> {
    Ok(data
        .pointer(pointer)
        .and_then(Value::as_str)
        .with_context(|| format!("configuration requires {pointer}"))?
        .to_owned())
}

fn strings(data: &Value, pointer: &str) -> Result<Vec<String>> {
    data.pointer(pointer)
        .and_then(Value::as_array)
        .with_context(|| format!("configuration requires {pointer}"))?
        .iter()
        .map(|value| {
            value
                .as_str()
                .context("configuration list requires strings")
                .map(str::to_owned)
        })
        .collect()
}

/// `--force` replaces binaries another package or an earlier source path installed, so cutover and reapplication converge on this source.
pub(crate) fn cargo_install_arguments(
    path: &Path,
    root: &Path,
    binaries: &[&str],
) -> Vec<OsString> {
    let mut arguments = args(&["install", "--locked", "--force", "--path"]);
    arguments.push(crate::without_verbatim_prefix(path).into());
    arguments.push("--root".into());
    arguments.push(crate::without_verbatim_prefix(root).into());
    for binary in binaries {
        arguments.extend(args(&["--bin", binary]));
    }
    arguments
}

/// Windows has no `exec` for a launcher script, so a copy of dotguard named `git.exe` serves as the Git wrapper, and the PowerShell profile places it ahead of Git for Windows.
pub(crate) fn git_wrapper(bin: &Path) -> Result<()> {
    let wrapper = bin.join("git.exe");
    let wanted = fs::read(bin.join("dotguard.exe")).context("dotguard.exe is not installed")?;
    if fs::read(&wrapper).is_ok_and(|current| current == wanted) {
        return Ok(());
    }
    let aside = crate::aside::move_aside(&wrapper)?;
    if let Err(error) = replace(&wrapper, &wanted, true) {
        crate::aside::restore(&wrapper, aside);
        return Err(error);
    }
    Ok(())
}

/// The Git guard, which every Git command starts.
pub const GUARD_CRATE: &str = "crates/dotguard";
/// The installed runtime behind every hook and launcher.
pub const RUNTIME_CRATE: &str = "crates/dotfiles";
/// The workspace profile that builds the guard small, with link-time optimization, which Cargo offers only for a whole profile.
pub const GUARD_PROFILE: &str = "release-guard";

const CLAUDE_INSTALLER: &str = "https://claude.ai/install.sh";
const ONEPASSWORD_APP: &str = "/Applications/1Password.app";
const OWNER_SSH: &str = "git@github.com:P4suta";

/// `cargo install` overwrites each binary in place, which Windows refuses while that binary runs, so each one moves aside first.
fn install_crate(
    context: &TemplateData,
    runner: &mut impl Runner,
    relative: &str,
    binaries: &[&str],
) -> Result<()> {
    let root = context.home.join(".local");
    let mut moved = Vec::new();
    for binary in binaries {
        let target = root
            .join("bin")
            .join(format!("{binary}{}", std::env::consts::EXE_SUFFIX));
        let aside = crate::aside::move_aside(&target)?;
        moved.push((target, aside));
    }
    // mise runs Cargo with the toolchain the checkout pins, so no host Cargo decides the build.
    let mut arguments = vec![
        "-C".into(),
        crate::without_verbatim_prefix(&context.source).into(),
        "x".into(),
        "--".into(),
        "cargo".into(),
    ];
    arguments.extend(cargo_install_arguments(
        &context.source.join(relative),
        &root,
        binaries,
    ));
    if relative == GUARD_CRATE {
        arguments.extend(args(&["--profile", GUARD_PROFILE]));
    }
    let result = checked(runner, Tool::Mise, &arguments);
    if result.is_err() {
        for (target, aside) in moved {
            crate::aside::restore(&target, aside);
        }
    }
    result?;
    Ok(())
}

pub(crate) fn vendor_installer(
    runner: &mut impl Runner,
    url: &str,
    arguments: &[&str],
) -> Result<()> {
    let scope = tempfile::tempdir()?;
    let script = scope.path().join("vendor-installer.sh");
    let mut download = args(&[
        "--proto",
        "=https",
        "--tlsv1.2",
        "--fail",
        "--silent",
        "--show-error",
        "--location",
        "--max-time",
        "300",
        "--output",
    ]);
    download.push(script.clone().into());
    download.push(url.into());
    checked(runner, Tool::Curl, &download)?;
    let mut invocation = vec![script.into()];
    invocation.extend(args(arguments));
    checked(runner, Tool::Sh, &invocation)?;
    Ok(())
}

pub(crate) fn execute(context: &TemplateData, runner: &mut impl Runner, step: Step) -> Result<()> {
    match step {
        Step::Runtime => runtime(context, runner)?,
        Step::Adapters => {
            crate::line_endings::install(&context.home)?;
            let binary = context.home.join(format!(
                ".local/bin/skill-ops{}",
                std::env::consts::EXE_SUFFIX
            ));
            skill_ops::skill_install::install(
                &context.source,
                &binary,
                &context.home,
                context.profile,
            )?;
            prose::remove_reply_checks(&context.home)?;
        }
        Step::Mise => mise(runner)?,
        Step::Github => github(runner)?,
        Step::Nix => nix(context, runner)?,
        Step::Tools | Step::Upgrade => tools(context, runner, matches!(step, Step::Upgrade))?,
        Step::Integrations => integrations(context, runner)?,
        Step::Chat => chat(context, runner)?,
        Step::Desktop => crate::desktop::refresh(context, runner)?,
        Step::Ocaml => ocaml(context, runner)?,
        Step::Racket => {
            let mut arguments = args(&[
                "pkg",
                "install",
                "--skip-installed",
                "--batch",
                "--auto",
                "--no-docs",
            ]);
            arguments.extend(
                strings(&context.data, &context.platform("/racket/collections"))?
                    .into_iter()
                    .map(Into::into),
            );
            checked(runner, Tool::Raco, &arguments)?;
        }
        Step::Haskell => haskell(context, runner)?,
        Step::Guard => {
            install_crate(
                context,
                runner,
                GUARD_CRATE,
                &[
                    "dotguard",
                    dotguard::RENOVATE_BINARY,
                    "reaper",
                    "herdr-agent",
                ],
            )?;
            let reaper = context.home.join(".local/bin/reaper");
            for name in ["dev-reaper", "session-reaper"] {
                replace(
                    &context.home.join(".local/bin").join(name),
                    &fs::read(&reaper)?,
                    true,
                )?;
            }
        }
        #[cfg(windows)]
        Step::Ocomment if context.profile == Profile::Windows => {
            crate::windows::steps::run(context, step)?;
        }
        Step::Ocomment | Step::Domyjob | Step::Fleet => source_tool(context, runner, step)?,
        Step::MacosDefaults => macos_defaults(context, runner)?,
        Step::ForgeKeys => forge_keys(runner)?,
        Step::Claude => {
            if !runner.available(Tool::Claude) {
                vendor_installer(runner, CLAUDE_INSTALLER, &[])?;
            }
        }
        Step::Fcitx5 => fcitx(context, runner)?,
        Step::RetireTmux => retire_tmux(context, runner)?,
        #[cfg(windows)]
        Step::Wezterm => crate::windows::wezterm::wezterm(&context.home, runner)?,
        #[cfg(windows)]
        Step::Scoop
        | Step::Winget
        | Step::PythonStub
        | Step::Herdr
        | Step::Sshd
        | Step::Keyboard
        | Step::StorageScout => crate::windows::steps::run(context, step)?,
        #[cfg(not(windows))]
        Step::Wezterm
        | Step::Scoop
        | Step::Winget
        | Step::PythonStub
        | Step::Herdr
        | Step::Sshd
        | Step::Keyboard
        | Step::StorageScout => anyhow::bail!("the {} step runs only on Windows", step.name()),
    }
    Ok(())
}

/// The installed runtime, the Git guard, and on the Mac the `CodeRabbit` guard, all built from this checkout.
fn runtime(context: &TemplateData, runner: &mut impl Runner) -> Result<()> {
    install_crate(context, runner, RUNTIME_CRATE, &["dotfiles"])?;
    install_crate(context, runner, "crates/skill-ops", &["skill-ops"])?;
    install_crate(context, runner, "crates/pr-workflow", &["pr-workflow"])?;
    if context.profile == Profile::Windows {
        install_crate(
            context,
            runner,
            GUARD_CRATE,
            &["dotguard", dotguard::RENOVATE_BINARY],
        )?;
        git_wrapper(&context.home.join(".local/bin"))?;
    } else {
        install_crate(
            context,
            runner,
            GUARD_CRATE,
            &[
                "dotguard",
                dotguard::RENOVATE_BINARY,
                "reaper",
                "herdr-agent",
            ],
        )?;
    }
    if context.profile == Profile::Mac {
        install_crate(context, runner, "crates/coderabbit-guard", &["coderabbit"])?;
        coderabbit_guard::install(&context.home.join(".local/bin/coderabbit"), &context.home)?;
    }
    Ok(())
}

/// The mise launcher installs the pinned release and verifies its checksum before the runtime exists, so the step only confirms it.
fn mise(runner: &mut impl Runner) -> Result<()> {
    ensure!(
        runner.available(Tool::Mise),
        "mise is missing; the mise launcher installs the pinned release, so apply again"
    );
    Ok(())
}

fn github(runner: &mut impl Runner) -> Result<()> {
    if !runner.available(Tool::Gh) {
        checked(runner, Tool::Mise, &args(&["install", "gh@latest", "-y"]))?;
        checked(runner, Tool::Mise, &args(&["reshim"]))?;
    }
    let reply = runner.run(Tool::Gh, &args(&["auth", "status"]))?;
    if reply.success {
        checked(
            runner,
            Tool::Gh,
            &args(&["config", "set", "-h", "github.com", "git_protocol", "ssh"]),
        )?;
    } else {
        eprintln!(
            "Warning: GitHub authentication is required; run gh auth login before installing tools."
        );
    }
    Ok(())
}

fn nix(context: &TemplateData, runner: &mut impl Runner) -> Result<()> {
    let nix = Tool::Nix;
    if !runner.available(nix) {
        return Ok(());
    }
    let flake = context.home.join(".config/dotfiles-nix");
    let listed = checked(runner, nix, &args(&["profile", "list"]))?;
    if String::from_utf8(listed)?.contains(flake.to_str().context("Nix profile path is not UTF-8")?)
    {
        checked(runner, nix, &args(&["profile", "upgrade", "--all"]))?;
    } else {
        checked(runner, nix, &["profile".into(), "add".into(), flake.into()])?;
    }
    Ok(())
}

fn integrations(context: &TemplateData, runner: &mut impl Runner) -> Result<()> {
    if context.profile == Profile::Windows {
        #[cfg(windows)]
        crate::windows::steps::run(context, Step::Integrations)?;
        crate::integrations::agent_integrations(runner, true);
        return Ok(());
    }
    let remote = ["SSH_CONNECTION", "SSH_CLIENT", "SSH_TTY"]
        .iter()
        .any(|name| std::env::var_os(name).is_some_and(|value| !value.is_empty()));
    let gui = !remote
        && match context.profile {
            Profile::Mac => Path::new(ONEPASSWORD_APP).is_dir(),
            Profile::Linux => runner.available(Tool::OnePassword),
            _ => false,
        };
    crate::integrations::integrations(&context.home, runner, gui)
}

fn chat(context: &TemplateData, runner: &mut impl Runner) -> Result<()> {
    let next = "set [data.domyjob.chat] peers = ['SSH_ALIAS', ...] in this host's machine-local chezmoi config and apply again; while unavailable, declare chat.sh or chat.ps1 in setup.ignore with a reason";
    let configuration: model::data::Domyjob = serde_json::from_value(
        context
            .data
            .get("domyjob")
            .cloned()
            .with_context(|| format!("chat setup is incomplete: {next}"))?,
    )
    .with_context(|| format!("invalid domyjob chat configuration: {next}"))?;
    let peers = configuration.chat.peers;
    ensure!(!peers.is_empty(), "chat setup is incomplete: {next}");
    for (index, peer) in peers.iter().enumerate() {
        ensure!(
            peer.bytes()
                .next()
                .is_some_and(|byte| byte.is_ascii_alphanumeric())
                && peer
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
                && !peers[..index].contains(peer),
            "chat peers require distinct literal SSH aliases; {next}"
        );
    }
    let mut setup = args(&["chat", "--json", "setup"]);
    setup.extend(peers.into_iter().map(Into::into));
    let configured = runner.run(Tool::Domyjob, &setup)?;
    ensure!(
        configured.success,
        "domyjob chat setup is incomplete; run `domyjob chat doctor` and apply again:\n{}",
        String::from_utf8_lossy(&configured.bytes)
    );
    let diagnosis = runner.run(Tool::Domyjob, &args(&["chat", "doctor"]))?;
    ensure!(
        diagnosis.success,
        "domyjob chat remains incomplete; follow these findings and apply again:\n{}",
        String::from_utf8_lossy(&diagnosis.bytes)
    );
    Ok(())
}

fn native_domyjob(runner: &mut impl Runner) -> Result<bool> {
    if !runner.available(Tool::Domyjob) {
        return Ok(false);
    }
    for command in [
        args(&["chat", "setup", "--help"]),
        args(&["chat", "doctor", "--help"]),
        args(&["mcp", "--help"]),
    ] {
        if !runner.run(Tool::Domyjob, &command)?.success {
            return Ok(false);
        }
    }
    Ok(true)
}

fn macos_defaults(context: &TemplateData, runner: &mut impl Runner) -> Result<()> {
    ensure!(
        context.profile == Profile::Mac,
        "macOS defaults require the Mac profile"
    );
    for (key, pointer, kind) in [
        ("KeyRepeat", "/macos/keyboard/key_repeat", "-int"),
        (
            "InitialKeyRepeat",
            "/macos/keyboard/initial_key_repeat",
            "-int",
        ),
        (
            "ApplePressAndHoldEnabled",
            "/macos/keyboard/press_and_hold",
            "-bool",
        ),
    ] {
        let value = context
            .data
            .pointer(&context.platform(pointer))
            .context("keyboard setting is missing")?
            .to_string();
        checked(
            runner,
            Tool::Defaults,
            &args(&["write", "-g", key, kind, &value]),
        )?;
    }
    Ok(())
}

/// The domyjob client configuration, whose `machines` table names every machine this host starts jobs on.
const DOMYJOB_CONFIG: &str = ".config/domyjob/config.toml";

/// The machines this host's domyjob configuration names, in order, or none without a readable configuration.
fn domyjob_machines(home: &Path) -> Vec<String> {
    let mut machines: Vec<String> = fs::read_to_string(home.join(DOMYJOB_CONFIG))
        .ok()
        .and_then(|text| text.parse::<toml::Table>().ok())
        .and_then(|table| {
            table
                .get("machines")
                .and_then(toml::Value::as_table)
                .map(|machines| machines.keys().cloned().collect())
        })
        .unwrap_or_default();
    machines.sort();
    machines
}

/// Removes the finished jobs this host started on each machine, which otherwise keep their workspaces and once filled the Windows system drive.
/// A machine that can't answer keeps its jobs until the next upgrade.
fn clean_domyjob(context: &TemplateData, runner: &mut impl Runner) {
    for machine in domyjob_machines(&context.home) {
        optional(runner, Tool::Domyjob, &["clean", &machine]);
    }
}

fn tools(context: &TemplateData, runner: &mut impl Runner, upgrade: bool) -> Result<()> {
    if upgrade {
        clean_domyjob(context, runner);
    }
    if context.profile == Profile::Windows {
        #[cfg(windows)]
        crate::windows::steps::run(context, Step::Tools)?;
        if upgrade {
            execute(context, runner, Step::Integrations)?;
        }
        return Ok(());
    }
    // Both the apply and the timer converge to the reviewed pins, so no unattended run rewrites a managed file or a lock.
    // Version changes arrive only as pull requests.
    if upgrade && context.profile != Profile::Mac && runner.available(Tool::Nix) {
        optional(runner, Tool::Nix, &["profile", "upgrade", "--all"]);
    }
    checked(runner, Tool::Mise, &args(&["install", "-y"]))?;
    if !context.home.join(".local/share/atuin/history.db").exists() {
        optional(runner, Tool::Atuin, &["import", "auto"]);
    }
    optional(runner, Tool::Tldr, &["--update"]);
    // An apply runs the integrations step on its own, and the timer refreshes them after it upgrades.
    if upgrade {
        execute(context, runner, Step::Integrations)?;
    } else {
        services(context, runner)?;
    }
    Ok(())
}

/// Launchd stops a running job before it removes it, and `bootout` returns before the job exits, so a bootstrap that follows at once fails with EINPROGRESS.
const AGENT_REMOVAL: std::time::Duration = std::time::Duration::from_secs(30);
const AGENT_REMOVAL_POLL: std::time::Duration = std::time::Duration::from_millis(100);

fn agent_loaded(runner: &mut impl Runner, target: &str) -> Result<bool> {
    Ok(runner
        .run(Tool::Launchctl, &args(&["print", target]))?
        .success)
}

/// Boots a `LaunchAgent` out and returns once launchd no longer knows it.
pub(crate) fn unload_agent(runner: &mut impl Runner, domain: &str, label: &str) -> Result<()> {
    let target = format!("{domain}/{label}");
    if !agent_loaded(runner, &target)? {
        return Ok(());
    }
    optional(runner, Tool::Launchctl, &["bootout", &target]);
    let deadline = std::time::Instant::now() + AGENT_REMOVAL;
    while agent_loaded(runner, &target)? {
        ensure!(
            std::time::Instant::now() < deadline,
            "{label} was still loaded {} seconds after its bootout",
            AGENT_REMOVAL.as_secs()
        );
        std::thread::sleep(AGENT_REMOVAL_POLL);
    }
    Ok(())
}

/// Replaces a `LaunchAgent`'s loaded definition with the one in `plist`, loading it when absent.
///
/// # Errors
///
/// Returns an error when `launchctl` can't start, when the old job stays loaded 30 seconds after its bootout, or when `launchctl bootstrap` fails.
pub fn reload_agent(
    runner: &mut impl Runner,
    domain: &str,
    label: &str,
    plist: &Path,
) -> Result<()> {
    unload_agent(runner, domain, label)?;
    checked(
        runner,
        Tool::Launchctl,
        &["bootstrap".into(), domain.into(), plist.into()],
    )?;
    Ok(())
}

fn services(context: &TemplateData, runner: &mut impl Runner) -> Result<()> {
    fs::create_dir_all(context.home.join(".local/state/dotfiles/log"))?;
    if context.profile == Profile::Mac {
        let uid = String::from_utf8(checked(runner, Tool::Id, &args(&["-u"]))?)?;
        let domain = format!("gui/{}", uid.trim());
        unload_agent(runner, &domain, "dev.dotfiles.mise-upgrade")?;
        for label in [
            "tools-upgrade",
            "desktop-refresh",
            "session-reaper",
            "dev-reaper",
            "dev-cleanup",
            "storage-scout",
        ] {
            let label = format!("dev.dotfiles.{label}");
            let plist = context
                .home
                .join("Library/LaunchAgents")
                .join(format!("{label}.plist"));
            if plist.is_file() {
                reload_agent(runner, &domain, &label, &plist)?;
            }
        }
    } else if context.profile == Profile::Linux
        && runner.available(Tool::Systemctl)
        && runner
            .run(Tool::Systemctl, &args(&["--user", "show-environment"]))?
            .success
    {
        optional(
            runner,
            Tool::Systemctl,
            &["--user", "stop", "mise-upgrade.timer"],
        );
        let obsolete = context
            .home
            .join(".config/systemd/user/timers.target.wants/mise-upgrade.timer");
        if fs::symlink_metadata(&obsolete).is_ok() {
            fs::remove_file(obsolete)?;
        }
        checked(runner, Tool::Systemctl, &args(&["--user", "daemon-reload"]))?;
        for unit in [
            "tools-upgrade.timer",
            "session-reaper.timer",
            "dotfiles-desktop-refresh.timer",
            "storage-scout.service",
        ] {
            checked(
                runner,
                Tool::Systemctl,
                &args(&["--user", "enable", "--now", unit]),
            )?;
        }
    }
    Ok(())
}

fn ocaml(context: &TemplateData, runner: &mut impl Runner) -> Result<()> {
    let compiler = string(&context.data, "/ocaml/compiler")?;
    let switch = string(&context.data, "/ocaml/switch")?;
    let mut initialization = args(&[
        "init",
        "--bare",
        "--no-setup",
        "--enable-completion",
        "--enable-shell-hook",
        "--yes",
    ]);
    initialization.push(
        if context.profile == Profile::Mac {
            "--shell=zsh"
        } else {
            "--shell=bash"
        }
        .into(),
    );
    if context.home.join(".opam/config").is_file() {
        initialization.push("--reinit".into());
    }
    checked(runner, Tool::Opam, &initialization)?;
    let listed = String::from_utf8(checked(
        runner,
        Tool::Opam,
        &args(&["switch", "list", "--short"]),
    )?)?;
    if !listed.lines().any(|line| line == switch) {
        checked(
            runner,
            Tool::Opam,
            &args(&[
                "switch",
                "create",
                &switch,
                &format!("ocaml-base-compiler.{compiler}"),
                "--yes",
            ]),
        )?;
    }
    checked(
        runner,
        Tool::Opam,
        &args(&["switch", "set", &switch, "--yes"]),
    )?;
    let mut installation = args(&["install", "--switch", &switch, "--yes"]);
    installation.extend(
        strings(&context.data, "/ocaml/platform_tools")?
            .into_iter()
            .map(Into::into),
    );
    checked(runner, Tool::Opam, &installation)?;
    Ok(())
}

fn haskell(context: &TemplateData, runner: &mut impl Runner) -> Result<()> {
    checked(runner, Tool::Cabal, &args(&["update"]))?;
    let config = std::env::var_os("CABAL_CONFIG")
        .map_or_else(|| context.home.join(".config/cabal/config"), PathBuf::from);
    let current = fs::read_to_string(&config)?;
    let setting = format!("installdir: {}", context.home.join(".local/bin").display());
    let mut found = false;
    let mut lines = Vec::new();
    for line in current.lines() {
        if line.starts_with("installdir:") {
            lines.push(setting.clone());
            found = true;
        } else {
            lines.push(line.to_owned());
        }
    }
    if !found {
        lines.push(setting);
    }
    replace(&config, format!("{}\n", lines.join("\n")).as_bytes(), false)?;
    let version = string(
        &context.data,
        &context.platform("/tool_versions/haskell-language-server"),
    )?;
    let installation = context
        .home
        .join(".local/share/mise/installs/github-haskell-haskell-language-server")
        .join(version);
    let realbin = installation.join("realbin");
    fs::create_dir_all(&realbin)?;
    let mut wrappers = 0;
    for entry in fs::read_dir(&installation)? {
        let path = entry?.path();
        let Some(name) = path
            .file_name()
            .and_then(|name| name.to_str())
            .and_then(|name| name.strip_suffix(".in"))
        else {
            continue;
        };
        if !name.starts_with("haskell-language-server-") {
            continue;
        }
        let binary = realbin.join(name);
        if !binary.exists() {
            let old = installation.join("bin/real").join(name);
            fs::rename(
                if old.is_file() {
                    old
                } else {
                    installation.join("bin").join(name)
                },
                &binary,
            )?;
        }
        let wrapper = fs::read_to_string(&path)?
            .replace("@@EXE_DIR@@", realbin.to_str().context("invalid HLS path")?);
        replace(
            &installation.join("bin").join(name),
            wrapper.as_bytes(),
            true,
        )?;
        wrappers += 1;
    }
    ensure!(wrappers > 0, "HLS vendor wrapper templates are missing");
    if !runner.available(Tool::Hlint) {
        checked(runner, Tool::Cabal, &args(&["install", "hlint"]))?;
    }
    Ok(())
}

/// A tool the owner builds from one of their own repositories.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum OwnerSource {
    Ocomment,
    Domyjob,
    Fleet,
}

impl OwnerSource {
    fn repository(self) -> &'static str {
        match self {
            Self::Ocomment => "OComment",
            Self::Domyjob => "domyjob",
            Self::Fleet => "fleet",
        }
    }

    /// An argument the installed binary accepts without side effects, since domyjob lacks `--version`.
    fn probe(self) -> &'static str {
        match self {
            Self::Ocomment | Self::Fleet => "--version",
            Self::Domyjob => "--help",
        }
    }

    fn clone_url(self) -> String {
        format!("{OWNER_SSH}/{}", self.repository())
    }

    /// `git` arguments that clone this source into `checkout`.
    #[must_use]
    pub fn clone_arguments(self, checkout: &Path) -> Vec<OsString> {
        vec!["clone".into(), self.clone_url().into(), checkout.into()]
    }
}

fn source_tool(context: &TemplateData, runner: &mut impl Runner, step: Step) -> Result<()> {
    if step == Step::Domyjob && native_domyjob(runner)? {
        return Ok(());
    }
    let (source, relative, binary) = match step {
        Step::Ocomment => (OwnerSource::Ocomment, "rust/ocomment", Tool::Ocomment),
        Step::Domyjob => (OwnerSource::Domyjob, "crates/domyjob", Tool::Domyjob),
        Step::Fleet => (OwnerSource::Fleet, "", Tool::Fleet),
        _ => unreachable!(),
    };
    let checkout = owner_checkout(context, source)?;
    if !checkout.join(".git").exists() {
        fs::create_dir_all(checkout.parent().context("checkout parent is missing")?)?;
        checked(runner, Tool::Git, &source.clone_arguments(&checkout))?;
    }
    let arguments = cargo_install_arguments(
        &checkout.join(relative),
        &context.home.join(if matches!(step, Step::Domyjob) {
            ".cargo"
        } else {
            ".local"
        }),
        &[],
    );
    checked(runner, Tool::Cargo, &arguments)?;
    checked(runner, binary, &args(&[source.probe()]))?;
    Ok(())
}

/// GitHub's current web-flow signing key, which signs commits merged through the web interface.
const FORGE_CURRENT: &str = "968479A1AFF927E37D1A566BB5690EEEBB952194";
/// The key GitHub signs web commits with.
const GITHUB_WEB_FLOW_KEY: &str = "https://github.com/web-flow.gpg";
/// In `gpg --with-colons` output, the fingerprint fills the tenth field, eight after the record type that names it.
const FINGERPRINT_AFTER_TYPE: usize = 8;
/// The web-flow key GitHub retired in January 2024, which older signatures still need.
const FORGE_RETIRED: &str = "5DE3E0509C47EA3CF04A42D34AEE18F83AFDEB23";

/// Primary-key fingerprints from `gpg --with-colons` output, without the subkeys after `sub` records.
fn primary_fingerprints(records: &str) -> Vec<&str> {
    let mut primary = false;
    let mut fingerprints = Vec::new();
    for line in records.lines() {
        let mut fields = line.split(':');
        match fields.next() {
            Some("pub") => primary = true,
            Some("sub") => primary = false,
            Some("fpr") if primary => {
                fingerprints.push(fields.nth(FINGERPRINT_AFTER_TYPE).unwrap_or_default());
                primary = false;
            }
            _ => {}
        }
    }
    fingerprints
}

#[must_use]
pub fn verified_forge_key(records: &str) -> bool {
    let primary_count = records
        .lines()
        .filter(|line| line.starts_with("pub:"))
        .count();
    let fingerprints = primary_fingerprints(records);
    crate::profile_rules::forge_bundle_accepted(
        primary_count.max(fingerprints.len()),
        fingerprints
            .iter()
            .filter(|fingerprint| ![FORGE_CURRENT, FORGE_RETIRED].contains(fingerprint))
            .count()
            + primary_count.saturating_sub(fingerprints.len()),
        fingerprints.contains(&FORGE_CURRENT),
    )
}

fn forge_keys(runner: &mut impl Runner) -> Result<()> {
    if !runner.available(Tool::Gpg) {
        eprintln!("Warning: gpg is required to verify forge signatures.");
        return Ok(());
    }
    if runner
        .run(Tool::Gpg, &args(&["--list-keys", FORGE_CURRENT]))?
        .success
    {
        return Ok(());
    }
    let scope = tempfile::tempdir()?;
    let key = scope.path().join("web-flow.gpg");
    checked(
        runner,
        Tool::Curl,
        &[
            "--proto".into(),
            "=https".into(),
            "--tlsv1.2".into(),
            "--fail".into(),
            "--silent".into(),
            "--show-error".into(),
            "--location".into(),
            "--max-time".into(),
            "30".into(),
            "--output".into(),
            key.clone().into(),
            GITHUB_WEB_FLOW_KEY.into(),
        ],
    )?;
    let records = checked(
        runner,
        Tool::Gpg,
        &[
            "--show-keys".into(),
            "--with-colons".into(),
            key.clone().into(),
        ],
    )?;
    ensure!(
        verified_forge_key(&String::from_utf8(records)?),
        "forge key fingerprint or primary-key count differs"
    );
    checked(
        runner,
        Tool::Gpg,
        &["--quiet".into(), "--import".into(), key.into()],
    )?;
    Ok(())
}

fn fcitx(context: &TemplateData, runner: &mut impl Runner) -> Result<()> {
    if !runner.available(Tool::ImConfig) || !runner.available(Tool::Fcitx5) {
        return Ok(());
    }
    let source = context.home.join(".xinputrc");
    let current = fs::read_to_string(&source).unwrap_or_default();
    if !current
        .lines()
        .any(|line| line.split_whitespace().collect::<Vec<_>>() == ["run_im", "fcitx5"])
    {
        let backup = context
            .home
            .join(".local/state/dotfiles/backups/xinputrc.pre-fcitx5");
        if source.is_file() && !backup.exists() {
            replace(&backup, current.as_bytes(), false)?;
        }
        checked(runner, Tool::ImConfig, &args(&["-n", "fcitx5"]))?;
    }
    if runner.available(Tool::Fcitx5Remote)
        && runner.run(Tool::Fcitx5Remote, &args(&["--check"]))?.success
    {
        optional(runner, Tool::Fcitx5Remote, &["-r"]);
    }
    Ok(())
}

fn retire_tmux(context: &TemplateData, runner: &mut impl Runner) -> Result<()> {
    if runner.available(Tool::Tmux) && runner.run(Tool::Tmux, &args(&["list-sessions"]))?.success {
        return Ok(());
    }
    let config = context.home.join(".config/tmux/tmux.conf");
    if config.is_file() {
        let retired = context.home.join(".local/state/dotfiles/retired/tmux");
        fs::create_dir_all(&retired)?;
        let unique = tempfile::Builder::new()
            .prefix("tmux.conf.")
            .tempfile_in(retired)?;
        let (_file, destination) = unique.keep()?;
        fs::rename(config, destination)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    struct Refusing;
    impl crate::runtime::Runner for Refusing {
        fn available(&self, _: Tool) -> bool {
            true
        }
        fn run(&mut self, tool: Tool, _: &[OsString]) -> Result<crate::runtime::Reply> {
            assert!(matches!(tool, Tool::Git | Tool::Domyjob));
            Ok(crate::runtime::Reply {
                success: false,
                bytes: Vec::new(),
            })
        }
        fn run_installed(&mut self, _: &Path, _: &[OsString]) -> Result<crate::runtime::Reply> {
            unreachable!()
        }
    }

    /// Records every command, offers only mise and Nix, and reports no loaded launchd job.
    struct Recording(Vec<String>);
    impl crate::runtime::Runner for Recording {
        fn available(&self, tool: Tool) -> bool {
            matches!(tool, Tool::Mise | Tool::Nix | Tool::Domyjob)
        }
        fn run(&mut self, tool: Tool, arguments: &[OsString]) -> Result<crate::runtime::Reply> {
            let words: Vec<_> = arguments.iter().map(|a| a.to_string_lossy()).collect();
            self.0
                .push(format!("{} {}", tool.program(), words.join(" ")));
            Ok(crate::runtime::Reply {
                success: tool != Tool::Launchctl,
                bytes: Vec::new(),
            })
        }
        fn run_installed(&mut self, _: &Path, _: &[OsString]) -> Result<crate::runtime::Reply> {
            unreachable!()
        }
    }

    #[derive(Default)]
    struct ChatRecording {
        commands: Vec<String>,
        fail: Option<&'static str>,
    }

    impl crate::runtime::Runner for ChatRecording {
        fn available(&self, tool: Tool) -> bool {
            tool == Tool::Domyjob
        }

        fn run(&mut self, tool: Tool, arguments: &[OsString]) -> Result<crate::runtime::Reply> {
            assert_eq!(tool, Tool::Domyjob);
            let command = arguments
                .iter()
                .map(|word| word.to_string_lossy())
                .collect::<Vec<_>>()
                .join(" ");
            let success = self.fail != Some(command.as_str());
            self.commands.push(command);
            Ok(crate::runtime::Reply {
                success,
                bytes: if success {
                    Vec::new()
                } else {
                    b"FIX peer: fixture peer unavailable".to_vec()
                },
            })
        }

        fn run_installed(&mut self, _: &Path, _: &[OsString]) -> Result<crate::runtime::Reply> {
            unreachable!()
        }
    }

    #[test]
    fn chat_setup_uses_local_peers_and_requires_a_clean_doctor() -> Result<()> {
        let scope = tempfile::tempdir()?;
        for profile in [Profile::Mac, Profile::Linux, Profile::Wsl, Profile::Windows] {
            let context = TemplateData {
                source: scope.path().into(),
                home: scope.path().into(),
                profile,
                data: serde_json::json!({"domyjob": {"chat": {"peers": ["peer-one", "peer-two"]}}}),
            };
            let mut runner = ChatRecording::default();
            execute(&context, &mut runner, Step::Chat)?;
            assert_eq!(
                runner.commands,
                ["chat --json setup peer-one peer-two", "chat doctor"]
            );
            assert!(Step::Domyjob.runs_on(profile));
            assert!(Step::Chat.runs_on(profile));
        }
        Ok(())
    }

    #[test]
    fn invalid_chat_configuration_reports_the_next_action_before_any_command() -> Result<()> {
        let scope = tempfile::tempdir()?;
        for data in [
            serde_json::json!({}),
            serde_json::json!({"domyjob": {"chat": {"peers": []}}}),
            serde_json::json!({"domyjob": {"chat": {"peers": ["--replace"]}}}),
            serde_json::json!({"domyjob": {"chat": {"peers": ["peer", "peer"]}}}),
            serde_json::json!({"domyjob": {"chat": {"peers": ["peer other"]}}}),
            serde_json::json!({"domyjob": {"chat": {"peers": [1]}}}),
            serde_json::json!({"domyjob": {"chat": {"peers": ["peer"], "replace": true}}}),
        ] {
            let context = TemplateData {
                source: scope.path().into(),
                home: scope.path().into(),
                profile: Profile::Linux,
                data,
            };
            let mut runner = ChatRecording::default();
            let error = execute(&context, &mut runner, Step::Chat)
                .unwrap_err()
                .to_string();
            assert!(error.contains("data.domyjob.chat"), "{error}");
            assert!(error.contains("setup.ignore"), "{error}");
            assert_eq!(runner.commands, Vec::<String>::new());
        }
        Ok(())
    }

    #[test]
    fn chat_setup_and_doctor_failures_keep_the_actual_findings() -> Result<()> {
        let scope = tempfile::tempdir()?;
        let context = TemplateData {
            source: scope.path().into(),
            home: scope.path().into(),
            profile: Profile::Linux,
            data: serde_json::json!({"domyjob": {"chat": {"peers": ["peer"]}}}),
        };
        for (fail, commands) in [("chat --json setup peer", 1), ("chat doctor", 2)] {
            let mut runner = ChatRecording {
                fail: Some(fail),
                ..ChatRecording::default()
            };
            let error = execute(&context, &mut runner, Step::Chat)
                .unwrap_err()
                .to_string();
            assert!(error.contains("fixture peer unavailable"), "{error}");
            assert_eq!(runner.commands.len(), commands);
        }
        Ok(())
    }

    #[test]
    fn a_compatible_native_domyjob_needs_no_checkout_credentials_or_build() -> Result<()> {
        let scope = tempfile::tempdir()?;
        let context = TemplateData {
            source: scope.path().into(),
            home: scope.path().into(),
            profile: Profile::Windows,
            data: serde_json::json!({}),
        };
        let mut runner = ChatRecording::default();
        preflight(&context, &mut runner, &[Step::Domyjob])?;
        execute(&context, &mut runner, Step::Domyjob)?;
        assert_eq!(
            runner.commands,
            ["chat setup --help", "chat doctor --help", "mcp --help"].repeat(2)
        );
        Ok(())
    }

    #[test]
    fn retiring_tmux_preserves_installed_versions() -> Result<()> {
        let scope = tempfile::tempdir()?;
        let context = TemplateData {
            source: scope.path().into(),
            home: scope.path().into(),
            profile: Profile::Linux,
            data: serde_json::json!({}),
        };
        let mut runner = Recording(Vec::new());
        retire_tmux(&context, &mut runner)?;
        assert!(runner.0.is_empty(), "{:?}", runner.0);
        Ok(())
    }

    #[test]
    fn neither_the_apply_nor_the_timer_moves_a_pin() {
        for profile in [Profile::Mac, Profile::Linux, Profile::Wsl] {
            for upgrade in [false, true] {
                let scope = tempfile::tempdir().unwrap();
                let context = TemplateData {
                    source: scope.path().into(),
                    home: scope.path().into(),
                    profile,
                    data: serde_json::json!({}),
                };
                let mut runner = Recording(Vec::new());
                tools(&context, &mut runner, upgrade).unwrap();
                assert!(
                    runner
                        .0
                        .iter()
                        .any(|command| command.ends_with("install -y")),
                    "{profile:?} never converges: {:?}",
                    runner.0
                );
                for command in &runner.0 {
                    for rewrite in ["--bump", "self-update", "flake update", "brew upgrade"] {
                        assert!(
                            !command.contains(rewrite),
                            "{profile:?} runs {command} unattended"
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn the_timer_cleans_finished_domyjob_jobs_on_every_configured_machine() {
        let scope = tempfile::tempdir().unwrap();
        let context = TemplateData {
            source: scope.path().into(),
            home: scope.path().into(),
            profile: Profile::Mac,
            data: serde_json::json!({}),
        };
        let config = scope.path().join(DOMYJOB_CONFIG);
        fs::create_dir_all(config.parent().unwrap()).unwrap();
        fs::write(
            &config,
            "[machines.win]\nlabels = [\"windows\"]\n\n[machines.linux]\n",
        )
        .unwrap();
        for upgrade in [false, true] {
            let mut runner = Recording(Vec::new());
            tools(&context, &mut runner, upgrade).unwrap();
            let cleaned: Vec<&str> = runner
                .0
                .iter()
                .map(String::as_str)
                .filter(|command| command.starts_with("domyjob clean"))
                .collect();
            let expected: &[&str] = if upgrade {
                &["domyjob clean linux", "domyjob clean win"]
            } else {
                &[]
            };
            assert_eq!(cleaned, expected, "{:?}", runner.0);
        }
        fs::write(&config, "machines = [").unwrap();
        assert_eq!(domyjob_machines(scope.path()), Vec::<String>::new());
    }

    #[test]
    fn every_owner_source_without_credentials_stops_before_application() {
        let scope = tempfile::tempdir().unwrap();
        let context = TemplateData {
            source: scope.path().into(),
            home: scope.path().into(),
            profile: Profile::Mac,
            data: serde_json::json!({"paths": {"projects": scope.path()}}),
        };
        for (step, repository) in [
            (Step::Fleet, "fleet"),
            (Step::Ocomment, "OComment"),
            (Step::Domyjob, "domyjob"),
        ] {
            let refused = preflight(&context, &mut Refusing, &[Step::Runtime, step]);
            assert!(refused.unwrap_err().to_string().contains("setup.ignore"));
            std::fs::create_dir_all(
                scope
                    .path()
                    .join(format!("github.com/P4suta/{repository}/.git")),
            )
            .unwrap();
            preflight(&context, &mut Refusing, &[step]).unwrap();
        }
    }

    #[test]
    fn cargo_receives_drive_paths_without_the_verbatim_prefix() {
        use std::path::{Path, PathBuf};
        assert_eq!(
            crate::without_verbatim_prefix(Path::new(r"\\?\C:\Users\fixture\source")),
            PathBuf::from(r"C:\Users\fixture\source")
        );
        assert_eq!(
            crate::without_verbatim_prefix(Path::new(r"\\?\UNC\server\share")),
            PathBuf::from(r"\\?\UNC\server\share")
        );
        assert_eq!(
            crate::without_verbatim_prefix(Path::new("/source/xtask")),
            PathBuf::from("/source/xtask")
        );
    }

    #[test]
    fn helper_installation_replaces_binaries_from_another_source() {
        let arguments = super::cargo_install_arguments(
            std::path::Path::new("/source/xtask"),
            std::path::Path::new("/home/fixture/.local"),
            &["skill-ops"],
        );
        assert_eq!(
            arguments,
            [
                "install",
                "--locked",
                "--force",
                "--path",
                "/source/xtask",
                "--root",
                "/home/fixture/.local",
                "--bin",
                "skill-ops",
            ]
        );
    }
    use super::*;

    /// The shape GitHub serves at <https://github.com/web-flow.gpg> since its 2024 key rotation.
    const PUBLISHED: &str = "pub:e:2048:1:4AEE18F83AFDEB23:1502898241:1705435200::-:::sc::::::23::0:\nfpr:::::::::5DE3E0509C47EA3CF04A42D34AEE18F83AFDEB23:\npub:-:4096:1:B5690EEEBB952194:1705428342:::-:::scSC::::::23::0:\nfpr:::::::::968479A1AFF927E37D1A566BB5690EEEBB952194:\n";

    #[test]
    fn the_published_bundle_is_accepted_and_unpinned_or_stale_bundles_are_refused() {
        assert!(verified_forge_key(PUBLISHED));
        let current_only = format!("pub:::::::::\nfpr:::::::::{FORGE_CURRENT}:\n");
        assert!(verified_forge_key(&current_only));
        let retired_only = format!("pub:::::::::\nfpr:::::::::{FORGE_RETIRED}:\n");
        assert!(!verified_forge_key(&retired_only));
        assert!(!verified_forge_key(&format!(
            "pub:::::::::\nfpr:::::::::OTHER:\n{current_only}"
        )));
        assert!(!verified_forge_key(&format!(
            "{current_only}pub:::::::::\n"
        )));
        let subkey = format!("{current_only}sub:::::::::\nfpr:::::::::SUBKEY:\n");
        assert!(verified_forge_key(&subkey));
    }
}
