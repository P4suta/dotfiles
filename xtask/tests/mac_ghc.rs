#![cfg(unix)]

use anyhow::{Context, Result, ensure};
use std::collections::BTreeMap;
use std::fs;
use std::path::Path;
use xtask::{profile_rules::Profile, profiles};

fn installer(scope: &Path, data: &Path) -> Result<()> {
    let plugin = data.join("plugins/ghc/bin");
    fs::create_dir_all(&plugin)?;
    let helper = scope.join("ghc-fixture");
    let status = platform::process::command("rustc")
        .args(["--edition", "2024"])
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/fixtures/mac_ghc.rs"
        ))
        .arg("-o")
        .arg(&helper)
        .status()?;
    ensure!(status.success(), "compile the GHC installer fixture");
    for name in ["list-all", "install"] {
        fs::copy(&helper, plugin.join(name))?;
    }
    Ok(())
}

#[test]
fn the_mac_ghc_pin_installs_without_bootstrapping_during_version_listing() -> Result<()> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    let scope = tempfile::tempdir()?;
    let home = scope.path().join("home");
    fs::create_dir_all(&home)?;
    let answers = profiles::Answers::fixture(
        Profile::Mac,
        home.join("projects").to_string_lossy().into_owned(),
    );
    let config = scope.path().join("chezmoi.toml");
    fs::write(&config, profiles::configuration(&root, &answers)?)?;
    let rendered = profiles::chezmoi(&root, scope.path(), &config, &home)
        .args(["execute-template", "--file"])
        .arg(root.join("home/dot_config/mise/config.toml.tmpl"))
        .output()?;
    ensure!(
        rendered.status.success(),
        "render the Mac tool configuration"
    );
    let rendered: toml::Value = toml::from_str(std::str::from_utf8(&rendered.stdout)?)?;
    let pins: toml::Value = toml::from_str(&fs::read_to_string(
        root.join("home/.chezmoidata/versions.toml"),
    )?)?;
    let version = pins["platforms"]["mac"]["tool_versions"]["ghc"]
        .as_str()
        .context("Mac GHC release pin")?;
    let candidate = dotfiles::host_checks::Scratch::new(&config)?;
    let command = candidate.command("mise");
    let data = command
        .get_envs()
        .find_map(|(name, value)| (name == "MISE_DATA_DIR").then_some(value).flatten())
        .context("candidate mise storage")?;
    installer(scope.path(), Path::new(data))?;
    let global = command
        .get_envs()
        .find_map(|(name, value)| {
            (name == "MISE_GLOBAL_CONFIG_FILE")
                .then_some(value)
                .flatten()
        })
        .context("candidate mise configuration")?;
    fs::create_dir_all(
        Path::new(global)
            .parent()
            .context("configuration directory")?,
    )?;
    let mut tools = BTreeMap::from([
        (
            "tools",
            BTreeMap::from([("ghc", rendered["tools"]["ghc"].clone())]),
        ),
        (
            "tool_alias",
            BTreeMap::from([("ghc", rendered["tool_alias"]["ghc"].clone())]),
        ),
    ]);
    fs::write(global, toml::to_string(&tools)?)?;
    let listing = scope.path().join("listing");
    let mut command = candidate.command("mise");
    command
        .args(["install", "--yes", "ghc"])
        .env("HOST_GHC_VERSION", version)
        .env("HOST_GHC_LISTING", &listing);
    let output = platform::process::mise_install(command)?.output()?;
    ensure!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        !listing.exists(),
        "version listing bootstrapped the installer"
    );
    let output = candidate
        .command("mise")
        .args(["x", "--", "ghc", "--version"])
        .env("HOST_GHC_VERSION", version)
        .output()?;
    ensure!(output.status.success(), "run the installed GHC release");
    assert_eq!(String::from_utf8(output.stdout)?.trim(), version);
    tools
        .get_mut("tools")
        .context("fixture tool configuration")?
        .insert("ghc", toml::Value::String(version.into()));
    fs::write(global, toml::to_string(&tools)?)?;
    let mut command = candidate.command("mise");
    command
        .args(["install", "--yes", "ghc"])
        .env("HOST_GHC_VERSION", version)
        .env("HOST_GHC_LISTING", &listing);
    let broken = platform::process::mise_install(command)?.output()?;
    assert!(
        !broken.status.success(),
        "the broken pin installed successfully"
    );
    assert!(
        listing.exists(),
        "the broken pin did not invoke version listing"
    );
    Ok(())
}
