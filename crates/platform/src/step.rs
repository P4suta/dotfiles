//! The setup steps, and for each one the profiles that run it, what it relies on, when it reruns, and the inputs that rerun it.
//! `cargo xtask gen` writes one launcher per step and OS family from these declarations, numbered in run order.

use crate::order_rules;
use crate::tool::Tool;
use model::profile_rules::Profile;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, clap::ValueEnum)]
pub enum Step {
    Mise,
    Runtime,
    Github,
    Scoop,
    Winget,
    Nix,
    Desktop,
    Tools,
    Wezterm,
    Herdr,
    Integrations,
    PythonStub,
    Keyboard,
    Sshd,
    StorageScout,
    Fcitx5,
    MacosDefaults,
    ForgeKeys,
    Claude,
    Ocaml,
    Haskell,
    Racket,
    Guard,
    Ocomment,
    Domyjob,
    Fleet,
    RetireTmux,
    Upgrade,
    Adapters,
}

/// The OS families whose launchers share one directory under `.chezmoiscripts/`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Family {
    Unix,
    Windows,
}

impl Family {
    pub const ALL: [Self; 2] = [Self::Unix, Self::Windows];

    /// The directory under `.chezmoiscripts/`.
    #[must_use]
    pub const fn directory(self) -> &'static str {
        match self {
            Self::Unix => "unix",
            Self::Windows => "windows",
        }
    }

    /// The extension of the launcher scripts.
    #[must_use]
    pub const fn extension(self) -> &'static str {
        match self {
            Self::Unix => "sh",
            Self::Windows => "ps1",
        }
    }

    /// The profiles whose launchers live in this family's directory.
    #[must_use]
    pub const fn profiles(self) -> &'static [Profile] {
        match self {
            Self::Unix => &[Profile::Mac, Profile::Linux, Profile::Wsl],
            Self::Windows => &[Profile::Windows],
        }
    }
}

/// Whether chezmoi runs a launcher before or after it writes the files.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    Before,
    After,
}

/// When chezmoi runs a launcher again.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Condition {
    /// Every apply, for a step that corrects drift that no input records.
    Always,
    /// When the step's inputs change.
    OnChange,
}

/// How a launcher starts its step.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Launch {
    /// The launcher installs the pinned mise itself, because nothing else exists yet.
    PinnedMise,
    /// The launcher builds the runtime from the checkout with the pinned toolchain through mise.
    SourceRuntime,
    /// The launcher runs the installed runtime.
    InstalledRuntime,
}

const MAC: &[Profile] = &[Profile::Mac];
const LINUX: &[Profile] = &[Profile::Linux];
const WINDOWS: &[Profile] = &[Profile::Windows];
const DESKTOP: &[Profile] = &[Profile::Mac, Profile::Linux];
const EVERY: &[Profile] = &[Profile::Mac, Profile::Linux, Profile::Wsl, Profile::Windows];

impl Step {
    /// Every step in declaration order, which breaks ties in the run order.
    pub const ALL: [Self; 29] = [
        Self::Mise,
        Self::Runtime,
        Self::Github,
        Self::Scoop,
        Self::Winget,
        Self::Nix,
        Self::Desktop,
        Self::Tools,
        Self::Wezterm,
        Self::Herdr,
        Self::Integrations,
        Self::PythonStub,
        Self::Keyboard,
        Self::Sshd,
        Self::StorageScout,
        Self::Fcitx5,
        Self::MacosDefaults,
        Self::ForgeKeys,
        Self::Claude,
        Self::Ocaml,
        Self::Haskell,
        Self::Racket,
        Self::Guard,
        Self::Ocomment,
        Self::Domyjob,
        Self::Fleet,
        Self::RetireTmux,
        Self::Upgrade,
        Self::Adapters,
    ];

    /// The step's name on the command line and in its launcher.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Mise => "mise",
            Self::Runtime => "runtime",
            Self::Github => "github",
            Self::Scoop => "scoop",
            Self::Winget => "winget",
            Self::Nix => "nix",
            Self::Desktop => "desktop",
            Self::Tools => "tools",
            Self::Wezterm => "wezterm",
            Self::Herdr => "herdr",
            Self::Integrations => "integrations",
            Self::PythonStub => "python-stub",
            Self::Keyboard => "keyboard",
            Self::Sshd => "sshd",
            Self::StorageScout => "storage-scout",
            Self::Fcitx5 => "fcitx5",
            Self::MacosDefaults => "macos-defaults",
            Self::ForgeKeys => "forge-keys",
            Self::Claude => "claude",
            Self::Ocaml => "ocaml",
            Self::Haskell => "haskell",
            Self::Racket => "racket",
            Self::Guard => "guard",
            Self::Ocomment => "ocomment",
            Self::Domyjob => "domyjob",
            Self::Fleet => "fleet",
            Self::RetireTmux => "retire-tmux",
            Self::Upgrade => "upgrade",
            Self::Adapters => "adapters",
        }
    }

    /// The profiles whose apply runs this step.
    /// The upgrade timers run `Upgrade`, so no launcher does.
    #[must_use]
    pub const fn profiles(self) -> &'static [Profile] {
        match self {
            Self::Mise | Self::Runtime | Self::Github | Self::Integrations | Self::Adapters => {
                EVERY
            }
            Self::Tools => EVERY,
            Self::Scoop
            | Self::Winget
            | Self::Wezterm
            | Self::Herdr
            | Self::PythonStub
            | Self::Keyboard
            | Self::Sshd
            | Self::StorageScout => WINDOWS,
            Self::Nix | Self::Fcitx5 | Self::RetireTmux => LINUX,
            Self::Desktop | Self::Ocaml => DESKTOP,
            Self::MacosDefaults
            | Self::ForgeKeys
            | Self::Claude
            | Self::Haskell
            | Self::Racket
            | Self::Guard
            | Self::Domyjob
            | Self::Fleet => MAC,
            Self::Ocomment => &[Profile::Linux, Profile::Windows],
            Self::Upgrade => &[],
        }
    }

    /// Whether this step runs on `profile`.
    #[must_use]
    pub fn runs_on(self, profile: Profile) -> bool {
        self.profiles().contains(&profile)
    }

    /// Steps whose effects this step relies on, which must run first when both run in one apply.
    /// Every step the installed runtime launches also relies on `Runtime`, and `Adapters` runs last.
    #[must_use]
    pub fn after(self) -> &'static [Step] {
        match self {
            Self::Runtime | Self::Github => &[Self::Mise],
            Self::Desktop => &[Self::Nix],
            Self::Tools | Self::Upgrade => &[Self::Mise, Self::Nix, Self::Desktop, Self::Scoop],
            Self::Integrations => &[Self::Tools, Self::Herdr],
            Self::Ocomment | Self::Domyjob | Self::Fleet | Self::Ocaml | Self::Haskell => {
                &[Self::Tools]
            }
            Self::Racket => &[Self::Desktop],
            Self::Guard => &[Self::Runtime],
            Self::Adapters => &ALL_BUT_ADAPTERS,
            Self::Mise
            | Self::Scoop
            | Self::Winget
            | Self::Nix
            | Self::Wezterm
            | Self::Herdr
            | Self::PythonStub
            | Self::Keyboard
            | Self::Sshd
            | Self::StorageScout
            | Self::Fcitx5
            | Self::MacosDefaults
            | Self::ForgeKeys
            | Self::Claude
            | Self::RetireTmux => &[],
        }
    }

    /// How this step's launcher starts it.
    #[must_use]
    pub const fn launch(self) -> Launch {
        match self {
            Self::Mise => Launch::PinnedMise,
            Self::Runtime => Launch::SourceRuntime,
            _ => Launch::InstalledRuntime,
        }
    }

    /// Whether chezmoi runs the launcher before or after it writes the files.
    #[must_use]
    pub const fn phase(self) -> Phase {
        match self {
            Self::Mise | Self::Runtime | Self::Github => Phase::Before,
            _ => Phase::After,
        }
    }

    /// When chezmoi runs this step's launcher of `family` again.
    /// A step that corrects drift no input records runs on every apply.
    #[must_use]
    pub const fn condition(self, family: Family) -> Condition {
        match (self, family) {
            (
                Self::Mise
                | Self::Runtime
                | Self::Integrations
                | Self::Winget
                | Self::Wezterm
                | Self::Adapters,
                _,
            )
            | (Self::Ocomment, Family::Windows) => Condition::Always,
            _ => Condition::OnChange,
        }
    }

    /// The template terms whose values rerun this step on `profile`, which the launcher hashes through the `inputs` partial.
    #[must_use]
    pub const fn inputs(self, profile: Profile) -> &'static [&'static str] {
        match (self, profile) {
            (Self::Nix, _) => &[
                "(include \"dot_config/dotfiles-nix/flake.nix.tmpl\")",
                ".platforms.linux.nix",
            ],
            (Self::Desktop, Profile::Mac) => &[
                "(includeTemplate \"dot_config/homebrew/Brewfile.tmpl\" .)",
                ".platforms.mac.brew",
                ".platforms.mac.desktop",
            ],
            (Self::Desktop, _) => &[".platforms.linux.desktop"],
            (Self::Tools, Profile::Mac) => &[
                "(includeTemplate \"native/mise/mac.toml.tmpl\" .)",
                ".platforms.mac.tools",
                ".platforms.mac.mise",
                ".platforms.mac.tool_versions",
            ],
            (Self::Tools, Profile::Linux) => &[
                "(includeTemplate \"native/mise/linux.toml.tmpl\" .)",
                ".platforms.linux.tools",
                ".platforms.linux.mise",
                ".platforms.linux.tool_versions",
            ],
            (Self::Tools, Profile::Wsl) => &[
                "(includeTemplate \"native/mise/wsl.toml.tmpl\" .)",
                ".platforms.wsl.tools",
            ],
            (Self::Tools, Profile::Windows) => &[
                "(includeTemplate \"native/mise/windows.toml.tmpl\" .)",
                ".platforms.windows.tools",
                ".platforms.windows.scoop.apps",
            ],
            (Self::Scoop, _) => &[".platforms.windows.scoop"],
            (Self::Herdr, _) => &[".platforms.windows.herdr"],
            (Self::Keyboard, _) => &[".platforms.windows.keyboard"],
            (Self::Sshd, _) => &[
                ".platforms.windows.sshd",
                ".signing.enabled",
                ".signing.public_key",
            ],
            (Self::StorageScout, _) => {
                &["(includeTemplate \"native/storage-scout/windows.toml.tmpl\" .)"]
            }
            (Self::MacosDefaults, _) => &[".platforms.mac.macos"],
            (Self::Ocaml, _) => &[".ocaml"],
            (Self::Haskell, _) => {
                &["(index .platforms.mac.tool_versions \"haskell-language-server\")"]
            }
            (Self::Racket, _) => &[".platforms.mac.racket"],
            (Self::Ocomment | Self::Domyjob | Self::Fleet, _) => &[".paths.projects"],
            _ => &[],
        }
    }

    /// Packaged tools this step invokes beyond those the host or this repository pins.
    /// Cloning owner sources fires the global Git hooks, which run Lefthook for repositories that configure it.
    /// The Windows integrations generate the shell initialization of Starship and zoxide.
    #[must_use]
    pub fn requires(self, profile: Profile) -> &'static [Tool] {
        match (self, profile) {
            (Self::Domyjob | Self::Fleet, _) | (Self::Ocomment, Profile::Linux) => {
                &[Tool::Lefthook]
            }
            (Self::Integrations, Profile::Windows) => &[Tool::Starship, Tool::Zoxide],
            _ => &[],
        }
    }

    /// The steps this step's launcher in `family` relies on, the declared ones and the runtime that every installed-runtime launcher runs.
    #[must_use]
    pub fn prerequisites(self) -> Vec<Step> {
        let mut steps = self.after().to_vec();
        if self.launch() == Launch::InstalledRuntime && !steps.contains(&Self::Runtime) {
            steps.push(Self::Runtime);
        }
        steps
    }
}

const ALL_BUT_ADAPTERS: [Step; 28] = {
    let mut steps = [Step::Mise; 28];
    let mut index = 0;
    while index < 28 {
        steps[index] = Step::ALL[index];
        index += 1;
    }
    steps
};

/// A dependency that no run order of a family can honor.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Disorder {
    /// The steps' dependencies form a cycle.
    Cycle,
    /// A step that runs before the files relies on one that runs after them.
    Phase(Step, Step),
}

/// The steps that `family` launches, in run order: each after what it relies on, before-phase steps first, ties in declaration order.
///
/// # Errors
///
/// Returns the disorder when the dependencies form a cycle or a before-phase step relies on an after-phase one.
pub fn sequence(family: Family) -> Result<Vec<Step>, Disorder> {
    let steps: Vec<Step> = Step::ALL
        .into_iter()
        .filter(|step| {
            family
                .profiles()
                .iter()
                .any(|profile| step.runs_on(*profile))
        })
        .collect();
    let mut after = Vec::with_capacity(steps.len());
    for step in &steps {
        let mut mask = 0u32;
        for prerequisite in step.prerequisites() {
            if let Some(index) = steps.iter().position(|other| *other == prerequisite) {
                if step.phase() == Phase::Before && prerequisite.phase() == Phase::After {
                    return Err(Disorder::Phase(*step, prerequisite));
                }
                mask |= 1 << index;
            }
        }
        if step.phase() == Phase::After {
            for (index, other) in steps.iter().enumerate() {
                if other.phase() == Phase::Before {
                    mask |= 1 << index;
                }
            }
        }
        after.push(mask);
    }
    let mut order = vec![0usize; steps.len()];
    if !order_rules::order(&after, &mut order) {
        return Err(Disorder::Cycle);
    }
    Ok(order.into_iter().map(|index| steps[index]).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_declaration_lists_every_step_once_with_its_command_line_name() {
        use clap::ValueEnum;
        for (index, step) in Step::ALL.iter().enumerate() {
            assert_eq!(
                Step::ALL.iter().position(|other| other == step),
                Some(index)
            );
            assert_eq!(
                step.to_possible_value()
                    .map(|value| value.get_name().to_owned()),
                Some(step.name().to_owned())
            );
        }
        assert_eq!(Step::value_variants().len(), Step::ALL.len());
    }

    #[test]
    fn mise_comes_first_and_adapters_last_in_every_family() {
        for family in Family::ALL {
            let sequence = sequence(family).unwrap();
            assert_eq!(sequence.first(), Some(&Step::Mise));
            assert_eq!(sequence.get(1), Some(&Step::Runtime));
            assert_eq!(sequence.last(), Some(&Step::Adapters));
            let position = |step| sequence.iter().position(|other| *other == step);
            assert!(position(Step::Github) > position(Step::Mise));
            assert!(position(Step::Tools) > position(Step::Desktop).or(Some(0)));
        }
    }

    #[test]
    fn the_unix_order_installs_nix_before_the_tools_that_rely_on_it() {
        let sequence = sequence(Family::Unix).unwrap();
        let position = |step| sequence.iter().position(|other| *other == step).unwrap();
        assert!(position(Step::Nix) < position(Step::Tools));
        assert!(position(Step::Tools) < position(Step::Integrations));
        assert!(!sequence.contains(&Step::Upgrade));
        assert!(!sequence.contains(&Step::Scoop));
    }

    #[test]
    fn only_the_steps_a_profile_runs_launch_in_its_family() {
        let windows = sequence(Family::Windows).unwrap();
        assert!(windows.contains(&Step::Scoop));
        assert!(!windows.contains(&Step::Nix));
        assert!(windows.iter().all(|step| step.runs_on(Profile::Windows)));
    }
}
