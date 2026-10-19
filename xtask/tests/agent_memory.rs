use serde_json::{Value, json};
use xtask::agent_memory::violations;

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
    assert_eq!(
        violations(&dump(Some(CLAUDE_OFF), Some(CODEX_OFF), Some(&off))).unwrap(),
        Vec::<String>::new()
    );
}

#[test]
fn an_unmanaged_client_needs_no_settings() {
    assert_eq!(violations(&json!({})).unwrap(), Vec::<String>::new());
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

/// Renders a repository `modify_` template for one profile over a host file, as an apply would.
fn render_merged(profile: &str, template: &str, target: &str, host: &str) -> String {
    render_merged_with_mode(profile, template, target, host, None, false)
}

fn render_merged_with_mode(
    profile: &str,
    template: &str,
    target: &str,
    host: &str,
    consent: Option<&str>,
    stopped: bool,
) -> String {
    let root = xtask::source_state(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap(),
    );
    let scope = tempfile::tempdir().unwrap();
    let source = scope.path().join("source");
    let destination = scope.path().join("home with ' quote");
    for file in [
        template,
        ".chezmoitemplates/partials/sh_quote",
        ".chezmoitemplates/partials/ps_quote",
        ".chezmoitemplates/partials/codex_runtime_hook",
        ".chezmoitemplates/partials/without_skill_ops_hooks",
        ".chezmoitemplates/partials/without_runtime_handlers",
        ".chezmoitemplates/partials/skill_ops_experimental",
        ".chezmoitemplates/native/claude-settings/wsl.json.tmpl",
    ] {
        let copy = source.join(file);
        std::fs::create_dir_all(copy.parent().unwrap()).unwrap();
        std::fs::copy(root.join(file), copy).unwrap();
    }
    let path = destination.join(target);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, host).unwrap();
    for (relative, content) in [
        (platform::incidents::EXPERIMENTAL, consent),
        (
            platform::incidents::DISABLED,
            stopped.then_some("owner stop"),
        ),
    ] {
        if let Some(content) = content {
            let file = destination.join(relative);
            std::fs::create_dir_all(file.parent().unwrap()).unwrap();
            std::fs::write(file, content).unwrap();
        }
    }
    let config = scope.path().join("chezmoi.toml");
    std::fs::write(
        &config,
        format!("[data]\nprofile = {profile:?}\n[data.platforms.{profile}]\n"),
    )
    .unwrap();
    let render = || {
        let rendered = xtask::profiles::chezmoi(&source, scope.path(), &config, &destination)
            .env("HOME", &destination)
            .env("USERPROFILE", &destination)
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
    };
    let first = render();
    std::fs::write(&path, &first).unwrap();
    assert_eq!(render(), first, "reapplication must be idempotent");
    first
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
                .is_some_and(
                    |command| command.ends_with(" claude-bash") && runs_the_runtime(command)
                ),
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

/// Whether a rendered hook command starts the installed runtime, whose path the host spells with either separator.
fn runs_the_runtime(command: &str) -> bool {
    command.contains("dotfiles' ") || command.contains("dotfiles.exe' ")
}

#[test]
fn canonical_codex_bash_events_reach_the_shared_shell_guard() {
    for profile in ["mac", "linux", "windows", "wsl"] {
        let rendered: Value = serde_json::from_str(&render_merged(
            profile,
            "dot_codex/modify_hooks.json",
            ".codex/hooks.json",
            "{}",
        ))
        .unwrap();
        let groups = rendered["hooks"]["PreToolUse"].as_array().unwrap();
        let shell = groups
            .iter()
            .find(|group| {
                group["hooks"].as_array().is_some_and(|hooks| {
                    hooks.iter().any(|hook| {
                        hook["command"].as_str().is_some_and(|command| {
                            command.ends_with(" claude-bash") && runs_the_runtime(command)
                        })
                    })
                })
            })
            .unwrap();
        let matcher = shell["matcher"].as_str().unwrap();
        assert!(
            matcher.split('|').any(|name| name == "Bash"),
            "canonical Codex shell event is unmatched: {profile}: {shell}"
        );
        assert!(
            shell["hooks"]
                .as_array()
                .unwrap()
                .iter()
                .all(|hook| hook.get("onFailure").is_none()),
            "unsupported Codex failure policy: {shell}"
        );
    }
}

#[test]
fn codex_runtime_hooks_have_native_windows_commands() {
    for profile in ["mac", "linux", "windows", "wsl"] {
        let rendered: Value = serde_json::from_str(&render_merged(
            profile,
            "dot_codex/modify_hooks.json",
            ".codex/hooks.json",
            "{}",
        ))
        .unwrap();
        let handlers: Vec<_> = rendered["hooks"]
            .as_object()
            .unwrap()
            .values()
            .flat_map(|groups| groups.as_array().unwrap())
            .flat_map(|group| group["hooks"].as_array().unwrap())
            .filter(|hook| hook["command"].as_str().is_some_and(runs_the_runtime))
            .collect();
        assert_eq!(handlers.len(), 4, "{profile}: {rendered}");
        for handler in handlers {
            assert_eq!(
                handler["commandWindows"].is_string(),
                profile == "windows",
                "{profile}: {handler}"
            );
        }
    }
}

#[cfg(windows)]
const CODEX_HOOK_PROBE: &str = "DOTFILES_CODEX_HOOK_PROBE";
#[cfg(windows)]
const CODEX_HOOK_INPUT: &str = "hook input 日本語\n";

#[cfg(windows)]
#[test]
fn codex_hook_child_receives_standard_input() {
    use std::io::Read;

    let Some(receipt) = std::env::var_os(CODEX_HOOK_PROBE) else {
        return;
    };
    let mut input = String::new();
    std::io::stdin().read_to_string(&mut input).unwrap();
    std::fs::write(receipt, &input).unwrap();
    assert_eq!(input, CODEX_HOOK_INPUT);
}

#[cfg(windows)]
fn windows_codex_hook(tool: &std::path::Path) -> Value {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap();
    let scope = tempfile::tempdir().unwrap();
    let config = scope.path().join("chezmoi.json");
    std::fs::write(&config, "{}").unwrap();
    let output = xtask::profiles::chezmoi(
        &xtask::source_state(root),
        scope.path(),
        &config,
        &scope.path().join("home"),
    )
    .arg("--override-data")
    .arg(json!({"hook": {"profile": "windows", "tool": tool, "arguments": "--exact codex_hook_child_receives_standard_input --nocapture"}}).to_string())
    .args(["execute-template", "{{ includeTemplate \"partials/codex_runtime_hook\" .hook }}"])
    .output()
    .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}

#[cfg(windows)]
fn run_codex_hook(command: &str, receipt: &std::path::Path, input: &str) -> std::process::Output {
    use std::io::Write;
    use std::os::windows::process::CommandExt;
    use std::process::Stdio;

    let mut child = platform::process::command(std::env::var_os("ComSpec").unwrap())
        .args(["/D", "/S", "/C"])
        .raw_arg(format!("\"{command}\""))
        .env(CODEX_HOOK_PROBE, receipt)
        .env("PATH", "")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(input.as_bytes())
        .unwrap();
    child.wait_with_output().unwrap()
}

#[cfg(windows)]
#[test]
fn codex_windows_hooks_preserve_paths_input_and_child_failures() {
    let scope = tempfile::tempdir().unwrap();
    let directory = scope
        .path()
        .join("home with ' & %USERPROFILE% ! $ (日本語)");
    std::fs::create_dir(&directory).unwrap();
    let tool = directory.join("runtime.exe");
    std::fs::copy(std::env::current_exe().unwrap(), &tool).unwrap();
    let receipt = scope.path().join("receipt");
    let hook = windows_codex_hook(&tool);

    let legacy = run_codex_hook(
        hook["command"].as_str().unwrap(),
        &receipt,
        CODEX_HOOK_INPUT,
    );
    assert!(!legacy.status.success());
    assert!(!receipt.exists(), "the legacy command reached the child");

    let command = hook["commandWindows"].as_str().unwrap();
    let valid = run_codex_hook(command, &receipt, CODEX_HOOK_INPUT);
    assert!(
        valid.status.success(),
        "{}",
        String::from_utf8_lossy(&valid.stderr)
    );
    assert_eq!(std::fs::read_to_string(&receipt).unwrap(), CODEX_HOOK_INPUT);

    let rejected = run_codex_hook(command, &receipt, "wrong input");
    assert_eq!(rejected.status.code(), Some(101));
    assert_eq!(std::fs::read_to_string(&receipt).unwrap(), "wrong input");

    std::fs::remove_file(&receipt).unwrap();
    let missing = windows_codex_hook(&directory.join("missing-runtime.exe"));
    let failed = run_codex_hook(
        missing["commandWindows"].as_str().unwrap(),
        &receipt,
        CODEX_HOOK_INPUT,
    );
    assert_eq!(failed.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&failed.stderr).contains("missing-runtime.exe"));
    assert!(!receipt.exists());
}

#[test]
fn supported_edit_clients_register_the_issue_guard_without_skill_consent() {
    let existing = json!({"hooks":{"PreToolUse":[{"matcher":"Write","metadata":{"keep":true},"hooks":[{"type":"command","command":"/old/dotfiles issue-guard"},{"type":"command","command":"foreign-write-check"}]}]}}).to_string();
    for profile in ["mac", "linux", "windows", "wsl"] {
        for (source, target) in [
            ("dot_claude/modify_settings.json", ".claude/settings.json"),
            ("dot_codex/modify_hooks.json", ".codex/hooks.json"),
        ] {
            let rendered: Value =
                serde_json::from_str(&render_merged(profile, source, target, &existing)).unwrap();
            let groups = rendered["hooks"]["PreToolUse"].as_array().unwrap();
            let commands: Vec<_> = groups
                .iter()
                .flat_map(|group| group["hooks"].as_array().unwrap())
                .filter_map(|hook| hook["command"].as_str())
                .collect();
            assert!(
                commands.contains(&"foreign-write-check"),
                "{profile}: {rendered}"
            );
            assert!(
                !commands.contains(&"/old/dotfiles issue-guard"),
                "{profile}: {rendered}"
            );
            assert_eq!(commands.iter().filter(|command| command.ends_with(" issue-guard") && runs_the_runtime(command)).count(), 1, "{profile}: {rendered}");
            assert_eq!(groups.first().unwrap()["metadata"]["keep"], json!(true));
        }
    }
}

/// The timing hook runs before and after every shell call.
/// An installer keeps the post-tool groups that other installers added while it replaces its own.
#[test]
fn the_timing_hook_joins_the_shell_events_and_keeps_other_post_tool_groups() {
    let host = json!({"hooks": {
        "PreToolUse": [
            {"matcher": "Read", "hooks": [{"type": "command", "command": "custom-observer"}]},
            {"matcher": "Bash|PowerShell", "hooks": [{"type": "command", "command": "/old/dotfiles-xtask claude-timing"}]},
            {"hooks": [{"type": "command", "command": "skill-ops observe-tool"}]}
        ],
        "PostToolUse": [
            {"matcher": "Write", "hooks": [{"type": "command", "command": "dotfiles line-endings hook"}]},
            {"matcher": "Bash|PowerShell", "hooks": [{"type": "command", "command": "/old/dotfiles-xtask claude-timing"}]}
        ]
    }});
    let host = skill_ops::skill_install::merge_hooks(host, "claude")
        .unwrap()
        .to_string();
    for profile in ["mac", "linux", "windows", "wsl"] {
        let rendered: Value = serde_json::from_str(&render_merged(
            profile,
            "dot_claude/modify_settings.json",
            ".claude/settings.json",
            &host,
        ))
        .unwrap();
        let timing = |group: &Value| {
            group["matcher"] == json!("Bash|PowerShell")
                && group["hooks"].as_array().is_some_and(|hooks| {
                    hooks.iter().any(|hook| {
                        hook["command"].as_str().is_some_and(|command| {
                            command.ends_with(" claude-timing") && runs_the_runtime(command)
                        })
                    })
                })
        };
        let pre = rendered["hooks"]["PreToolUse"].as_array().unwrap();
        assert_eq!(pre.len(), 4, "{profile}: {rendered}");
        assert_eq!(pre[0]["hooks"][0]["command"], json!("custom-observer"));
        assert_eq!(
            pre[1]["hooks"][0]["command"],
            json!("skill-ops observe-tool")
        );
        assert!(timing(&pre[2]), "{profile}: {rendered}");
        assert_eq!(pre[3]["matcher"], json!("Edit|Write|MultiEdit|apply_patch"));
        assert!(
            pre[3]["hooks"][0]["command"]
                .as_str()
                .is_some_and(
                    |command| command.ends_with(" issue-guard") && runs_the_runtime(command)
                ),
            "{profile}: {rendered}"
        );
        for event in ["PostToolUse", "PostToolUseFailure"] {
            let groups = rendered["hooks"][event].as_array().unwrap();
            assert_eq!(
                groups.iter().filter(|group| timing(group)).count(),
                1,
                "{profile} {event}"
            );
            assert!(
                !rendered.to_string().contains("/old/dotfiles-xtask"),
                "{profile}"
            );
        }
        assert_eq!(
            rendered["hooks"]["PostToolUse"][0]["hooks"][0]["command"],
            json!("dotfiles line-endings hook"),
            "{profile}"
        );
    }
}

fn assert_foreign_handlers_preserved(rendered: &Value) -> Vec<&str> {
    assert_eq!(rendered["foreign"], json!({"keep":true}));
    assert_eq!(
        rendered["hooks"]["SessionResume"],
        json!([
            {"matcher":"*", "metadata":{"keep":7}, "hooks":[
                {"type":"command", "command":"custom session", "timeout":23}
            ]}, {"hooks":[]}
        ])
    );
    assert_eq!(
        rendered["hooks"]["PostToolUse"][0]["metadata"],
        json!({"keep":9})
    );
    assert_eq!(
        rendered["hooks"]["PostToolUse"][0]["hooks"],
        json!([
            {"type":"command", "command":"prose hook"},
            {"type":"command", "command":"/usr/bin/timing claude-timing"},
            {"type":"command", "command":"skill-ops observe-tool"}
        ])
    );
    let commands: Vec<_> = rendered["hooks"]
        .as_object()
        .unwrap()
        .values()
        .flat_map(|groups| groups.as_array().unwrap())
        .flat_map(|group| group["hooks"].as_array().unwrap())
        .filter_map(|handler| handler["command"].as_str())
        .collect();
    assert!(commands.contains(&"foreign guard"));
    assert!(commands.contains(&"foreign stop"));
    assert!(commands.contains(&"/usr/bin/check claude-bash"));
    assert!(commands.contains(&"/usr/bin/assure assure stop"));
    assert!(
        commands
            .iter()
            .any(|command| command.ends_with(" assure stop"))
    );
    commands
}

#[test]
fn experimental_hooks_require_local_consent_and_preserve_every_foreign_handler() {
    let host = json!({"foreign": {"keep": true}, "hooks": {
        "SessionResume": [
            {"matcher":"*", "metadata":{"keep":7}, "hooks":[
                {"type":"command", "command":"skill-ops observe --client claude"},
                {"type":"command", "command":"custom session", "timeout":23}
            ]},
            {"hooks":[]},
            {"hooks":[{"type":"command", "command":"skill-ops stop"}]}
        ],
        "PreToolUse": [{"matcher":"Bash", "hooks":[
            {"type":"command", "command":"/old/dotfiles claude-bash"},
            {"type":"command", "command":"skill-ops observe --client codex"},
            {"type":"command", "command":"foreign guard"},
            {"type":"command", "command":"/usr/bin/check claude-bash"}
        ]}],
        "PostToolUse": [{"matcher":"Read", "metadata":{"keep":9}, "hooks":[
            {"type":"command", "command":"skill-ops observe --client claude"},
            {"type":"command", "command":"skill-ops observe --client codex"},
            {"type":"command", "command":"prose hook"},
            {"type":"command", "command":"/usr/bin/timing claude-timing"},
            {"type":"command", "command":"skill-ops observe-tool"}
        ]}],
        "Stop": [{"hooks":[
            {"type":"command", "command":"skill-ops stop"},
            {"type":"command", "command":"foreign stop"},
            {"type":"command", "command":"/usr/bin/assure assure stop"}
        ]}]
    }})
    .to_string();
    for profile in ["mac", "linux", "windows", "wsl"] {
        for (template, target, client) in [
            (
                "dot_claude/modify_settings.json",
                ".claude/settings.json",
                "claude",
            ),
            ("dot_codex/modify_hooks.json", ".codex/hooks.json", "codex"),
        ] {
            for (consent, stopped, enabled) in [
                (None, false, false),
                (Some("enabled\n"), false, true),
                (Some("enabled\r\n"), false, false),
                (Some("enabled\n"), true, false),
            ] {
                let rendered: Value = serde_json::from_str(&render_merged_with_mode(
                    profile, template, target, &host, consent, stopped,
                ))
                .unwrap();
                let commands = assert_foreign_handlers_preserved(&rendered);
                let observe = format!("skill-ops observe --client {client}");
                assert_eq!(
                    commands
                        .iter()
                        .filter(|command| **command == observe)
                        .count(),
                    usize::from(enabled)
                );
                assert_eq!(
                    commands
                        .iter()
                        .filter(|command| **command == "skill-ops stop")
                        .count(),
                    usize::from(enabled)
                );
            }
        }
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
    let claude = xtask::agent_memory::reenabled(".claude/settings.json", CLAUDE_OFF).unwrap();
    let codex = xtask::agent_memory::reenabled(".codex/config.toml", CODEX_OFF).unwrap();
    let found = violations(&dump(Some(&claude), Some(&codex), Some(&off()))).unwrap();
    assert_eq!(found.len(), 6, "{found:?}");
    assert!(
        found.iter().all(|line| line.ends_with("is Enabled")),
        "{found:?}"
    );
    assert!(claude.contains("\"model\":\"opus\""), "{claude}");
    assert!(codex.contains("model = \"o3\""), "{codex}");
    assert_eq!(
        xtask::agent_memory::reenabled(".other.json", "{\"a\":1}").unwrap(),
        "{\"a\":1}"
    );
}

#[test]
fn the_memory_refusal_names_the_cause_and_a_runnable_next_action() {
    let message = xtask::agent_memory::refusal(
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

#[test]
fn optional_spoken_hooks_keep_mixed_foreign_handlers_and_reapply_once() {
    let host = json!({ "foreign": {"keep":true}, "hooks": {
        "Notification": [{"matcher":"*", "metadata":{"keep":7}, "hooks":[
            {"type":"command", "command":"/old/dotfiles notifications agent claude"},
            {"type":"command", "command":"foreign notification", "timeout":23},
            {"type":"command", "command":"/usr/bin/custom notifications agent claude"}
        ]}],
        "Stop": [{"hooks":[{"type":"command", "command":"foreign stop"}]}]
    }})
    .to_string();
    for profile in ["mac", "linux", "windows", "wsl"] {
        let rendered: Value = serde_json::from_str(&render_merged(
            profile,
            "dot_claude/modify_settings.json",
            ".claude/settings.json",
            &host,
        ))
        .unwrap();
        assert_eq!(rendered["foreign"], json!({"keep":true}));
        assert_eq!(
            rendered["hooks"]["Notification"][0]["metadata"],
            json!({"keep":7})
        );
        assert_eq!(
            rendered["hooks"]["Notification"][0]["hooks"],
            json!([
                {"type":"command", "command":"foreign notification", "timeout":23},
                {"type":"command", "command":"/usr/bin/custom notifications agent claude"}
            ])
        );
        for event in [
            "Notification",
            "UserPromptSubmit",
            "PostToolUse",
            "PostToolUseFailure",
        ] {
            let handlers = rendered["hooks"][event]
                .as_array()
                .unwrap()
                .iter()
                .flat_map(|group| group["hooks"].as_array().unwrap())
                .filter(|handler| {
                    handler["command"].as_str().is_some_and(|command| {
                        command.ends_with(" notifications agent claude")
                            && runs_the_runtime(command)
                    })
                })
                .collect::<Vec<_>>();
            assert_eq!(handlers.len(), 1);
            assert_eq!(handlers[0]["async"], true);
            assert!(handlers[0].get("onFailure").is_none());
        }
        assert!(
            rendered["hooks"]["Stop"]
                .as_array()
                .unwrap()
                .iter()
                .flat_map(|group| group["hooks"].as_array().unwrap())
                .all(|handler| !handler["command"]
                    .as_str()
                    .is_some_and(|command| command.ends_with(" notifications agent claude")))
        );
    }
}

#[test]
fn codex_completion_mux_preserves_the_original_notify_vector_and_private_keys() {
    for profile in ["mac", "linux", "windows", "wsl"] {
        let original = vec!["foreign-notify", "argument with spaces"];
        let host = "notify = [\"foreign-notify\", \"argument with spaces\"]\nmodel = \"owner-model\"\n[projects.\"private-project\"]\ntrust_level = \"trusted\"\n";
        let rendered = render_merged(
            profile,
            "dot_codex/modify_config.toml",
            ".codex/config.toml",
            host,
        );
        let parsed: toml::Value = toml::from_str(&rendered).unwrap();
        assert_eq!(parsed["model"].as_str(), Some("owner-model"));
        assert_eq!(
            parsed["projects"]["private-project"]["trust_level"].as_str(),
            Some("trusted")
        );
        let notify = parsed["notify"].as_array().unwrap();
        assert_eq!(notify[1].as_str(), Some("notifications"));
        assert_eq!(notify[2].as_str(), Some("agent"));
        assert_eq!(notify[3].as_str(), Some("codex"));
        assert_eq!(notify[4].as_str(), Some("--forward-notify"));
        let forwarded: Vec<String> = serde_json::from_str(notify[5].as_str().unwrap()).unwrap();
        assert_eq!(forwarded, original);
    }
}
