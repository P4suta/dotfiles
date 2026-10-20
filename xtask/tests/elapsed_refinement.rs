use std::{fs, path::Path};
use xtask::elapsed_refinement::{GENERATED, SOURCE, translate, with_source};
use xtask::obligations::MANIFEST;

fn root() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap()
}

#[test]
fn the_generated_postcondition_is_bound_to_the_native_proof() {
    assert_eq!(
        fs::read_to_string(root().join(GENERATED)).unwrap(),
        translate(&fs::read_to_string(root().join(SOURCE)).unwrap()).unwrap()
    );
}

#[test]
fn altered_native_claims_domains_calls_and_names_are_refused() {
    let source = fs::read_to_string(root().join(SOURCE)).unwrap();
    for (before, after) in [
        (
            "let whole_millis: u128 = kani::any();",
            "let whole_millis: u128 = u128::from(kani::any::<u64>());",
        ),
        (
            "let remainder_nanos: u32 = kani::any();",
            "let remainder_nanos: u32 = kani::any(); kani::assume(remainder_nanos == 0);",
        ),
        (
            "whole_millis + u128::from(remainder_nanos != 0)",
            "whole_millis",
        ),
        (
            "let elapsed = elapsed_millis(whole_millis, remainder_nanos);",
            "let elapsed = model(whole_millis, remainder_nanos);",
        ),
        (
            "#[kani::proof]\n    #[kani::unwind(1)]\n    fn host_elapsed_rounds",
            "#[kani::proof]\n    #[kani::unwind(1)]\n    #[kani::stub(elapsed_millis, model)]\n    fn host_elapsed_rounds",
        ),
        (
            "pub const NANOS_PER_MILLI: u32 = 1_000_000;",
            "pub const NANOS_PER_MILLI: u32 = 999_999;",
        ),
        (
            "use super::*;",
            "use super::*; use another::elapsed_millis;",
        ),
        (
            "use super::*;",
            "use super::*; const NANOS_PER_MILLI: u32 = 2;",
        ),
        ("#[cfg(test)]\nmod tests", "#[cfg(kani)]\nmod tests"),
    ] {
        let changed = source.replace(before, after);
        assert_ne!(changed, source, "missing mutation: {before}");
        assert!(translate(&changed).is_err(), "accepted {after}");
    }
    assert!(
        translate(&format!(
            "macro_rules! assert_eq {{ ($($t:tt)*) => {{}} }}\n{source}"
        ))
        .is_err()
    );
}

#[test]
fn verification_failure_stale_claims_and_changed_source_cannot_complete() {
    let fixture = tempfile::tempdir().unwrap();
    for path in [SOURCE, GENERATED, MANIFEST] {
        fs::create_dir_all(fixture.path().join(path).parent().unwrap()).unwrap();
        fs::write(
            fixture.path().join(path),
            fs::read(root().join(path)).unwrap(),
        )
        .unwrap();
    }
    assert!(with_source(fixture.path(), || Ok(())).is_ok());
    assert!(with_source::<()>(fixture.path(), || anyhow::bail!("native proof failed")).is_err());
    assert!(
        with_source(fixture.path(), || {
            fs::write(fixture.path().join(SOURCE), "changed source")?;
            Ok(())
        })
        .is_err()
    );
    fs::write(
        fixture.path().join(SOURCE),
        fs::read(root().join(SOURCE)).unwrap(),
    )
    .unwrap();
    let manifest = fs::read_to_string(fixture.path().join(MANIFEST)).unwrap();
    assert!(
        with_source(fixture.path(), || {
            fs::write(fixture.path().join(MANIFEST), format!("{manifest}\n"))?;
            Ok(())
        })
        .is_err()
    );
    fs::write(fixture.path().join(MANIFEST), manifest).unwrap();
    fs::write(fixture.path().join(GENERATED), "stale claims").unwrap();
    assert!(with_source(fixture.path(), || Ok(())).is_err());
}

#[test]
fn missing_duplicated_or_substituted_obligations_cannot_skip_verification() {
    let fixture = tempfile::tempdir().unwrap();
    for path in [SOURCE, GENERATED, MANIFEST] {
        fs::create_dir_all(fixture.path().join(path).parent().unwrap()).unwrap();
        fs::write(
            fixture.path().join(path),
            fs::read(root().join(path)).unwrap(),
        )
        .unwrap();
    }
    let manifest = fs::read_to_string(root().join(MANIFEST)).unwrap();
    let kani = manifest
        .split("[[obligation]]")
        .find(|entry| entry.contains(&format!("source = \"{SOURCE}\"")))
        .unwrap();
    let lean = manifest
        .split("[[obligation.theorems]]")
        .find(|entry| entry.contains("name = \"Dotfiles.Elapsed.ceiling_upper\""))
        .unwrap();
    for changed in [
        manifest.replace(&format!("[[obligation]]{kani}"), ""),
        manifest.replace(SOURCE, "crates/rules/src/another_rules.rs"),
        format!("{manifest}\n[[obligation]]{kani}"),
        manifest.replace(&format!("[[obligation.theorems]]{lean}"), ""),
        manifest.replace(lean, &format!("{lean}[[obligation.theorems]]{lean}")),
        manifest.replace(
            "module = \"Dotfiles.Elapsed\"",
            "module = \"Dotfiles.Other\"",
        ),
        manifest.replace(xtask::elapsed_refinement::BRIDGE, "another.rs::test"),
        manifest.replace(
            "kind = \"source-translation\"",
            "kind = \"generated-vectors\"",
        ),
    ] {
        assert_ne!(changed, manifest);
        xtask::obligations::parse_manifest(&changed).unwrap();
        fs::write(fixture.path().join(MANIFEST), changed).unwrap();
        let mut called = false;
        assert!(
            with_source(fixture.path(), || {
                called = true;
                Ok(())
            })
            .is_err()
        );
        assert!(!called);
    }
}
