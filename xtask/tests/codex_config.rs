use anyhow::Result;
use serde_json::json;
use std::fs;
use std::path::Path;
use xtask::{profile_rules::Profile, profile_snapshot::Combination, profiles};

#[test]
fn codex_sandbox_preference_preserves_host_permissions_and_other_profiles() -> Result<()> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    for combination in Combination::all() {
        for previous in [None, Some(false), Some(true)] {
            let scope = tempfile::tempdir()?;
            let destination = scope.path().join("home");
            let target = destination.join(".codex/config.toml");
            fs::create_dir_all(target.parent().unwrap())?;
            let config_path = scope.path().join("chezmoi.json");
            let mut answers = profiles::Answers::fixture(combination.profile, "/projects".into());
            answers.role = combination.role.name();
            if combination.signing {
                answers.public_key = "ssh-ed25519 AAAA fixture".into();
            }
            let mut data = profiles::configuration_data(&root, &answers)?;
            data["chezmoi"] = profiles::fixture_host(
                combination.profile,
                &profiles::chezmoi_path(&root),
                &profiles::chezmoi_path(&destination),
            );
            fs::write(&config_path, serde_json::to_vec(&json!({"data": data}))?)?;
            let mut host: toml::Table = toml::from_str(
                "model = 'host-model'\napproval_policy = 'on-request'\n\
                 [windows]\nsandbox = 'elevated'\n\
                 [features]\ncustom_host_feature = true\n",
            )?;
            if let Some(preference) = previous {
                host["features"]
                    .as_table_mut()
                    .unwrap()
                    .insert("prefer_mxc".into(), preference.into());
            }
            fs::write(&target, toml::to_string(&host)?)?;
            let rendered = profiles::output(
                profiles::chezmoi(&root, scope.path(), &config_path, &destination)
                    .arg("--override-data")
                    .arg(serde_json::to_string(&data)?)
                    .arg("cat")
                    .arg(&target),
                "render existing Codex configuration",
            )?;
            let rendered: toml::Table = toml::from_str(std::str::from_utf8(&rendered.stdout)?)?;
            let preference = rendered["features"]
                .get("prefer_mxc")
                .and_then(toml::Value::as_bool);
            assert_eq!(
                preference,
                if combination.profile == Profile::Windows {
                    Some(true)
                } else {
                    previous
                },
                "{} with previous preference {previous:?}",
                combination.name()
            );
            for key in ["model", "approval_policy", "windows"] {
                assert_eq!(rendered[key], host[key], "{}: {key}", combination.name());
            }
            assert_eq!(
                rendered["features"]["custom_host_feature"].as_bool(),
                Some(true)
            );
        }
    }
    Ok(())
}
