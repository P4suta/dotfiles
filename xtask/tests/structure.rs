//! The structural gates refuse a fixture core that reaches an effect, returns bare text, or panics, and admit it once fixed.

use anyhow::Result;
use rules::boundary_rules::{Breach, breach};
use std::fs;
use std::path::{Path, PathBuf};
use xtask::structure::{CATALOG, CORE, core};
use xtask::tool::Tool;

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("the package sits inside the workspace")
        .to_path_buf()
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
fn this_repository_keeps_its_core_boundary() -> Result<()> {
    assert_eq!(breach(core(&root())?), None);
    Ok(())
}
