use crate::profile_rules::{Mode, Profile, permitted};
use crate::runtime::{Native, Runner, args, checked, optional, replace};
use crate::tool::Tool;
use anyhow::{Context, Result, ensure};
use serde_json::Value;
use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, clap::ValueEnum)]
pub enum Step {
    Runtime,
    Mise,
    Github,
    Nix,
    Tools,
    Upgrade,
    Integrations,
    Desktop,
    Fcitx5,
    Ocaml,
    Haskell,
    Racket,
    Guard,
    Ocomment,
    Domyjob,
    Fleet,
    MacosDefaults,
    ForgeKeys,
    Claude,
    RetireTmux,
    Adapters,
    Wezterm,
}

pub struct ContextData {
    pub source: PathBuf,
    pub home: PathBuf,
    pub profile: Profile,
    pub data: Value,
}

fn merge(target: &mut Value, values: &Value) {
    if let (Some(target), Some(values)) = (target.as_object_mut(), values.as_object()) {
        for (key, value) in values {
            if let Some(current) = target.get_mut(key) {
                merge(current, value);
            } else {
                target.insert(key.clone(), value.clone());
            }
        }
    } else {
        *target = values.clone();
    }
}

pub fn load(source: &Path, config: &Path, home: &Path) -> Result<ContextData> {
    let scope = tempfile::tempdir()?;
    let output = crate::profiles::chezmoi(source, scope.path(), config, home)
        .args(["data", "--format", "json"])
        .output()?;
    ensure!(
        output.status.success(),
        "machine-local profile could not be read"
    );
    let raw: Value = serde_json::from_slice(&output.stdout)?;
    let profile = crate::profiles::configured_profile(&raw)?;
    let mut data = raw["platforms"][profile.name()].clone();
    merge(&mut data, &raw);
    Ok(ContextData {
        source: source.to_path_buf(),
        home: home.to_path_buf(),
        profile,
        data,
    })
}

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
    let mut native = Native::new(&context.home)?;
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
            kerl.push_str(&format!(" --with-ssl={}", prefix.trim()));
        }
    }
    native
        .environment
        .push(("KERL_CONFIGURE_OPTIONS".into(), kerl.into()));
    if let Some(jobs) = context.data.pointer("/mise/jobs").and_then(Value::as_u64) {
        native
            .environment
            .push(("MISE_JOBS".into(), jobs.to_string().into()));
    }
    execute(&context, &mut native, step)
}

impl Step {
    /// Steps whose effects this step relies on: when both run in one application, these must run first.
    /// Every step is listed, so a new step cannot be added without deciding what it depends on.
    pub fn after(self) -> &'static [Step] {
        match self {
            Self::Ocomment | Self::Domyjob | Self::Fleet => &[Self::Tools],
            Self::Ocaml | Self::Haskell | Self::Integrations => &[Self::Tools],
            Self::Racket => &[Self::Desktop],
            Self::Adapters | Self::Guard => &[Self::Runtime],
            Self::Tools | Self::Upgrade => &[Self::Mise],
            Self::Runtime
            | Self::Mise
            | Self::Github
            | Self::Nix
            | Self::Desktop
            | Self::Fcitx5
            | Self::MacosDefaults
            | Self::ForgeKeys
            | Self::Claude
            | Self::RetireTmux
            | Self::Wezterm => &[],
        }
    }
}

impl Step {
    /// Packaged tools this step invokes, beyond those the host or this repository pins.
    /// Owner sources are cloned under the global Git hooks, which run Lefthook for repositories that configure it.
    /// On Windows the tools step runs `dotctl setup tools`, which installs Lefthook's hooks into the source checkout.
    pub fn requires(self, profile: Profile) -> &'static [Tool] {
        match self {
            Self::Ocomment | Self::Domyjob | Self::Fleet => &[Tool::Lefthook],
            Self::Tools if profile == Profile::Windows => dotctl_requires("tools"),
            Self::Runtime
            | Self::Mise
            | Self::Github
            | Self::Nix
            | Self::Tools
            | Self::Upgrade
            | Self::Integrations
            | Self::Desktop
            | Self::Fcitx5
            | Self::Ocaml
            | Self::Haskell
            | Self::Racket
            | Self::Guard
            | Self::MacosDefaults
            | Self::ForgeKeys
            | Self::Claude
            | Self::RetireTmux
            | Self::Adapters
            | Self::Wezterm => &[],
        }
    }
}

/// Packaged tools the Windows `dotctl setup` commands invoke.
fn dotctl_requires(command: &str) -> &'static [Tool] {
    match command {
        "tools" => &[Tool::Lefthook],
        "shell" => &[Tool::Starship, Tool::Zoxide],
        _ => &[],
    }
}

/// Packaged tools the rendered scripts require, through xtask steps and `dotctl` commands.
pub fn scripted_requirements(profile: Profile, scripts: &str) -> Vec<Tool> {
    let mut tools: Vec<Tool> = scripted_steps(scripts)
        .into_iter()
        .flat_map(|step| step.requires(profile).iter().copied())
        .collect();
    for words in scripts
        .lines()
        .map(|line| line.split_whitespace().collect::<Vec<_>>())
    {
        for triple in words.windows(3) {
            if triple[0].trim_matches(['\'', '"']).ends_with("dotctl.exe") && triple[1] == "setup" {
                tools.extend(dotctl_requires(triple[2]));
            }
        }
    }
    tools.sort();
    tools.dedup();
    tools
}

/// Dependency pairs `(step, prerequisite)` that a run order breaks: the prerequisite runs later, or only later.
pub fn ordering_violations(sequence: &[Step]) -> Vec<(Step, Step)> {
    let mut violations = Vec::new();
    for (index, step) in sequence.iter().enumerate() {
        for prerequisite in step.after() {
            let first = sequence
                .iter()
                .position(|candidate| candidate == prerequisite);
            if first.is_some_and(|first| first > index) {
                violations.push((*step, *prerequisite));
            }
        }
    }
    violations
}

/// Setup steps a rendered script runs, in script order, from `dotfiles-xtask ... setup STEP` invocations.
pub fn scripted_steps(scripts: &str) -> Vec<Step> {
    use clap::ValueEnum;
    let mut steps = Vec::new();
    for words in scripts
        .lines()
        .map(|line| line.split_whitespace().collect::<Vec<_>>())
    {
        for pair in words.windows(2) {
            if pair[0] == "setup"
                && let Ok(step) = Step::from_str(pair[1], false)
                && !steps.contains(&step)
            {
                steps.push(step);
            }
        }
    }
    steps
}

/// Requirements a step cannot satisfy midway: checked for every scripted step before application changes anything.
pub fn preflight(context: &ContextData, runner: &mut impl Runner, steps: &[Step]) -> Result<()> {
    let mut unmet = Vec::new();
    for &step in steps {
        if let Some(source) = owner_source(step)
            && !source.public()
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
        "setup cannot complete on this host; run from a session that satisfies it or name the script in setup.skip:\n{}",
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

fn owner_checkout(context: &ContextData, source: OwnerSource) -> Result<PathBuf> {
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

fn install_crate(
    context: &ContextData,
    runner: &mut impl Runner,
    relative: &str,
    binaries: &[&str],
) -> Result<()> {
    let arguments = cargo_install_arguments(
        &context.source.join(relative),
        &context.home.join(".local"),
        binaries,
    );
    checked(runner, Tool::Cargo, &arguments)?;
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

pub fn execute(context: &ContextData, runner: &mut impl Runner, step: Step) -> Result<()> {
    match step {
        Step::Runtime => {
            install_crate(
                context,
                runner,
                "xtask",
                &["dotfiles-xtask", "skill-ops", "pr-workflow"],
            )?;
            if context.profile == Profile::Windows {
                install_crate(context, runner, "tools/dotctl", &["dotctl"])?;
            } else {
                install_crate(
                    context,
                    runner,
                    "guard",
                    &["dotguard", "reaper", "herdr-agent"],
                )?;
            }
            if context.profile == Profile::Mac {
                install_crate(context, runner, "xtask", &["coderabbit"])?;
                crate::review_guard::install(
                    &context.home.join(".local/bin/coderabbit"),
                    &context.home,
                )?;
            }
        }
        Step::Adapters => {
            let binary = context.home.join(format!(
                ".local/bin/skill-ops{}",
                std::env::consts::EXE_SUFFIX
            ));
            crate::skill_install::install(&context.source, &binary, &context.home)?;
        }
        Step::Mise => {
            if !runner.available(Tool::Mise) {
                if context.profile == Profile::Windows {
                    checked(
                        runner,
                        Tool::Winget,
                        &args(&[
                            "install",
                            "--id",
                            "jdx.mise",
                            "--exact",
                            "--silent",
                            "--accept-source-agreements",
                            "--accept-package-agreements",
                        ]),
                    )?;
                } else {
                    vendor_installer(runner, "https://mise.run", &[])?;
                }
            }
        }
        Step::Github => {
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
        }
        Step::Nix => {
            let nix = Tool::Nix;
            if runner.available(nix) {
                let flake = context.home.join(".config/dotfiles-nix");
                let listed = checked(runner, nix, &args(&["profile", "list"]))?;
                if String::from_utf8(listed)?
                    .contains(flake.to_str().context("Nix profile path is not UTF-8")?)
                {
                    checked(runner, nix, &args(&["profile", "upgrade", "--all"]))?;
                } else {
                    checked(runner, nix, &["profile".into(), "add".into(), flake.into()])?;
                }
            }
        }
        Step::Tools | Step::Upgrade => tools(context, runner, matches!(step, Step::Upgrade))?,
        Step::Integrations => {
            if context.profile == Profile::Windows {
                checked(runner, Tool::Dotctl, &args(&["setup", "shell"]))?;
                crate::runtime::agent_integrations(runner, true)?;
            } else {
                let remote = ["SSH_CONNECTION", "SSH_CLIENT", "SSH_TTY"]
                    .iter()
                    .any(|name| std::env::var_os(name).is_some_and(|value| !value.is_empty()));
                let gui = !remote
                    && match context.profile {
                        Profile::Mac => Path::new("/Applications/1Password.app").is_dir(),
                        Profile::Linux => runner.available(Tool::OnePassword),
                        _ => false,
                    };
                crate::runtime::integrations(&context.home, runner, gui)?;
            }
        }
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
                strings(&context.data, "/racket/collections")?
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
                "guard",
                &["dotguard", "reaper", "herdr-agent"],
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
        Step::Ocomment | Step::Domyjob | Step::Fleet => source_tool(context, runner, step)?,
        Step::MacosDefaults => {
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
                    .pointer(pointer)
                    .context("keyboard setting is missing")?
                    .to_string();
                checked(
                    runner,
                    Tool::Defaults,
                    &args(&["write", "-g", key, kind, &value]),
                )?;
            }
        }
        Step::ForgeKeys => forge_keys(runner)?,
        Step::Claude => {
            if !runner.available(Tool::Claude) {
                vendor_installer(runner, "https://claude.ai/install.sh", &[])?;
            }
        }
        Step::Fcitx5 => fcitx(context, runner)?,
        Step::RetireTmux => retire_tmux(context, runner)?,
        Step::Wezterm => crate::windows_setup::wezterm(&context.home, runner)?,
    }
    Ok(())
}

fn tools(context: &ContextData, runner: &mut impl Runner, upgrade: bool) -> Result<()> {
    if context.profile == Profile::Windows {
        let mut arguments = args(&["setup", "tools", "--source"]);
        arguments.push(context.source.clone().into());
        arguments.extend(args(&[
            "--scoop-apps",
            &strings(&context.data, "/scoop/apps")?.join(","),
        ]));
        checked(runner, Tool::Dotctl, &arguments)?;
        return Ok(());
    }
    if upgrade {
        if context.profile == Profile::Mac {
            optional(runner, Tool::Brew, &["update", "--quiet"]);
            optional(runner, Tool::Brew, &["upgrade", "--quiet"]);
        } else {
            optional(
                runner,
                Tool::Nix,
                &[
                    "flake",
                    "update",
                    "--flake",
                    context
                        .home
                        .join(".config/dotfiles-nix")
                        .to_str()
                        .context("invalid flake path")?,
                ],
            );
            optional(runner, Tool::Nix, &["profile", "upgrade", "--all"]);
        }
        optional(runner, Tool::Mise, &["self-update", "-y", "--no-plugins"]);
    } else {
        checked(runner, Tool::Mise, &args(&["install", "-y"]))?;
    }
    let mut arguments = args(&["upgrade", "--bump", "-y"]);
    if context.profile == Profile::Mac {
        arguments.extend(args(&[
            "--exclude",
            "ghc",
            "--exclude",
            "cabal",
            "--exclude",
            "github:haskell/haskell-language-server",
        ]));
    }
    checked(runner, Tool::Mise, &arguments)?;
    if !context.home.join(".local/share/atuin/history.db").exists() {
        optional(runner, Tool::Atuin, &["import", "auto"]);
    }
    optional(runner, Tool::Tldr, &["--update"]);
    execute(context, runner, Step::Integrations)?;
    if !upgrade {
        services(context, runner)?;
    }
    Ok(())
}

fn services(context: &ContextData, runner: &mut impl Runner) -> Result<()> {
    fs::create_dir_all(context.home.join(".local/state/dotfiles/log"))?;
    if context.profile == Profile::Mac {
        let uid = String::from_utf8(checked(runner, Tool::Id, &args(&["-u"]))?)?;
        let domain = format!("gui/{}", uid.trim());
        optional(
            runner,
            Tool::Launchctl,
            &["bootout", &format!("{domain}/dev.dotfiles.mise-upgrade")],
        );
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
                optional(
                    runner,
                    Tool::Launchctl,
                    &["bootout", &format!("{domain}/{label}")],
                );
                checked(
                    runner,
                    Tool::Launchctl,
                    &["bootstrap".into(), domain.clone().into(), plist.into()],
                )?;
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

fn ocaml(context: &ContextData, runner: &mut impl Runner) -> Result<()> {
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

fn haskell(context: &ContextData, runner: &mut impl Runner) -> Result<()> {
    checked(runner, Tool::Cabal, &args(&["update"]))?;
    let config = std::env::var_os("CABAL_CONFIG")
        .map(PathBuf::from)
        .unwrap_or_else(|| context.home.join(".config/cabal/config"));
    let content = fs::read_to_string(&config)?;
    let setting = format!("installdir: {}", context.home.join(".local/bin").display());
    let mut found = false;
    let mut lines = Vec::new();
    for line in content.lines() {
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
    let version = string(&context.data, "/tool_versions/haskell-language-server")?;
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

    /// Public sources must clone without credentials, so an unattended or remote application never needs the owner's SSH key for them.
    fn public(self) -> bool {
        match self {
            Self::Ocomment | Self::Domyjob => true,
            Self::Fleet => false,
        }
    }

    fn clone_url(self) -> String {
        if self.public() {
            format!("https://github.com/P4suta/{}.git", self.repository())
        } else {
            format!("git@github.com:P4suta/{}", self.repository())
        }
    }

    /// `git` arguments that clone this source into `checkout`.
    /// A public URL is mapped onto itself: git applies the longest matching `insteadOf`, so a host-wide HTTPS-to-SSH rewrite cannot turn it into an SSH clone.
    pub fn clone_arguments(self, checkout: &Path) -> Vec<OsString> {
        let mut arguments = Vec::new();
        let url = self.clone_url();
        if self.public() {
            arguments.push("-c".into());
            arguments.push(format!("url.{url}.insteadOf={url}").into());
        }
        arguments.push("clone".into());
        arguments.push(url.into());
        arguments.push(checkout.into());
        arguments
    }
}

fn source_tool(context: &ContextData, runner: &mut impl Runner, step: Step) -> Result<()> {
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
    checked(runner, binary, &args(&["--version"]))?;
    Ok(())
}

/// GitHub's current web-flow signing key, which signs commits merged through the web interface.
const FORGE_CURRENT: &str = "968479A1AFF927E37D1A566BB5690EEEBB952194";
/// The web-flow key GitHub retired in January 2024; its bundle still carries it for older signatures.
const FORGE_RETIRED: &str = "5DE3E0509C47EA3CF04A42D34AEE18F83AFDEB23";

/// Primary-key fingerprints from `gpg --with-colons` output; subkey fingerprints follow `sub` records and are skipped.
fn primary_fingerprints(records: &str) -> Vec<&str> {
    let mut primary = false;
    let mut fingerprints = Vec::new();
    for line in records.lines() {
        let mut fields = line.split(':');
        match fields.next() {
            Some("pub") => primary = true,
            Some("sub") => primary = false,
            Some("fpr") if primary => {
                fingerprints.push(fields.nth(8).unwrap_or_default());
                primary = false;
            }
            _ => {}
        }
    }
    fingerprints
}

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
            "https://github.com/web-flow.gpg".into(),
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

fn fcitx(context: &ContextData, runner: &mut impl Runner) -> Result<()> {
    if !runner.available(Tool::ImConfig) || !runner.available(Tool::Fcitx5) {
        return Ok(());
    }
    let source = context.home.join(".xinputrc");
    let content = fs::read_to_string(&source).unwrap_or_default();
    if !content
        .lines()
        .any(|line| line.split_whitespace().collect::<Vec<_>>() == ["run_im", "fcitx5"])
    {
        let backup = context
            .home
            .join(".local/state/dotfiles/backups/xinputrc.pre-fcitx5");
        if source.is_file() && !backup.exists() {
            replace(&backup, content.as_bytes(), false)?;
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

fn retire_tmux(context: &ContextData, runner: &mut impl Runner) -> Result<()> {
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
    if runner.available(Tool::Mise) && runner.run(Tool::Mise, &args(&["current", "tmux"]))?.success
    {
        checked(
            runner,
            Tool::Mise,
            &args(&["uninstall", "--all", "tmux", "-y"]),
        )?;
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
            assert_eq!(tool, Tool::Git);
            Ok(crate::runtime::Reply {
                success: false,
                bytes: Vec::new(),
            })
        }
        fn run_installed(&mut self, _: &Path, _: &[OsString]) -> Result<crate::runtime::Reply> {
            unreachable!()
        }
    }

    #[test]
    fn a_private_source_without_credentials_stops_before_application() {
        let scope = tempfile::tempdir().unwrap();
        let context = ContextData {
            source: scope.path().into(),
            home: scope.path().into(),
            profile: Profile::Mac,
            data: serde_json::json!({"paths": {"projects": scope.path()}}),
        };
        let refused = preflight(&context, &mut Refusing, &[Step::Runtime, Step::Fleet]);
        assert!(refused.unwrap_err().to_string().contains("setup.skip"));
        preflight(&context, &mut Refusing, &[Step::Runtime, Step::Ocomment]).unwrap();
        std::fs::create_dir_all(scope.path().join("github.com/P4suta/fleet/.git")).unwrap();
        preflight(&context, &mut Refusing, &[Step::Fleet]).unwrap();
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

    /// The shape GitHub serves at https://github.com/web-flow.gpg since its 2024 key rotation.
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
