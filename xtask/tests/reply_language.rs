//! One owner-local language value drives the policy, setting, and Stop commands.

use anyhow::Result;
use serde_json::{Value, json};
use std::fs;
use std::path::Path;
use xtask::profile_rules::Profile;
use xtask::profiles;

fn render(profile: Profile, language: Option<&str>, template: &str) -> Result<String> {
    render_current(profile, language, template, "")
}

fn render_current(
    profile: Profile,
    language: Option<&str>,
    template: &str,
    current: &str,
) -> Result<String> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    let scope = tempfile::tempdir()?;
    let home = scope.path().join("home");
    fs::create_dir(&home)?;
    let config = scope.path().join("chezmoi.json");
    let mut data = profiles::fixture(profile, root, &home)?;
    if let Some(language) = language {
        data["reply_language"] = json!(language);
    } else {
        data.as_object_mut().unwrap().remove("reply_language");
    }
    data["chezmoi"]["stdin"] = json!(current);
    fs::write(&config, serde_json::to_vec(&json!({"data": data}))?)?;
    let result = profiles::output(
        profiles::chezmoi(root, scope.path(), &config, &home)
            .arg("--override-data")
            .arg(serde_json::to_string(&data)?)
            .args(["execute-template", template]),
        "render reply-language consumers",
    )?;
    Ok(String::from_utf8(result.stdout)?)
}

#[test]
fn reapplying_a_language_change_replaces_managed_hooks_and_keeps_other_handlers() -> Result<()> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    for profile in Profile::ALL {
        for path in [
            "dot_claude/modify_settings.json",
            "dot_codex/modify_hooks.json",
        ] {
            let template = fs::read_to_string(root.join("home").join(path))?;
            let mut current = json!({"hooks": {"Stop": [{"hooks": [
                {"type": "command", "command": "dotfiles assure stop"},
                {"type": "command", "command": "owner-hook"}
            ]}]}})
            .to_string();
            for language in ["English", "Japanese", "Japanese"] {
                current = render_current(profile, Some(language), &template, &current)?;
                let hooks: Value = serde_json::from_str(&current)?;
                let handlers: Vec<_> = hooks["hooks"]["Stop"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .flat_map(|group| group["hooks"].as_array().unwrap())
                    .collect();
                assert_eq!(handlers.len(), 2, "{current}");
                assert_eq!(handlers[0]["command"], "owner-hook");
                assert!(
                    handlers[1]["command"]
                        .as_str()
                        .unwrap()
                        .ends_with(&format!("--reply-language {language}")),
                    "{current}"
                );
                if profile == Profile::Windows && path.contains("codex") {
                    assert!(
                        handlers[1]["commandWindows"]
                            .as_str()
                            .unwrap()
                            .contains(&format!("--reply-language {language}"))
                    );
                }
            }
        }
    }
    Ok(())
}

#[test]
fn unsupported_owner_language_is_refused_during_rendering() {
    assert!(
        render(
            Profile::Mac,
            Some("French"),
            "{{ includeTemplate \"partials/reply_language\" . }}"
        )
        .is_err()
    );
}

#[test]
fn every_profile_renders_the_selected_language_in_both_clients_and_the_policy() -> Result<()> {
    for profile in Profile::ALL {
        for (configured, expected) in [(None, "Japanese"), (Some("English"), "English")] {
            let policy = render(
                profile,
                configured,
                "{{ includeTemplate \"partials/agent_policy\" . }}",
            )?;
            assert!(policy.contains(&format!("- Reply in {expected}.")));
            assert!(policy.contains("outward-facing artifacts"));
            let setting: Value = serde_json::from_str(&render(
                profile,
                configured,
                "{{ includeTemplate \"partials/claude_settings\" . }}",
            )?)?;
            assert_eq!(setting["language"], expected);
            let root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
            for path in [
                "dot_claude/modify_settings.json",
                "dot_codex/modify_hooks.json",
            ] {
                let template = fs::read_to_string(root.join("home").join(path))?;
                let hooks: Value = serde_json::from_str(&render(profile, configured, &template)?)?;
                let commands: Vec<_> = hooks["hooks"]["Stop"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .flat_map(|group| group["hooks"].as_array().unwrap())
                    .filter_map(|hook| hook["command"].as_str())
                    .collect();
                assert_eq!(commands.len(), 1, "{profile:?}: {path}");
                assert!(
                    commands[0].contains(&format!("--reply-language {expected}")),
                    "{}",
                    commands[0]
                );
            }
        }
    }
    Ok(())
}
