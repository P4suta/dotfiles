use anyhow::{Context, Result, ensure};
use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

#[path = "../../platform/tests/support/fixture_process.rs"]
mod fixture_process;

fn root() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .unwrap()
}

/// The chezmoi source state of the checkout.
fn source() -> PathBuf {
    dotfiles::source_state(root())
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
    let output = successful(platform::process::command("mise").args(["which", name]))?;
    let path = PathBuf::from(String::from_utf8(output.stdout)?.trim());
    ensure!(
        path.is_absolute() && path.is_file(),
        "missing pinned {name}"
    );
    Ok(path)
}

fn copy_opencode_templates(destination: &Path) -> Result<()> {
    for directory in [".chezmoidata", ".chezmoitemplates/native/opencode"] {
        let target = destination.join(directory);
        fs::create_dir_all(&target)?;
        for entry in fs::read_dir(source().join(directory))? {
            let entry = entry?;
            fs::copy(entry.path(), target.join(entry.file_name()))?;
        }
    }
    let partial = ".chezmoitemplates/partials/domyjob_mcp";
    let target = destination.join(partial);
    fs::create_dir_all(target.parent().context("partial parent is missing")?)?;
    fs::copy(source().join(partial), target)?;
    fs::write(destination.join("chezmoi.toml"), "")?;
    Ok(())
}

fn opencode_command(chezmoi: &Path, fixture: &Path, home: &Path, profile: &str) -> Result<Command> {
    let os = if profile == "mac" {
        "darwin"
    } else if profile == "wsl" {
        "linux"
    } else {
        profile
    };
    let overrides = serde_json::to_string(&serde_json::json!({
        "profile": profile,
        "chezmoi": {"os": os, "homeDir": home},
    }))?;
    let template = fs::read_to_string(source().join("dot_config/opencode/opencode.json.tmpl"))?;
    let mut command = platform::process::command(chezmoi);
    command
        .arg("--config")
        .arg(fixture.join("chezmoi.toml"))
        .arg("--source")
        .arg(fixture)
        .arg("--cache")
        .arg(fixture.join("cache"))
        .arg("--persistent-state")
        .arg(fixture.join("state.boltdb"))
        .arg("--override-data")
        .arg(overrides)
        .args(["execute-template", &template])
        .env("HOME", home)
        .env("USERPROFILE", home)
        .env("DOTFILES_ONEPASSWORD_APP", home.join("absent-app"))
        .env("PATH", home);
    for name in ["SSH_CONNECTION", "SSH_CLIENT", "SSH_TTY"] {
        command.env_remove(name);
    }
    Ok(command)
}

#[test]
fn domyjob_command_json_matches_the_installer_serializer() -> Result<()> {
    let chezmoi = pinned_tool("chezmoi")?;
    let scope = tempfile::tempdir()?;
    let home = scope.path().join("uninstalled home");
    fs::create_dir(&home)?;
    copy_opencode_templates(scope.path())?;
    let mut corpus = vec![
        String::new(),
        "R&D <tag> ' quote \" and 🦀".to_owned(),
        "x\u{2028}y\u{2029}z".to_owned(),
        "\u{2028}\u{2028}\u{2029}\u{2029}\u{2028}".to_owned(),
        r"literal\u2028\u2029\backslash".to_owned(),
        "\"\\\u{2028}\\u2028\"\u{2029}\\u2029".to_owned(),
    ];
    corpus.extend(
        (0..=31)
            .filter_map(char::from_u32)
            .map(|value| value.to_string()),
    );
    let overrides = serde_json::to_string(&serde_json::json!({
        "chezmoi": {"homeDir": home}, "encoderInputs": corpus,
    }))?;
    let partial = fs::read_to_string(source().join(".chezmoitemplates/partials/domyjob_mcp"))?;
    let template = format!(
        "{partial}{{{{range .encoderInputs}}}}{{{{template \"domyjob_json_string\" .}}}}{{{{println}}}}{{{{end}}}}"
    );
    let output = successful(
        platform::process::command(&chezmoi)
            .arg("--config")
            .arg(scope.path().join("chezmoi.toml"))
            .arg("--source")
            .arg(scope.path())
            .arg("--cache")
            .arg(scope.path().join("cache"))
            .arg("--persistent-state")
            .arg(scope.path().join("state.boltdb"))
            .arg("--override-data")
            .arg(overrides)
            .arg("execute-template")
            .arg(template)
            .env("HOME", &home)
            .env("USERPROFILE", &home),
    )?;
    let expected = corpus
        .iter()
        .map(serde_json::to_string)
        .collect::<serde_json::Result<Vec<_>>>()?
        .join("\n");
    assert_eq!(String::from_utf8(output.stdout)?, format!("{expected}\n"));
    Ok(())
}

#[test]
fn opencode_uses_only_the_valid_pinned_domyjob_build() -> Result<()> {
    const FIRST_BUILD: &str = "0123456789abcdef";
    const NEXT_BUILD: &str = "fedcba9876543210";
    let chezmoi = pinned_tool("chezmoi")?;
    let temporary = tempfile::tempdir()?;
    copy_opencode_templates(temporary.path())?;
    for profile in ["mac", "linux", "windows", "wsl"] {
        let home = temporary.path().join(format!(
            "{profile} home & ' quote 🦀 \u{2028}\u{2029}\u{2028}"
        ));
        fs::create_dir(&home)?;
        let output = successful(&mut opencode_command(
            &chezmoi,
            temporary.path(),
            &home,
            profile,
        )?)?;
        let document: Value = serde_json::from_slice(&output.stdout)?;
        assert!(document.pointer("/mcp/domyjob").is_none(), "{profile}");
        let versions = home.join(".cargo").join("domyjob").join("versions");
        fs::create_dir_all(&versions)?;
        let pin = versions.join("pinned");
        let executable = if profile == "windows" {
            "domyjob.exe"
        } else {
            "domyjob"
        };
        for build in [FIRST_BUILD, NEXT_BUILD] {
            let program = versions.join(build).join("bin").join(executable);
            fs::create_dir_all(program.parent().context("program parent is missing")?)?;
            fs::write(&program, "owned executable fixture")?;
            fs::write(&pin, build)?;
            let output = successful(&mut opencode_command(
                &chezmoi,
                temporary.path(),
                &home,
                profile,
            )?)?;
            let document: Value = serde_json::from_slice(&output.stdout)?;
            assert_eq!(
                document.pointer("/mcp/domyjob"),
                Some(&serde_json::json!({
                    "type": "local", "command": [program, "mcp"], "enabled": true,
                })),
                "{profile} {build}"
            );
            let expected = format!(
                r#"{{"type":"local","command":[{},"mcp"],"enabled":true}}"#,
                serde_json::to_string(&program)?,
            );
            let text = String::from_utf8(output.stdout)?;
            assert!(
                text.contains(&format!("\"domyjob\": {expected}")),
                "{profile}: {text}"
            );
            if build == NEXT_BUILD {
                assert!(!text.contains(FIRST_BUILD), "{profile}: {text}");
            }
        }
        let program = versions.join(NEXT_BUILD).join("bin").join(executable);
        fs::remove_file(&program)?;
        for directory in [false, true] {
            if directory {
                fs::create_dir(&program)?;
            }
            let output = opencode_command(&chezmoi, temporary.path(), &home, profile)?.output()?;
            assert!(
                !output.status.success(),
                "{profile}: accepted a missing or non-file program"
            );
            assert!(String::from_utf8(output.stderr)?.contains("pinned domyjob executable"));
        }
        for invalid in [
            "",
            "0123456789abcde",
            "0123456789ABCDEF",
            "../0123456789abcdef",
            "0123456789abcdef0",
        ] {
            fs::write(&pin, invalid)?;
            let output = opencode_command(&chezmoi, temporary.path(), &home, profile)?.output()?;
            assert!(!output.status.success(), "{profile}: accepted {invalid:?}");
            assert!(String::from_utf8(output.stderr)?.contains("domyjob build pin"));
        }
        fs::remove_file(&pin)?;
        fs::create_dir(&pin)?;
        let output = opencode_command(&chezmoi, temporary.path(), &home, profile)?.output()?;
        assert!(
            !output.status.success(),
            "{profile}: accepted a non-file pin"
        );
        assert!(String::from_utf8(output.stderr)?.contains("domyjob build pin"));
    }
    Ok(())
}

struct OpenCodeApplyFixture {
    scope: tempfile::TempDir,
    checkout: PathBuf,
    home: PathBuf,
    config: PathBuf,
    state: PathBuf,
    installed_template: PathBuf,
}

impl OpenCodeApplyFixture {
    fn new() -> Result<Self> {
        let scope = match std::env::var_os(skill_ops::INCIDENT_EVIDENCE_ROOT) {
            Some(directory) => tempfile::tempdir_in(directory)?,
            None => tempfile::tempdir()?,
        };
        let checkout = scope.path().join("checkout");
        let managed = checkout.join("home");
        let home = scope
            .path()
            .join("native home & ' quote 🦀 \u{2028}\u{2029}\u{2028}");
        fs::create_dir_all(&home)?;
        copy_opencode_templates(&managed)?;
        fs::write(checkout.join(".chezmoiroot"), "home\n")?;
        for krate in platform::build_inputs::CRATES {
            fs::create_dir_all(checkout.join(krate).join("src"))?;
        }
        let profile = dotfiles::profiles::native_profile()?;
        let config = scope.path().join("native.toml");
        fs::write(&config, format!("[data]\nprofile = {:?}\n", profile.name()))?;
        let state = scope.path().join("chezmoi-state");
        let installed_template = managed.join("dot_config/opencode/opencode.json.tmpl");
        fs::create_dir_all(
            installed_template
                .parent()
                .context("template parent is missing")?,
        )?;
        fs::copy(Self::production_template(), &installed_template)?;
        Ok(Self {
            scope,
            checkout,
            home,
            config,
            state,
            installed_template,
        })
    }

    fn production_template() -> PathBuf {
        source().join("dot_config/opencode/opencode.json.tmpl")
    }

    fn isolate(&self, command: &mut Command) {
        command
            .env("HOME", &self.home)
            .env("USERPROFILE", &self.home)
            .env("XDG_CONFIG_HOME", self.home.join(".config"))
            .env("XDG_STATE_HOME", self.home.join(".local/state"))
            .env("XDG_CACHE_HOME", self.home.join(".cache"))
            .env("DOTFILES_ONEPASSWORD_APP", self.home.join("absent-app"));
        for name in ["SSH_CONNECTION", "SSH_CLIENT", "SSH_TTY"] {
            command.env_remove(name);
        }
    }

    fn renderer(&self) -> Command {
        let mut command = dotfiles::profiles::chezmoi_with(
            &self.checkout,
            &dotfiles::profiles::Store::scoped(&self.state),
            &self.config,
            &self.home,
        );
        self.isolate(&mut command);
        command
    }

    fn apply(&self, backup: &str) -> Result<Output> {
        let mut command =
            fixture_process::command(env!("CARGO_BIN_EXE_dotfiles"), self.scope.path());
        self.isolate(&mut command);
        command
            .current_dir(self.scope.path())
            .arg("--root")
            .arg(&self.checkout)
            .args(["profile", "apply", "--config"])
            .arg(&self.config)
            .arg("--destination")
            .arg(&self.home)
            .arg("--state")
            .arg(&self.state)
            .arg("--backup")
            .arg(self.scope.path().join(backup))
            .arg("--live")
            .env("DOTFILES_EXPECTED_FAILURE", "");
        Ok(command.output()?)
    }

    fn registration(&self) -> Result<Vec<u8>> {
        const BUILD: &str = "0123456789abcdef";
        let versions = self.home.join(".cargo").join("domyjob").join("versions");
        let program = versions.join(BUILD).join("bin").join(if cfg!(windows) {
            "domyjob.exe"
        } else {
            "domyjob"
        });
        fs::create_dir_all(program.parent().context("program parent is missing")?)?;
        fs::write(&program, "owned executable fixture")?;
        fs::write(versions.join("pinned"), BUILD)?;
        let registered = successful(
            self.renderer()
                .args(["execute-template", "--file"])
                .arg(Self::production_template()),
        )?
        .stdout;
        let expected = format!(
            r#"{{"type":"local","command":[{},"mcp"],"enabled":true}}"#,
            serde_json::to_string(&program)?,
        );
        assert!(String::from_utf8_lossy(&registered).contains(&format!("\"domyjob\": {expected}")));
        Ok(registered)
    }

    fn preserve(self) {
        if std::env::var_os(skill_ops::INCIDENT_EVIDENCE_ROOT).is_some() {
            let _ = self.scope.keep();
        }
    }
}

#[test]
fn canonical_opencode_registration_converges_without_an_external_edit_refusal() -> Result<()> {
    let fixture = OpenCodeApplyFixture::new()?;
    let consent = fixture.home.join(platform::incidents::EXPERIMENTAL);
    fs::create_dir_all(consent.parent().context("experimental marker parent")?)?;
    fs::write(consent, skill_ops::incident_rules::EXPERIMENTAL_OPT_IN)?;
    let target = fixture.home.join(".config/opencode/opencode.json");
    let historical = successful(fixture.renderer().arg("cat").arg(&target))?.stdout;
    fs::write(&fixture.installed_template, &historical)?;
    let initial = fixture.apply("initial-backup")?;
    ensure!(
        initial.status.success(),
        "{}",
        String::from_utf8_lossy(&initial.stderr)
    );
    let registered = fixture.registration()?;
    let before: Value = serde_json::from_slice(&historical)?;
    let mut after: Value = serde_json::from_slice(&registered)?;
    let mcp = after["mcp"]
        .as_object_mut()
        .context("MCP registrations are an object")?;
    mcp.remove("domyjob");
    if mcp.is_empty() && before.get("mcp").is_none() {
        after
            .as_object_mut()
            .context("configuration is an object")?
            .remove("mcp");
    }
    assert_eq!(
        after, before,
        "registration changes only the domyjob member"
    );
    fs::write(&target, &registered)?;
    let refused = fixture.apply("refused-backup")?;
    assert!(!refused.status.success());
    let stderr = String::from_utf8(refused.stderr)?;
    eprint!("{stderr}");
    assert!(
        stderr.contains("managed files changed outside chezmoi"),
        "{stderr}"
    );
    assert!(
        stderr.contains(".config/opencode/opencode.json"),
        "{stderr}"
    );
    assert!(!fixture.scope.path().join("refused-backup").exists());
    assert_eq!(fs::read(&target)?, registered);
    let incidents = skill_ops::incidents(&fixture.scope.path().join("state"))?;
    assert_eq!(incidents.len(), 1);
    assert_eq!(incidents[0].rule, "tool.failure");
    assert!(
        incidents[0]
            .evidence
            .iter()
            .any(|evidence| evidence.contains(".config/opencode/opencode.json"))
    );
    fs::copy(
        OpenCodeApplyFixture::production_template(),
        &fixture.installed_template,
    )?;
    let applied = fixture.apply("canonical-backup")?;
    ensure!(
        applied.status.success(),
        "{}",
        String::from_utf8_lossy(&applied.stderr)
    );
    assert_eq!(fs::read(&target)?, registered);
    let reapplied = fixture.apply("repeated-backup")?;
    ensure!(
        reapplied.status.success(),
        "{}",
        String::from_utf8_lossy(&reapplied.stderr)
    );
    assert_eq!(fs::read(&target)?, registered);
    assert_eq!(
        skill_ops::incidents(&fixture.scope.path().join("state"))?,
        incidents
    );
    fixture.preserve();
    Ok(())
}

#[test]
fn opencode_context7_requires_a_local_linux_desktop() -> Result<()> {
    let chezmoi = pinned_tool("chezmoi")?;
    let template = fs::read_to_string(source().join("dot_config/opencode/opencode.json.tmpl"))?;
    let temporary = tempfile::tempdir()?;
    let config = temporary.path().join("chezmoi.toml");
    fs::write(&config, "")?;
    copy_opencode_templates(temporary.path())?;
    let home = temporary.path().join("home");
    fs::create_dir(&home)?;
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
                &serde_json::json!({"profile": profile, "chezmoi": {"os": os, "homeDir": home}}),
            )?;
            for mask in 0..=8 {
                let mut command = platform::process::command(&chezmoi);
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
            platform::process::command("gpg")
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
                        *argument = json!(keyring);
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
            Ok(platform::process::command(&self.ansible)
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
        let original = root().join("crates/dotfiles/tests/fixtures/debsig-original.asc");
        let updated = root().join("crates/dotfiles/tests/fixtures/debsig-updated.asc");
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

#[test]
fn bundle_launchers_written_by_earlier_repositories_are_taken_over() {
    let scope = tempfile::tempdir().unwrap();
    let binary = std::path::Path::new("/Applications/Ghostty.app/Contents/MacOS/ghostty");
    for earlier in dotfiles::desktop::earlier_bundle_launchers(binary) {
        let target = scope.path().join("ghostty");
        std::fs::write(&target, &earlier).unwrap();
        dotfiles::desktop::bundle_shim(&target, binary).unwrap();
        assert!(
            std::fs::read_to_string(&target)
                .unwrap()
                .contains("# dotfiles bundle launcher")
        );
    }
    let foreign = scope.path().join("foreign");
    std::fs::write(&foreign, "#!/bin/sh\nexec /opt/other \"$@\"\n").unwrap();
    assert!(dotfiles::desktop::bundle_shim(&foreign, binary).is_err());
}
