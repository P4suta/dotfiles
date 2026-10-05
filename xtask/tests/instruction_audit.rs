use dotfiles_xtask::instruction_audit::{check, dispatcher_findings, findings, profile_findings};
use dotfiles_xtask::instruction_rules::{
    Class, Definition, Holder, Line, admitted, follow_up, removed_row_held,
};
use std::fs;
use std::path::Path;

const PARTIAL: &str = "# Rules\n\n## Language\n\n- Reply in Japanese.\n- Write commits in English.\n{{- if stat $notes }}\n";

const AUDIT: &str = "# Audit

## Retained lines

### `policy`

> # Rules

Heading: names the file.

> ## Language

Heading: groups the language lines.

> - Reply in Japanese.

Judgment: the reader decides.

> - Write commits in English.

Follow-up F1: a gate is planned.

> {{- if stat $notes }}

Directive: renders nothing.

## Removed lines

| Former line | Held by | Mechanism |
| --- | --- | --- |
| Never push. | Gate | `guard/src/push.rs` refuses it. |
| Load `demo` for demos. | Skill | `demo` states it. |
| Reply to me in Japanese. | Merged | Reworded as `Reply in Japanese`. |

## Follow-ups

### F1: Language gate

Add the gate.
";

fn exists(path: &str) -> bool {
    matches!(path, "guard/src/push.rs" | "docs/push.md")
}

const SKILLS: [&str; 1] = ["demo"];

fn audit(partials: &[(&str, &str)], audit: &str) -> Vec<String> {
    findings(partials, audit, &exists, &SKILLS)
}

#[test]
fn a_fully_classified_partial_passes() {
    assert_eq!(audit(&[("policy", PARTIAL)], AUDIT), Vec::<String>::new());
}

#[test]
fn an_unclassified_line_names_its_location_and_the_next_action() {
    let partial = format!("{PARTIAL}- Push freely.\n");
    let reported = audit(&[("policy", &partial)], AUDIT);
    assert_eq!(reported.len(), 1, "{reported:?}");
    assert!(reported[0].contains("policy:8"), "{}", reported[0]);
    assert!(reported[0].contains("- Push freely."), "{}", reported[0]);
    assert!(reported[0].contains("just check"), "{}", reported[0]);
}

#[test]
fn headings_and_template_directives_are_audited_like_any_line() {
    for line in [
        "## Always push to main",
        "{{ \"- Push freely.\" }}",
        "  {{ include \"notes\" }}",
    ] {
        let partial = format!("{PARTIAL}{line}\n");
        let reported = audit(&[("policy", &partial)], AUDIT);
        assert_eq!(reported.len(), 1, "{line}\n{reported:?}");
        assert!(reported[0].contains("policy:8"), "{}", reported[0]);
    }
}

#[test]
fn a_template_action_that_renders_output_is_refused_whatever_its_class() {
    for line in [
        "{{ include $notes | trim }}",
        "{{ \"Always push.\" }}",
        "{{- if $notes }}Always push.{{ end }}",
        "- Push to {{ .branch }} freely.",
    ] {
        let partial = format!(
            "{PARTIAL}{line}
"
        );
        for class in [
            "Directive: renders nothing.",
            "Judgment: the reader decides.",
        ] {
            let audit_text = AUDIT.replace(
                "
## Removed lines",
                &format!(
                    "
> {line}

{class}

## Removed lines"
                ),
            );
            let reported = audit(&[("policy", &partial)], &audit_text);
            assert_eq!(
                reported.len(),
                1,
                "{line} {class}
{reported:?}"
            );
            assert!(reported[0].contains("renders"), "{}", reported[0]);
            assert!(reported[0].contains("just check"), "{}", reported[0]);
        }
    }
}

const PROMPT: &str = "---
description: Review diffs
---

Review the change.
";

const PROMPT_AUDIT: &str = "# Audit

## Retained lines

### `dot_config/opencode/agents/reviewer.md`

> ---

Setting: delimits the front matter.

> description: Review diffs

Setting: tells the client when to pick the agent.

> Review the change.

Judgment: what a review finds is judgment.

## Removed lines

| Former line | Held by | Mechanism |
| --- | --- | --- |

## Follow-ups
";

#[test]
fn front_matter_lines_are_settings_and_body_lines_are_not() {
    let prompts = [("dot_config/opencode/agents/reviewer.md", PROMPT)];
    let exists = |path: &str| path == "dot_config/opencode/agents/reviewer.md";
    assert_eq!(
        findings(&prompts, PROMPT_AUDIT, &exists, &SKILLS),
        Vec::<String>::new()
    );
    for (from, to) in [
        ("Setting: tells", "Judgment: tells"),
        ("Judgment: what", "Setting: what"),
    ] {
        let reported = findings(&prompts, &PROMPT_AUDIT.replace(from, to), &exists, &SKILLS);
        assert_eq!(
            reported.len(),
            1,
            "{to}
{reported:?}"
        );
    }
    let body = format!(
        "{PROMPT}---
"
    );
    let reported = findings(
        &[("dot_config/opencode/agents/reviewer.md", &body)],
        PROMPT_AUDIT,
        &exists,
        &SKILLS,
    );
    assert_eq!(reported.len(), 1, "{reported:?}");
    assert!(reported[0].contains("`---`"), "{}", reported[0]);
}

#[test]
fn a_class_must_match_the_shape_of_its_line() {
    for (from, to) in [
        (
            "Judgment: the reader decides.",
            "Heading: the reader decides.",
        ),
        (
            "Judgment: the reader decides.",
            "Directive: the reader decides.",
        ),
        (
            "Heading: groups the language lines.",
            "Judgment: a heading.",
        ),
        ("Directive: renders nothing.", "Judgment: a directive."),
        ("Directive: renders nothing.", "Heading: a directive."),
    ] {
        let reported = audit(&[("policy", PARTIAL)], &AUDIT.replace(from, to));
        assert_eq!(reported.len(), 1, "{to}\n{reported:?}");
        assert!(reported[0].contains("just check"), "{}", reported[0]);
    }
}

#[test]
fn a_quote_for_a_removed_line_is_stale() {
    let partial = PARTIAL.replace("- Reply in Japanese.\n", "");
    let reported = audit(&[("policy", &partial)], AUDIT);
    assert_eq!(reported.len(), 2, "{reported:?}");
    assert!(
        reported[0].contains("no longer in `policy`"),
        "{}",
        reported[0]
    );
    assert!(
        reported[1].contains("`Reply to me in Japanese.` needs"),
        "{}",
        reported[1]
    );
}

#[test]
fn a_line_quoted_under_another_partial_is_not_classified() {
    let audit_text = AUDIT.replace("### `policy`", "### `other`");
    let reported = audit(&[("policy", PARTIAL)], &audit_text);
    assert_eq!(reported.len(), 10, "{reported:?}");
}

#[test]
fn a_quote_needs_a_judgment_or_a_defined_follow_up() {
    for audit_text in [
        AUDIT.replace("Judgment: the reader decides.", "The reader decides."),
        AUDIT.replace("Judgment: the reader decides.", "Judgment:"),
    ] {
        let reported = audit(&[("policy", PARTIAL)], &audit_text);
        assert_eq!(reported.len(), 1, "{audit_text}\n{reported:?}");
    }
    let reported = audit(
        &[("policy", PARTIAL)],
        &AUDIT.replace("Follow-up F1:", "Follow-up F2:"),
    );
    assert_eq!(reported.len(), 2, "{reported:?}");
    assert!(
        reported.iter().any(|line| line.contains("`### F1:`")),
        "{reported:?}"
    );
}

#[test]
fn a_duplicate_quote_is_reported() {
    let audit_text = AUDIT.replace(
        "\n## Removed lines",
        "\n> - Reply in Japanese.\n\nJudgment: again.\n\n## Removed lines",
    );
    let reported = audit(&[("policy", PARTIAL)], &audit_text);
    assert_eq!(reported.len(), 1, "{reported:?}");
    assert!(reported[0].contains("more than once"), "{}", reported[0]);
}

#[test]
fn a_follow_up_is_defined_once_and_referenced() {
    let duplicate = AUDIT.replace(
        "Add the gate.\n",
        "Add the gate.\n\n### F1: Again\n\nAgain.\n",
    );
    let reported = audit(&[("policy", PARTIAL)], &duplicate);
    assert_eq!(reported.len(), 1, "{reported:?}");
    assert!(
        reported[0].contains("defined more than once"),
        "{}",
        reported[0]
    );
    assert!(reported[0].contains("just check"), "{}", reported[0]);

    let unreferenced = AUDIT.replace(
        "Add the gate.\n",
        "Add the gate.\n\n### F2: Unused\n\nNothing refers to it.\n",
    );
    let reported = audit(&[("policy", PARTIAL)], &unreferenced);
    assert_eq!(reported.len(), 1, "{reported:?}");
    assert!(reported[0].contains("`### F2:`"), "{}", reported[0]);
    assert!(reported[0].contains("no retained line"), "{}", reported[0]);
}

#[test]
fn every_removed_line_names_what_holds_it() {
    for (from, to, expected) in [
        ("| Gate |", "| Hope |", "Held by"),
        (
            "`guard/src/push.rs` refuses it.",
            "a hook refuses it.",
            "source file",
        ),
        ("`demo` states it.", "a skill states it.", "existing skill"),
        (
            "| Merged | Reworded as `Reply in Japanese`.",
            "| Merged | Reworded as the retained reply line.",
            "retained line",
        ),
        (
            "`Reply in Japanese`",
            "`Never push freely`",
            "retained line",
        ),
        (
            "`guard/src/push.rs` refuses it.",
            "`docs/push.md` refuses it.",
            "source file",
        ),
        ("| Never push. | Gate |", "| Never push. |", "three cells"),
    ] {
        let reported = audit(&[("policy", PARTIAL)], &AUDIT.replace(from, to));
        assert_eq!(reported.len(), 1, "{to}\n{reported:?}");
        assert!(reported[0].contains(expected), "{}", reported[0]);
        assert!(reported[0].contains("just check"), "{}", reported[0]);
    }
}

#[test]
fn a_path_the_audit_names_must_exist() {
    let audit_text = AUDIT.replace(
        "`guard/src/push.rs`",
        "`guard/src/push.rs` and `guard/src/gone.rs`",
    );
    let reported = audit(&[("policy", PARTIAL)], &audit_text);
    assert_eq!(reported.len(), 1, "{reported:?}");
    assert!(
        reported[0].contains("`guard/src/gone.rs`"),
        "{}",
        reported[0]
    );
    assert!(reported[0].contains("just check"), "{}", reported[0]);
}

#[test]
fn a_path_inside_a_quoted_source_line_is_not_a_claim() {
    let line = "- Read `refs/remotes/origin/HEAD` first.";
    let partial = format!(
        "{PARTIAL}{line}
"
    );
    let audit_text = AUDIT.replace(
        "
## Removed lines",
        &format!(
            "
> {line}

Judgment: the reader decides.

## Removed lines"
        ),
    );
    assert_eq!(
        audit(&[("policy", &partial)], &audit_text),
        Vec::<String>::new()
    );
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

const DISPATCHER: &str = "{{- $context := merge (dict) . (index .platforms .profile) -}}
{{- if eq .profile \"mac\" -}}
{{ includeTemplate \"profiles/mac/dot_claude/CLAUDE.md.tmpl\" $context }}
{{- else if has .profile (list \"linux\" \"wsl\") -}}
{{ includeTemplate \"profiles/linux/dot_claude/CLAUDE.md.tmpl\" $context }}
{{- else -}}{{ fail \"Target is unavailable for this profile\" }}{{- end -}}
";

#[test]
fn a_dispatcher_only_selects_its_own_profile_file() {
    assert_eq!(
        dispatcher_findings("dot_claude/CLAUDE.md.tmpl", DISPATCHER),
        Vec::<String>::new()
    );
    for (from, to) in [
        ("{{- else -}}", "Always push.\n{{- else -}}"),
        (
            "profiles/linux/dot_claude/CLAUDE.md.tmpl",
            "profiles/linux/dot_codex/AGENTS.md.tmpl",
        ),
        ("profiles/linux/dot_claude/CLAUDE.md.tmpl", "agent_policy"),
        ("{{ fail ", "{{ \"Always push.\" }}{{ fail "),
        ("$context }}", "$context | printf \"%s Always push.\" }}"),
        ("{{- end -}}", "{{- end -}}{{"),
    ] {
        let text = DISPATCHER.replacen(from, to, 1);
        let reported = dispatcher_findings("dot_claude/CLAUDE.md.tmpl", &text);
        assert_eq!(reported.len(), 1, "{text}\n{reported:?}");
        assert!(
            reported[0].starts_with("dot_claude/CLAUDE.md.tmpl:"),
            "{}",
            reported[0]
        );
        assert!(reported[0].contains("just check"), "{}", reported[0]);
    }
}

#[test]
fn the_rule_core_admits_only_matching_classes_and_current_follow_ups() {
    assert!(admitted(Line::Text, Class::Judgment, false));
    assert!(admitted(Line::Text, Class::FollowUp, true));
    assert!(!admitted(Line::Text, Class::FollowUp, false));
    assert!(!admitted(Line::Text, Class::Directive, true));
    assert!(!admitted(Line::Heading, Class::Judgment, true));
    assert!(admitted(Line::Directive, Class::Directive, false));
    assert!(admitted(Line::Setting, Class::Setting, false));
    assert!(!admitted(Line::Text, Class::Setting, true));
    assert!(!admitted(Line::Output, Class::Directive, true));
    assert!(!removed_row_held(Some(Holder::Merged), true, true, false));
    assert!(!admitted(Line::Directive, Class::Missing, true));
    assert_eq!(follow_up(1, 2), Definition::Current);
    assert_eq!(follow_up(0, 1), Definition::Undefined);
    assert_eq!(follow_up(2, 1), Definition::Duplicate);
    assert_eq!(follow_up(1, 0), Definition::Unreferenced);
    assert!(removed_row_held(
        Some(Holder::GateAndSkill),
        true,
        true,
        false
    ));
    assert!(!removed_row_held(
        Some(Holder::GateAndSkill),
        true,
        false,
        false
    ));
    assert!(!removed_row_held(None, true, true, true));
}

fn write(root: &Path, relative: &str, text: &str) {
    let path = root.join(relative);
    fs::create_dir_all(path.parent().expect("parent")).expect("create directory");
    fs::write(path, text).expect("write fixture");
}

const PROFILE: &str = "{{ includeTemplate \"agent_policy\" . }}\n\n{{ includeTemplate \"prose_policy\" . }}\n\n{{ includeTemplate \"remote_machines\" . }}\n";

/// A minimal repository whose instruction files the audit fully classifies.
fn repository() -> tempfile::TempDir {
    let root = tempfile::tempdir().expect("temporary repository");
    let path = root.path();
    write(
        path,
        ".chezmoitemplates/agent_policy",
        "- Reply in Japanese.\n",
    );
    write(
        path,
        ".chezmoitemplates/prose_policy",
        "- Write one sentence per line.\n",
    );
    write(
        path,
        ".chezmoitemplates/remote_machines",
        "- Read the SSH aliases.\n",
    );
    write(
        path,
        "docs/agent-instruction-audit.md",
        "# Audit\n\n## Retained lines\n\n### `agent_policy`\n\n> - Reply in Japanese.\n\nJudgment: a.\n\n### `prose_policy`\n\n> - Write one sentence per line.\n\nJudgment: b.\n\n### `remote_machines`\n\n> - Read the SSH aliases.\n\nJudgment: c.\n\n## Removed lines\n\n| Former line | Held by | Mechanism |\n| --- | --- | --- |\n| Use demo. | Skill | `demo` states it. |\n\n## Follow-ups\n",
    );
    write(path, "dot_agents/skills/demo/SKILL.md", "demo\n");
    write(
        path,
        ".chezmoitemplates/profiles/mac/dot_claude/CLAUDE.md.tmpl",
        PROFILE,
    );
    write(
        path,
        "dot_claude/CLAUDE.md.tmpl",
        "{{- if eq .profile \"mac\" -}}\n{{ includeTemplate \"profiles/mac/dot_claude/CLAUDE.md.tmpl\" . }}\n{{- else -}}{{ fail \"Target is unavailable for this profile\" }}{{- end -}}\n",
    );
    root
}

fn refusal(root: &Path) -> String {
    format!("{:#}", check(root).expect_err("the audit must refuse"))
}

#[test]
fn the_check_accepts_a_fully_audited_repository() -> anyhow::Result<()> {
    check(repository().path())
}

#[test]
fn the_check_walks_every_profile_and_dispatcher() {
    let root = repository();
    write(
        root.path(),
        ".chezmoitemplates/profiles/mac/dot_claude/CLAUDE.md.tmpl",
        &format!("{PROFILE}Always push.\n"),
    );
    write(
        root.path(),
        "dot_claude/CLAUDE.md.tmpl",
        "Always push.\n{{ includeTemplate \"profiles/mac/dot_claude/CLAUDE.md.tmpl\" . }}\n",
    );
    let message = refusal(root.path());
    assert!(
        message.contains(".chezmoitemplates/profiles/mac/dot_claude/CLAUDE.md.tmpl:6"),
        "{message}"
    );
    assert!(message.contains("dot_claude/CLAUDE.md.tmpl:1"), "{message}");
}

#[test]
fn an_instruction_file_outside_the_audit_is_refused() {
    let root = repository();
    write(root.path(), "dot_gemini/GEMINI.md", "Always push.\n");
    write(root.path(), "node_modules/pkg/AGENTS.md", "Vendored.\n");
    let message = refusal(root.path());
    assert!(message.contains("dot_gemini/GEMINI.md"), "{message}");
    assert!(!message.contains("node_modules"), "{message}");
    assert!(message.contains("just check"), "{message}");
}

#[test]
fn every_file_a_client_loads_as_instructions_is_found() {
    for path in [
        "dot_codex/AGENTS.override.md",
        "dot_claude/CLAUDE.local.md",
        "dot_claude/rules/push.md",
        "private_dot_claude/exact_rules/push.md.tmpl",
        "dot_claude/agents/pusher.md",
        "dot_claude/commands/push.md",
        "dot_codex/prompts/push.md",
        "dot_config/opencode/agents/pusher.md",
        "dot_config/opencode/agent/pusher.md",
        "dot_config/opencode/commands/push.md",
        "dot_config/opencode/command/push.md",
    ] {
        let root = repository();
        write(
            root.path(),
            path,
            "Always push.
",
        );
        write(
            root.path(),
            "dot_agents/skills/demo/agents/openai.yaml",
            "x: y
",
        );
        let message = refusal(root.path());
        assert!(
            message.contains(path),
            "{path}
{message}"
        );
        assert!(!message.contains("openai.yaml"), "{message}");
        assert!(message.contains("just check"), "{message}");
    }
}

#[test]
fn a_prompt_file_is_audited_line_by_line() -> anyhow::Result<()> {
    let root = repository();
    let path = "dot_config/opencode/agents/reviewer.md";
    write(root.path(), path, PROMPT);
    let message = refusal(root.path());
    assert!(message.contains(&format!("{path}:5")), "{message}");
    let audit = fs::read_to_string(root.path().join("docs/agent-instruction-audit.md"))?;
    let section = PROMPT_AUDIT
        .split_once(
            "## Retained lines

",
        )
        .and_then(|(_, rest)| rest.split_once("## Removed lines"))
        .map(|(section, _)| section)
        .expect("prompt section");
    write(
        root.path(),
        "docs/agent-instruction-audit.md",
        &audit.replace("## Removed lines", &format!("{section}## Removed lines")),
    );
    check(root.path())
}

#[test]
fn a_missing_skill_tree_names_the_directory_and_the_next_action() {
    let root = repository();
    fs::remove_dir_all(root.path().join("dot_agents")).expect("remove skills");
    let message = refusal(root.path());
    assert!(message.contains("dot_agents/skills"), "{message}");
    assert!(message.contains("just check"), "{message}");
}

#[test]
fn a_repository_without_profile_templates_is_refused_with_its_location() {
    let root = repository();
    fs::remove_dir_all(root.path().join(".chezmoitemplates/profiles")).expect("remove profiles");
    fs::remove_file(root.path().join("dot_claude/CLAUDE.md.tmpl")).expect("remove dispatcher");
    let message = refusal(root.path());
    assert!(message.contains(".chezmoitemplates/profiles"), "{message}");
    assert!(message.contains("just check"), "{message}");
}

#[test]
fn an_unreadable_partial_names_the_file_and_the_next_action() {
    let root = repository();
    fs::remove_file(root.path().join(".chezmoitemplates/prose_policy")).expect("remove partial");
    let message = refusal(root.path());
    assert!(
        message.contains(".chezmoitemplates/prose_policy"),
        "{message}"
    );
    assert!(message.contains("just check"), "{message}");
}

#[test]
fn the_repository_instruction_files_are_fully_audited() -> anyhow::Result<()> {
    check(&Path::new(env!("CARGO_MANIFEST_DIR")).join(".."))
}
