use dotfiles_xtask::instruction_audit::{check, findings, profile_findings};
use std::path::Path;

const PARTIAL: &str = "# Rules\n\n## Language\n\n- Reply in Japanese.\n- Write commits in English.\n{{- if stat $notes }}\n";

const AUDIT: &str = "# Audit\n\n## Retained lines\n\n### `policy`\n\n> - Reply in Japanese.\n\nJudgment: the reader decides.\n\n> - Write commits in English.\n\nFollow-up F1: a gate is planned.\n\n## Follow-ups\n\n### F1: Language gate\n\nAdd the gate.\n";

#[test]
fn a_fully_classified_partial_passes() {
    assert_eq!(
        findings(&[("policy", PARTIAL)], AUDIT),
        Vec::<String>::new()
    );
}

#[test]
fn an_unclassified_line_names_its_location_and_the_next_action() {
    let partial = format!("{PARTIAL}- Push freely.\n");
    let reported = findings(&[("policy", &partial)], AUDIT);
    assert_eq!(reported.len(), 1, "{reported:?}");
    assert!(reported[0].contains("policy:8"), "{}", reported[0]);
    assert!(reported[0].contains("- Push freely."), "{}", reported[0]);
    assert!(reported[0].contains("just check"), "{}", reported[0]);
}

#[test]
fn a_quote_for_a_removed_line_is_stale() {
    let partial = PARTIAL.replace("- Reply in Japanese.\n", "");
    let reported = findings(&[("policy", &partial)], AUDIT);
    assert_eq!(reported.len(), 1, "{reported:?}");
    assert!(
        reported[0].contains("no longer in `policy`"),
        "{}",
        reported[0]
    );
}

#[test]
fn a_line_quoted_under_another_partial_is_not_classified() {
    let audit = AUDIT.replace("### `policy`", "### `other`");
    let reported = findings(&[("policy", PARTIAL)], &audit);
    assert_eq!(reported.len(), 4, "{reported:?}");
}

#[test]
fn a_quote_needs_a_judgment_or_a_defined_follow_up() {
    for audit in [
        AUDIT.replace("Judgment: the reader decides.", "The reader decides."),
        AUDIT.replace("Follow-up F1:", "Follow-up F2:"),
        AUDIT.replace("Judgment: the reader decides.", "Judgment:"),
    ] {
        let reported = findings(&[("policy", PARTIAL)], &audit);
        assert_eq!(reported.len(), 1, "{audit}\n{reported:?}");
    }
}

#[test]
fn a_duplicate_quote_is_reported() {
    let audit = AUDIT.replace(
        "\n## Follow-ups",
        "\n> - Reply in Japanese.\n\nJudgment: again.\n\n## Follow-ups",
    );
    let reported = findings(&[("policy", PARTIAL)], &audit);
    assert_eq!(reported.len(), 1, "{reported:?}");
    assert!(reported[0].contains("more than once"), "{}", reported[0]);
}

#[test]
fn profile_templates_may_only_include_audited_partials() {
    let partials = ["agent_policy", "prose_policy"];
    assert!(
        profile_findings(
            "p/CLAUDE.md.tmpl",
            "{{ includeTemplate \"agent_policy\" . }}\n\n{{ includeTemplate \"prose_policy\" . }}\n",
            &partials,
        )
        .is_empty()
    );
    for text in [
        "{{ includeTemplate \"agent_policy\" . }}\nAlways push.\n",
        "{{ includeTemplate \"secret_policy\" . }}\n",
    ] {
        let reported = profile_findings("p/CLAUDE.md.tmpl", text, &partials);
        assert_eq!(reported.len(), 1, "{text}\n{reported:?}");
        assert!(reported[0].contains("p/CLAUDE.md.tmpl:"), "{}", reported[0]);
    }
}

#[test]
fn the_repository_instruction_files_are_fully_audited() -> anyhow::Result<()> {
    check(&Path::new(env!("CARGO_MANIFEST_DIR")).join(".."))
}
