use dotguard::figure_retirement::Permit;
use rules::figure_migration_rules::{BranchIdentity, Evidence, Verification};

#[test]
fn retirement_repository_arguments_cannot_be_git_options() {
    const OBJECT_ID: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    let evidence = Evidence {
        archive: Verification::Complete,
        inventory: Verification::Complete,
        documents: Verification::Complete,
        remaining_references: 0,
        branch: BranchIdentity::Unchanged,
    };
    for remote in ["", "--mirror", "--delete", "-f"] {
        assert!(
            Permit::verified(remote, "diagrams", OBJECT_ID, "token", evidence).is_none(),
            "{remote}"
        );
    }
    for remote in ["origin", "https://github.com/owner/repository.git"] {
        assert!(
            Permit::verified(remote, "diagrams", OBJECT_ID, "token", evidence).is_some(),
            "{remote}"
        );
    }
}
