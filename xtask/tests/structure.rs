//! The structural gates refuse a fixture core that reaches an effect, returns bare text, or panics, and admit it once fixed.

use anyhow::Result;
use rules::boundary_rules::{Breach, breach};
use std::fs;
use std::path::{Path, PathBuf};
use syn::visit::Visit;
use xtask::structure::{CATALOG, CORE, core};
use xtask::tool::Tool;

#[derive(Default)]
struct ToolRemoval(Vec<String>);

impl<'ast> Visit<'ast> for ToolRemoval {
    fn visit_item_mod(&mut self, module: &'ast syn::ItemMod) {
        if module.attrs.iter().any(|attribute| {
            matches!(&attribute.meta, syn::Meta::List(list) if list.path.is_ident("cfg") && list.tokens.to_string() == "test")
        }) {
            return;
        }
        syn::visit::visit_item_mod(self, module);
    }

    fn visit_lit_str(&mut self, literal: &'ast syn::LitStr) {
        let value = literal.value();
        if matches!(value.as_str(), "prune" | "uninstall" | "implode") {
            self.0.push(value);
        }
    }
}

#[test]
fn apply_setup_cannot_delete_mise_versions() -> Result<()> {
    let root = root();
    for path in [
        "crates/dotfiles/src/setup.rs",
        "crates/dotfiles/src/windows/setup/tools.rs",
    ] {
        let mut removals = ToolRemoval::default();
        removals.visit_file(&syn::parse_file(&fs::read_to_string(root.join(path))?)?);
        assert!(
            removals.0.is_empty(),
            "{path} removes tool versions: {:?}",
            removals.0
        );
    }
    for operation in ["prune", "uninstall", "implode"] {
        let mut removals = ToolRemoval::default();
        removals.visit_file(&syn::parse_file(&format!(
            "fn install() {{ command().arg({operation:?}); }}"
        ))?);
        assert_eq!(removals.0, [operation]);
    }
    Ok(())
}

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("the package sits inside the workspace")
        .to_path_buf()
}

#[test]
fn the_compiler_refuses_a_process_that_bypasses_git_environment_isolation() -> Result<()> {
    let fixture = tempfile::tempdir()?;
    fs::create_dir(fixture.path().join("src"))?;
    fs::write(
        fixture.path().join("Cargo.toml"),
        "[package]\nname = 'process-boundary-fixture'\nversion = '0.0.0'\nedition = '2024'\n",
    )?;
    fs::copy(
        root().join("clippy.toml"),
        fixture.path().join("clippy.toml"),
    )?;
    let run = || {
        Tool::Cargo
            .command()
            .current_dir(fixture.path())
            .args([
                "clippy",
                "--offline",
                "--quiet",
                "--",
                "-D",
                "clippy::disallowed_methods",
            ])
            .output()
    };
    for source in [
        "pub fn spawn() { let _ = std::process::Command::new(\"cargo\"); }",
        "use std::process::Command as Child; pub fn spawn() { let _ = Child::new(\"cargo\"); }",
    ] {
        fs::write(fixture.path().join("src/lib.rs"), source)?;
        let output = run()?;
        assert!(!output.status.success());
        assert!(String::from_utf8_lossy(&output.stderr).contains("disallowed"));
    }
    fs::write(
        fixture.path().join("src/lib.rs"),
        "pub fn spawn(command: &mut std::process::Command) -> std::io::Result<std::process::Child> { command.spawn() }",
    )?;
    let output = run()?;
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    Ok(())
}

/// Whether the catalog admits `files` written into a fixture repository, and its report.
fn scan(files: &[(&str, &str)]) -> Result<(bool, String)> {
    let fixture = tempfile::tempdir()?;
    for (path, text) in files {
        let path = fixture.path().join(path);
        fs::create_dir_all(path.parent().expect("a fixture path has a parent"))?;
        fs::write(path, text)?;
    }
    let output = Tool::AstGrep
        .command()
        .current_dir(fixture.path())
        .arg("scan")
        .arg("--config")
        .arg(root().join(CATALOG).join("sgconfig.yml"))
        .arg(".")
        .output()?;
    Ok((
        output.status.success(),
        String::from_utf8_lossy(&output.stdout).into_owned(),
    ))
}

const VIOLATING: &str = "pub fn label(path: &str) -> String {
    let bytes = std::fs::read(path).unwrap();
    String::from_utf8_lossy(&bytes).into_owned()
}
";

const FIXED: &str = "pub struct Label<'a>(&'a [u8]);

pub fn label(bytes: &[u8]) -> Option<Label<'_>> {
    bytes.first().map(|_| Label(bytes))
}
";

#[test]
fn a_core_that_reads_a_file_returns_text_and_unwraps_is_refused_until_fixed() -> Result<()> {
    let (passed, report) = scan(&[("crates/rules/src/label_rules.rs", VIOLATING)])?;
    assert!(!passed, "{report}");
    for (rule, next) in [
        ("core-effect", "Gather the fact in an adapter"),
        ("core-bare-text", "Return a named type"),
        ("core-panic", "Return Option or Result"),
    ] {
        assert!(report.contains(&format!("error[{rule}]")), "{report}");
        assert!(report.contains(next), "{report}");
    }
    let (passed, report) = scan(&[("crates/rules/src/label_rules.rs", FIXED)])?;
    assert!(passed, "{report}");
    Ok(())
}

#[test]
fn the_same_code_outside_a_core_is_left_to_its_adapter() -> Result<()> {
    let (passed, report) = scan(&[("crates/tool/src/label.rs", VIOLATING)])?;
    assert!(passed, "{report}");
    Ok(())
}

fn fixture_core(manifest: &str, library: &str, stray: Option<&str>) -> Result<tempfile::TempDir> {
    let fixture = tempfile::tempdir()?;
    let core = fixture.path().join(CORE);
    fs::create_dir_all(core.join("src"))?;
    fs::create_dir_all(fixture.path().join("xtask/src"))?;
    fs::write(core.join("Cargo.toml"), manifest)?;
    fs::write(core.join("src/lib.rs"), library)?;
    if let Some(path) = stray {
        let path = fixture.path().join(path);
        fs::create_dir_all(path.parent().expect("a stray path has a parent"))?;
        fs::write(path, "")?;
    }
    Ok(fixture)
}

#[test]
fn a_core_with_a_dependency_std_an_undenied_lint_or_a_stray_rule_breaks_its_boundary() -> Result<()>
{
    let library = fs::read_to_string(root().join(CORE).join("src/lib.rs"))?;
    let manifest = "[package]\nname = \"rules\"\n";
    let pure = fixture_core(manifest, &library, None)?;
    assert_eq!(breach(core(pure.path())?), None);
    for (fixture, expected) in [
        (
            fixture_core(
                "[package]\nname = \"rules\"\n[dependencies]\nserde = \"1\"\n",
                &library,
                None,
            )?,
            Breach::Dependency,
        ),
        (
            fixture_core(manifest, &library.replace("#![no_std]\n", ""), None)?,
            Breach::Std,
        ),
        (
            fixture_core(manifest, &library.replace("clippy::unwrap_used,", ""), None)?,
            Breach::Undenied(0),
        ),
        (
            fixture_core(manifest, &library, Some("crates/tool/src/label_rules.rs"))?,
            Breach::Stray,
        ),
    ] {
        assert_eq!(breach(core(fixture.path())?), Some(expected));
    }
    Ok(())
}

#[test]
fn each_generic_anti_pattern_is_refused_with_its_rule_and_fix_and_admitted_once_fixed() -> Result<()>
{
    let violating = [
        (
            "src/retry.rs",
            "pub fn wait(attempt: u64) -> u64 { attempt * 1500 }\n",
            "magic-number",
            "Name the value in a const",
        ),
        (
            "src/remote.rs",
            "pub fn releases() -> String { format!(\"https://example.com/{}\", 1) }\n",
            "hard-coded-location",
            "Name it in a const",
        ),
        (
            "src/clean.rs",
            "pub fn clean(path: &std::path::Path) { let _ = std::fs::remove_file(path); }\n",
            "swallowed-error",
            "Return the error, report it",
        ),
        (
            "scripts/setup.sh",
            "#!/bin/sh\nmise install || true\n",
            "shell-swallowed-error",
            "Test the command in an if",
        ),
        (
            "plugins/hook.ts",
            "export function run(): void { try { start(); } catch (error) {} }\n",
            "empty-catch",
            "Rethrow the error",
        ),
    ];
    for (path, text, rule, next) in violating {
        let (passed, report) = scan(&[(path, text)])?;
        assert!(!passed, "{rule}: {report}");
        assert!(report.contains(&format!("error[{rule}]")), "{report}");
        assert!(report.contains(next), "{report}");
    }
    let fixed = [
        (
            "src/retry.rs",
            "const BACKOFF_MS: u64 = 1500;\npub fn wait(attempt: u64) -> u64 { attempt * BACKOFF_MS }\n",
        ),
        (
            "src/remote.rs",
            "const RELEASES: &str = \"https://example.com\";\npub fn releases() -> String { format!(\"{RELEASES}/{}\", 1) }\n",
        ),
        (
            "src/clean.rs",
            "pub fn clean(path: &std::path::Path) -> std::io::Result<()> { std::fs::remove_file(path) }\n",
        ),
        (
            "scripts/setup.sh",
            "#!/bin/sh\nif ! mise install; then echo 'mise install failed' >&2; exit 1; fi\n",
        ),
        (
            "plugins/hook.ts",
            "export function run(): void { try { start(); } catch (error) { console.error('start failed', error); throw error; } }\n",
        ),
    ];
    let (passed, report) = scan(&fixed)?;
    assert!(passed, "{report}");
    Ok(())
}

#[test]
fn the_catalog_refuses_an_unexplained_language_and_a_rule_no_pattern_names() -> Result<()> {
    let fixture = tempfile::tempdir()?;
    let catalog = fixture.path();
    fs::create_dir_all(catalog.join("rules"))?;
    fs::create_dir_all(catalog.join("rule-tests"))?;
    fs::write(
        catalog.join("catalog.toml"),
        "languages = [\"rust\", \"shell\"]\n[[pattern]]\nid = \"magic-number\"\nweakness = \"CWE-1106\"\nrust = \"ast-grep:magic-number\"\n",
    )?;
    fs::write(
        catalog.join("rules/magic-number.yml"),
        "id: magic-number\nlanguage: rust\nnote: Name it.\nmetadata:\n  cwe: CWE-1106\nrule:\n  kind: integer_literal\n",
    )?;
    fs::write(
        catalog.join("rules/orphan.yml"),
        "id: orphan\nlanguage: rust\nrule:\n  kind: integer_literal\n",
    )?;
    fs::write(
        catalog.join("rule-tests/magic-number-test.yml"),
        "id: magic-number\n",
    )?;
    let findings = xtask::structure::catalog_findings(catalog)?;
    assert!(
        findings
            .iter()
            .any(|finding| finding.contains("magic-number neither checks shell")),
        "{findings:?}"
    );
    assert!(
        findings
            .iter()
            .any(|finding| finding.contains("no catalog pattern names the rule orphan")),
        "{findings:?}"
    );
    assert!(
        findings
            .iter()
            .any(|finding| finding.contains("the rule orphan lacks its next action")),
        "{findings:?}"
    );
    assert_eq!(
        xtask::structure::catalog_findings(&root().join(CATALOG))?,
        Vec::<String>::new()
    );
    Ok(())
}

#[test]
fn this_repository_keeps_its_core_boundary() -> Result<()> {
    assert_eq!(breach(core(&root())?), None);
    Ok(())
}
