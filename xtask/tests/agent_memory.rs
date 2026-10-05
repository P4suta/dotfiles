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

/// Renders a repository `modify_` template for one profile against a host file, as application would.
fn render_merged(profile: &str, template: &str, target: &str, host: &str) -> String {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap();
    let scope = tempfile::tempdir().unwrap();
    let source = scope.path().join("source");
    let destination = scope.path().join("home");
    for file in [
        template,
        ".chezmoitemplates/sh_quote",
        ".chezmoitemplates/profiles/wsl/dot_claude/settings.json.tmpl",
    ] {
        let copy = source.join(file);
        std::fs::create_dir_all(copy.parent().unwrap()).unwrap();
        std::fs::copy(root.join(file), copy).unwrap();
    }
    let path = destination.join(target);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, host).unwrap();
    let config = scope.path().join("chezmoi.toml");
    std::fs::write(
        &config,
        format!("[data]\nprofile = {profile:?}\n[data.platforms.{profile}]\n"),
    )
    .unwrap();
    let rendered = dotfiles_xtask::profiles::chezmoi(&source, scope.path(), &config, &destination)
        .arg("cat")
        .arg(&path)
        .output()
        .unwrap();
    assert!(
        rendered.status.success(),
        "{}",
        String::from_utf8_lossy(&rendered.stderr)
    );
    String::from_utf8(rendered.stdout).unwrap()
}

#[test]
fn application_turns_off_memory_a_client_turned_back_on() {
    let host = r#"{"autoMemoryEnabled":true,"model":"opus","permissions":{"allow":["Bash(host)"],"deny":["Bash(host)"],"defaultMode":"auto"}}"#;
    for profile in ["mac", "linux", "windows", "wsl"] {
        let rendered: Value = serde_json::from_str(&render_merged(
            profile,
            "dot_claude/modify_settings.json",
            ".claude/settings.json",
            host,
        ))
        .unwrap();
        assert_eq!(rendered["autoMemoryEnabled"], json!(false), "{profile}");
        let hook = &rendered["hooks"]["PreToolUse"][0];
        assert_eq!(hook["matcher"], json!("Bash|PowerShell"), "{profile}");
        assert!(
            hook["hooks"][0]["command"]
                .as_str()
                .is_some_and(|command| command.ends_with(" claude-bash")
                    && command.contains("dotfiles-xtask")),
            "{profile}: {hook}"
        );
        assert_eq!(rendered["model"], json!("opus"), "{profile}");
        assert_eq!(
            rendered["permissions"]["defaultMode"],
            json!("auto"),
            "{profile}"
        );
        let claude = rendered.to_string();
        assert!(
            violations(&dump(Some(&claude), Some(CODEX_OFF), Some(&off())))
                .unwrap()
                .is_empty(),
            "{profile}: {claude}"
        );
    }
    let codex_host = "model = \"o3\"\n[features]\nmemories = true\nexternal_agent_memory_import = true\nchronicle = true\n[memories]\ngenerate_memories = true\nuse_memories = true\n";
    for profile in ["mac", "linux", "windows", "wsl"] {
        let codex = render_merged(
            profile,
            "dot_codex/modify_config.toml",
            ".codex/config.toml",
            codex_host,
        );
        assert!(
            violations(&dump(Some(CLAUDE_OFF), Some(&codex), Some(&off())))
                .unwrap()
                .is_empty(),
            "{profile}: {codex}"
        );
        assert!(codex.contains("model = \"o3\""), "{codex}");
    }
}

/// Every key a profile sets takes the profile's value, an array included as one value, and every other key keeps the host's value.
#[test]
fn a_profile_array_replaces_the_host_array_whether_or_not_it_is_empty() {
    let host =
        r#"{"permissions":{"allow":["Bash(host)"],"deny":["Bash(host)"],"ask":["Bash(host)"]}}"#;
    let rendered: Value = serde_json::from_str(&render_merged(
        "wsl",
        "dot_claude/modify_settings.json",
        ".claude/settings.json",
        host,
    ))
    .unwrap();
    let permissions = &rendered["permissions"];
    assert_eq!(permissions["allow"], json!([]));
    let deny = permissions["deny"].as_array().unwrap();
    assert!(deny.contains(&json!("Bash(sudo:*)")));
    assert!(!deny.contains(&json!("Bash(host)")));
    assert_eq!(permissions["ask"], json!(["Bash(host)"]));
}

#[test]
fn a_host_file_with_memory_turned_back_on_enables_every_switch() {
    let claude =
        dotfiles_xtask::agent_memory::reenabled(".claude/settings.json", CLAUDE_OFF).unwrap();
    let codex = dotfiles_xtask::agent_memory::reenabled(".codex/config.toml", CODEX_OFF).unwrap();
    let found = violations(&dump(Some(&claude), Some(&codex), Some(&off()))).unwrap();
    assert_eq!(found.len(), 6, "{found:?}");
    assert!(
        found.iter().all(|line| line.ends_with("is Enabled")),
        "{found:?}"
    );
    assert!(claude.contains("\"model\":\"opus\""), "{claude}");
    assert!(codex.contains("model = \"o3\""), "{codex}");
    assert_eq!(
        dotfiles_xtask::agent_memory::reenabled(".other.json", "{\"a\":1}").unwrap(),
        "{\"a\":1}"
    );
}

#[test]
fn the_memory_refusal_names_the_cause_and_a_runnable_next_action() {
    let message = dotfiles_xtask::agent_memory::refusal(
        "linux",
        &[".claude/settings.json: autoMemoryEnabled is Unset".into()],
    );
    let (cause, action) = message.split_once('\n').unwrap();
    assert!(
        cause.contains("linux") && cause.contains("autoMemoryEnabled is Unset"),
        "{message}"
    );
    assert!(action.contains("`just profiles`"), "{message}");
}
