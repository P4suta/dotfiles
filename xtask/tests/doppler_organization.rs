use rules::doppler_organization_rules::{Change, Cutover, Evidence, Mode, Scope, Verdict, decide};

fn verified() -> Evidence {
    Evidence {
        scope: Scope {
            workplace: true,
            repository: true,
        },
        capacity: true,
        cutover: Cutover {
            destination: true,
            consumer: true,
        },
        unused: true,
    }
}

#[test]
fn read_only_organization_never_admits_a_remote_change() {
    for change in [
        Change::Prepare,
        Change::DisableSourceSync,
        Change::RetireConsumer,
        Change::RemoveUnused,
    ] {
        assert_ne!(decide(Mode::ReadOnly, change, verified()), Verdict::Allowed);
    }
}

#[test]
fn a_healthy_mirror_does_not_prove_that_a_consumer_has_migrated() {
    let evidence = Evidence {
        cutover: Cutover {
            destination: true,
            consumer: false,
        },
        ..verified()
    };
    assert_eq!(
        decide(Mode::Apply, Change::DisableSourceSync, evidence),
        Verdict::Allowed
    );
    assert_ne!(
        decide(Mode::Apply, Change::RetireConsumer, evidence),
        Verdict::Allowed
    );
}

#[test]
fn preparation_and_cleanup_refuse_an_unknown_scope_or_an_active_dependency() {
    assert_ne!(
        decide(
            Mode::Apply,
            Change::Prepare,
            Evidence {
                scope: Scope {
                    workplace: false,
                    repository: true
                },
                ..verified()
            }
        ),
        Verdict::Allowed
    );
    assert_ne!(
        decide(
            Mode::Apply,
            Change::Prepare,
            Evidence {
                scope: Scope {
                    workplace: true,
                    repository: false
                },
                ..verified()
            }
        ),
        Verdict::Allowed
    );
    assert_ne!(
        decide(
            Mode::Apply,
            Change::Prepare,
            Evidence {
                capacity: false,
                ..verified()
            }
        ),
        Verdict::Allowed
    );
    assert_ne!(
        decide(
            Mode::Apply,
            Change::RemoveUnused,
            Evidence {
                unused: false,
                ..verified()
            }
        ),
        Verdict::Allowed
    );
}

#[test]
#[ignore = "requires the pinned Kani driver; run explicitly before organization mutations"]
fn organization_proves_mutation_and_retirement_guards_and_rejects_false_claims()
-> anyhow::Result<()> {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap();
    let source = root.join("crates/rules/src/doppler_organization_rules.rs");
    let directory = tempfile::tempdir()?;
    let flags = std::collections::BTreeSet::from(["stubbing"]);
    xtask::skill_proofs::verify_file(
        &source,
        directory.path(),
        &[
            "proofs::config_inventories_require_complete_declared_states",
            "proofs::changes_require_explicit_apply_and_verified_scope",
            "proofs::consumer_retirement_needs_live_cutover_evidence",
            "proofs::preparation_and_cleanup_require_their_own_evidence",
        ],
        false,
        &flags,
    )?;
    let manifest = xtask::obligations::manifest(root)?;
    let probes = manifest
        .obligations
        .iter()
        .find_map(|obligation| match obligation {
            xtask::obligations::Obligation::Kani { source, probes, .. }
                if source == "crates/rules/src/doppler_organization_rules.rs" =>
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
            "reject_read_only_organization_writing",
            "reject_retirement_before_consumer_verification",
            "reject_removing_a_used_config",
            "reject_shared_inventory_without_a_complete_state",
            "reject_distinct_inventory_with_legacy_drift",
        ],
        true,
        &flags,
    )
}
