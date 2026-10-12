//! The Lean obligations: the vectors equal what the Rust code gives, and the axiom report refuses `sorry`, `native_decide`, and a user axiom.

use std::path::Path;
use xtask::lean::{axiom_refusals, vectors};

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
