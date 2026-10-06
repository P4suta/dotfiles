//! The setup steps a launcher runs, and what each relies on.

use crate::tool::Tool;
use model::profile_rules::Profile;

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

impl Step {
    /// Steps whose effects this step relies on, which must run first when both run in one apply.
    #[must_use]
    pub fn after(self) -> &'static [Step] {
        match self {
            Self::Ocomment
            | Self::Domyjob
            | Self::Fleet
            | Self::Ocaml
            | Self::Haskell
            | Self::Integrations => &[Self::Tools],
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

    /// Packaged tools this step invokes beyond those the host or this repository pins.
    /// Cloning owner sources fires the global Git hooks, which run Lefthook for repositories that configure it.
    /// The Windows integrations generate the shell initialization of Starship and zoxide.
    #[must_use]
    pub fn requires(self, profile: Profile) -> &'static [Tool] {
        match self {
            Self::Ocomment | Self::Domyjob | Self::Fleet => &[Tool::Lefthook],
            Self::Integrations if profile == Profile::Windows => &[Tool::Starship, Tool::Zoxide],
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
