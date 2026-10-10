use anyhow::{Context, Result, ensure};
use std::fs;
use std::path::Path;
use xtask::profile_rules::Profile;
use xtask::{profile_snapshot::Combination, profiles};

fn rendered(root: &Path, combination: Combination, scope: &Path) -> Result<toml::Table> {
    let home = scope.join("home");
    fs::create_dir_all(&home)?;
    let mut answers = profiles::Answers::fixture(combination.profile, "/fixture/projects".into());
    answers.role = combination.role.name();
    if combination.signing {
        answers.public_key = "ssh-ed25519 AAAA fixture".into();
    }
    let config = scope.join("chezmoi.toml");
    fs::write(&config, profiles::configuration(root, &answers)?)?;
    let output = profiles::chezmoi(root, scope, &config, &home)
        .arg("cat")
        .arg(home.join(".config/mise/config.toml"))
        .output()?;
    ensure!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    Ok(toml::from_str(std::str::from_utf8(&output.stdout)?)?)
}

#[test]
fn every_profile_requests_the_github_host_from_the_cli_login() -> Result<()> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .context("checkout")?;
    for combination in Combination::all() {
        let scope = tempfile::tempdir()?;
        let config = rendered(root, combination, scope.path())?;
        let expected = if combination.profile == Profile::Windows {
            "gh auth token --hostname %MISE_CREDENTIAL_HOST%"
        } else {
            "gh auth token --hostname \"$MISE_CREDENTIAL_HOST\""
        };
        assert_eq!(
            config
                .get("settings")
                .and_then(|settings| settings.get("github"))
                .and_then(|github| github.get("credential_command"))
                .and_then(toml::Value::as_str),
            Some(expected),
            "{}",
            combination.name()
        );
    }
    Ok(())
}

#[test]
fn mise_uses_a_keyring_login_and_falls_back_when_the_cli_has_no_login() -> Result<()> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .context("checkout")?;
    let scope = tempfile::tempdir()?;
    let bin = scope.path().join("bin");
    fs::create_dir(&bin)?;
    let compiled = platform::process::command("rustc")
        .arg(Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/gh_token.rs"))
        .arg("-o")
        .arg(bin.join(format!("gh{}", std::env::consts::EXE_SUFFIX)))
        .status()?;
    ensure!(compiled.success(), "compile gh fixture");
    let combination = Combination::all()
        .into_iter()
        .find(|combination| combination.profile == dotfiles::profiles::native_profile().unwrap())
        .context("native profile")?;
    let config = rendered(root, combination, scope.path())?;
    let global = scope.path().join("global.toml");
    fs::write(
        &global,
        toml::to_string(&config["settings"]["github"])
            .map(|github| format!("[settings.github]\n{github}"))?,
    )?;
    let empty = scope.path().join("empty.toml");
    fs::write(&empty, "")?;
    let mut paths = vec![bin];
    paths.extend(std::env::split_paths(
        &std::env::var_os("PATH").context("PATH")?,
    ));
    let path = std::env::join_paths(paths)?;
    for (login, expected) in [("yes", "fixture-keyring-token\n"), ("no", "")] {
        for host in ["github.com", "github.fixture.invalid"] {
            let mut command = platform::process::command("mise");
            command
                .env_clear()
                .env("PATH", &path)
                .current_dir(scope.path());
            for name in ["SystemRoot", "COMSPEC", "PATHEXT"] {
                if let Some(value) = std::env::var_os(name) {
                    command.env(name, value);
                }
            }
            for name in [
                "HOME",
                "USERPROFILE",
                "APPDATA",
                "LOCALAPPDATA",
                "XDG_CONFIG_HOME",
                "MISE_CONFIG_DIR",
                "MISE_DATA_DIR",
                "MISE_CACHE_DIR",
                "MISE_STATE_DIR",
                "GH_CONFIG_DIR",
            ] {
                command.env(name, scope.path().join(name));
            }
            let output = command
                .env("MISE_GLOBAL_CONFIG_FILE", &global)
                .env("MISE_SYSTEM_CONFIG_FILE", &empty)
                .env("GH_FIXTURE_LOG", scope.path().join("gh.log"))
                .env("GH_FIXTURE_LOGIN", login)
                .args(["token", "github", "--raw", host])
                .output()?;
            assert_eq!(
                output.status.success(),
                login == "yes",
                "{login} {host}: {}",
                String::from_utf8_lossy(&output.stderr)
            );
            assert_eq!(
                std::str::from_utf8(&output.stdout)?.replace("\r\n", "\n"),
                expected
            );
            assert_eq!(
                fs::read_to_string(scope.path().join("gh.log"))?,
                format!("auth token --hostname {host}")
            );
        }
    }
    Ok(())
}
