use std::collections::BTreeSet;

#[test]
#[ignore = "requires the pinned Kani driver; run explicitly before workplace mutations"]
fn doppler_mutation_guard_proves_the_production_contract_and_rejects_false_claims()
-> anyhow::Result<()> {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap();
    let source = root.join("crates/rules/src/doppler_migration_rules.rs");
    let directory = tempfile::tempdir()?;
    let flags = BTreeSet::from(["stubbing"]);
    xtask::skill_proofs::verify_file(
        &source,
        directory.path(),
        &["proofs::copy_requires_explicit_apply_and_distinct_verified_workplaces"],
        false,
        &flags,
    )?;
    let manifest = xtask::obligations::manifest(root)?;
    let probes = manifest
        .obligations
        .iter()
        .find_map(|obligation| match obligation {
            xtask::obligations::Obligation::Kani { source, probes, .. }
                if source == "crates/rules/src/doppler_migration_rules.rs" =>
            {
                Some(probes)
            }
            _ => None,
        })
        .unwrap();
    let probe = directory.path().join("counterexample.rs");
    std::fs::write(
        &probe,
        format!("#[path = {source:?}] mod production;\n{probes}"),
    )?;
    xtask::skill_proofs::verify_file(
        &probe,
        directory.path(),
        &[
            "reject_copy_from_an_unverified_workplace",
            "reject_read_only_planning_mutating_the_destination",
        ],
        true,
        &flags,
    )
}
