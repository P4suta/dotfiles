//! Each enforcement point refuses a fixture repository with an unowned shape and admits it once its owner exists.

use anyhow::{Result, ensure};
use dotfiles::assure::{self, Catalog};
use dotfiles::refusal::Refusal;
use dotfiles::shape_rules::{Cause, Verdict};
use std::fs;
use std::path::Path;

#[path = "../../dotguard/tests/support/fixture_git.rs"]
mod fixture_git;

fn catalog() -> Result<Catalog> {
    assure::catalog(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("../../home/dot_config/assure/shapes.toml"),
    )
}

struct Fixture {
    _scope: tempfile::TempDir,
    root: std::path::PathBuf,
    config: std::path::PathBuf,
}

impl Fixture {
    fn new() -> Result<Self> {
        let scope = tempfile::tempdir()?;
        let config = scope.path().join("gitconfig");
        fs::write(
            &config,
            "[user]\n\tname = Fixture\n\temail = fixture@example.com\n[commit]\n\tgpgsign = false\n[init]\n\tdefaultBranch = main\n",
        )?;
        let root = scope.path().join("repository");
        fs::create_dir(&root)?;
        let fixture = Self {
            _scope: scope,
            root,
            config,
        };
        fixture.git(&["init", "--quiet"])?;
        fixture.write("README.md", "fixture\n")?;
        fixture.commit("start")?;
        Ok(fixture)
    }

    fn git(&self, arguments: &[&str]) -> Result<String> {
        let output = fixture_git::command("git", &self.config)
            .current_dir(&self.root)
            .args(arguments)
            .output()?;
        ensure!(
            output.status.success(),
            "git {arguments:?}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        Ok(String::from_utf8(output.stdout)?.trim().to_owned())
    }

    fn write(&self, path: &str, text: &str) -> Result<()> {
        let path = self.root.join(path);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(path, text)?;
        Ok(())
    }

    fn stage(&self) -> Result<()> {
        self.git(&["add", "--all"]).map(drop)
    }

    /// Records the working tree as a commit without the machine hooks, as an older repository would hold it.
    fn commit(&self, message: &str) -> Result<String> {
        self.stage()?;
        self.git(&["commit", "--quiet", "--allow-empty", "-m", message])?;
        self.git(&["rev-parse", "HEAD"])
    }
}

fn rule(result: Result<()>) -> Option<String> {
    result
        .err()
        .map(|error| match error.downcast_ref::<Refusal>() {
            Some(refusal) => refusal.rule.clone(),
            None => format!("not a refusal: {error:#}"),
        })
}

const PARSER: &str = "pub fn parse(text: &str) -> Option<u8> {\n    text.parse().ok()\n}\n";
const TARGET: &str = "#![no_main]\nlibfuzzer_sys::fuzz_target!(|data: &[u8]| {});\n";
const OWNERS: &str =
    "[[owner]]\nshape = \"parser\"\npath = \"src/lib.rs\"\nby = \"fuzz/fuzz_targets/parse.rs\"\n";

#[test]
fn an_unowned_parser_is_refused_at_every_point_and_admitted_once_its_fuzz_target_and_corpus_exist()
-> Result<()> {
    let catalog = catalog()?;
    let fixture = Fixture::new()?;
    let root = &fixture.root;
    let base = fixture.git(&["rev-parse", "HEAD"])?;

    fixture.write("src/lib.rs", PARSER)?;
    let report = assure::start(root, &catalog)?.unwrap_or_default();
    assert!(
        report.contains("src/lib.rs:1: parser has no owner"),
        "{report}"
    );
    assert!(report.contains("fuzz/fuzz_targets"), "{report}");
    let stopped = assure::stop(root, &catalog)?;
    assert_eq!(stopped.verdict, Verdict::Added);
    let response = assure::stop_response(
        stopped
            .refusal(&catalog)
            .map(|refusal| refusal.text())
            .as_deref(),
        false,
    );
    assert_eq!(response["decision"], "block");
    assert_eq!(assure::stop_response(Some("gap"), true)["continue"], false);
    fixture.stage()?;
    assert_eq!(
        rule(assure::commit(root, &catalog)).as_deref(),
        Some("assure.added")
    );

    fixture.write(OWNERS_PATH, OWNERS)?;
    fixture.write("fuzz/fuzz_targets/parse.rs", TARGET)?;
    fixture.stage()?;
    let broken = assure::stop(root, &catalog)?;
    assert_eq!(broken.verdict, Verdict::Added);
    assert_eq!(
        broken.gaps.first().map(|gap| gap.cause),
        Some(Cause::BrokenOwner)
    );
    assert_eq!(
        rule(assure::commit(root, &catalog)).as_deref(),
        Some("assure.added")
    );

    fixture.write("fuzz/corpus/parse/seed", "7")?;
    fixture.stage()?;
    assert_eq!(rule(assure::commit(root, &catalog)), None);
    assert_eq!(assure::stop(root, &catalog)?.verdict, Verdict::Admit);
    assert_eq!(assure::start(root, &catalog)?, None);
    let owned = fixture.commit("own the parser")?;

    let push = |from: &str, to: &str| format!("refs/heads/main {to} refs/heads/main {from}\n");
    assert_eq!(
        rule(assure::push(
            root,
            &catalog,
            "origin",
            push(&base, &owned).as_bytes()
        )),
        None
    );
    fixture.git(&["update-ref", "refs/remotes/origin/main", &base])?;
    assert_eq!(rule(assure::ready(root, &catalog, "main", &owned)), None);

    fixture.write("src/other.rs", PARSER)?;
    let unowned = fixture.commit("add a second parser")?;
    assert_eq!(
        rule(assure::push(
            root,
            &catalog,
            "origin",
            push(&owned, &unowned).as_bytes()
        ))
        .as_deref(),
        Some("assure.added")
    );
    assert_eq!(
        rule(assure::ready(root, &catalog, "main", &unowned)).as_deref(),
        Some("assure.added")
    );
    Ok(())
}

const OWNERS_PATH: &str = "assure.toml";

#[test]
fn old_gaps_refuse_no_commit_or_push_but_a_touch_must_close_them_and_a_pr_must_close_one()
-> Result<()> {
    let catalog = catalog()?;
    let fixture = Fixture::new()?;
    let root = &fixture.root;
    fixture.write("src/a.rs", PARSER)?;
    fixture.write("src/b.rs", PARSER)?;
    let old = fixture.commit("older gaps")?;
    fixture.git(&["update-ref", "refs/remotes/origin/main", &old])?;

    fixture.write("README.md", "changed\n")?;
    fixture.stage()?;
    assert_eq!(rule(assure::commit(root, &catalog)), None);
    assert_eq!(assure::stop(root, &catalog)?.verdict, Verdict::Admit);
    let unrelated = fixture.commit("unrelated")?;
    let line = format!("refs/heads/main {unrelated} refs/heads/main {old}\n");
    assert_eq!(
        rule(assure::push(root, &catalog, "origin", line.as_bytes())),
        None
    );
    assert_eq!(
        rule(assure::ready(root, &catalog, "main", &unrelated)).as_deref(),
        Some("assure.stalled")
    );

    fixture.write("src/a.rs", &format!("{PARSER}// a touch\n"))?;
    let touched = assure::stop(root, &catalog)?;
    assert_eq!(touched.verdict, Verdict::Touched);
    assert_eq!(touched.gaps.len(), 1);

    fixture.write(
        "src/a.rs",
        &format!("// Exempt as disproportionate: a fixture parser of one digit\n{PARSER}"),
    )?;
    assert_eq!(assure::stop(root, &catalog)?.verdict, Verdict::Admit);
    let closing = fixture.commit("exempt one parser")?;
    assert_eq!(rule(assure::ready(root, &catalog, "main", &closing)), None);

    fixture.write(
        "src/b.rs",
        &format!("// Exempt as later: someday\n{PARSER}"),
    )?;
    let malformed = assure::stop(root, &catalog)?;
    assert_eq!(malformed.verdict, Verdict::Touched);
    assert_eq!(
        malformed.gaps.first().map(|gap| gap.cause),
        Some(Cause::MalformedExemption)
    );
    Ok(())
}

#[test]
fn a_pr_that_weakens_a_budget_is_refused() -> Result<()> {
    let catalog = catalog()?;
    let fixture = Fixture::new()?;
    let root = &fixture.root;
    fixture.write(
        OWNERS_PATH,
        "[[budget]]\nname = \"mutants-missed\"\nceiling = 0\n",
    )?;
    let base = fixture.commit("budget")?;
    fixture.git(&["update-ref", "refs/remotes/origin/main", &base])?;
    fixture.write(
        OWNERS_PATH,
        "[[budget]]\nname = \"mutants-missed\"\nceiling = 1\n",
    )?;
    let raised = fixture.commit("raise the ceiling")?;
    assert_eq!(
        rule(assure::ready(root, &catalog, "main", &raised)).as_deref(),
        Some("assure.lowered")
    );
    fixture.write(OWNERS_PATH, "")?;
    let removed = fixture.commit("drop the budget")?;
    assert_eq!(
        rule(assure::ready(root, &catalog, "main", &removed)).as_deref(),
        Some("assure.lowered")
    );
    fixture.write(
        OWNERS_PATH,
        "[[budget]]\nname = \"mutants-missed\"\nceiling = 0\n",
    )?;
    let kept = fixture.commit("keep the budget")?;
    assert_eq!(rule(assure::ready(root, &catalog, "main", &kept)), None);
    Ok(())
}

#[test]
fn a_decision_module_is_owned_by_its_own_harness_and_tests_are_never_read() -> Result<()> {
    let catalog = catalog()?;
    let fixture = Fixture::new()?;
    let root = &fixture.root;
    fixture.write("src/gate_rules.rs", "pub fn admit() -> bool { true }\n")?;
    fixture.write("tests/parse.rs", PARSER)?;
    fixture.stage()?;
    let refused = assure::stop(root, &catalog)?;
    assert_eq!(refused.verdict, Verdict::Added);
    assert_eq!(refused.gaps.len(), 1);
    assert_eq!(
        refused.gaps.first().map(|gap| gap.shape.as_str()),
        Some("decision")
    );
    fixture.write(
        "src/gate_rules.rs",
        "pub fn admit() -> bool { true }\n#[cfg(kani)]\n#[kani::proof]\nfn admits() { assert!(admit()); }\n",
    )?;
    fixture.stage()?;
    assert_eq!(rule(assure::commit(root, &catalog)), None);
    Ok(())
}

#[test]
fn this_repository_has_no_gap() -> Result<()> {
    let catalog = catalog()?;
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let survey = assure::survey(&root, &catalog, &assure::Tree::WorkingTree, None)?;
    let gaps: Vec<String> = survey
        .gaps
        .iter()
        .map(|gap| assure::describe(&catalog, gap))
        .collect();
    assert!(gaps.is_empty(), "{gaps:#?}");
    Ok(())
}
