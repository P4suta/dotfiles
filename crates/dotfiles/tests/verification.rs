//! Explicit receipts preserve check ordering and source identity without coupling managed hooks.

use anyhow::{Result, ensure};
use serde_json::{Value, json};
use sha2::{Digest as _, Sha256};
use std::fs;
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::process::{Output, Stdio};

#[path = "../../dotguard/tests/support/fixture_git.rs"]
mod fixture_git;

#[path = "support/catalog.rs"]
mod catalog;

#[path = "support/mutation_inventory.rs"]
mod mutation_inventory;

#[test]
fn module_mutation_selection_covers_every_visitor_method() -> Result<()> {
    let selection = regex::Regex::new(mutation_inventory::ADAPTER_SELECTION)?;
    for method in [
        "visit_item_mod_mut",
        "visit_item_macro_mut",
        "visit_macro_mut",
    ] {
        let native = format!(
            "{}: replace <impl syn::visit_mut::VisitMut for ModuleDeclarations<'_>>::{method} with ()",
            mutation_inventory::SOURCE
        );
        ensure!(selection.is_match(&native), "selection omits {method}");
    }
    Ok(())
}

#[test]
fn changed_source_functions_cannot_escape_the_mutation_selection() -> Result<()> {
    let selection = regex::Regex::new(mutation_inventory::ADAPTER_SELECTION)?;
    for name in [
        "replace verification -> bool with true",
        "replace || with && in verification",
    ] {
        ensure!(
            selection.is_match(&format!("{}:206:5: {name}", mutation_inventory::SOURCE)),
            "selection omits {name}"
        );
    }
    Ok(())
}

#[test]
fn function_changes_preserve_literal_bytes_and_require_real_native_owners() -> Result<()> {
    let before = "fn unchanged() -> bool { false } fn verification() -> bool { false }";
    let after = "fn unchanged() -> bool { false } fn verification() -> bool { true } fn added() {}";
    assert_eq!(
        mutation_inventory::changed_functions(before, after)?,
        ["added".to_owned(), "verification".to_owned()].into()
    );
    ensure!(mutation_inventory::changed_functions(before, &before.replace(' ', "\n"))?.is_empty());
    ensure!(
        !mutation_inventory::changed_functions(
            "fn f() {let _=\"a b\";}",
            "fn f() {let _=\"ab\";}"
        )?
        .is_empty()
    );
    let trait_before = "struct Owner; impl syn::visit_mut::VisitMut for Owner { fn visit_file_mut(&mut self, file: &mut syn::File) {} }";
    let trait_after = trait_before.replace("{}", "{ self.visit_item_mut(&mut file.items[0]); }");
    let methods = mutation_inventory::changed_functions(trait_before, &trait_after)?;
    assert_eq!(
        methods,
        ["<implsyn::visit_mut::VisitMutforOwner>::visit_file_mut".to_owned()].into()
    );
    let entry = |file: &str, name: &str| json!({"file":file,"function":{"function_name":name}});
    mutation_inventory::validate_changed(
        "",
        after,
        &json!([
            entry(mutation_inventory::SOURCE, "unchanged"),
            entry(mutation_inventory::SOURCE, "verification"),
            entry(mutation_inventory::SOURCE, "added")
        ]),
    )?;
    for omitted in ["added", "verification"] {
        let other = if omitted == "added" {
            "verification"
        } else {
            "added"
        };
        for invalid in [
            None,
            Some(entry("other.rs", omitted)),
            Some(entry(
                mutation_inventory::SOURCE,
                &format!("Other::{omitted}"),
            )),
        ] {
            let mut entries = vec![entry(mutation_inventory::SOURCE, other)];
            entries.extend(invalid);
            let error =
                mutation_inventory::validate_changed(before, after, &json!(entries)).unwrap_err();
            ensure!(error.to_string().contains(omitted));
        }
    }
    ensure!(mutation_inventory::changed_functions("", "mod production { fn f() {} }").is_err());
    ensure!(mutation_inventory::changed_functions("", "fn f() {} fn f() {} ").is_err());
    ensure!(
        mutation_inventory::changed_functions("", "#[cfg(test)] mod tests {fn f() {}}")?.is_empty()
    );
    ensure!(mutation_inventory::changed_functions("", "fn malformed(").is_err());
    Ok(())
}

#[test]
fn native_mutation_shards_form_an_exact_partition() -> Result<()> {
    let first = json!({"file":"source.rs","name":"first"});
    let second = json!({"file":"source.rs","name":"second"});
    let expected = json!([first, second]);
    mutation_inventory::complete(&expected, &[json!([first]), json!([second])])?;
    for shards in [
        vec![],
        vec![json!([first])],
        vec![json!([first]), json!([first, second])],
        vec![json!([first, first, second])],
        vec![json!([first, second]), json!([])],
        vec![json!([first, second, {"file":"source.rs","name":"extra"}])],
        vec![json!([first, {"file":"other.rs","name":"second"}])],
        vec![json!([first, {"file":"source.rs"}])],
        vec![json!([first, {"file":"source.rs","name":"second","replacement":"changed"}])],
    ] {
        ensure!(mutation_inventory::complete(&expected, &shards).is_err());
    }
    ensure!(mutation_inventory::complete(&json!([]), &[]).is_err());
    ensure!(mutation_inventory::complete(&json!([first, first]), &[json!([first])]).is_err());
    Ok(())
}

#[test]
fn mutation_campaign_collects_every_shard_and_reports_every_failure() -> Result<()> {
    for failed in [vec![], vec![0], vec![1], vec![2], vec![0, 2]] {
        let mut visited = Vec::new();
        let result = mutation_inventory::collect_shards(3, |index| {
            visited.push(index);
            ensure!(!failed.contains(&index), "injected failure {index}");
            Ok(json!(index))
        });
        assert_eq!(visited, [0, 1, 2]);
        if failed.is_empty() {
            assert_eq!(result?, [json!(0), json!(1), json!(2)]);
        } else {
            let error = result.unwrap_err().to_string();
            for index in failed {
                ensure!(error.contains(&format!("injected failure {index}")));
            }
        }
    }
    ensure!(mutation_inventory::collect_shards(0, |_| Ok(json!(0))).is_err());
    Ok(())
}

#[test]
fn module_mutation_inventory_rejects_omitted_or_misattributed_methods() -> Result<()> {
    let source = include_str!("../src/verification/source.rs");
    let methods = [
        "visit_item_mod_mut",
        "visit_item_macro_mut",
        "visit_macro_mut",
    ];
    let entries: Vec<_> = methods.iter().map(|method| json!({
        "file": mutation_inventory::SOURCE,
        "function": {"function_name": format!("<impl syn::visit_mut::VisitMut for ModuleDeclarations<'_>>::{method}")}
    })).collect();
    mutation_inventory::validate(source, &json!(entries))?;
    ensure!(
        mutation_inventory::module_methods(source)?.len() == methods.len(),
        "visitor method fixture changed"
    );
    for omitted in 0..entries.len() {
        let mut incomplete = entries.clone();
        incomplete.remove(omitted);
        ensure!(mutation_inventory::validate(source, &json!(incomplete)).is_err());
        for field in ["file", "function"] {
            let mut misattributed = entries.clone();
            misattributed[omitted][field] = if field == "file" {
                json!("other.rs")
            } else {
                json!({"function_name": format!("<impl syn::visit_mut::VisitMut for OtherModuleDeclarations<'_>>::{}", methods[omitted])})
            };
            ensure!(mutation_inventory::validate(source, &json!(misattributed)).is_err());
        }
    }
    ensure!(mutation_inventory::validate(source, &json!([])).is_err());
    ensure!(mutation_inventory::validate(source, &json!({})).is_err());
    ensure!(mutation_inventory::validate("", &json!(entries)).is_err());
    Ok(())
}

struct Fixture {
    root: PathBuf,
    home: PathBuf,
    plan: PathBuf,
    _scope: tempfile::TempDir,
}

impl Fixture {
    fn new(behavior: bool) -> Result<Self> {
        let scope = tempfile::tempdir()?;
        let root = scope.path().join("repository");
        let home = scope.path().join("home");
        fs::create_dir_all(root.join("src"))?;
        fs::create_dir_all(root.join("tests"))?;
        let request = home
            .join(".local/state/dotfiles/answers")
            .join(platform::build_inputs::hex(&Sha256::digest(b"session")));
        fs::create_dir_all(&request)?;
        fs::write(
            request.join("request.json"),
            serde_json::to_vec(&json!({
                "version": 1, "generation": ".tmpfixture", "prompt": "Implement the requested behavior."
            }))?,
        )?;
        fs::write(
            root.join("Cargo.toml"),
            "[package]\nname = \"verification-fixture\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
        )?;
        fs::write(
            root.join("Cargo.lock"),
            "# This file is automatically @generated by Cargo.\n# It is not intended for manual editing.\nversion = 4\n\n[[package]]\nname = \"verification-fixture\"\nversion = \"0.1.0\"\n",
        )?;
        fs::write(root.join(".gitignore"), "/target/\n")?;
        let rust = platform::tool::Tool::Mise
            .command()
            .args(["where", "rust"])
            .output()?;
        ensure!(rust.status.success(), "{rust:?}");
        let rust = String::from_utf8(rust.stdout)?;
        let version = Path::new(rust.trim())
            .file_name()
            .unwrap()
            .to_str()
            .unwrap();
        fs::write(
            root.join("mise.toml"),
            toml::to_string(&json!({"tools": {"rust": version}}))?,
        )?;
        fs::write(
            root.join("src/lib.rs"),
            "pub fn allowed() -> bool { false }\n",
        )?;
        fs::write(
            root.join("tests/contract.rs"),
            "#[test]\nfn contract() { assert!(verification_fixture::allowed()); }\n",
        )?;
        let output = fixture_git::command("git", &home.join("gitconfig"))
            .args(["init", "--quiet"])
            .current_dir(&root)
            .output()?;
        ensure!(output.status.success(), "{output:?}");
        let plan = scope.path().join("plan.json");
        let mut checks = vec![json!({
            "id": "tests", "method": "test", "tool": "cargo", "arguments": ["test", "--offline", "--test", "contract", "--", "--test-threads=1"],
            "report": "libtest", "expected": ["contract"]
        })];
        if behavior {
            checks.extend([
                json!({"id": "proof", "method": "proof", "tool": "just", "arguments": ["proofs"], "report": "kani", "expected": ["allowed"], "rejecting": ["refuses_false_claim"]}),
                json!({"id": "mutation", "method": "mutation", "tool": "cargo", "arguments": ["mutants", "--locked"], "report": "mutants", "expected": ["src/lib.rs"], "report_path": "target/mutants/outcomes.json", "verifier_version": "fixture"})
            ]);
        }
        let contract = if behavior {
            json!({"path": "src", "change": "behavior", "risks": ["pure"], "checks": ["tests", "proof", "mutation"]})
        } else {
            json!({"path": "tests", "change": "verification", "risks": [], "checks": ["tests"]})
        };
        fs::write(
            &plan,
            serde_json::to_vec(&json!({"version": 1, "contracts": [contract], "checks": checks}))?,
        )?;
        Ok(Self {
            root,
            home,
            plan,
            _scope: scope,
        })
    }

    fn invoke_at(&self, cwd: &Path, arguments: &[&str], input: Option<Value>) -> Result<Output> {
        let executable = std::env::var_os("DOTFILES_VERIFICATION_EXECUTABLE")
            .unwrap_or_else(|| env!("CARGO_BIN_EXE_dotfiles").into());
        ensure!(
            Path::new(&executable).is_absolute(),
            "verification fixture executable must be absolute"
        );
        let mut command = fixture_git::command(executable, &self.home.join("gitconfig"));
        let original = platform::home::home().unwrap();
        command.env("MISE_DATA_DIR", original.join(".local/share/mise"));
        for (name, directory) in [("CARGO_HOME", ".cargo"), ("RUSTUP_HOME", ".rustup")] {
            command.env(
                name,
                std::env::var_os(name).unwrap_or_else(|| original.join(directory).into()),
            );
        }
        let mut child = command
            .current_dir(cwd)
            .env("HOME", &self.home)
            .env("USERPROFILE", &self.home)
            .env("CARGO_TARGET_DIR", self.root.join("target"))
            .env("MISE_TRUSTED_CONFIG_PATHS", &self.root)
            .args(arguments)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;
        if let Some(input) = input {
            child
                .stdin
                .take()
                .unwrap()
                .write_all(&serde_json::to_vec(&input)?)?;
        }
        Ok(child.wait_with_output()?)
    }

    fn invoke(&self, arguments: &[&str]) -> Result<Output> {
        self.invoke_at(&self.root, arguments, None)
    }

    fn begin(&self) -> Result<()> {
        let result = self.invoke(&[
            "verify",
            "begin",
            "--session",
            "session",
            "--plan",
            self.plan.to_str().unwrap(),
        ])?;
        ensure!(result.status.success(), "{result:?}");
        Ok(())
    }

    fn run(&self, red: bool) -> Result<Output> {
        let mut arguments = vec!["verify", "run", "--session", "session", "--check", "tests"];
        if red {
            arguments.push("--red");
        }
        self.invoke(&arguments)
    }

    fn production(&self, allowed: bool) -> Result<()> {
        fs::write(
            self.root.join("src/lib.rs"),
            format!("pub fn allowed() -> bool {{ {allowed} }}\n"),
        )?;
        Ok(())
    }

    fn full_tests(&self) -> Result<()> {
        let mut full: Value = serde_json::from_slice(&fs::read(&self.plan)?)?;
        let mut check = full["checks"][0].clone();
        check["id"] = "all-tests".into();
        check["scope"] = "full".into();
        full["checks"].as_array_mut().unwrap().push(check);
        full["contracts"][0]["checks"]
            .as_array_mut()
            .unwrap()
            .push("all-tests".into());
        for path in ["Cargo.toml", "Cargo.lock", "mise.toml"] {
            full["contracts"].as_array_mut().unwrap().push(json!({"path": path, "change": "configuration", "risks": [], "checks": ["all-tests"]}));
        }
        full["contracts"].as_array_mut().unwrap().push(json!({"path": "tests", "change": "verification", "risks": [], "checks": ["all-tests"]}));
        fs::write(&self.plan, serde_json::to_vec(&full)?)?;
        self.begin()?;
        let full = self.invoke(&[
            "verify",
            "run",
            "--session",
            "session",
            "--check",
            "all-tests",
        ])?;
        ensure!(full.status.success(), "{full:?}");
        let full = self.invoke(&["verify", "check", "--session", "session", "--release"])?;
        ensure!(
            !full.status.success() && String::from_utf8_lossy(&full.stderr).contains("TDD=true"),
            "full-scope tests lost the original TDD order: {full:?}"
        );
        Ok(())
    }
}

#[test]
fn completion_readers_share_a_native_lock_and_writers_stay_exclusive() -> Result<()> {
    let fixture = Fixture::new(false)?;
    fixture.begin()?;
    let directory = fixture
        .home
        .join(".local/state/dotfiles/answers")
        .join(platform::build_inputs::hex(&Sha256::digest(b"session")));
    let reader = fs::File::options()
        .read(true)
        .write(true)
        .open(directory.join("verification.lock"))?;
    reader.try_lock_shared()?;
    for _ in 0..16 {
        let checked = fixture.invoke(&["verify", "check", "--session", "session"])?;
        ensure!(checked.status.success(), "{checked:?}");
    }
    let writer = fixture.invoke(&[
        "verify",
        "begin",
        "--session",
        "session",
        "--plan",
        fixture.plan.to_str().unwrap(),
    ])?;
    ensure!(
        !writer.status.success()
            && String::from_utf8_lossy(&writer.stderr).contains("holds this session"),
        "{writer:?}"
    );
    Ok(())
}

#[test]
fn an_active_native_writer_refuses_completion_until_it_releases() -> Result<()> {
    let fixture = Fixture::new(false)?;
    fixture.begin()?;
    let directory = fixture
        .home
        .join(".local/state/dotfiles/answers")
        .join(platform::build_inputs::hex(&Sha256::digest(b"session")));
    let writer = fs::File::options()
        .read(true)
        .write(true)
        .open(directory.join("verification.lock"))?;
    writer.try_lock()?;
    let checked = fixture.invoke(&["verify", "check", "--session", "session"])?;
    ensure!(
        !checked.status.success()
            && String::from_utf8_lossy(&checked.stderr).contains("holds this session"),
        "{checked:?}"
    );
    drop(writer);
    let checked = fixture.invoke(&["verify", "check", "--session", "session"])?;
    ensure!(checked.status.success(), "{checked:?}");
    Ok(())
}

#[test]
fn project_checks_do_not_require_a_second_completion_plan() -> Result<()> {
    let fixture = Fixture::new(false)?;
    catalog::install(&fixture.home)?;
    let begun = fixture.invoke_at(
        &fixture.root,
        &["answer", "begin"],
        Some(json!({"session_id": "session", "cwd": fixture.root, "prompt": "Implement the requested behavior."})),
    )?;
    ensure!(begun.status.success(), "{begun:?}");
    fixture.production(true)?;
    let stopped = fixture.invoke_at(
        &fixture.root,
        &["assure", "stop"],
        Some(json!({"session_id": "session", "cwd": fixture.root})),
    )?;
    ensure!(stopped.status.success(), "{stopped:?}");
    let decision: Value = serde_json::from_slice(&stopped.stdout)?;
    assert_ne!(decision["decision"], "block", "{stopped:?}");
    Ok(())
}

#[test]
fn a_bounded_check_reenters_the_current_runtime_and_never_credits_a_refused_image() -> Result<()> {
    let fixture = Fixture::new(false)?;
    let mut plan: Value = serde_json::from_slice(&fs::read(&fixture.plan)?)?;
    plan["checks"][0]["tool"] = "dotfiles".into();
    plan["checks"][0]["arguments"] = json!([
        "bounded", "run", "--image", "unpinned", "--", "cargo", "test"
    ]);
    fs::write(&fixture.plan, serde_json::to_vec(&plan)?)?;
    fixture.begin()?;
    fs::write(
        fixture.root.join("tests/contract.rs"),
        "#[test]\nfn contract() { assert!(!verification_fixture::allowed()); }\n",
    )?;
    let result = fixture.run(false)?;
    assert!(!result.status.success());
    assert!(
        String::from_utf8_lossy(&result.stderr).contains("immutable image"),
        "{result:?}"
    );
    let completion = fixture.invoke(&["verify", "check", "--session", "session"])?;
    assert!(!completion.status.success());
    Ok(())
}

#[test]
fn verification_configuration_changes_preserve_executable_code() -> Result<()> {
    for (path, before, after, admitted) in [
        (
            "justfile",
            "test:\n    cargo test\n",
            "test:\n    cargo test --locked\n",
            true,
        ),
        (
            "src/inventory.rs",
            "pub const CASES: &[&str] = &[\"first\"];",
            "pub const CASES: &[&str] = &[\"first\", \"second\"];",
            true,
        ),
        (
            "src/inventory.rs",
            "pub const CASES: &[&str] = &[\"first\"]; pub fn run() -> bool { false }",
            "pub const CASES: &[&str] = &[\"second\"]; pub fn run() -> bool { false }",
            true,
        ),
        (
            "src/inventory.rs",
            "pub const CASES: &[&str] = &[\"first\"]; pub fn run() -> bool { false }",
            "pub const CASES: &[&str] = &[\"second\"]; pub fn run() -> bool { true }",
            false,
        ),
        (
            "src/inventory.rs",
            "pub const CASES: &[&str] = &[\"first\"]; pub fn run() -> bool { false }",
            "pub const CASES: &[&str] = &[\"second\"];",
            false,
        ),
        (
            "src/inventory.rs",
            "pub const CASES: &[&str] = &[\"first\"];",
            "pub const CASES: &[&str] = &[concat!(\"first\", \"second\")];",
            false,
        ),
        (
            "src/inventory.rs",
            "pub const CASES: &[&str] = &[\"first\"];",
            "#[custom] pub const CASES: &[&str] = &[\"second\"];",
            false,
        ),
        (
            "src/inventory.rs",
            "pub const CASES: &[&str] = &[\"first\"];",
            "#![cfg(any())] pub const CASES: &[&str] = &[\"second\"];",
            false,
        ),
    ] {
        let fixture = Fixture::new(false)?;
        fs::write(fixture.root.join(path), before)?;
        let mut plan: Value = serde_json::from_slice(&fs::read(&fixture.plan)?)?;
        plan["contracts"] =
            json!([{"path":path,"change":"configuration","risks":[],"checks":["tests"]}]);
        fs::write(&fixture.plan, serde_json::to_vec(&plan)?)?;
        fixture.begin()?;
        fs::write(fixture.root.join(path), after)?;
        let output = fixture.invoke(&["verify", "check", "--session", "session"])?;
        let error = String::from_utf8(output.stderr)?;
        ensure!(!output.status.success(), "an untested configuration passed");
        ensure!(
            error.contains(if admitted {
                "verification.missing"
            } else {
                "configuration contract covers production"
            }),
            "wrong classification for {path}: {error}"
        );
    }
    outlined_module_helpers_preserve_production_owners()
}

fn outlined_module_helpers_preserve_production_owners() -> Result<()> {
    for (declaration, admitted) in [
        ("#[cfg(kani)] mod checks;", true),
        ("#[cfg(test)] mod checks;", true),
        ("mod checks;", false),
        (
            "#[cfg(kani)] mod checks; #[path=\"checks.rs\"] mod alias;",
            false,
        ),
        ("#[cfg(kani)] mod checks; include!(\"checks.rs\");", false),
    ] {
        let fixture = Fixture::new(false)?;
        fs::write(fixture.root.join("src/lib.rs"), declaration)?;
        let path = "src/checks.rs";
        fs::write(fixture.root.join(path), "fn helper() -> bool { false }")?;
        let mut plan: Value = serde_json::from_slice(&fs::read(&fixture.plan)?)?;
        plan["contracts"] =
            json!([{"path":path,"change":"verification","risks":[],"checks":["tests"]}]);
        fs::write(&fixture.plan, serde_json::to_vec(&plan)?)?;
        fixture.begin()?;
        fs::write(fixture.root.join(path), "fn helper() -> bool { true }")?;
        let output = fixture.invoke(&["verify", "check", "--session", "session"])?;
        let error = String::from_utf8(output.stderr)?;
        ensure!(!output.status.success(), "untested module changes passed");
        ensure!(
            error.contains(if admitted {
                "verification.missing"
            } else {
                "verification-only contract changed production"
            }),
            "{declaration}: {error}"
        );
    }
    Ok(())
}

#[test]
fn explicit_checks_refuse_missing_results_and_late_red_cannot_rewrite_history() -> Result<()> {
    let fixture = Fixture::new(true)?;
    fixture.begin()?;
    fixture.production(true)?;
    let check = fixture.invoke(&["verify", "check", "--session", "session"])?;
    ensure!(
        !check.status.success()
            && String::from_utf8_lossy(&check.stderr).contains("verification.missing"),
        "{check:?}"
    );
    let red = fixture.run(true)?;
    ensure!(
        !red.status.success()
            && String::from_utf8_lossy(&red.stderr).contains("ran after production changed"),
        "{red:?}"
    );
    Ok(())
}

#[test]
fn actual_red_and_green_do_not_replace_missing_proofs_or_mutation_and_drift_is_stale() -> Result<()>
{
    let fixture = Fixture::new(true)?;
    fixture.begin()?;
    let red = fixture.run(true)?;
    ensure!(red.status.success(), "{red:?}");
    fixture.production(true)?;
    let green = fixture.run(false)?;
    ensure!(green.status.success(), "{green:?}");
    let check = fixture.invoke(&["verify", "check", "--session", "session"])?;
    ensure!(
        !check.status.success() && String::from_utf8_lossy(&check.stderr).contains("TDD=true"),
        "{check:?}"
    );
    let request = fixture
        .home
        .join(".local/state/dotfiles/answers")
        .join(platform::build_inputs::hex(&Sha256::digest(b"session")))
        .join("request.json");
    fixture.full_tests()?;
    let mut generation: Value = serde_json::from_slice(&fs::read(&request)?)?;
    generation["generation"] = ".tmpcontinuation".into();
    fs::write(request, serde_json::to_vec(&generation)?)?;
    let continued = fixture.invoke(&["verify", "check", "--session", "session"])?;
    ensure!(
        !continued.status.success(),
        "a new user message erased unfinished verification"
    );
    fs::write(
        fixture.root.join("src/lib.rs"),
        "pub fn allowed() -> bool { std::hint::black_box(false) }\n",
    )?;
    let stale = fixture.invoke(&["verify", "check", "--session", "session"])?;
    ensure!(
        !stale.status.success()
            && String::from_utf8_lossy(&stale.stderr).contains("verification.stale"),
        "{stale:?}"
    );
    let failed = fixture.run(false)?;
    ensure!(!failed.status.success(), "{failed:?}");
    let refused = fixture.invoke(&["verify", "check", "--session", "session"])?;
    ensure!(
        String::from_utf8_lossy(&refused.stderr).contains("attempt is incomplete"),
        "{refused:?}"
    );
    fixture.production(false)?;
    let restored = fixture.invoke(&["verify", "check", "--session", "session"])?;
    ensure!(
        restored.status.success(),
        "restoring the baseline retained a verification obligation: {restored:?}"
    );
    Ok(())
}

#[test]
fn verification_only_work_passes_real_tests_and_a_source_edit_inside_a_test_invalidates_the_run()
-> Result<()> {
    let fixture = Fixture::new(false)?;
    fixture.begin()?;
    fs::write(
        fixture.root.join("tests/contract.rs"),
        "#[test]\nfn contract() { assert!(!verification_fixture::allowed()); }\n",
    )?;
    let green = fixture.run(false)?;
    ensure!(green.status.success(), "{green:?}");
    let checked = fixture.invoke(&["verify", "check", "--session", "session"])?;
    ensure!(checked.status.success(), "{checked:?}");
    let release = fixture.invoke(&["verify", "check", "--session", "session", "--release"])?;
    ensure!(
        !release.status.success(),
        "affected-only evidence admitted release"
    );
    fs::write(
        fixture.root.join("tests/contract.rs"),
        "#[test]\nfn contract() { std::fs::write(\"src/lib.rs\", \"pub fn allowed() -> bool { true }\\n\").unwrap(); }\n",
    )?;
    let changed = fixture.run(false)?;
    ensure!(
        !changed.status.success()
            && String::from_utf8_lossy(&changed.stderr)
                .contains("source changed while verification"),
        "{changed:?}"
    );
    Ok(())
}

#[test]
fn project_checks_allow_edits_without_a_parallel_verification_plan() -> Result<()> {
    let fixture = Fixture::new(true)?;
    let payload = json!({"session_id": "session", "cwd": fixture.root.parent().unwrap(), "tool_name": "Write", "tool_input": {"file_path": fixture.root.join("src/lib.rs")}});
    let admitted = fixture.invoke_at(
        fixture.root.parent().unwrap(),
        &["issue-guard"],
        Some(payload.clone()),
    )?;
    ensure!(admitted.status.success(), "{admitted:?}");
    fixture.begin()?;
    let mut narrowed: Value = serde_json::from_slice(&fs::read(&fixture.plan)?)?;
    narrowed["contracts"][0] =
        json!({"path": "src", "change": "verification", "risks": [], "checks": ["tests"]});
    fs::write(&fixture.plan, serde_json::to_vec(&narrowed)?)?;
    let change = fixture.invoke(&[
        "verify",
        "begin",
        "--session",
        "session",
        "--plan",
        fixture.plan.to_str().unwrap(),
    ])?;
    ensure!(
        !change.status.success(),
        "existing behavior obligations were narrowed"
    );
    let admitted = fixture.invoke_at(
        fixture.root.parent().unwrap(),
        &["issue-guard"],
        Some(payload),
    )?;
    ensure!(admitted.status.success(), "{admitted:?}");
    Ok(())
}

#[test]
fn project_checks_leave_an_active_verification_runner_alone() -> Result<()> {
    let fixture = Fixture::new(false)?;
    fixture.begin()?;
    let directory = fixture
        .home
        .join(".local/state/dotfiles/answers")
        .join(platform::build_inputs::hex(&Sha256::digest(b"session")));
    let writer = fs::File::options()
        .read(true)
        .write(true)
        .open(directory.join("verification.lock"))?;
    writer.try_lock()?;
    let payload = json!({"session_id":"session","cwd":fixture.root,"tool_name":"Write","tool_input":{"file_path":fixture.root.join("ios/UITests/DirectoryUITests.swift")}});
    let edited = fixture.invoke_at(&fixture.root, &["issue-guard"], Some(payload))?;
    ensure!(edited.status.success(), "{edited:?}");
    let forbidden = json!({"session_id":"session","cwd":fixture.root,"tool_name":"Write","tool_input":{"file_path":fixture.root.join(".git/config")}});
    let refused = fixture.invoke_at(&fixture.root, &["issue-guard"], Some(forbidden))?;
    ensure!(
        !refused.status.success(),
        "Git metadata writes must remain forbidden: {refused:?}"
    );
    Ok(())
}

#[test]
fn a_refactor_requires_a_passing_baseline_without_inventing_a_red_failure() -> Result<()> {
    let fixture = Fixture::new(false)?;
    let original = "fn baseline() -> bool { false }\npub fn allowed() -> bool { baseline() }\n";
    let changed = "fn decision() -> bool { false }\npub fn allowed() -> bool { decision() }\n";
    fs::write(fixture.root.join("src/lib.rs"), original)?;
    fs::write(
        fixture.root.join("tests/contract.rs"),
        "#[test]\nfn contract() { assert!(!verification_fixture::allowed()); }\n",
    )?;
    let mut plan: Value = serde_json::from_slice(&fs::read(&fixture.plan)?)?;
    plan["contracts"][0]["path"] = "src".into();
    plan["contracts"][0]["change"] = "refactor".into();
    fs::write(&fixture.plan, serde_json::to_vec(&plan)?)?;
    fixture.begin()?;
    fs::write(fixture.root.join("src/lib.rs"), changed)?;
    let late = fixture.invoke(&[
        "verify",
        "run",
        "--session",
        "session",
        "--check",
        "tests",
        "--baseline",
    ])?;
    ensure!(
        !late.status.success(),
        "a late passing baseline rewrote chronology"
    );
    fs::write(fixture.root.join("src/lib.rs"), original)?;
    let before = fixture.invoke(&[
        "verify",
        "run",
        "--session",
        "session",
        "--check",
        "tests",
        "--baseline",
    ])?;
    ensure!(before.status.success(), "{before:?}");
    fs::write(fixture.root.join("src/lib.rs"), changed)?;
    let after = fixture.run(false)?;
    ensure!(after.status.success(), "{after:?}");
    let checked = fixture.invoke(&["verify", "check", "--session", "session"])?;
    ensure!(checked.status.success(), "{checked:?}");
    Ok(())
}
