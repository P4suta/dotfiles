use std::fs;
use std::path::Path;
use xtask::contrast_refinement::{GENERATED, SOURCE, translate};

fn source() -> String {
    fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .join(SOURCE),
    )
    .unwrap()
}

#[test]
fn the_generated_definition_is_bound_to_the_production_arithmetic() {
    let generated = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join(GENERATED);
    assert_eq!(
        fs::read_to_string(generated).unwrap(),
        translate(&source()).unwrap()
    );
}

#[test]
fn changed_production_operations_and_constants_change_the_model() {
    let source = source();
    let original = translate(&source).unwrap();
    for (before, after) in [
        ("lighter * MILLIS / darker", "lighter + MILLIS / darker"),
        ("5_000_000_000", "4_000_000_000"),
        ("first >= second", "second >= first"),
    ] {
        assert_ne!(translate(&source.replace(before, after)).unwrap(), original);
    }
}

#[test]
fn unmodeled_operations_are_refused() {
    let source = source();
    for (before, after) in [
        (
            "lighter * MILLIS / darker",
            "lighter.wrapping_mul(MILLIS) / darker",
        ),
        ("lighter * MILLIS / darker", "lighter - MILLIS / darker"),
        ("lighter: u64", "lighter: i64"),
        ("FLARE + first", "external::FLARE + first"),
    ] {
        assert!(
            translate(&source.replace(before, after)).is_err(),
            "{after}"
        );
    }
}
