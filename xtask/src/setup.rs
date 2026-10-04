use crate::profile_rules::{Mode, Profile, permitted};
use crate::runtime::{Native, Runner, args, checked, optional, replace};
use anyhow::{Context, Result, ensure};
use serde_json::Value;
use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, Debug, clap::ValueEnum)]
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
        && native.available("brew")
        && let Ok(prefix) = checked(&mut native, "brew", &args(&["--prefix", "openssl@3"]))
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
    checked(runner, "cargo", &arguments)?;
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
    checked(runner, "curl", &download)?;
    let mut invocation = vec![script.into()];
    invocation.extend(args(arguments));
    checked(runner, "sh", &invocation)?;
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
            if !runner.available("mise") {
                if context.profile == Profile::Windows {
                    checked(
                        runner,
                        "winget",
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
            if !runner.available("gh") {
                checked(runner, "mise", &args(&["install", "gh@latest", "-y"]))?;
                checked(runner, "mise", &args(&["reshim"]))?;
            }
            let reply = runner.run("gh", &args(&["auth", "status"]))?;
            if reply.success {
                checked(
                    runner,
                    "gh",
                    &args(&["config", "set", "-h", "github.com", "git_protocol", "ssh"]),
                )?;
            } else {
                eprintln!(
                    "Warning: GitHub authentication is required; run gh auth login before installing tools."
                );
            }
        }
        Step::Nix => {
            let nix = "/nix/var/nix/profiles/default/bin/nix";
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
                checked(runner, "dotctl", &args(&["setup", "shell"]))?;
                crate::runtime::agent_integrations(runner, true)?;
            } else {
                let remote = ["SSH_CONNECTION", "SSH_CLIENT", "SSH_TTY"]
                    .iter()
                    .any(|name| std::env::var_os(name).is_some_and(|value| !value.is_empty()));
                let gui = !remote
                    && match context.profile {
                        Profile::Mac => Path::new("/Applications/1Password.app").is_dir(),
                        Profile::Linux => runner.available("1password"),
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
            checked(runner, "raco", &arguments)?;
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
                    "defaults",
                    &args(&["write", "-g", key, kind, &value]),
                )?;
            }
        }
        Step::ForgeKeys => forge_keys(runner)?,
        Step::Claude => {
            if !runner.available("claude") {
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
        checked(runner, "dotctl", &arguments)?;
        return Ok(());
    }
    if upgrade {
        if context.profile == Profile::Mac {
            optional(runner, "brew", &["update", "--quiet"]);
            optional(runner, "brew", &["upgrade", "--quiet"]);
        } else {
            optional(
                runner,
                "/nix/var/nix/profiles/default/bin/nix",
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
            optional(
                runner,
                "/nix/var/nix/profiles/default/bin/nix",
                &["profile", "upgrade", "--all"],
            );
        }
        optional(runner, "mise", &["self-update", "-y", "--no-plugins"]);
    } else {
        checked(runner, "mise", &args(&["install", "-y"]))?;
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
    checked(runner, "mise", &arguments)?;
    if !context.home.join(".local/share/atuin/history.db").exists() {
        optional(runner, "atuin", &["import", "auto"]);
    }
    optional(runner, "tldr", &["--update"]);
    execute(context, runner, Step::Integrations)?;
    if !upgrade {
        services(context, runner)?;
    }
    Ok(())
}

fn services(context: &ContextData, runner: &mut impl Runner) -> Result<()> {
    fs::create_dir_all(context.home.join(".local/state/dotfiles/log"))?;
    if context.profile == Profile::Mac {
        let uid = String::from_utf8(checked(runner, "id", &args(&["-u"]))?)?;
        let domain = format!("gui/{}", uid.trim());
        optional(
            runner,
            "launchctl",
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
                    "launchctl",
                    &["bootout", &format!("{domain}/{label}")],
                );
                checked(
                    runner,
                    "launchctl",
                    &["bootstrap".into(), domain.clone().into(), plist.into()],
                )?;
            }
        }
    } else if context.profile == Profile::Linux
        && runner.available("systemctl")
        && runner
            .run("systemctl", &args(&["--user", "show-environment"]))?
            .success
    {
        optional(
            runner,
            "systemctl",
            &["--user", "stop", "mise-upgrade.timer"],
        );
        let obsolete = context
            .home
            .join(".config/systemd/user/timers.target.wants/mise-upgrade.timer");
        if fs::symlink_metadata(&obsolete).is_ok() {
            fs::remove_file(obsolete)?;
        }
        checked(runner, "systemctl", &args(&["--user", "daemon-reload"]))?;
        for unit in [
            "tools-upgrade.timer",
            "session-reaper.timer",
            "dotfiles-desktop-refresh.timer",
            "storage-scout.service",
        ] {
            checked(
                runner,
                "systemctl",
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
    checked(runner, "opam", &initialization)?;
    let listed = String::from_utf8(checked(
        runner,
        "opam",
        &args(&["switch", "list", "--short"]),
    )?)?;
    if !listed.lines().any(|line| line == switch) {
        checked(
            runner,
            "opam",
            &args(&[
                "switch",
                "create",
                &switch,
                &format!("ocaml-base-compiler.{compiler}"),
                "--yes",
            ]),
        )?;
    }
    checked(runner, "opam", &args(&["switch", "set", &switch, "--yes"]))?;
    let mut installation = args(&["install", "--switch", &switch, "--yes"]);
    installation.extend(
        strings(&context.data, "/ocaml/platform_tools")?
            .into_iter()
            .map(Into::into),
    );
    checked(runner, "opam", &installation)?;
    Ok(())
}

fn haskell(context: &ContextData, runner: &mut impl Runner) -> Result<()> {
    checked(runner, "cabal", &args(&["update"]))?;
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
    if !runner.available("hlint") {
        checked(runner, "cabal", &args(&["install", "hlint"]))?;
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

    /// `git` arguments that clone this source into `checkout`.
    /// A public URL is mapped onto itself: git applies the longest matching `insteadOf`, so a host-wide HTTPS-to-SSH rewrite cannot turn it into an SSH clone.
    pub fn clone_arguments(self, checkout: &Path) -> Vec<OsString> {
        let mut arguments = Vec::new();
        let url = if self.public() {
            let url = format!("https://github.com/P4suta/{}.git", self.repository());
            arguments.push("-c".into());
            arguments.push(format!("url.{url}.insteadOf={url}").into());
            url
        } else {
            format!("git@github.com:P4suta/{}", self.repository())
        };
        arguments.push("clone".into());
        arguments.push(url.into());
        arguments.push(checkout.into());
        arguments
    }
}

fn source_tool(context: &ContextData, runner: &mut impl Runner, step: Step) -> Result<()> {
    let (source, relative, binary) = match step {
        Step::Ocomment => (OwnerSource::Ocomment, "rust/ocomment", "ocomment"),
        Step::Domyjob => (OwnerSource::Domyjob, "crates/domyjob", "domyjob"),
        Step::Fleet => (OwnerSource::Fleet, "", "fleet"),
        _ => unreachable!(),
    };
    let projects = string(&context.data, "/paths/projects")?;
    ensure!(
        !projects.is_empty(),
        "machine-local project root is missing"
    );
    let checkout = Path::new(&projects)
        .join("github.com/P4suta")
        .join(source.repository());
    if !checkout.join(".git").exists() {
        fs::create_dir_all(checkout.parent().context("checkout parent is missing")?)?;
        checked(runner, "git", &source.clone_arguments(&checkout))?;
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
    checked(runner, "cargo", &arguments)?;
    checked(runner, binary, &args(&["--version"]))?;
    Ok(())
}

const FORGE_FINGERPRINT: &str = "5DE3E0509C47EA3CF04A42D34AEE18F83AFDEB23";

pub fn verified_forge_key(records: &str) -> bool {
    let primary: Vec<_> = records
        .lines()
        .filter(|line| line.starts_with("pub:"))
        .collect();
    crate::profile_rules::forge_key_accepted(
        primary.len(),
        records
            .lines()
            .find(|line| line.starts_with("fpr:"))
            .and_then(|line| line.split(':').nth(9))
            == Some(FORGE_FINGERPRINT),
    )
}

fn forge_keys(runner: &mut impl Runner) -> Result<()> {
    if !runner.available("gpg") {
        eprintln!("Warning: gpg is required to verify forge signatures.");
        return Ok(());
    }
    if runner
        .run("gpg", &args(&["--list-keys", FORGE_FINGERPRINT]))?
        .success
    {
        return Ok(());
    }
    let scope = tempfile::tempdir()?;
    let key = scope.path().join("web-flow.gpg");
    checked(
        runner,
        "curl",
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
        "gpg",
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
        "gpg",
        &["--quiet".into(), "--import".into(), key.into()],
    )?;
    Ok(())
}

fn fcitx(context: &ContextData, runner: &mut impl Runner) -> Result<()> {
    if !runner.available("im-config") || !runner.available("fcitx5") {
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
        checked(runner, "im-config", &args(&["-n", "fcitx5"]))?;
    }
    if runner.available("fcitx5-remote")
        && runner.run("fcitx5-remote", &args(&["--check"]))?.success
    {
        optional(runner, "fcitx5-remote", &["-r"]);
    }
    Ok(())
}

fn retire_tmux(context: &ContextData, runner: &mut impl Runner) -> Result<()> {
    if runner.available("tmux") && runner.run("tmux", &args(&["list-sessions"]))?.success {
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
    if runner.available("mise") && runner.run("mise", &args(&["current", "tmux"]))?.success {
        checked(runner, "mise", &args(&["uninstall", "--all", "tmux", "-y"]))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
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

    #[test]
    fn a_secondary_fingerprint_or_an_extra_primary_key_cannot_authorize_import() {
        let expected = format!("pub:::::::::\nfpr:::::::::{FORGE_FINGERPRINT}:\n");
        assert!(verified_forge_key(&expected));
        assert!(!verified_forge_key(&format!(
            "pub:::::::::\nfpr:::::::::OTHER:\n{expected}"
        )));
        assert!(!verified_forge_key(&format!("{expected}pub:::::::::\n")));
    }
}
