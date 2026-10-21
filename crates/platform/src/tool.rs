//! Every external program the dotfiles crates run, with where each host gets it.

use crate::step::Step;
use model::profile_rules::Profile;
use std::path::PathBuf;
use std::process::Command;

/// The Nix installer's default profile, which a login shell may not have on its `PATH` yet.
const NIX: &str = "/nix/var/nix/profiles/default/bin/nix";

/// Bash at its fixed system path, so a user's `PATH` can't substitute another.
const BASH: &str = "/bin/bash";
const OCOMMENT_PIN: &str = "cargo:https://github.com/P4suta/OComment";
const NJUTEST_PIN: &str = "cargo:https://github.com/P4suta/njutest";
const NJUTEST_PROVISION: Provision = Provision::PinnedEnvironment {
    environment: "njutest",
    key: NJUTEST_PIN,
};
#[cfg(windows)]
const WINDOWS_ARCHIVE: &str = "System32/tar.exe";
#[cfg(target_os = "linux")]
const ZSH: &str = "zsh";
#[cfg(not(target_os = "linux"))]
const ZSH: &str = "/bin/zsh";

/// The JSON pointer of the Nix package list in the chezmoi data.
pub const NIX_PACKAGES: &str = "/nix/packages";

/// How a host obtains a program before something runs it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Provision {
    /// Pinned in this repository's `mise.toml` under this key, so checks and CI resolve the same version.
    Pinned(&'static str),
    /// An exact tool pin loaded only for its selected mise environment.
    PinnedEnvironment {
        environment: &'static str,
        key: &'static str,
    },
    /// Part of the operating system or its base developer tools wherever the calling code runs.
    Host,
    /// Installed by this setup step before any later step relies on it.
    Setup(Step),
    /// A program the owner installs outside this repository, which callers probe for before use.
    Owner,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Tool {
    Actionlint,
    AnsibleGalaxy,
    AnsiblePlaybook,
    AstGrep,
    Atuin,
    Bash,
    Brew,
    Bun,
    Cabal,
    Carapace,
    Cargo,
    CargoDeny,
    Chezmoi,
    Claude,
    Code,
    Codex,
    Curl,
    CurlExe,
    Defaults,
    DesktopFileValidate,
    Docker,
    DockerBuildx,
    Domyjob,
    Doppler,
    Dotguard,
    Env,
    Erl,
    FcCache,
    Fcitx5,
    Fcitx5Remote,
    Fleet,
    Fzf,
    Gh,
    GhStack,
    Ghostty,
    Git,
    Gitleaks,
    Gleam,
    GnomeTerminal,
    Go,
    Gpg,
    Gsettings,
    Herdr,
    HerdrAgent,
    Hlint,
    Icacls,
    Id,
    ImConfig,
    Java,
    Just,
    Kani,
    Kitty,
    Launchctl,
    Lefthook,
    Mise,
    Nix,
    Njutest,
    Nohup,
    Nu,
    Ocaml,
    Ocomment,
    OnePassword,
    Op,
    Opam,
    Opencode,
    Pwsh,
    Raco,
    Rustc,
    Rustfmt,
    Rustup,
    Sh,
    Shellcheck,
    Ssh,
    SshKeygen,
    Starship,
    Sudo,
    Systemctl,
    Tailscale,
    Taplo,
    TarExe,
    Television,
    Tldr,
    Tmux,
    Typos,
    Unzip,
    UpdateDesktopDatabase,
    Vale,
    Vivid,
    Winget,
    Whoami,
    Wslpath,
    XdgMime,
    Yamllint,
    Zizmor,
    Zoxide,
    Zsh,
}

impl Tool {
    pub const ALL: [Self; 96] = [
        Self::Actionlint,
        Self::AnsibleGalaxy,
        Self::AnsiblePlaybook,
        Self::AstGrep,
        Self::Atuin,
        Self::Bash,
        Self::Brew,
        Self::Bun,
        Self::Cabal,
        Self::Carapace,
        Self::Cargo,
        Self::CargoDeny,
        Self::Chezmoi,
        Self::Claude,
        Self::Code,
        Self::Codex,
        Self::Curl,
        Self::CurlExe,
        Self::Defaults,
        Self::DesktopFileValidate,
        Self::Docker,
        Self::DockerBuildx,
        Self::Domyjob,
        Self::Doppler,
        Self::Dotguard,
        Self::Env,
        Self::Erl,
        Self::FcCache,
        Self::Fcitx5,
        Self::Fcitx5Remote,
        Self::Fleet,
        Self::Fzf,
        Self::Gh,
        Self::GhStack,
        Self::Ghostty,
        Self::Git,
        Self::Gitleaks,
        Self::Gleam,
        Self::GnomeTerminal,
        Self::Go,
        Self::Gpg,
        Self::Gsettings,
        Self::Herdr,
        Self::HerdrAgent,
        Self::Hlint,
        Self::Icacls,
        Self::Id,
        Self::ImConfig,
        Self::Java,
        Self::Just,
        Self::Kani,
        Self::Kitty,
        Self::Launchctl,
        Self::Lefthook,
        Self::Mise,
        Self::Nix,
        Self::Njutest,
        Self::Nohup,
        Self::Nu,
        Self::Ocaml,
        Self::Ocomment,
        Self::OnePassword,
        Self::Op,
        Self::Opam,
        Self::Opencode,
        Self::Pwsh,
        Self::Raco,
        Self::Rustc,
        Self::Rustfmt,
        Self::Rustup,
        Self::Sh,
        Self::Shellcheck,
        Self::Ssh,
        Self::SshKeygen,
        Self::Starship,
        Self::Sudo,
        Self::Systemctl,
        Self::Tailscale,
        Self::Taplo,
        Self::TarExe,
        Self::Television,
        Self::Tldr,
        Self::Tmux,
        Self::Typos,
        Self::Unzip,
        Self::UpdateDesktopDatabase,
        Self::Vale,
        Self::Vivid,
        Self::Winget,
        Self::Whoami,
        Self::Wslpath,
        Self::XdgMime,
        Self::Yamllint,
        Self::Zizmor,
        Self::Zoxide,
        Self::Zsh,
    ];

    /// The program name, or the absolute path where the provider installs it outside PATH.
    #[must_use]
    pub fn program(self) -> &'static str {
        match self {
            Self::Actionlint => "actionlint",
            Self::CargoDeny => "cargo-deny",
            Self::Zizmor => "zizmor",
            Self::AnsibleGalaxy => "ansible-galaxy",
            Self::AnsiblePlaybook => "ansible-playbook",
            Self::AstGrep => "ast-grep",
            Self::Atuin => "atuin",
            Self::Brew => "brew",
            Self::Bun => "bun",
            Self::Cabal => "cabal",
            Self::Cargo => "cargo",
            Self::Chezmoi => "chezmoi",
            Self::Claude => "claude",
            Self::Codex => "codex",
            Self::Curl => "curl",
            Self::Defaults => "defaults",
            Self::DesktopFileValidate => "desktop-file-validate",
            Self::Doppler => "doppler",
            Self::Dotguard => "dotguard",
            Self::FcCache => "fc-cache",
            Self::Fcitx5 => "fcitx5",
            Self::Fcitx5Remote => "fcitx5-remote",
            Self::Fzf => "fzf",
            Self::Gh => "gh",
            Self::GhStack => "gh-stack",
            Self::Ghostty => "ghostty",
            Self::Git => "git",
            Self::Gitleaks => "gitleaks",
            Self::Gpg => "gpg",
            Self::Gsettings => "gsettings",
            Self::Herdr => "herdr",
            Self::Hlint => "hlint",
            Self::Icacls => "icacls.exe",
            Self::Id => "id",
            Self::ImConfig => "im-config",
            Self::Kani => "kani",
            Self::Mise => "mise",
            Self::Nix => NIX,
            Self::Njutest => "njutest",
            Self::Nu => "nu",
            Self::OnePassword => "1password",
            Self::Opam => "opam",
            Self::Pwsh => "pwsh",
            Self::Raco => "raco",
            Self::Sh => "sh",
            Self::Shellcheck => "shellcheck",
            Self::Ssh if cfg!(windows) => "ssh.exe",
            Self::Ssh => "ssh",
            Self::SshKeygen if cfg!(windows) => "ssh-keygen.exe",
            Self::SshKeygen => "ssh-keygen",
            Self::Sudo => "sudo",
            Self::Systemctl => "systemctl",
            Self::Taplo => "taplo",
            Self::Typos => "typos",
            Self::Yamllint => "yamllint",
            Self::Rustfmt => "rustfmt",
            Self::Rustup => "rustup",
            Self::Tldr => "tldr",
            Self::Tmux => "tmux",
            Self::Wslpath => "wslpath",
            Self::XdgMime => "xdg-mime",
            Self::Erl => "erl",
            Self::Gleam => "gleam",
            Self::Go => "go",
            Self::Java => "java",
            Self::Just => "just",
            Self::Ocaml => "ocaml",
            Self::Rustc => "rustc",
            Self::Bash => BASH,
            Self::Carapace => "carapace",
            Self::Code => "code",
            Self::Docker => "docker",
            Self::DockerBuildx => "docker-cli-plugin-docker-buildx",
            Self::Domyjob => "domyjob",
            Self::Fleet => "fleet",
            Self::Nohup => "nohup",
            Self::Ocomment => "ocomment",
            Self::Op => "op",
            Self::Opencode => "opencode",
            Self::Starship => "starship",
            Self::Tailscale => "tailscale",
            Self::Television => "tv",
            Self::Vale => "vale",
            Self::Vivid => "vivid",
            Self::Zoxide => "zoxide",
            Self::Zsh => ZSH,
            Self::CurlExe => "curl.exe",
            Self::Env => "env",
            Self::GnomeTerminal => "gnome-terminal",
            Self::HerdrAgent => "herdr-agent",
            Self::Kitty => "kitty",
            Self::Launchctl => "launchctl",
            Self::Lefthook => "lefthook",
            Self::TarExe => "tar.exe",
            Self::Unzip => "unzip",
            Self::UpdateDesktopDatabase => "update-desktop-database",
            Self::Winget => "winget",
            Self::Whoami => "whoami.exe",
        }
    }

    #[must_use]
    pub fn provision(self) -> Provision {
        match self {
            Self::AnsibleGalaxy | Self::AnsiblePlaybook => Provision::Pinned("pypi:ansible-core"),
            Self::Actionlint => Provision::Pinned("actionlint"),
            Self::AstGrep => Provision::Pinned("ast-grep"),
            Self::CargoDeny => Provision::Pinned("cargo-deny"),
            Self::Zizmor => Provision::Pinned("zizmor"),
            Self::Bun => Provision::Pinned("bun"),
            Self::Cargo | Self::Rustfmt => Provision::Pinned("rust"),
            Self::Typos => Provision::Pinned("typos"),
            Self::Yamllint => Provision::Pinned("yamllint"),
            Self::Chezmoi => Provision::Pinned("chezmoi"),
            Self::GhStack => Provision::Pinned("github:github/gh-stack"),
            Self::Ocomment => Provision::Pinned(OCOMMENT_PIN),
            Self::Njutest => NJUTEST_PROVISION,
            Self::Gitleaks => Provision::Pinned("gitleaks"),
            Self::Gh => Provision::Pinned("gh"),
            Self::Docker => Provision::Pinned("docker-cli"),
            Self::DockerBuildx => Provision::Pinned("aqua:docker/buildx"),
            Self::Kani => Provision::Pinned("cargo:kani-verifier"),
            Self::Nu => Provision::Pinned("nu"),
            Self::Pwsh => Provision::Pinned("powershell"),
            Self::Shellcheck => Provision::Pinned("shellcheck"),
            Self::Taplo => Provision::Pinned("taplo"),
            Self::Vale => Provision::Pinned("vale"),
            Self::Zsh if cfg!(target_os = "linux") => Provision::Pinned("conda:zsh"),
            Self::Zsh
            | Self::Bash
            | Self::Curl
            | Self::CurlExe
            | Self::Defaults
            | Self::Env
            | Self::Git
            | Self::Icacls
            | Self::Id
            | Self::Launchctl
            | Self::Nohup
            | Self::Sh
            | Self::Ssh
            | Self::SshKeygen
            | Self::Sudo
            | Self::Systemctl
            | Self::TarExe
            | Self::Unzip
            | Self::Winget
            | Self::Whoami
            | Self::Wslpath => Provision::Host,
            Self::Atuin
            | Self::Cabal
            | Self::Carapace
            | Self::Doppler
            | Self::Fzf
            | Self::Erl
            | Self::Gleam
            | Self::Go
            | Self::Hlint
            | Self::Java
            | Self::Just
            | Self::Lefthook
            | Self::Opam
            | Self::Opencode
            | Self::Rustc
            | Self::Rustup
            | Self::Starship
            | Self::Television
            | Self::Tldr
            | Self::Vivid
            | Self::Zoxide => Provision::Setup(Step::Tools),
            Self::HerdrAgent => Provision::Setup(Step::Runtime),
            Self::Claude => Provision::Setup(Step::Claude),
            Self::Domyjob => Provision::Setup(Step::Domyjob),
            Self::Dotguard => Provision::Setup(Step::Guard),
            Self::Fleet => Provision::Setup(Step::Fleet),
            Self::Mise => Provision::Setup(Step::Mise),
            Self::Ocaml => Provision::Setup(Step::Ocaml),
            Self::Brew
            | Self::Code
            | Self::Codex
            | Self::DesktopFileValidate
            | Self::FcCache
            | Self::Fcitx5
            | Self::Fcitx5Remote
            | Self::Ghostty
            | Self::GnomeTerminal
            | Self::Gpg
            | Self::Gsettings
            | Self::Herdr
            | Self::ImConfig
            | Self::Kitty
            | Self::Nix
            | Self::OnePassword
            | Self::Op
            | Self::Raco
            | Self::Tailscale
            | Self::Tmux
            | Self::UpdateDesktopDatabase
            | Self::XdgMime => Provision::Owner,
        }
    }

    /// The package list entry that installs this tool.
    #[must_use]
    pub fn package(self, profile: Profile) -> Option<(&'static str, &'static str)> {
        let pointer = match profile {
            Profile::Mac => "/brew/formulae",
            Profile::Linux => NIX_PACKAGES,
            Profile::Windows => "/scoop/apps",
            Profile::Wsl => return None,
        };
        match self {
            Self::Lefthook => Some((pointer, "lefthook")),
            Self::Starship => Some((pointer, "starship")),
            Self::Zoxide => Some((pointer, "zoxide")),
            // mise already installs the Doppler CLI from its release with a checksum on the Mac, and runs the same on Windows.
            Self::Doppler => Some(match profile {
                Profile::Linux => (pointer, "doppler"),
                _ => ("/tools/common", "doppler"),
            }),
            _ => None,
        }
    }

    /// A Git hook exports its repository through these variables, and a check's fixture repositories would otherwise write to the hook's real index.
    #[must_use]
    pub fn command(self) -> Command {
        let mut command = crate::process::command(self.executable());
        if let Provision::PinnedEnvironment { environment, .. } = self.provision() {
            command.env("MISE_ENV", environment);
        }
        command
    }

    #[must_use]
    pub fn executable(self) -> PathBuf {
        #[cfg(windows)]
        if matches!(self, Self::TarExe | Self::Icacls | Self::Whoami) {
            return std::env::var_os("SystemRoot")
                .map(PathBuf::from)
                .filter(|root| root.is_absolute())
                .unwrap_or_else(|| crate::home::DEFAULT_SYSTEM_ROOT.into())
                .join(if self == Self::TarExe {
                    WINDOWS_ARCHIVE.into()
                } else {
                    PathBuf::from("System32").join(self.program())
                });
        }
        self.program().into()
    }

    /// A Doppler command with the user's absolute home as its authentication scope.
    /// Returns `None` when the environment names no absolute home.
    #[must_use]
    pub fn doppler_for_user() -> Option<Command> {
        let home = crate::home::home().filter(|home| home.is_absolute())?;
        let mut command = Self::Doppler.command();
        command.arg("--scope").arg(home);
        Some(command)
    }

    /// A chezmoi run that reads the invoking checkout.
    /// The default source names a bare repository whose head lags, so no installer may fall back to it.
    #[must_use]
    pub fn chezmoi_in(root: &std::path::Path) -> Command {
        let mut command = Self::Chezmoi.command();
        command.arg("--source").arg(root);
        command
    }

    /// Reads the effective machine-local configuration without overriding its source.
    #[must_use]
    pub fn chezmoi_configuration(config: &std::path::Path) -> Command {
        let mut command = Self::Chezmoi.command();
        command
            .arg("--config")
            .arg(config)
            .args(["--no-tty", "dump-config", "--format", "json"]);
        command
    }

    /// A process for this program that keeps a Git hook's repository variables.
    #[must_use]
    pub fn hook_command(self) -> Command {
        crate::process::repository_command(self.executable())
    }
}

/// The repository-local variables that `git rev-parse --local-env-vars` lists.
pub use crate::process::GIT_LOCAL_ENVIRONMENT;

/// A program chosen at run time rather than from the registry: a consumer command the owner passes through, or a verified vendor installer.
#[must_use]
pub fn external(program: impl AsRef<std::ffi::OsStr>) -> Command {
    crate::process::command(program)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn optional_tool_selects_its_named_environment_from_any_directory() {
        let Provision::PinnedEnvironment { environment, key } = Tool::Njutest.provision() else {
            panic!("assessment tool requires its optional provider")
        };
        assert_eq!(key, NJUTEST_PIN);
        assert_eq!(environment, "njutest");
        assert!(
            Tool::Njutest
                .command()
                .get_envs()
                .any(|(name, value)| name == "MISE_ENV"
                    && value == Some(std::ffi::OsStr::new(environment)))
        );
        assert!(
            !Tool::Cargo
                .command()
                .get_envs()
                .any(|(name, _)| name == "MISE_ENV")
        );
    }
}
