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
    assert_eq!(found.evidence.1, "keys");
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
