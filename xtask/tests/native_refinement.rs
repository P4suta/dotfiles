use std::{fs, path::Path};
use xtask::{elapsed_refinement, native_refinement, obligations};

fn root() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap()
}

#[test]
fn the_dependency_projection_matches_its_actual_native_byte_contract() {
    let source = fs::read_to_string(root().join(native_refinement::SOURCE)).unwrap();
    native_refinement::projection(&source).unwrap();
    for (before, after) in [
        ("login.as_bytes(),", "b\"actor\","),
        ("author_type == Some(BOT_AUTHOR_TYPE),", "false,"),
        ("mergeable == Some(CONFLICTING_MERGEABILITY),", "false,"),
        (
            "const BOT_AUTHOR_TYPE: &str = \"Bot\";",
            "const BOT_AUTHOR_TYPE: &str = \"User\";",
        ),
        (
            "#[derive(Clone, Copy, Debug, Eq, PartialEq)]\npub enum DependencySkip",
            "#[derive(Clone, Copy, Debug)]\npub enum DependencySkip",
        ),
        (
            "const LOGIN_BYTES: usize = 16;",
            "const LOGIN_BYTES: usize = 8;",
        ),
        ("let author_bot = kani::any();", "let author_bot = true;"),
        (
            "spec_dependency_skip_bytes(login, author_bot, is_bot, conflicting);",
            "kani::assume(author_bot);",
        ),
        (
            "pub fn dependency_skip(",
            "#[cfg(test)] pub fn dependency_skip(",
        ),
        (
            "use super::*;",
            "use super::*; fn dependency_skip_bytes() {}",
        ),
    ] {
        let changed = source.replace(before, after);
        assert_ne!(changed, source, "missing mutation: {before}");
        assert!(
            native_refinement::projection(&changed).is_err(),
            "accepted {after}"
        );
    }
    for declaration in [
        "const None: Option<u8> = Some(0);",
        "const r#None: Option<u8> = Some(0);",
        "fn r#Some<T>(value: T) -> Option<T> { None }",
        "use other::dependency_skip_bytes;",
        "macro_rules! assert_eq { ($($t:tt)*) => {} }",
    ] {
        assert!(native_refinement::projection(&format!("{declaration}\n{source}")).is_err());
    }
}

#[test]
fn complete_campaigns_reject_changed_projection_inputs_and_verifier_failure() {
    let fixture = tempfile::tempdir().unwrap();
    for path in [
        native_refinement::SOURCE,
        elapsed_refinement::SOURCE,
        elapsed_refinement::GENERATED,
        obligations::MANIFEST,
    ] {
        fs::create_dir_all(fixture.path().join(path).parent().unwrap()).unwrap();
        fs::copy(root().join(path), fixture.path().join(path)).unwrap();
    }
    native_refinement::with_source(fixture.path(), || Ok(())).unwrap();
    let manifest = fs::read_to_string(fixture.path().join(obligations::MANIFEST)).unwrap();
    for changed in [
        manifest.replace(
            native_refinement::SOURCE,
            "crates/rules/src/missing_pr_rules.rs",
        ),
        format!(
            "{manifest}\n[[obligation]]\nverifier = \"kani\"\nsource = \"{}\"\nprobes = '''#[kani::proof] fn reject_false() {{ assert!(false); }}'''\n",
            native_refinement::SOURCE
        ),
    ] {
        fs::write(fixture.path().join(obligations::MANIFEST), changed).unwrap();
        let error = native_refinement::with_source(fixture.path(), || Ok(())).unwrap_err();
        assert!(
            error
                .to_string()
                .contains("dependency native obligation is missing or duplicated")
        );
    }
    fs::write(fixture.path().join(obligations::MANIFEST), manifest).unwrap();
    assert!(
        native_refinement::with_source::<()>(fixture.path(), || anyhow::bail!("verifier failed"))
            .is_err()
    );
    assert!(
        native_refinement::with_source(fixture.path(), || {
            fs::write(fixture.path().join(native_refinement::SOURCE), "changed")?;
            Ok(())
        })
        .is_err()
    );
}
