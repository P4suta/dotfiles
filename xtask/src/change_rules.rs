/// The kind of change a path makes; each class selects the gates that verify it.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Class {
    Prose,
    Skills,
    Rust,
    Profiles,
    /// CI, tooling, lock files, the classifier itself, and every path no rule names.
    Everything,
}

/// A group of checks; CI jobs and local hooks run exactly the gates a change selects.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Gate {
    Prose,
    Skills,
    Rust,
    Profiles,
}

pub const GATES: [Gate; 4] = [Gate::Prose, Gate::Skills, Gate::Rust, Gate::Profiles];

impl Gate {
    pub const fn name(self) -> &'static str {
        match self {
            Self::Prose => "prose",
            Self::Skills => "skills",
            Self::Rust => "rust",
            Self::Profiles => "profiles",
        }
    }

    const fn bit(self) -> u8 {
        1 << self as u8
    }
}

/// A set of gates.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Gates(u8);

impl Gates {
    pub const NONE: Self = Self(0);
    pub const ALL: Self = Self(0b1111);

    pub const fn of(gate: Gate) -> Self {
        Self(gate.bit())
    }

    pub const fn with(self, gate: Gate) -> Self {
        Self(self.0 | gate.bit())
    }

    pub const fn union(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }

    pub const fn contains(self, gate: Gate) -> bool {
        self.0 & gate.bit() != 0
    }

    pub const fn is_empty(self) -> bool {
        self.0 == 0
    }

    pub const fn covers(self, other: Self) -> bool {
        self.0 & other.0 == other.0
    }

    /// The names in declaration order, comma-separated, as `parse` reads them back.
    pub fn names(self) -> String {
        GATES
            .iter()
            .copied()
            .filter(|gate| self.contains(*gate))
            .map(Gate::name)
            .collect::<Vec<_>>()
            .join(",")
    }

    pub fn parse(text: &str) -> Result<Self, String> {
        text.split(',')
            .filter(|name| !name.is_empty())
            .try_fold(Self::NONE, |gates, name| {
                GATES
                    .iter()
                    .copied()
                    .find(|gate| gate.name() == name)
                    .map(|gate| gates.with(gate))
                    .ok_or_else(|| format!("unknown gate: {name}"))
            })
    }
}

/// The gates a class selects: prose checks run for every change, and wider classes add the checks their content can break.
/// Skills deploy through the native profile, so they also select its rehearsal; the xtask crates validate and apply everything, so Rust selects every gate.
pub const fn gates(class: Class) -> Gates {
    let prose = Gates::of(Gate::Prose);
    match class {
        Class::Prose => prose,
        Class::Skills => prose.with(Gate::Skills).with(Gate::Profiles),
        Class::Profiles => prose.with(Gate::Profiles),
        Class::Rust | Class::Everything => Gates::ALL,
    }
}

#[derive(Clone, Copy)]
enum Match {
    Exact,
    Prefix,
    Suffix,
}

/// The first matching rule decides, so a narrower rule precedes the broader one it overrides.
const RULES: [(Match, &[u8], Class); 36] = [
    (Match::Prefix, b".github/", Class::Everything),
    (Match::Prefix, b"xtask/proofs/", Class::Everything),
    (Match::Prefix, b"xtask/src/change", Class::Everything),
    (Match::Prefix, b"dot_config/lefthook/", Class::Everything),
    (Match::Exact, b"mise.toml", Class::Everything),
    (Match::Exact, b"justfile", Class::Everything),
    (Match::Exact, b"lefthook.yml", Class::Everything),
    (Match::Exact, b"package.json", Class::Everything),
    (Match::Exact, b"tsconfig.json", Class::Everything),
    (Match::Exact, b".gitattributes", Class::Everything),
    (Match::Exact, b".gitignore", Class::Everything),
    (Match::Exact, b".typos.toml", Class::Everything),
    (Match::Exact, b".yamllint", Class::Everything),
    (Match::Exact, b".ocomment.toml", Class::Everything),
    (Match::Exact, b".chezmoiignore", Class::Everything),
    (Match::Suffix, b".lock", Class::Everything),
    (Match::Suffix, b"Cargo.toml", Class::Everything),
    (Match::Suffix, b".rs", Class::Rust),
    (Match::Prefix, b"xtask/", Class::Rust),
    (Match::Prefix, b"guard/", Class::Rust),
    (Match::Prefix, b"tools/", Class::Rust),
    (Match::Prefix, b"dot_agents/skills/", Class::Skills),
    (Match::Prefix, b"dot_claude/skills/", Class::Skills),
    (Match::Prefix, b"docs/skills/", Class::Skills),
    (Match::Prefix, b"dot_config/skill-ops/", Class::Skills),
    (Match::Prefix, b"dot_", Class::Profiles),
    (Match::Prefix, b"run_", Class::Profiles),
    (Match::Prefix, b"private_", Class::Profiles),
    (Match::Prefix, b"AppData/", Class::Profiles),
    (Match::Prefix, b"Library/", Class::Profiles),
    (Match::Prefix, b".chezmoi", Class::Profiles),
    (Match::Prefix, b"ops/", Class::Profiles),
    (Match::Prefix, b"policy/", Class::Profiles),
    (Match::Prefix, b"examples/", Class::Profiles),
    (Match::Prefix, b"docs/", Class::Prose),
    (Match::Suffix, b".md", Class::Prose),
];

fn has_prefix(path: &[u8], prefix: &[u8]) -> bool {
    if prefix.len() > path.len() {
        return false;
    }
    let mut index = 0;
    while index < prefix.len() {
        if path[index] != prefix[index] {
            return false;
        }
        index += 1;
    }
    true
}

fn has_suffix(path: &[u8], suffix: &[u8]) -> bool {
    if suffix.len() > path.len() {
        return false;
    }
    let start = path.len() - suffix.len();
    let mut index = 0;
    while index < suffix.len() {
        if path[start + index] != suffix[index] {
            return false;
        }
        index += 1;
    }
    true
}

/// The class a rule assigns to the path, or `None` when no rule names it.
pub fn matched(path: &[u8]) -> Option<Class> {
    let mut index = 0;
    while index < RULES.len() {
        let (kind, text, class) = RULES[index];
        let hit = match kind {
            Match::Exact => path == text,
            Match::Prefix => has_prefix(path, text),
            Match::Suffix => has_suffix(path, text),
        };
        if hit {
            return Some(class);
        }
        index += 1;
    }
    None
}

/// A path no rule names might be anything, so it selects every gate.
pub const fn resolve(found: Option<Class>) -> Class {
    match found {
        Some(class) => class,
        None => Class::Everything,
    }
}

pub fn classify(path: &[u8]) -> Class {
    resolve(matched(path))
}

/// The gates a change set selects: the union over its paths, and none for an empty change.
pub fn select<'a>(paths: impl IntoIterator<Item = &'a str>) -> Gates {
    paths.into_iter().fold(Gates::NONE, |selected, path| {
        selected.union(gates(classify(path.as_bytes())))
    })
}

#[cfg(kani)]
#[kani::proof]
#[kani::unwind(40)]
fn every_path_selects_a_gate_and_unknown_paths_select_all() {
    let bytes: [u8; 24] = kani::any();
    let length: usize = kani::any();
    kani::assume(length <= 24);
    let path = &bytes[..length];
    let found = matched(path);
    let selected = gates(resolve(found));
    assert!(!selected.is_empty());
    if found.is_none() {
        assert_eq!(selected, Gates::ALL);
    }
    if found.is_some() {
        assert!(selected.contains(Gate::Prose));
    }
    kani::cover!(found.is_none() && selected == Gates::ALL);
    kani::cover!(found == Some(Class::Prose));
    kani::cover!(found == Some(Class::Rust));
    kani::cover!(found.is_some() && selected != Gates::ALL);
}

#[cfg(kani)]
#[kani::proof]
#[kani::unwind(40)]
fn ci_tooling_lock_files_and_the_classifier_select_everything() {
    let bytes: [u8; 24] = kani::any();
    let length: usize = kani::any();
    kani::assume(length <= 24);
    let path = &bytes[..length];
    let infrastructure = has_prefix(path, b".github/")
        || has_prefix(path, b"xtask/src/change")
        || has_prefix(path, b"xtask/proofs/")
        || has_suffix(path, b".lock")
        || has_suffix(path, b"Cargo.toml")
        || path == b"mise.toml"
        || path == b"lefthook.yml";
    let selected = gates(classify(path));
    if infrastructure {
        assert_eq!(selected, Gates::ALL);
    }
    kani::cover!(infrastructure && selected == Gates::ALL);
    kani::cover!(!infrastructure && selected != Gates::ALL);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn selected(paths: &[&str]) -> String {
        select(paths.iter().copied()).names()
    }

    #[test]
    fn a_markdown_only_change_selects_the_prose_gate_alone() {
        assert_eq!(selected(&["README.md", "docs/windows-host.md"]), "prose");
    }

    #[test]
    fn a_rust_change_selects_every_gate_because_xtask_validates_and_applies_everything() {
        assert_eq!(
            selected(&["xtask/src/hooks.rs"]),
            "prose,skills,rust,profiles"
        );
        assert_eq!(
            selected(&["guard/src/refusal.rs"]),
            "prose,skills,rust,profiles"
        );
    }

    #[test]
    fn a_skill_change_selects_the_skills_and_profile_gates_without_rust() {
        assert_eq!(
            selected(&["dot_agents/skills/just/SKILL.md"]),
            "prose,skills,profiles"
        );
        assert_eq!(
            selected(&["docs/skills/decisions/x.json"]),
            "prose,skills,profiles"
        );
    }

    #[test]
    fn a_profile_change_selects_the_profile_gate_without_rust() {
        assert_eq!(selected(&["dot_zshrc.tmpl"]), "prose,profiles");
        assert_eq!(selected(&["dot_claude/CLAUDE.md.tmpl"]), "prose,profiles");
        assert_eq!(selected(&[".chezmoitemplates/x/y.tmpl"]), "prose,profiles");
    }

    #[test]
    fn ci_tooling_lock_files_and_the_classifier_select_everything() {
        for path in [
            ".github/workflows/required.yml",
            "mise.toml",
            "lefthook.yml",
            "justfile",
            "bun.lock",
            "xtask/Cargo.lock",
            "xtask/Cargo.toml",
            "xtask/src/change.rs",
            "xtask/src/change_rules.rs",
            "xtask/proofs/Dockerfile",
        ] {
            assert_eq!(select([path]), Gates::ALL, "{path}");
        }
    }

    #[test]
    fn an_unknown_path_selects_everything() {
        assert_eq!(select(["mystery/file.bin"]), Gates::ALL);
        assert_eq!(select(["README.md", "mystery"]), Gates::ALL);
        assert_eq!(matched(b"mystery"), None);
    }

    #[test]
    fn a_change_set_selects_the_union_of_its_paths_and_an_empty_one_selects_nothing() {
        assert_eq!(
            selected(&[
                "README.md",
                "dot_zshrc.tmpl",
                "dot_agents/skills/just/SKILL.md"
            ]),
            "prose,skills,profiles"
        );
        assert!(select([]).is_empty());
    }

    #[test]
    fn narrow_rules_override_broader_ones() {
        assert_eq!(classify(b"docs/skills/x.md"), Class::Skills);
        assert_eq!(classify(b"dot_agents/skills/a/scripts/x.rs"), Class::Rust);
        assert_eq!(classify(b"ops/README.md"), Class::Profiles);
    }

    #[test]
    fn gate_names_round_trip() {
        let all = Gates::ALL.names();
        assert_eq!(all, "prose,skills,rust,profiles");
        assert_eq!(Gates::parse(&all), Ok(Gates::ALL));
        assert_eq!(Gates::parse(""), Ok(Gates::NONE));
        assert!(Gates::parse("prose,heavy").is_err());
    }
}
