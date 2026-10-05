use dotfiles_xtask::ready_push::gate_with;
use dotfiles_xtask::ready_push_rules::Pr;
use dotfiles_xtask::refusal::Refusal;
use std::cell::Cell;

const GITHUB: &str = "git@github.com:P4suta/dotfiles.git";
const HEAD: &str = "1111111111111111111111111111111111111111";
const ZERO: &str = "0000000000000000000000000000000000000000";

fn update(branch: &str) -> String {
    format!("refs/heads/{branch} {HEAD} refs/heads/{branch} {ZERO}\n")
}

fn refusal(error: &anyhow::Error) -> &Refusal {
    error
        .downcast_ref::<Refusal>()
        .expect("a structured refusal")
}

#[test]
fn a_push_to_a_ready_pr_is_refused_until_it_returns_to_draft() {
    let state = Cell::new(Pr::Ready);
    let asked = Cell::new(0);
    let lookup = |repository: &str, branch: &str| {
        assert_eq!((repository, branch), ("P4suta/dotfiles", "feature"));
        asked.set(asked.get() + 1);
        (state.get(), Some(17))
    };
    let error = gate_with(GITHUB, update("feature").as_bytes(), lookup).unwrap_err();
    let record = refusal(&error);
    assert!(record.is_complete(), "{record:?}");
    assert_eq!(record.rule, "push.ready");
    assert_eq!(
        record.next.as_deref(),
        Some("gh pr ready 17 --repo P4suta/dotfiles --undo")
    );
    assert_eq!(record.waiver, None);
    state.set(Pr::Draft);
    gate_with(GITHUB, update("feature").as_bytes(), lookup).unwrap();
    assert_eq!(asked.get(), 2);
}

#[test]
fn branches_without_a_pr_deletions_tags_and_other_remotes_are_admitted() {
    let absent = |_: &str, _: &str| (Pr::Absent, None);
    gate_with(GITHUB, update("new").as_bytes(), absent).unwrap();
    let never = |_: &str, _: &str| -> (Pr, Option<u64>) { panic!("not a gated push") };
    gate_with(
        GITHUB,
        format!("(delete) {ZERO} refs/heads/feature {HEAD}\n").as_bytes(),
        never,
    )
    .unwrap();
    gate_with(
        GITHUB,
        format!("refs/tags/v1 {HEAD} refs/tags/v1 {ZERO}\n").as_bytes(),
        never,
    )
    .unwrap();
    gate_with(
        "ssh://hub/~/git/dotfiles.git",
        update("feature").as_bytes(),
        never,
    )
    .unwrap();
}

#[test]
fn an_unknown_pr_state_is_refused_with_the_query_that_establishes_it() {
    let unknown = |_: &str, _: &str| (Pr::Unknown, None);
    let error = gate_with(GITHUB, update("feature").as_bytes(), unknown).unwrap_err();
    let record = refusal(&error);
    assert_eq!(record.rule, "push.ready");
    assert_eq!(
        record.next.as_deref(),
        Some("gh pr list --repo P4suta/dotfiles --head feature --state open")
    );
}
