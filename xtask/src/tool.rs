//! Every external program xtask runs, with where each host gets it.
//!
//! Spawning a process anywhere else is a lint error (see `clippy.toml`), so a new dependency on a host tool cannot appear without a declared provision.

use crate::profile_rules::Profile;
use crate::setup::Step;
use std::process::Command;

/// How a host obtains a program before something runs it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Provision {
    /// Pinned in this repository's `mise.toml` under this key, so checks and CI resolve the same version.
    Pinned(&'static str),
    /// Part of the operating system or its base developer tools wherever the calling code runs.
    Host,
    /// Installed by this setup step before any later step relies on it.
    Setup(Step),
    /// An application the owner installs outside this repository; callers probe for it before use.
    Owner,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Tool {
    AnsibleGalaxy,
    AnsiblePlaybook,
    Atuin,
    Bash,
    Brew,
    Bun,
    Cabal,
    Carapace,
    Cargo,
    Chezmoi,
    Claude,
    Code,
    Codex,
    Curl,
    CurlExe,
    Defaults,
    DesktopFileValidate,
    Docker,
    Domyjob,
    Doppler,
    Dotctl,
    Dotguard,
    Env,
    Erl,
    FcCache,
    Fcitx5,
    Fcitx5Remote,
    Fleet,
    Gh,
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
    Sh,
    Shellcheck,
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
    Unzip,
    UpdateDesktopDatabase,
    Vivid,
    Winget,
    Wslpath,
    XdgMime,
    Zoxide,
    Zsh,
}

impl Tool {
    pub const ALL: [Self; 81] = [
        Self::AnsibleGalaxy,
        Self::AnsiblePlaybook,
        Self::Atuin,
        Self::Bash,
        Self::Brew,
        Self::Bun,
        Self::Cabal,
        Self::Carapace,
        Self::Cargo,
        Self::Chezmoi,
        Self::Claude,
        Self::Code,
        Self::Codex,
        Self::Curl,
        Self::CurlExe,
        Self::Defaults,
        Self::DesktopFileValidate,
        Self::Docker,
        Self::Domyjob,
        Self::Doppler,
        Self::Dotctl,
        Self::Dotguard,
        Self::Env,
        Self::Erl,
        Self::FcCache,
        Self::Fcitx5,
        Self::Fcitx5Remote,
        Self::Fleet,
        Self::Gh,
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
        Self::Sh,
        Self::Shellcheck,
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
        Self::Unzip,
        Self::UpdateDesktopDatabase,
        Self::Vivid,
        Self::Winget,
        Self::Wslpath,
        Self::XdgMime,
        Self::Zoxide,
        Self::Zsh,
    ];

    /// The program name, or the absolute path where the provider installs it outside PATH.
    pub fn program(self) -> &'static str {
        match self {
            Self::AnsibleGalaxy => "ansible-galaxy",
            Self::AnsiblePlaybook => "ansible-playbook",
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
            Self::Dotctl => "dotctl",
            Self::Dotguard => "dotguard",
            Self::FcCache => "fc-cache",
            Self::Fcitx5 => "fcitx5",
            Self::Fcitx5Remote => "fcitx5-remote",
            Self::Gh => "gh",
            Self::Ghostty => "ghostty",
            Self::Git => "git",
            Self::Gitleaks => "gitleaks",
            Self::Gpg => "gpg",
            Self::Gsettings => "gsettings",
            Self::Herdr => "herdr",
            Self::Hlint => "hlint",
            Self::Id => "id",
            Self::ImConfig => "im-config",
            Self::Kani => "kani",
            Self::Mise => "mise",
            Self::Nix => "/nix/var/nix/profiles/default/bin/nix",
            Self::Nu => "nu",
            Self::OnePassword => "1password",
            Self::Opam => "opam",
            Self::Pwsh => "pwsh",
            Self::Raco => "raco",
            Self::Sh => "sh",
            Self::Shellcheck => "shellcheck",
            Self::SshKeygen => {
                if cfg!(windows) {
                    "ssh-keygen.exe"
                } else {
                    "ssh-keygen"
                }
            }
            Self::Sudo => "sudo",
            Self::Systemctl => "systemctl",
            Self::Taplo => "taplo",
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
            Self::Bash => "/bin/bash",
            Self::Carapace => "carapace",
            Self::Code => "code",
            Self::Docker => "docker",
            Self::Domyjob => "domyjob",
            Self::Fleet => "fleet",
            Self::Nohup => "nohup",
            Self::Ocomment => "ocomment",
            Self::Op => "op",
            Self::Opencode => "opencode",
            Self::Starship => "starship",
            Self::Tailscale => "tailscale",
            Self::Television => "tv",
            Self::Vivid => "vivid",
            Self::Zoxide => "zoxide",
            Self::Zsh => "/bin/zsh",
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
        }
    }

    pub fn provision(self) -> Provision {
        match self {
            Self::AnsibleGalaxy | Self::AnsiblePlaybook => Provision::Pinned("pypi:ansible-core"),
            Self::Bun => Provision::Pinned("bun"),
            Self::Cargo => Provision::Pinned("rust"),
            Self::Chezmoi => Provision::Pinned("chezmoi"),
            Self::Gitleaks => Provision::Pinned("gitleaks"),
            Self::Kani => Provision::Pinned("cargo:kani-verifier"),
            Self::Nu => Provision::Pinned("nu"),
            Self::Pwsh => Provision::Pinned("powershell"),
            Self::Shellcheck => Provision::Pinned("shellcheck"),
            Self::Taplo => Provision::Pinned("taplo"),
            Self::Curl
            | Self::Defaults
            | Self::Git
            | Self::Id
            | Self::Sh
            | Self::SshKeygen
            | Self::Sudo
            | Self::Systemctl
            | Self::Wslpath => Provision::Host,
            Self::CurlExe => Provision::Host,
            Self::Env => Provision::Host,
            Self::GnomeTerminal => Provision::Owner,
            Self::HerdrAgent => Provision::Setup(Step::Runtime),
            Self::Kitty => Provision::Owner,
            Self::Launchctl => Provision::Host,
            Self::Lefthook => Provision::Setup(Step::Tools),
            Self::TarExe => Provision::Host,
            Self::Unzip => Provision::Host,
            Self::UpdateDesktopDatabase => Provision::Owner,
            Self::Winget => Provision::Host,
            Self::Bash => Provision::Host,
            Self::Carapace => Provision::Setup(Step::Tools),
            Self::Code => Provision::Owner,
            Self::Docker => Provision::Owner,
            Self::Domyjob => Provision::Setup(Step::Domyjob),
            Self::Fleet => Provision::Setup(Step::Fleet),
            Self::Nohup => Provision::Host,
            Self::Ocomment => Provision::Setup(Step::Ocomment),
            Self::Op => Provision::Owner,
            Self::Opencode => Provision::Setup(Step::Tools),
            Self::Starship => Provision::Setup(Step::Tools),
            Self::Tailscale => Provision::Owner,
            Self::Television => Provision::Setup(Step::Tools),
            Self::Vivid => Provision::Setup(Step::Tools),
            Self::Zoxide => Provision::Setup(Step::Tools),
            Self::Zsh => Provision::Host,
            Self::Erl => Provision::Setup(Step::Tools),
            Self::Gleam => Provision::Setup(Step::Tools),
            Self::Go => Provision::Setup(Step::Tools),
            Self::Java => Provision::Setup(Step::Tools),
            Self::Just => Provision::Setup(Step::Tools),
            Self::Ocaml => Provision::Setup(Step::Ocaml),
            Self::Rustc => Provision::Setup(Step::Tools),
            Self::Mise => Provision::Setup(Step::Mise),
            Self::Gh => Provision::Setup(Step::Github),
            Self::Claude => Provision::Setup(Step::Claude),
            Self::Dotctl => Provision::Setup(Step::Runtime),
            Self::Dotguard => Provision::Setup(Step::Guard),
            Self::Atuin | Self::Cabal | Self::Doppler | Self::Hlint | Self::Opam | Self::Tldr => {
                Provision::Setup(Step::Tools)
            }
            Self::Brew
            | Self::Codex
            | Self::DesktopFileValidate
            | Self::FcCache
            | Self::Fcitx5
            | Self::Fcitx5Remote
            | Self::Ghostty
            | Self::Gpg
            | Self::Gsettings
            | Self::Herdr
            | Self::ImConfig
            | Self::Nix
            | Self::OnePassword
            | Self::Raco
            | Self::Tmux
            | Self::XdgMime => Provision::Owner,
        }
    }

    /// The package list a profile installs this tool from, as a platform data pointer and entry, for tools a setup step or an installed entry point requires.
    pub fn package(self, profile: Profile) -> Option<(&'static str, &'static str)> {
        let pointer = match profile {
            Profile::Mac => "/brew/formulae",
            Profile::Linux => "/nix/packages",
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

    /// The only place xtask creates a process for a named program.
    #[allow(clippy::disallowed_methods)]
    pub fn command(self) -> Command {
        Command::new(self.program())
    }
}

/// A program chosen at run time rather than from the registry: a consumer command the owner passes through, or a verified vendor installer.
#[allow(clippy::disallowed_methods)]
pub fn external(program: impl AsRef<std::ffi::OsStr>) -> Command {
    Command::new(program)
}
