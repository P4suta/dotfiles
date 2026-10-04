#![allow(
    clippy::disallowed_methods,
    reason = "integration tests spawn the binaries and real tools they verify"
)]

use anyhow::{Context, Result, ensure};
use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn root() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap()
}

fn successful(command: &mut Command) -> Result<Output> {
    let output = command.output().context("start native verification tool")?;
    ensure!(
        output.status.success(),
        "{command:?}: {}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    Ok(output)
}

fn pinned_tool(name: &str) -> Result<PathBuf> {
    let output = successful(Command::new("mise").args(["which", name]))?;
    let path = PathBuf::from(String::from_utf8(output.stdout)?.trim());
    ensure!(
        path.is_absolute() && path.is_file(),
        "missing pinned {name}"
    );
    Ok(path)
}

#[test]
fn opencode_context7_requires_a_local_linux_desktop() -> Result<()> {
    let chezmoi = pinned_tool("chezmoi")?;
    let template = fs::read_to_string(root().join("dot_config/opencode/opencode.json.tmpl"))?;
    let temporary = tempfile::tempdir()?;
    let config = temporary.path().join("chezmoi.toml");
    fs::write(&config, "")?;
    fs::copy(
        root().join(".chezmoidata.json"),
        temporary.path().join(".chezmoidata.json"),
    )?;
    for (profile, file) in [
        ("linux", "opencode.json.tmpl"),
        ("mac", "opencode.json.tmpl"),
        ("windows", "opencode.json"),
    ] {
        let path = format!(".chezmoitemplates/profiles/{profile}/dot_config/opencode/{file}");
        fs::create_dir_all(
            temporary
                .path()
                .join(&path)
                .parent()
                .context("fixture parent is missing")?,
        )?;
        fs::copy(root().join(&path), temporary.path().join(&path))?;
    }
    let bin = temporary.path().join("bin");
    fs::create_dir(&bin)?;
    let desktop = bin.join(if cfg!(windows) {
        "1password.exe"
    } else {
        "1password"
    });
    let app = temporary.path().join("1Password.app");
    let ssh = ["SSH_CONNECTION", "SSH_CLIENT", "SSH_TTY"];
    for native in [false, true] {
        if native {
            fs::create_dir(&app)?;
            fs::write(&desktop, "")?;
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                fs::set_permissions(&desktop, fs::Permissions::from_mode(0o755))?;
            }
        }
        for os in ["linux", "darwin", "windows"] {
            let profile = if os == "darwin" { "mac" } else { os };
            let override_data = serde_json::to_string(
                &serde_json::json!({"profile": profile, "chezmoi": {"os": os}}),
            )?;
            for mask in 0..=8 {
                let mut command = Command::new(&chezmoi);
                command
                    .arg("--config")
                    .arg(&config)
                    .arg("--source")
                    .arg(temporary.path())
                    .arg("--cache")
                    .arg(temporary.path().join("cache"))
                    .arg("--persistent-state")
                    .arg(temporary.path().join("state.boltdb"))
                    .arg("--override-data")
                    .arg(&override_data)
                    .args(["execute-template", &template])
                    .env("DOTFILES_ONEPASSWORD_APP", &app)
                    .env("PATH", &bin);
                for (index, name) in ssh.iter().enumerate() {
                    command.env_remove(name);
                    if mask == 8 {
                        command.env(name, "");
                    } else if mask & (1 << index) != 0 {
                        command.env(name, "remote");
                    }
                }
                let output = successful(&mut command)?;
                let document: Value = serde_json::from_slice(&output.stdout)?;
                let expected = os == "windows"
                    || (matches!(os, "linux" | "darwin") && native && matches!(mask, 0 | 8));
                assert_eq!(
                    document.get("plugin"),
                    expected
                        .then(|| serde_json::json!(["@upstash/context7-opencode"]))
                        .as_ref(),
                    "os={os}, native={native}, ssh={mask}"
                );
                assert_eq!(document["autoupdate"], false);
            }
        }
    }
    Ok(())
}

#[cfg(target_os = "linux")]
mod debsig {
    use super::*;
    use serde_json::json;
    use yaml_rust2::{Yaml, YamlLoader};

    fn json_value(value: &Yaml) -> Result<Value> {
        Ok(match value {
            Yaml::String(value) => Value::String(value.clone()),
            Yaml::Integer(value) => json!(value),
            Yaml::Boolean(value) => json!(value),
            Yaml::Null => Value::Null,
            Yaml::Array(values) => {
                Value::Array(values.iter().map(json_value).collect::<Result<_>>()?)
            }
            Yaml::Hash(values) => Value::Object(
                values
                    .iter()
                    .map(|(key, value)| {
                        Ok((
                            key.as_str()
                                .context("task key must be a string")?
                                .to_owned(),
                            json_value(value)?,
                        ))
                    })
                    .collect::<Result<_>>()?,
            ),
            _ => anyhow::bail!("unsupported task YAML value: {value:?}"),
        })
    }

    fn task(name: &str) -> Result<Value> {
        let source = fs::read_to_string(root().join("ops/tasks/desktop.yml"))?;
        let documents = YamlLoader::load_from_str(&source)?;
        let tasks = documents[0]
            .as_vec()
            .context("desktop tasks must be a sequence")?;
        let task = tasks
            .iter()
            .find(|task| task["name"].as_str() == Some(name))
            .context("missing production task")?;
        json_value(task)
    }

    fn expected_keyring(fixture: &Path, destination: &Path, home: &Path) -> Result<Vec<u8>> {
        successful(
            Command::new("gpg")
                .env("GNUPGHOME", home)
                .args(["--batch", "--dearmor", "--output"])
                .arg(destination)
                .arg(fixture),
        )?;
        Ok(fs::read(destination)?)
    }

    struct Fixture {
        directory: tempfile::TempDir,
        ansible: PathBuf,
        source: PathBuf,
        keyring: PathBuf,
        download: Value,
        conversion: Value,
    }

    impl Fixture {
        fn new() -> Result<Self> {
            let directory = tempfile::tempdir()?;
            let source = directory.path().join("downloaded.asc");
            let keyring = directory.path().join("debsig.gpg");
            let mut download = task("Install the 1Password repository signing key")?;
            download["become"] = json!(false);
            let parameters = download["ansible.builtin.get_url"]
                .as_object_mut()
                .context("get_url mapping")?;
            parameters.remove("owner");
            parameters.remove("group");
            parameters.insert("dest".into(), json!(source));
            let mut conversion = task("Install the 1Password debsig verification keyring")?;
            conversion["become"] = json!(false);
            conversion["register"] = json!("fixture_conversion");
            let parameters = conversion["ansible.builtin.command"]
                .as_object_mut()
                .context("command mapping")?;
            let argv = parameters["argv"].as_array_mut().context("command argv")?;
            for argument in argv {
                match argument.as_str().context("string argv")? {
                    "/etc/apt/keyrings/1password.asc" => *argument = json!(source),
                    "/usr/share/debsig/keyrings/AC2D62742012EA22/debsig.gpg" => {
                        *argument = json!(keyring)
                    }
                    _ => {}
                }
            }
            let creates = parameters["creates"]
                .as_str()
                .context("creates expression")?
                .replace(
                    "/usr/share/debsig/keyrings/AC2D62742012EA22/debsig.gpg",
                    keyring.to_str().context("UTF-8 fixture path")?,
                );
            parameters.insert("creates".into(), json!(creates));
            fs::write(
                directory.path().join("ansible.cfg"),
                "[defaults]\nretry_files_enabled = False\n",
            )?;
            fs::create_dir(directory.path().join("gnupg"))?;
            {
                use std::os::unix::fs::PermissionsExt;
                fs::set_permissions(
                    directory.path().join("gnupg"),
                    fs::Permissions::from_mode(0o700),
                )?;
            }
            Ok(Self {
                directory,
                ansible: pinned_tool("ansible-playbook")?,
                source,
                keyring,
                download,
                conversion,
            })
        }

        fn convert(&self, fixture: &Path, changed: bool) -> Result<Output> {
            let mut download = self.download.clone();
            download["ansible.builtin.get_url"]["url"] =
                json!(format!("file://{}", fixture.display()));
            let playbook = self.directory.path().join("playbook.json");
            fs::write(
                &playbook,
                serde_json::to_vec(&json!([{
                    "name": "Verify production debsig conversion",
                    "hosts": "localhost",
                    "connection": "local",
                    "gather_facts": false,
                    "vars": {"ansible_python_interpreter": "/usr/bin/python3"},
                    "tasks": [download, self.conversion, {
                        "name": "Check the expected conversion transition",
                        "ansible.builtin.assert": {"that": [format!("fixture_conversion.changed == {}", if changed { "true" } else { "false" })]}
                    }]
                }]))?,
            )?;
            Ok(Command::new(&self.ansible)
                .current_dir(self.directory.path())
                .args(["--inventory", "localhost,", "--connection", "local"])
                .arg(&playbook)
                .env("ANSIBLE_CONFIG", self.directory.path().join("ansible.cfg"))
                .env("ANSIBLE_LOCAL_TEMP", self.directory.path().join("local"))
                .env("ANSIBLE_REMOTE_TEMP", self.directory.path().join("remote"))
                .env("GNUPGHOME", self.directory.path().join("gnupg"))
                .env("LC_ALL", "C.UTF-8")
                .output()?)
        }

        fn accepted(&self, fixture: &Path, changed: bool) -> Result<()> {
            let output = self.convert(fixture, changed)?;
            ensure!(
                output.status.success(),
                "{}\n{}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
            Ok(())
        }
    }

    #[test]
    fn production_keyring_conversion_tracks_key_updates_and_missing_output() -> Result<()> {
        let fixture = Fixture::new()?;
        let original = root().join("xtask/tests/fixtures/debsig-original.asc");
        let updated = root().join("xtask/tests/fixtures/debsig-updated.asc");
        let home = fixture.directory.path().join("gnupg");
        let expected_original = expected_keyring(
            &original,
            &fixture.directory.path().join("original.gpg"),
            &home,
        )?;
        let expected_updated = expected_keyring(
            &updated,
            &fixture.directory.path().join("updated.gpg"),
            &home,
        )?;
        assert_ne!(expected_original, expected_updated);
        fixture.accepted(&original, true)?;
        assert_eq!(fs::read(&fixture.keyring)?, expected_original);
        fixture.accepted(&original, false)?;
        fixture.accepted(&updated, true)?;
        assert_eq!(fs::read(&fixture.keyring)?, expected_updated);
        fs::remove_file(&fixture.keyring)?;
        fixture.accepted(&updated, true)?;
        assert_eq!(fs::read(&fixture.keyring)?, expected_updated);
        let invalid = fixture.directory.path().join("invalid.asc");
        fs::write(&invalid, "invalid OpenPGP input")?;
        let output = fixture.convert(&invalid, true)?;
        assert!(
            !output.status.success(),
            "a failed converter must fail the playbook"
        );
        assert!(String::from_utf8_lossy(&output.stdout).contains("no valid OpenPGP data"));
        assert_eq!(fs::read(&fixture.source)?, b"invalid OpenPGP input");
        Ok(())
    }
}
