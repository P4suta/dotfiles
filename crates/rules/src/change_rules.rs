/// The change a path makes.
/// Each class selects the gates that verify it.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Class {
    Prose,
    Skills,
    Rust,
    /// A rule module or the proof inventory, which the Kani harnesses verify.
    Rules,
    Profiles,
    /// CI, tooling, lock files, the classifier itself, and every path no rule names.
    Everything,
}

/// A group of checks.
/// CI jobs and local hooks run exactly the gates a change selects.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Gate {
    Prose,
    Skills,
    Rust,
    Profiles,
    /// The Kani proofs, which only a rule module, the proof inventory, or the tooling can change.
    Proofs,
}

pub const GATES: [Gate; 5] = [
    Gate::Prose,
    Gate::Skills,
    Gate::Rust,
    Gate::Profiles,
    Gate::Proofs,
];

impl Gate {
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Prose => "prose",
            Self::Skills => "skills",
            Self::Rust => "rust",
            Self::Profiles => "profiles",
            Self::Proofs => "proofs",
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
    pub const ALL: Self = Self(0b1_1111);

    #[must_use]
    pub const fn of(gate: Gate) -> Self {
        Self(gate.bit())
    }

    #[must_use]
    pub const fn with(self, gate: Gate) -> Self {
        Self(self.0 | gate.bit())
    }

    #[must_use]
    pub const fn without(self, gate: Gate) -> Self {
        Self(self.0 & !gate.bit())
    }

    #[must_use]
    pub const fn union(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }

    #[must_use]
    pub const fn contains(self, gate: Gate) -> bool {
        self.0 & gate.bit() != 0
    }

    #[must_use]
    pub const fn is_empty(self) -> bool {
        self.0 == 0
    }

    #[must_use]
    pub const fn covers(self, other: Self) -> bool {
        self.0 & other.0 == other.0
    }

    /// Reads the comma-separated gate names that `Display` writes.
    ///
    /// # Errors
    ///
    /// Returns the first name that no gate carries.
    pub fn parse(text: &str) -> Result<Self, UnknownGate<'_>> {
        text.split(',')
            .filter(|name| !name.is_empty())
            .try_fold(Self::NONE, |gates, name| {
                GATES
                    .iter()
                    .copied()
                    .find(|gate| gate.name() == name)
                    .map(|gate| gates.with(gate))
                    .ok_or(UnknownGate(name))
            })
    }
}

/// The names in declaration order, comma-separated, as `parse` reads them back.
impl core::fmt::Display for Gates {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let mut separator = "";
        for gate in GATES.iter().copied().filter(|gate| self.contains(*gate)) {
            write!(formatter, "{separator}{}", gate.name())?;
            separator = ",";
        }
        Ok(())
    }
}

/// A name in a gate list that no gate carries.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct UnknownGate<'a>(pub &'a str);

impl core::fmt::Display for UnknownGate<'_> {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(formatter, "unknown gate: {}", self.0)
    }
}

/// The gates a class selects: prose checks run for every change, and wider classes add the checks their content can break.
/// Skills deploy through the native profile, so they also select its rehearsal.
/// The xtask crates validate and apply everything, so Rust selects every gate but the proofs.
/// Each proof harness compiles one rule module alone, so only a rule module, the proof inventory, a model under `specs/`, or the tooling selects the proofs.
#[must_use]
pub const fn gates(class: Class) -> Gates {
    let prose = Gates::of(Gate::Prose);
    match class {
        Class::Prose => prose,
        Class::Skills => prose.with(Gate::Skills).with(Gate::Profiles),
        Class::Profiles => prose.with(Gate::Profiles),
        Class::Rust => Gates::ALL.without(Gate::Proofs),
        Class::Rules | Class::Everything => Gates::ALL,
    }
}

#[derive(Clone, Copy)]
enum Match {
    Exact,
    Prefix,
    Suffix,
}

/// The first matching rule decides, so a narrower rule precedes the broader one it overrides.
const RULES: [(Match, &[u8], Class); 38] = [
    (Match::Prefix, b".github/", Class::Everything),
    (Match::Prefix, b"xtask/proofs/", Class::Everything),
    (Match::Prefix, b"xtask/src/change", Class::Everything),
    (
        Match::Prefix,
        b"home/dot_config/lefthook/",
        Class::Everything,
    ),
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
    (Match::Exact, b".chezmoiroot", Class::Everything),
    (Match::Exact, b"home/.chezmoiignore", Class::Everything),
    (Match::Suffix, b".lock", Class::Everything),
    (Match::Suffix, b"Cargo.toml", Class::Everything),
    (Match::Exact, b"xtask/src/skill_proofs.rs", Class::Rules),
    (Match::Exact, b"policy/obligations.toml", Class::Rules),
    (Match::Prefix, b"specs/", Class::Rules),
    (Match::Exact, b"policy/baselines.toml", Class::Rust),
    (Match::Suffix, b"_rules.rs", Class::Rules),
    (Match::Suffix, b".rs", Class::Rust),
    (Match::Prefix, b"xtask/", Class::Rust),
    (Match::Prefix, b"crates/", Class::Rust),
    (Match::Prefix, b".cargo/", Class::Everything),
    (Match::Exact, b"clippy.toml", Class::Everything),
    (Match::Exact, b"deny.toml", Class::Rust),
    (Match::Prefix, b"home/dot_agents/skills/", Class::Skills),
    (Match::Prefix, b"home/dot_claude/skills/", Class::Skills),
    (Match::Prefix, b"docs/skills/", Class::Skills),
    (Match::Prefix, b"home/dot_config/skill-ops/", Class::Skills),
    (Match::Prefix, b"home/", Class::Profiles),
    (Match::Prefix, b"ops/", Class::Profiles),
    (Match::Prefix, b"policy/", Class::Profiles),
    (Match::Prefix, b"docs/", Class::Prose),
    (Match::Suffix, b".md", Class::Prose),
];

/// Slice patterns walk both sides without an index, which keeps the proof's formula as small as indexing would.
fn has_prefix(mut path: &[u8], mut prefix: &[u8]) -> bool {
    while let [expected, rest @ ..] = prefix {
        let [actual, tail @ ..] = path else {
            return false;
        };
        if actual != expected {
            return false;
        }
        (path, prefix) = (tail, rest);
    }
    true
}

fn has_suffix(mut path: &[u8], mut suffix: &[u8]) -> bool {
    while let [rest @ .., expected] = suffix {
        let [head @ .., actual] = path else {
            return false;
        };
        if actual != expected {
            return false;
        }
        (path, suffix) = (head, rest);
    }
    true
}

/// The class a rule assigns to the path, or `None` when no rule names it.
#[must_use]
pub fn matched(path: &[u8]) -> Option<Class> {
    let mut index = 0;
    while let Some(&(kind, text, class)) = RULES.get(index) {
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

/// A path no rule names could hold anything, so it selects every gate.
#[must_use]
pub const fn resolve(found: Option<Class>) -> Class {
    match found {
        Some(class) => class,
        None => Class::Everything,
    }
}

#[must_use]
pub fn classify(path: &[u8]) -> Class {
    resolve(matched(path))
}

/// The gates a change set selects: the union over its paths, and none for an empty change.
///
/// Exempt as no-decision: it folds the proved `classify` and `gates` over the paths with a union and decides nothing itself.
/// Evidence: `crates/rules/src/change_rules.rs::tests::a_change_set_selects_the_union_of_its_paths_and_an_empty_one_selects_nothing`.
/// Reconsider when it gains a branch.
pub fn select<'a>(paths: impl IntoIterator<Item = &'a str>) -> Gates {
    paths.into_iter().fold(Gates::NONE, |selected, path| {
        selected.union(gates(classify(path.as_bytes())))
    })
}

/// The properties Kani proves for every input and the tests check over every input they list.
#[cfg(any(test, kani))]
mod proofs {
    use super::*;

    /// Every path selects the prose gate, and a path no rule names selects every gate.
    pub fn spec_selects(path: &[u8]) -> Option<Class> {
        let found = matched(path);
        let selected = gates(resolve(found));
        assert!(selected.contains(Gate::Prose));
        if found.is_none() {
            assert_eq!(selected, Gates::ALL);
        }
        found
    }

    /// CI, tooling, lock files, and the classifier select every gate.
    pub fn spec_infrastructure(path: &[u8]) -> Gates {
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
        selected
    }

    /// Only a rule module, the proof inventory, or the tooling selects the proofs.
    pub fn spec_proofs(path: &[u8]) -> Class {
        let class = classify(path);
        let proves = gates(class).contains(Gate::Proofs);
        assert_eq!(proves, matches!(class, Class::Rules | Class::Everything));
        if has_suffix(path, b"_rules.rs") {
            assert!(proves);
        }
        class
    }

    #[cfg(kani)]
    fn any_path(bytes: &[u8; 24]) -> &[u8] {
        let length: usize = kani::any();
        kani::assume(length <= 24);
        &bytes[..length]
    }

    #[cfg(kani)]
    #[kani::proof]
    #[kani::unwind(40)]
    fn every_path_selects_a_gate_and_unknown_paths_select_all() {
        let bytes: [u8; 24] = kani::any();
        let path = any_path(&bytes);
        let found = spec_selects(path);
        kani::cover!(found.is_none());
        kani::cover!(found == Some(Class::Prose));
    }

    #[cfg(kani)]
    #[kani::proof]
    #[kani::unwind(40)]
    fn ci_tooling_lock_files_and_the_classifier_select_everything() {
        let bytes: [u8; 24] = kani::any();
        let path = any_path(&bytes);
        let selected = spec_infrastructure(path);
        kani::cover!(selected == Gates::ALL);
        kani::cover!(selected != Gates::ALL);
    }

    #[cfg(kani)]
    #[kani::proof]
    #[kani::unwind(40)]
    fn only_a_rule_module_the_inventory_or_the_tooling_selects_the_proofs() {
        let bytes: [u8; 24] = kani::any();
        let path = any_path(&bytes);
        let class = spec_proofs(path);
        kani::cover!(class == Class::Rules);
        kani::cover!(class == Class::Rust);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec::Vec;

    #[cfg(miri)]
    #[test]
    fn examples_share_one_miri_test() {
        a_gate_set_adds_removes_and_covers_exactly_its_own_gates();
        an_unknown_gate_names_itself();
        a_markdown_only_change_selects_the_prose_gate_alone();
        a_rust_change_selects_every_gate_but_the_proofs_because_xtask_validates_and_applies_everything();
        a_rule_module_or_the_proof_inventory_also_selects_the_proofs();
        a_skill_change_selects_the_skills_and_profile_gates_without_rust();
        a_profile_change_selects_the_profile_gate_without_rust();
        ci_tooling_lock_files_and_the_classifier_select_everything();
        an_unknown_path_selects_everything();
        a_change_set_selects_the_union_of_its_paths_and_an_empty_one_selects_nothing();
        narrow_rules_override_broader_ones();
        gate_names_round_trip();
    }

    /// Every rule's own text, and that text with a byte before, after, and missing, so each boundary of each rule meets the specification.
    #[test]
    #[cfg_attr(miri, ignore = "Miri takes seconds over every rule and neighbour")]
    fn every_rule_and_its_neighbours_meet_the_classification_specification() {
        for (_, text, _) in RULES {
            let mut variants = alloc::vec![text.to_vec()];
            for extra in [&b"x"[..], b"/", b".rs", b"_rules.rs"] {
                variants.push([text, extra].concat());
                variants.push([extra, text].concat());
            }
            if let Some((_, shorter)) = text.split_first() {
                variants.push(shorter.to_vec());
            }
            if let Some((_, shorter)) = text.split_last() {
                variants.push(shorter.to_vec());
            }
            variants.push(Vec::new());
            for path in variants {
                proofs::spec_selects(&path);
                proofs::spec_infrastructure(&path);
                proofs::spec_proofs(&path);
            }
        }
    }

    #[cfg_attr(not(miri), test)]
    fn a_gate_set_adds_removes_and_covers_exactly_its_own_gates() {
        for gate in GATES {
            let one = Gates::of(gate);
            assert!(one.contains(gate) && !one.is_empty());
            assert_eq!(one.with(gate), one);
            assert_eq!(Gates::ALL.without(gate).with(gate), Gates::ALL);
            assert!(!Gates::ALL.without(gate).contains(gate));
            assert!(Gates::ALL.covers(one) && one.covers(Gates::NONE));
            assert!(!Gates::NONE.covers(one));
            assert!(!one.covers(Gates::ALL));
            for other in GATES {
                assert_eq!(one.contains(other), gate == other);
                assert!(one.with(other).contains(other));
            }
        }
        assert!(Gates::NONE.is_empty());
        assert_eq!(Gates::default(), Gates::NONE);
    }

    #[cfg_attr(not(miri), test)]
    fn an_unknown_gate_names_itself() {
        assert_eq!(UnknownGate("heavy").to_string(), "unknown gate: heavy");
    }

    use alloc::string::{String, ToString};

    fn selected(paths: &[&str]) -> String {
        select(paths.iter().copied()).to_string()
    }

    #[cfg_attr(not(miri), test)]
    fn a_markdown_only_change_selects_the_prose_gate_alone() {
        assert_eq!(selected(&["README.md", "docs/windows-host.md"]), "prose");
    }

    #[cfg_attr(not(miri), test)]
    fn a_rust_change_selects_every_gate_but_the_proofs_because_xtask_validates_and_applies_everything()
     {
        assert_eq!(
            selected(&["xtask/src/hooks.rs"]),
            "prose,skills,rust,profiles"
        );
        assert_eq!(
            selected(&["crates/dotguard/src/refusal.rs"]),
            "prose,skills,rust,profiles"
        );
    }

    #[cfg_attr(not(miri), test)]
    fn a_rule_module_or_the_proof_inventory_also_selects_the_proofs() {
        for path in [
            "crates/rules/src/order_rules.rs",
            "crates/rules/src/lint_rules.rs",
            "xtask/src/skill_proofs.rs",
            "policy/obligations.toml",
            "specs/pr_ready.qnt",
            "specs/traces/pr_ready.0.itf.json",
            "home/dot_agents/skills/a/scripts/x_rules.rs",
        ] {
            assert_eq!(
                selected(&[path]),
                "prose,skills,rust,profiles,proofs",
                "{path}"
            );
        }
        assert!(!select(["README.md", "xtask/src/lint.rs"]).contains(Gate::Proofs));
    }

    #[cfg_attr(not(miri), test)]
    fn a_skill_change_selects_the_skills_and_profile_gates_without_rust() {
        assert_eq!(
            selected(&["home/dot_agents/skills/just/SKILL.md"]),
            "prose,skills,profiles"
        );
        assert_eq!(
            selected(&["docs/skills/decisions/x.json"]),
            "prose,skills,profiles"
        );
    }

    #[cfg_attr(not(miri), test)]
    fn a_profile_change_selects_the_profile_gate_without_rust() {
        assert_eq!(selected(&["home/dot_zshrc.tmpl"]), "prose,profiles");
        assert_eq!(
            selected(&["home/dot_claude/CLAUDE.md.tmpl"]),
            "prose,profiles"
        );
        assert_eq!(
            selected(&["home/.chezmoitemplates/x/y.tmpl"]),
            "prose,profiles"
        );
    }

    #[cfg_attr(not(miri), test)]
    fn ci_tooling_lock_files_and_the_classifier_select_everything() {
        for path in [
            ".github/workflows/ci.yml",
            "mise.toml",
            "lefthook.yml",
            "justfile",
            "bun.lock",
            "Cargo.lock",
            "Cargo.toml",
            "clippy.toml",
            ".cargo/config.toml",
            "xtask/Cargo.toml",
            "xtask/src/change.rs",
            "crates/rules/src/change_rules.rs",
            "xtask/proofs/Dockerfile",
            ".chezmoiroot",
            "home/.chezmoiignore",
        ] {
            assert_eq!(select([path]), Gates::ALL, "{path}");
        }
    }

    #[cfg_attr(not(miri), test)]
    fn an_unknown_path_selects_everything() {
        assert_eq!(select(["mystery/file.bin"]), Gates::ALL);
        assert_eq!(select(["README.md", "mystery"]), Gates::ALL);
        assert_eq!(matched(b"mystery"), None);
    }

    #[cfg_attr(not(miri), test)]
    fn a_change_set_selects_the_union_of_its_paths_and_an_empty_one_selects_nothing() {
        assert_eq!(
            selected(&[
                "README.md",
                "home/dot_zshrc.tmpl",
                "home/dot_agents/skills/just/SKILL.md"
            ]),
            "prose,skills,profiles"
        );
        assert!(select([]).is_empty());
    }

    #[cfg_attr(not(miri), test)]
    fn narrow_rules_override_broader_ones() {
        assert_eq!(classify(b"docs/skills/x.md"), Class::Skills);
        assert_eq!(
            classify(b"home/dot_agents/skills/a/scripts/x.rs"),
            Class::Rust
        );
        assert_eq!(classify(b"ops/README.md"), Class::Profiles);
        assert_eq!(
            classify(b"home/dot_config/assure/rules/core-panic.yml"),
            Class::Profiles
        );
    }

    #[cfg_attr(not(miri), test)]
    fn gate_names_round_trip() {
        let all = Gates::ALL.to_string();
        assert_eq!(all, "prose,skills,rust,profiles,proofs");
        assert_eq!(Gates::parse(&all), Ok(Gates::ALL));
        assert_eq!(Gates::parse(""), Ok(Gates::NONE));
        assert_eq!(Gates::parse("prose,heavy"), Err(UnknownGate("heavy")));
    }
}
