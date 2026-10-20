use rules::obligation_rules::Reason;
use std::fs;
use std::path::Path;
use xtask::obligations::{
    Obligation, closure, exemption, metrics, parse_baselines, parse_manifest, pilots,
    unexempted_exclusions,
};

fn root() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap()
}

/// Every pilot's issue stays open.
const OPEN: fn(u32) -> anyhow::Result<bool> = |_| Ok(false);

/// No refusal.
const NONE: [String; 0] = [];

#[test]
fn the_repository_manifest_covers_every_subject_and_holds_every_metric() {
    xtask::obligations::check(root(), &OPEN).unwrap();
}

#[test]
fn an_alloy_check_needs_its_three_commands_at_the_recorded_scope() {
    let parsed = parse_manifest(
        "[[obligation]]\nverifier = \"alloy\"\nspec = \"specs/m.als\"\nsolver = \"sat4j\"\nchecks = [{ assertion = \"Holds\", instance = \"Exists\", counterexample = \"Breaks\", scope = \"for 3 but 4 steps\" }]\n",
    )
    .unwrap();
    let Obligation::Alloy(model) = &parsed.obligations[0] else {
        panic!("an Alloy obligation");
    };
    let scope = tempfile::tempdir().unwrap();
    fs::create_dir_all(scope.path().join("specs")).unwrap();
    let complete = "Holds: check { P } for 3 but 4 steps\nExists: run { Q } for 3 but 4 steps\nBreaks: check { R } for 3 but 4 steps\n";
    fs::write(scope.path().join("specs/m.als"), complete).unwrap();
    assert_eq!(xtask::obligations::close_alloy(scope.path(), model), NONE);
    fs::write(
        scope.path().join("specs/m.als"),
        complete.replace(
            "Exists: run { Q } for 3 but 4 steps",
            "Exists: run { Q } for 2",
        ),
    )
    .unwrap();
    assert_eq!(
        xtask::obligations::close_alloy(scope.path(), model).len(),
        1
    );
    assert!(
        parse_manifest(
            "[[obligation]]\nverifier = \"alloy\"\nspec = \"specs/m.als\"\nsolver = \"minisat\"\nchecks = []\n"
        )
        .is_err(),
        "a solver with a native library stays outside the image"
    );
}

#[test]
fn a_verifier_outside_the_toolbox_fails_to_parse() {
    let error =
        parse_manifest("[[obligation]]\nverifier = \"z3\"\nsource = \"a.smt2\"\n").unwrap_err();
    assert!(format!("{error:#}").contains("z3"), "{error:#}");
    assert!(
        parse_manifest(
            "[[obligation]]\nverifier = \"kani\"\nsource = \"crates/rules/src/a_rules.rs\"\nrlimit = 3\n"
        )
        .is_err()
    );
    let parsed = parse_manifest(
        "[[obligation]]\nverifier = \"kani-contract\"\nsource = \"s.rs\"\nfunction = \"age\"\nflags = [\"function-contracts\"]\n",
    )
    .unwrap();
    assert!(matches!(
        parsed.obligations[0],
        Obligation::KaniContract { .. }
    ));
}

#[test]
fn wall_time_is_representable_only_as_advisory() {
    assert!(parse_baselines("[[metric]]\nmetric = \"wall-millis\"\nexact = 5\n").is_err());
    assert!(parse_baselines("[[metric]]\nmetric = \"wall-millis\"\nmax = 5\n").is_err());
    let advisory = parse_baselines("[[metric]]\nmetric = \"wall-millis\"\nadvisory = 5\n").unwrap();
    assert_eq!(
        metrics(&advisory, &xtask::obligations::Closure::default(), 9),
        NONE
    );
}

#[test]
fn a_metric_refuses_a_regression_and_an_unrecorded_improvement() {
    let baselines = parse_baselines("[[metric]]\nmetric = \"unsafe-sites\"\nexact = 3\n").unwrap();
    let measured = xtask::obligations::Closure::default();
    assert_eq!(metrics(&baselines, &measured, 3), NONE);
    assert!(metrics(&baselines, &measured, 4)[0].contains("went from 3 to 4"));
    assert!(metrics(&baselines, &measured, 2)[0].contains("record 2"));
}

#[test]
fn an_exemption_needs_a_closed_reason_its_evidence_and_a_reconsideration() {
    let found = exemption(
        "Exempt as no-decision: maps a key. Evidence: `crates/rules/src/a_rules.rs::tests::keys`. Reconsider when it branches.",
    )
    .unwrap()
    .unwrap();
    assert_eq!(found.reason, Reason::NoDecision);
    assert_eq!(found.evidence.1, "tests::keys");
    assert!(exemption("Plain documentation.").unwrap().is_none());
    assert!(exemption("Exempt as too-hard: no. Evidence: `a.rs::t`. Reconsider when x.").is_err());
    assert!(exemption("Exempt as no-decision: no evidence. Reconsider when x.").is_err());
    assert!(exemption("Exempt as no-decision: x. Evidence: `a.rs::t`.").is_err());
}

/// A fixture checkout with one rule module and a production caller.
fn fixture(module: &str, caller: &str) -> tempfile::TempDir {
    let directory = tempfile::tempdir().unwrap();
    let rules = directory.path().join("crates/rules/src");
    fs::create_dir_all(&rules).unwrap();
    fs::write(rules.join("a_rules.rs"), module).unwrap();
    fs::write(rules.join("lib.rs"), "pub mod a_rules;\n").unwrap();
    fs::write(
        directory.path().join("crates/rules/Cargo.toml"),
        "[package]\nname = \"fixture-rules\"\nversion = \"0.0.0\"\n",
    )
    .unwrap();
    let tool = directory.path().join("crates/tool/src");
    fs::create_dir_all(&tool).unwrap();
    fs::write(tool.join("main.rs"), caller).unwrap();
    fs::create_dir_all(directory.path().join("xtask/src")).unwrap();
    directory
}

const PROVED: &str = "pub fn decide(a: bool) -> bool { !a }\n#[cfg(kani)]\n#[kani::proof]\nfn decide_negates() { let a: bool = kani::any(); assert!(decide(a) != a); }\n";
const CALLER: &str = "use rules::a_rules::decide;\nfn main() { decide(true); }\n";
const LISTED: &str = "[[obligation]]\nverifier = \"kani\"\nsource = \"crates/rules/src/a_rules.rs\"\nprobes = '''\n#[kani::proof]\nfn reject() { assert!(production::decide(true)); }\n'''\n";

#[test]
fn a_listed_proved_and_called_subject_passes() {
    let checkout = fixture(PROVED, CALLER);
    let measured = closure(checkout.path(), &parse_manifest(LISTED).unwrap()).unwrap();
    assert!(measured.refusals.is_empty(), "{:#?}", measured.refusals);
    assert_eq!((measured.harnesses, measured.probes), (1, 1));
}

#[test]
fn an_unowned_function_an_unlisted_module_and_a_stray_harness_fail() {
    let unowned = format!("{PROVED}pub fn helper(a: u8) -> u8 {{ a + 1 }}\n");
    let checkout = fixture(
        &unowned,
        &format!("{CALLER}fn other() {{ rules::a_rules::helper(1); }}\n"),
    );
    let measured = closure(checkout.path(), &parse_manifest(LISTED).unwrap()).unwrap();
    assert!(
        measured
            .refusals
            .iter()
            .any(|refusal| refusal.contains("`helper` has no harness")),
        "{:#?}",
        measured.refusals
    );
    let measured = closure(checkout.path(), &parse_manifest("").unwrap()).unwrap();
    assert!(
        measured
            .refusals
            .iter()
            .any(|refusal| refusal.contains("has no Kani obligation")),
        "{:#?}",
        measured.refusals
    );
    let stray = fixture(
        PROVED,
        &format!("{CALLER}#[kani::proof]\nfn elsewhere() {{}}\n"),
    );
    let measured = closure(stray.path(), &parse_manifest(LISTED).unwrap()).unwrap();
    assert!(
        measured
            .refusals
            .iter()
            .any(|refusal| refusal.contains("`elsewhere` lies outside")),
        "{:#?}",
        measured.refusals
    );
}

#[test]
fn a_proved_subject_without_a_production_caller_fails() {
    let checkout = fixture(PROVED, "fn main() {}\n");
    let measured = closure(checkout.path(), &parse_manifest(LISTED).unwrap()).unwrap();
    assert!(
        measured
            .refusals
            .iter()
            .any(|refusal| refusal.contains("no production code calls")),
        "{:#?}",
        measured.refusals
    );
}

#[test]
fn a_stale_exemption_fails_and_a_current_one_passes() {
    let exempt = |body: &str| {
        format!(
            "{PROVED}/// Exempt as no-decision: passes through. Evidence: `crates/rules/src/a_rules.rs::keeps`. Reconsider when it branches.\npub fn pass(a: u8) -> u8 {{ {body} }}\n#[cfg(test)]\nmod tests {{ #[test] fn keeps() {{}} }}\n"
        )
    };
    let current = fixture(&exempt("a"), CALLER);
    let measured = closure(current.path(), &parse_manifest(LISTED).unwrap()).unwrap();
    assert!(measured.refusals.is_empty(), "{:#?}", measured.refusals);
    assert_eq!(measured.exempt, 1);
    let stale = fixture(&exempt("if a > 1 { a } else { 0 }"), CALLER);
    let measured = closure(stale.path(), &parse_manifest(LISTED).unwrap()).unwrap();
    assert!(
        measured
            .refusals
            .iter()
            .any(|refusal| refusal.contains("`pass` carries an exemption that no longer holds")),
        "{:#?}",
        measured.refusals
    );
}

fn lean_fixture(
    path: &str,
    source: &str,
    function: &str,
    refinement: &str,
    registration: &str,
) -> (tempfile::TempDir, xtask::obligations::Manifest) {
    let directory = fixture(PROVED, CALLER);
    let package = directory.path().join("specs/lean");
    fs::create_dir_all(&package).unwrap();
    fs::write(
        package.join("lean-toolchain"),
        format!("leanprover/lean4:v{}\n", xtask::lean::LEAN_VERSION),
    )
    .unwrap();
    fs::write(
        package.join("A.lean"),
        "namespace A\ntheorem holds : True := by trivial\nend A\n",
    )
    .unwrap();
    fs::write(package.join("Axioms.lean"), "#print axioms A.holds\n").unwrap();
    fs::create_dir_all(directory.path().join("xtask/tests")).unwrap();
    fs::write(
        directory.path().join("xtask/Cargo.toml"),
        "[package]\nname = \"fixture\"\nversion = \"0.0.0\"\n",
    )
    .unwrap();
    fs::write(directory.path().join(path), source).unwrap();
    let manifest = parse_manifest(&format!(r#"
{registration}
[[obligation]]
verifier = "lean"
package = "specs/lean"
theorems = [{{ name = "A.holds", module = "A", bridge = "{path}::{function}", refinement = {refinement} }}]
"#)).unwrap();
    (directory, manifest)
}

fn lean_refusals(root: &Path, manifest: &xtask::obligations::Manifest) -> Vec<String> {
    let package = manifest
        .obligations
        .iter()
        .find_map(|obligation| match obligation {
            Obligation::Lean(package) => Some(package),
            _ => None,
        })
        .unwrap();
    xtask::obligations::close_lean(root, manifest, package)
}

#[test]
fn the_repository_lean_bridges_close_at_their_declared_implementation_scope() {
    let manifest = xtask::obligations::manifest(root()).unwrap();
    assert_eq!(lean_refusals(root(), &manifest), NONE);
}

const SELECTED_CASES: &str = r#"{ kind = "selected-cases", entrypoint = "checks" }"#;

#[test]
fn a_bridge_needs_an_enabled_registered_root_that_calls_the_declared_entrypoint() {
    for source in [
        "// fn bridge() {}\nfn checks() {}",
        "const TEXT: &str = \"fn bridge() {}\";\nfn checks() {}",
        "fn bridge() { checks(); } fn checks() {}",
        "#[test] #[ignore] fn bridge() { checks(); } fn checks() {}",
        "#[cfg(any())] #[test] fn bridge() { checks(); } fn checks() {}",
        "#[cfg_attr(test, ignore)] #[test] fn bridge() { checks(); } fn checks() {}",
        "#[cfg_attr(not(miri), ignore)] #[test] fn bridge() { checks(); } fn checks() {}",
        "#[cfg_attr(miri, test)] fn bridge() { checks(); } fn checks() {}",
        "#[cfg_attr(unknown, test)] fn bridge() { checks(); } fn checks() {}",
        "#[cfg_attr(test, cfg(any()))] #[test] fn bridge() { checks(); } fn checks() {}",
        "#[cfg_attr(test)] #[test] fn bridge() { checks(); } fn checks() {}",
        "#[cfg(any())] mod tests { #[test] fn bridge() { checks(); } fn checks() {} }",
        "#[test] fn bridge() {} fn checks() {}",
        "#[test] fn root() { fn dead() { bridge(); } } fn bridge() { checks(); } fn checks() {}",
        "#[test] fn root() { let dead = || bridge(); } fn bridge() { checks(); } fn checks() {}",
        "#[test] fn root() { if false { bridge(); } } fn bridge() { checks(); } fn checks() {}",
        "#[test] fn root() { other::bridge(); } fn bridge() { checks(); } fn checks() {}",
        "#[test] fn root() { let bridge = || {}; bridge(); } fn bridge() { checks(); } fn checks() {}",
        "#[test] fn root() { object.bridge(); } fn bridge() { checks(); } fn checks() {}",
    ] {
        let (directory, manifest) = lean_fixture(
            "xtask/tests/bridge.rs",
            source,
            "bridge",
            SELECTED_CASES,
            "",
        );
        assert_ne!(
            lean_refusals(directory.path(), &manifest),
            NONE.to_vec(),
            "{source}"
        );
    }
    for source in [
        "#[test] fn bridge() { checks(); } fn checks() {}",
        "#[cfg_attr(miri, ignore = \"Miri-only omission\")] #[test] fn bridge() { checks(); } fn checks() {}",
        "#[cfg_attr(not(miri), test)] fn bridge() { checks(); } fn checks() {}",
        "#[cfg_attr(test, cfg_attr(not(miri), test))] fn bridge() { checks(); } fn checks() {}",
        "#[test] fn root() { bridge(); } fn bridge() { checks(); } fn checks() {}",
        "#[test] fn root() { assert!(bridge()); } fn bridge() -> bool { checks(); true } fn checks() {}",
        "#[test] fn root() { self::bridge(); } fn bridge() { checks(); } fn checks() {}",
    ] {
        let (directory, manifest) = lean_fixture(
            "xtask/tests/bridge.rs",
            source,
            "bridge",
            SELECTED_CASES,
            "",
        );
        assert_eq!(lean_refusals(directory.path(), &manifest), NONE, "{source}");
    }
}

#[test]
fn cargo_registration_and_parent_configuration_cannot_be_invented() {
    let source = "#[test] fn bridge() { checks(); } fn checks() {}";
    for path in ["xtask/tests/bridge.txt", "crates/rules/src/a_rules.rs"] {
        let (directory, manifest) = lean_fixture(path, source, "bridge", SELECTED_CASES, "");
        if path.ends_with("a_rules.rs") {
            fs::write(
                directory.path().join("crates/rules/src/lib.rs"),
                "#[path = \"other.rs\"] pub mod a_rules;\n",
            )
            .unwrap();
        }
        assert_ne!(
            lean_refusals(directory.path(), &manifest),
            NONE.to_vec(),
            "{path}"
        );
    }
    let (directory, manifest) = lean_fixture(
        "crates/rules/src/a_rules.rs",
        source,
        "bridge",
        SELECTED_CASES,
        "",
    );
    fs::write(
        directory.path().join("crates/rules/src/lib.rs"),
        "#![cfg(any())]\npub mod a_rules;\n",
    )
    .unwrap();
    assert_ne!(lean_refusals(directory.path(), &manifest), NONE.to_vec());
    let (directory, manifest) = lean_fixture(
        "crates/rules/src/a_rules.rs",
        source,
        "bridge",
        SELECTED_CASES,
        "",
    );
    fs::write(
        directory.path().join("crates/rules/Cargo.toml"),
        "[package]\nname = \"fixture\"\nautolib = false\n",
    )
    .unwrap();
    assert_ne!(lean_refusals(directory.path(), &manifest), NONE.to_vec());
    let (directory, manifest) = lean_fixture(
        "xtask/tests/bridge.rs",
        source,
        "bridge",
        SELECTED_CASES,
        "",
    );
    fs::write(
        directory.path().join("xtask/Cargo.toml"),
        "[package]\nname = \"fixture\"\nautotests = false\n",
    )
    .unwrap();
    assert_ne!(lean_refusals(directory.path(), &manifest), NONE.to_vec());
}

#[test]
fn kani_scope_needs_an_execution_obligation_and_a_used_matching_bound() {
    let source =
        "const BOUND: usize = 5;\n#[cfg(kani)] #[kani::proof] fn bridge() { let _ = BOUND; }";
    let refinement = r#"{ kind = "bounded-kani", source = "crates/rules/src/a_rules.rs", constant = "BOUND", value = 5 }"#;
    let listed = "[[obligation]]\nverifier = \"kani\"\nsource = \"crates/rules/src/a_rules.rs\"\n";
    let (directory, manifest) = lean_fixture(
        "crates/rules/src/a_rules.rs",
        source,
        "bridge",
        refinement,
        listed,
    );
    assert_eq!(lean_refusals(directory.path(), &manifest), NONE);
    for (code, scope, registration) in [
        (
            source.replace("let _ = BOUND;", ""),
            refinement.to_owned(),
            listed,
        ),
        (
            source.to_owned(),
            refinement.replace("value = 5", "value = 6"),
            listed,
        ),
        (source.to_owned(), refinement.to_owned(), ""),
        (
            source.to_owned(),
            refinement.to_owned(),
            "[[obligation]]\nverifier = \"kani-contract\"\nsource = \"crates/rules/src/a_rules.rs\"\nfunction = \"bridge\"\nflags = [\"function-contracts\"]\n",
        ),
    ] {
        let (directory, manifest) = lean_fixture(
            "crates/rules/src/a_rules.rs",
            &code,
            "bridge",
            &scope,
            registration,
        );
        assert_ne!(lean_refusals(directory.path(), &manifest), NONE.to_vec());
    }
}

#[test]
fn refinement_roles_cannot_be_attached_to_an_unrelated_empty_test() {
    for refinement in [
        r#"{ kind = "generated-vectors" }"#,
        r#"{ kind = "source-translation" }"#,
    ] {
        let (directory, manifest) = lean_fixture(
            "xtask/tests/bridge.rs",
            "#[test] fn bridge() {}",
            "bridge",
            refinement,
            "",
        );
        assert_ne!(lean_refusals(directory.path(), &manifest), NONE.to_vec());
    }
    for source in [
        "use xtask::lean::vectors; #[test] fn bridge() { vectors(root(), true).unwrap(); }",
        "use xtask::lean::vectors as generate; #[test] fn bridge() { generate(root(), true).unwrap(); }",
    ] {
        let (directory, manifest) = lean_fixture(
            "xtask/tests/bridge.rs",
            source,
            "bridge",
            r#"{ kind = "generated-vectors" }"#,
            "",
        );
        assert_eq!(lean_refusals(directory.path(), &manifest), NONE);
    }
    let (directory, manifest) = lean_fixture(
        "xtask/tests/bridge.rs",
        "use xtask::lean::vectors; #[test] fn bridge() { let vectors = || {}; vectors(); }",
        "bridge",
        r#"{ kind = "generated-vectors" }"#,
        "",
    );
    assert_ne!(lean_refusals(directory.path(), &manifest), NONE.to_vec());
    assert!(parse_manifest(r#"[[obligation]]
verifier = "lean"
package = "specs/lean"
theorems = [{ name = "A.holds", module = "A", bridge = "xtask/tests/a.rs::bridge", refinement = { kind = "universal-rust" } }]
"#).is_err());
}

#[test]
fn an_unlisted_mutation_skip_fails() {
    let checkout = fixture(
        PROVED,
        &format!("{CALLER}#[mutants::skip]\nfn skipped() {{}}\n"),
    );
    let measured = closure(checkout.path(), &parse_manifest(LISTED).unwrap()).unwrap();
    assert!(
        measured
            .refusals
            .iter()
            .any(|refusal| refusal.contains("`skipped` skips mutation")),
        "{:#?}",
        measured.refusals
    );
    assert_eq!(
        unexempted_exclusions("exclude_re = [\n  \"replace .* with true\",\n]\n")
            .unwrap()
            .len(),
        1
    );
    assert_eq!(
        unexempted_exclusions(
            "exclude_re = [\n  # Exempt as disproportionate: loops forever. Evidence: `a.rs::t`. Reconsider when bounded.\n  \"replace .* with true\",\n]\n"
        )
        .unwrap(),
        NONE
    );
}

#[test]
fn a_pilot_whose_issue_closed_without_a_decision_fails() {
    let checkout = fixture(PROVED, CALLER);
    let manifest = parse_manifest(
        "[[pilot]]\ntool = \"bolero\"\nsubjects = [\"a\"]\nexit = \"x\"\nissue = 7\n",
    )
    .unwrap();
    assert_eq!(pilots(checkout.path(), &manifest, &OPEN).unwrap(), NONE);
    let closed = |_: u32| Ok(true);
    assert!(
        pilots(checkout.path(), &manifest, &closed).unwrap()[0]
            .contains("closed without a decision record")
    );
    fs::create_dir_all(checkout.path().join("docs/adr")).unwrap();
    fs::write(checkout.path().join("docs/adr/bolero.md"), "# Bolero\n").unwrap();
    let decided = parse_manifest(
        "[[pilot]]\ntool = \"bolero\"\nsubjects = [\"a\"]\nexit = \"x\"\nissue = 7\ndecision = \"docs/adr/bolero.md\"\n",
    )
    .unwrap();
    assert_eq!(pilots(checkout.path(), &decided, &closed).unwrap(), NONE);
    let unavailable = |_: u32| Err(anyhow::anyhow!("the issue service is unavailable"));
    assert_eq!(
        pilots(checkout.path(), &decided, &unavailable).unwrap(),
        NONE
    );
    assert!(pilots(checkout.path(), &manifest, &unavailable).is_err());
}

#[test]
fn a_quint_model_whose_state_graph_is_missing_fails() {
    let checkout = fixture(PROVED, CALLER);
    fs::create_dir_all(checkout.path().join("specs")).unwrap();
    fs::write(
        checkout.path().join("specs/m.qnt"),
        "module m {}\nmodule m_buggy {}\n",
    )
    .unwrap();
    let model = |graph: &str| {
        parse_manifest(&format!(
            "[[obligation]]\nverifier = \"quint\"\nspec = \"specs/m.qnt\"\nmain = \"m\"\nprobe = \"m_buggy\"\ninvariant = \"safe\"\nwitnesses = [\"w\"]\nseed = \"0x1\"\nsamples = 1\nsteps = 1\ndistinct_states = 1\ntraces = []\n{graph}"
        ))
        .unwrap()
    };
    let missing = |manifest| -> bool {
        closure(checkout.path(), &manifest)
            .unwrap()
            .refusals
            .iter()
            .any(|refusal| refusal.contains("the state graph specs/m.json is missing"))
    };
    assert!(!missing(model("")));
    assert!(missing(model("graph = \"specs/m.json\"\n")));
    fs::write(checkout.path().join("specs/m.json"), "{}").unwrap();
    assert!(!missing(model("graph = \"specs/m.json\"\n")));
}

#[test]
fn a_surviving_mutant_or_an_unnamed_timeout_fails_and_a_named_timeout_passes() {
    let baselines = parse_baselines(
        "[[metric]]\nmetric = \"mutants-missed\"\nexact = 0\n\n[[metric]]\nmetric = \"mutants-timed-out\"\nexact = 0\n",
    )
    .unwrap();
    let clean = xtask::obligations::Campaign::default();
    assert_eq!(
        xtask::obligations::mutation_metrics(&baselines, clean),
        NONE
    );
    let survivor = xtask::obligations::Campaign {
        missed: 1,
        timed_out: 0,
    };
    assert!(
        xtask::obligations::mutation_metrics(&baselines, survivor)[0]
            .contains("mutants-missed went from 0 to 1")
    );
    let manifest = parse_manifest(
        "[[non-terminating]]\nmutant = \"a.rs:1:1: replace += with *= in walk\"\nreason = \"the cursor stops moving\"\n",
    )
    .unwrap();
    let timeouts = "a.rs:1:1: replace += with *= in walk\nb.rs:2:2: replace + with * in other\n";
    assert_eq!(
        xtask::obligations::unexplained_timeouts(&manifest, timeouts),
        ["b.rs:2:2: replace + with * in other"]
    );
    let missing = parse_baselines("").unwrap();
    assert_eq!(
        xtask::obligations::mutation_metrics(&missing, clean).len(),
        2
    );
}

/// Whether the closure refuses `decide` for want of a production caller.
fn uncalled(module: &str, caller: &str) -> bool {
    let checkout = fixture(module, caller);
    closure(checkout.path(), &parse_manifest(LISTED).unwrap())
        .unwrap()
        .refusals
        .iter()
        .any(|refusal| refusal.contains("no production code calls"))
}

#[test]
fn a_caller_counts_as_production_unless_its_predicate_excludes_an_ordinary_build() {
    for predicate in [
        "any(windows, test)",
        "not(test)",
        "feature = \"test-utils\"",
        "all(unix, any(windows, not(kani)))",
        "any(test, not(target_os = \"linux\"))",
        "true",
        "test(unreadable)",
    ] {
        let caller =
            format!("#[cfg({predicate})]\nfn main() {{ rules::a_rules::decide(true); }}\n");
        assert!(!uncalled(PROVED, &caller), "{predicate}");
    }
    for predicate in [
        "test",
        "all(test, windows)",
        "any(test, kani)",
        "false",
        "any()",
    ] {
        let caller =
            format!("#[cfg({predicate})]\nfn main() {{ rules::a_rules::decide(true); }}\n");
        assert!(uncalled(PROVED, &caller), "{predicate}");
    }
    let both = "#[cfg(unix)]\n#[cfg(test)]\nfn main() { rules::a_rules::decide(true); }\n";
    assert!(uncalled(PROVED, both));
}

#[test]
fn only_items_that_compile_under_kani_supply_harnesses_and_proof() {
    let decide = "pub fn decide(a: bool) -> bool { !a }\n";
    let harness = "#[kani::proof]\nfn decide_negates() { let a: bool = kani::any(); assert!(decide(a) != a); }";
    for (predicate, proved) in [
        ("kani", true),
        ("any(test, kani)", true),
        ("all(test, not(kani))", false),
        ("test", false),
        ("all(kani, unix)", false),
    ] {
        let module =
            format!("{decide}#[cfg({predicate})]\nmod proofs {{\nuse super::*;\n{harness}\n}}\n");
        let checkout = fixture(&module, CALLER);
        let measured = closure(checkout.path(), &parse_manifest(LISTED).unwrap()).unwrap();
        assert_eq!(measured.harnesses == 1, proved, "{predicate}");
        assert_eq!(
            measured
                .refusals
                .iter()
                .any(|refusal| refusal.contains("`decide` has no harness")),
            !proved,
            "{predicate}: {:#?}",
            measured.refusals
        );
    }
    let test_only = format!(
        "{decide}#[cfg(any(test, kani))]\nmod proofs {{\nuse super::*;\n{harness}\n#[cfg(all(test, not(kani)))]\nfn helper() {{ decide(true); }}\n#[test]\nfn example() {{ decide(false); }}\n}}\n"
    );
    let checkout = fixture(&test_only, CALLER);
    let measured = closure(checkout.path(), &parse_manifest(LISTED).unwrap()).unwrap();
    assert!(measured.refusals.is_empty(), "{:#?}", measured.refusals);
    let unproved = format!(
        "{decide}#[cfg(any(test, kani))]\nmod proofs {{\nuse super::*;\n#[cfg(all(test, not(kani)))]\n{harness}\n#[test]\nfn example() {{ decide(false); }}\n}}\n#[cfg(kani)]\n#[kani::proof]\nfn other() {{}}\n"
    );
    let checkout = fixture(&unproved, CALLER);
    let measured = closure(checkout.path(), &parse_manifest(LISTED).unwrap()).unwrap();
    assert_eq!(measured.harnesses, 1);
    assert!(
        measured
            .refusals
            .iter()
            .any(|refusal| refusal.contains("`decide` has no harness")),
        "{:#?}",
        measured.refusals
    );
}
