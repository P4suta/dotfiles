use dotfiles_xtask::agent_memory::violations;
use serde_json::{Value, json};

const CLAUDE_OFF: &str = r#"{"model":"opus","autoMemoryEnabled":false}"#;
const CODEX_OFF: &str = "model = \"o3\"\n[features]\nmemories = false\nexternal_agent_memory_import = false\nchronicle = false\n\n[memories]\ngenerate_memories = false\nuse_memories = false\n";

fn opencode(read: &Value, edit: &Value, bash: &Value) -> String {
    format!(r#"{{"permission": {{"*": "allow", "read": {read}, "edit": {edit}, "bash": {bash}}}}}"#)
}

fn denied() -> Value {
    json!({
        "*": "allow",
        ".github/instructions/memory.instruction.md": "deny",
        "*/.github/instructions/memory.instruction.md": "deny"
    })
}

fn commands_denied() -> Value {
    json!({"*": "allow", "*memory.instruction.md*": "deny"})
}

fn off() -> String {
    opencode(&denied(), &denied(), &commands_denied())
}

fn dump(claude: Option<&str>, codex: Option<&str>, opencode: Option<&str>) -> Value {
    let mut dump = json!({
        ".claude/CLAUDE.md": {"contents": ""},
        ".codex/AGENTS.md": {"contents": ""},
        ".config/opencode/AGENTS.md": {"contents": ""}
    });
    for (target, contents) in [
        (".claude/settings.json", claude),
        (".codex/config.toml", codex),
        (".config/opencode/opencode.json", opencode),
    ] {
        if let Some(contents) = contents {
            dump[target] = json!({"contents": contents});
        }
    }
    dump
}

#[test]
fn a_profile_that_disables_every_switch_passes() {
    let off = off();
    assert!(
        violations(&dump(Some(CLAUDE_OFF), Some(CODEX_OFF), Some(&off)))
            .unwrap()
            .is_empty()
    );
}

#[test]
fn an_unmanaged_client_needs_no_settings() {
    assert!(violations(&json!({})).unwrap().is_empty());
}

#[test]
fn a_managed_client_without_rendered_settings_keeps_its_default_memory() {
    let off = off();
    let found = violations(&dump(None, Some(CODEX_OFF), Some(&off))).unwrap();
    assert_eq!(found.len(), 1);
    assert!(found[0].starts_with(".claude/settings.json is not rendered"));
}

#[test]
fn an_enabled_or_unset_switch_is_named() {
    let off = off();
    for (claude, state) in [
        (r#"{"autoMemoryEnabled":true}"#, "Enabled"),
        ("{}", "Unset"),
        (r#"{"autoMemoryEnabled":"false"}"#, "Unset"),
    ] {
        assert_eq!(
            violations(&dump(Some(claude), Some(CODEX_OFF), Some(&off))).unwrap(),
            [format!(
                ".claude/settings.json: autoMemoryEnabled is {state}"
            )]
        );
    }
    for line in [
        "
memories = false",
        "
external_agent_memory_import = false",
        "
chronicle = false",
        "
generate_memories = false",
        "
use_memories = false",
    ] {
        let codex = CODEX_OFF.replace(line, &line.replace("false", "true"));
        let found = violations(&dump(Some(CLAUDE_OFF), Some(&codex), Some(&off))).unwrap();
        assert_eq!(found.len(), 1, "{line}");
        assert!(found[0].ends_with("is Enabled"), "{found:?}");
    }
    let found = violations(&dump(Some(CLAUDE_OFF), Some("[features]\n"), Some(&off))).unwrap();
    assert_eq!(found.len(), 5);
}

#[test]
fn opencode_must_deny_reading_writing_and_commands_on_the_prompted_memory_file() {
    for (read, edit, bash, expected) in [
        (json!("allow"), denied(), commands_denied(), 2),
        (denied(), json!({"*": "allow"}), commands_denied(), 2),
        (denied(), denied(), json!({"*": "allow"}), 2),
        (Value::Null, Value::Null, Value::Null, 6),
        (
            json!({".github/instructions/memory.instruction.md": "deny"}),
            denied(),
            commands_denied(),
            1,
        ),
        (json!("deny"), json!("deny"), json!("deny"), 0),
    ] {
        let config = opencode(&read, &edit, &bash);
        let found = violations(&dump(Some(CLAUDE_OFF), Some(CODEX_OFF), Some(&config))).unwrap();
        assert_eq!(found.len(), expected, "{config}: {found:?}");
    }
}

#[test]
fn a_later_opencode_rule_overrides_an_earlier_deny() {
    let reordered = r#"{"permission": {"read": {".github/instructions/memory.instruction.md": "deny", "*/.github/instructions/memory.instruction.md": "deny", "*": "allow"}, "edit": "deny", "bash": "deny"}}"#;
    let found = violations(&dump(Some(CLAUDE_OFF), Some(CODEX_OFF), Some(reordered))).unwrap();
    assert_eq!(found.len(), 2, "{found:?}");
    assert!(
        found
            .iter()
            .all(|line| line.contains("permission.read") && line.ends_with("is Enabled"))
    );
    let overridden =
        r#"{"permission": {"read": "deny", "edit": "deny", "bash": "deny", "*": "allow"}}"#;
    let found = violations(&dump(Some(CLAUDE_OFF), Some(CODEX_OFF), Some(overridden))).unwrap();
    assert_eq!(found.len(), 6, "{found:?}");
}

#[test]
fn unparsable_rendered_settings_are_refused() {
    assert!(violations(&dump(Some("{"), Some(CODEX_OFF), Some("{}"))).is_err());
    assert!(violations(&dump(Some(CLAUDE_OFF), Some("[features"), Some("{}"))).is_err());
}
