//! The Lean obligations: the vectors equal what the Rust code gives, and the axiom report refuses `sorry`, `native_decide`, and a user axiom.

use std::path::Path;
use xtask::lean::{axiom_refusals, theorem_refusals, vectors};
use xtask::obligations::{LeanPackage, Obligation, parse_manifest};

fn root() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap()
}

/// The vectors that Lean checks by evaluation hold exactly what `skill_plan_rules::plan` gives.
#[test]
fn the_lean_vectors_equal_the_rust_results() {
    vectors(root(), true).unwrap();
}

#[test]
fn a_theorem_on_sorry_native_decide_or_a_user_axiom_is_refused() {
    let clean = "'Dotfiles.SkillPlan.plan_correct' depends on axioms: [propext, Classical.choice, Quot.sound]\n'Dotfiles.StepOrder.direct_edges_suffice' does not depend on any axioms\n";
    assert_eq!(axiom_refusals(clean), Vec::<String>::new());
    for (axiom, report) in [
        ("sorryAx", "'A.b' depends on axioms: [propext, sorryAx]"),
        (
            "Lean.ofReduceBool",
            "'A.b' depends on axioms: [Lean.ofReduceBool]",
        ),
        (
            "Dotfiles.trustMe",
            "'A.b' depends on axioms: [Dotfiles.trustMe, Quot.sound]",
        ),
    ] {
        assert_eq!(
            axiom_refusals(report),
            [format!("'A.b' rests on the axiom {axiom}")]
        );
    }
    assert_eq!(axiom_refusals("garbage").len(), 1);
}

fn package() -> LeanPackage {
    let manifest = parse_manifest(r#"
[[obligation]]
verifier = "lean"
package = "specs/lean"
theorems = [
    { name = "A.first", module = "A", bridge = "xtask/tests/a.rs::first", refinement = { kind = "selected-cases", entrypoint = "checks" } },
    { name = "A.second", module = "A", bridge = "xtask/tests/a.rs::second", refinement = { kind = "selected-cases", entrypoint = "checks" } },
]
"#).unwrap();
    let Obligation::Lean(package) = manifest.obligations.into_iter().next().unwrap() else {
        panic!("a Lean package");
    };
    package
}

#[test]
fn every_expected_theorem_has_exactly_one_valid_axiom_report() {
    let package = package();
    let clean = "'A.first' depends on axioms: [propext, Quot.sound]\n'A.second' does not depend on any axioms\n";
    assert_eq!(theorem_refusals(clean, &package), Vec::<String>::new());
    for invalid in [
        String::new(),
        "'A.first' does not depend on any axioms".to_owned(),
        clean.replace("A.second", "A.first"),
        format!("{clean}'A.third' does not depend on any axioms\n"),
        clean.replace(
            "depends on axioms: [propext, Quot.sound]",
            "arbitrary does not depend on any axioms",
        ),
        clean.replace("[propext, Quot.sound]", "propext, Quot.sound"),
        clean.replace("[propext, Quot.sound]", "[[propext, Quot.sound]]"),
        clean.replace("[propext, Quot.sound]", "[]"),
        clean.replace("[propext, Quot.sound]", "[propext, ]"),
        clean.replace("[propext, Quot.sound]", "[propext, sorryAx]"),
    ] {
        assert_ne!(
            theorem_refusals(&invalid, &package),
            Vec::<String>::new(),
            "{invalid}"
        );
    }
}

#[test]
fn an_empty_or_duplicated_expected_theorem_set_is_refused() {
    let mut empty = package();
    empty.theorems.clear();
    assert_ne!(theorem_refusals("", &empty), Vec::<String>::new());
    let mut duplicate = package();
    duplicate.theorems[1].name = duplicate.theorems[0].name.clone();
    assert_ne!(
        theorem_refusals("'A.first' does not depend on any axioms", &duplicate),
        Vec::<String>::new()
    );
}

#[test]
fn the_linux_x86_64_kernel_runs_offline_with_read_only_source_on_every_host() {
    let arguments = xtask::lean::run_arguments(
        Path::new("checkout"),
        "dotfiles-lean:fixture",
        "lake build --wfail",
    );
    let values: Vec<_> = arguments
        .iter()
        .map(|value| value.to_string_lossy())
        .collect();
    for (option, expected) in [("--platform", "linux/amd64"), ("--network", "none")] {
        let position = values
            .iter()
            .position(|value| value == option)
            .expect(option);
        assert_eq!(values[position + 1], expected);
    }
    assert!(values.contains(&r#"type=bind,"source=checkout",target=/source,readonly"#.into()));
    assert_eq!(
        values[values.len() - 3..],
        ["bash", "-c", "lake build --wfail"]
    );
}
