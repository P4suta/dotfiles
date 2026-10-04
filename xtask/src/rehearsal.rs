//! Native application rehearsal on a disposable host.
//!
//! The profile gate renders and applies files in owned fixtures, which cannot exercise setup scripts, native verification, rollback, or the takeover of a machine that earlier repositories provisioned.
//! This runs the real `profile apply` against the host's own home, twice, after seeding earlier provisioning, so those paths fail in CI instead of on an owner's machine.

use crate::profile_rules::Profile;
use crate::profiles::{NativeAction, native_profile, operate};
use anyhow::{Context, Result, ensure};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

/// A setup script the rehearsal host cannot run, with the reason recorded in the generated configuration.
struct Omitted {
    script: &'static str,
    reason: &'static str,
}

const TOOLCHAIN: &str = "builds a full language toolchain that the rehearsal host does not need to prove orchestration; native application exercises it";
const PRIVATE_SOURCE: &str = "clones a private repository with the owner's GitHub credentials, which a disposable host must not hold";

fn omitted(profile: Profile) -> Vec<Omitted> {
    match profile {
        Profile::Mac => vec![
            Omitted {
                script: "setup-ocaml.sh",
                reason: TOOLCHAIN,
            },
            Omitted {
                script: "setup-haskell.sh",
                reason: TOOLCHAIN,
            },
            Omitted {
                script: "setup-racket.sh",
                reason: TOOLCHAIN,
            },
            Omitted {
                script: "install-fleet.sh",
                reason: PRIVATE_SOURCE,
            },
        ],
        Profile::Linux | Profile::Wsl => vec![Omitted {
            script: "setup-ocaml.sh",
            reason: TOOLCHAIN,
        }],
        Profile::Windows => Vec::new(),
    }
}

fn toml_string(value: &str) -> String {
    serde_json::Value::String(value.to_owned()).to_string()
}

/// The machine-local configuration for the rehearsal: an anonymous identity, no secrets, and one representative entry in each provisioning list.
pub fn configuration(profile: Profile, home: &Path) -> String {
    let projects = home.join("projects");
    let mut text = String::new();
    if profile == Profile::Windows {
        text.push_str(
            "[interpreters.ps1]\ncommand = \"pwsh\"\nargs = [\"-NoLogo\", \"-NoProfile\"]\n\n",
        );
    }
    text.push_str(&format!(
        "[data]\nprofile = {}\nrole = \"personal\"\n\n[data.git]\nname = \"Rehearsal\"\nemail = \"rehearsal@example.invalid\"\n\n[data.signing]\nenabled = false\npublic_key = \"\"\npubkey = \"\"\nkey_comment = \"\"\n\n[data.herdr_agent]\npublic_keys = []\n\n[data.doppler]\nproject = \"\"\nconfig = \"\"\n\n[data.doppler.secrets]\ncodex = []\nclaude = []\nopencode = []\n\n[data.paths]\nprojects = {}\n\n",
        toml_string(profile.name()),
        toml_string(&projects.to_string_lossy()),
    ));
    let name = profile.name();
    text.push_str(&format!(
        "[data.platforms.{name}.tools]\ncommon = [\"jq\"]\npersonal = []\nwork = []\n\n"
    ));
    match profile {
        Profile::Mac => text.push_str(
            "[data.platforms.mac.brew]\nformulae = [\"jq\"]\ncasks = []\n\n[data.platforms.mac.racket]\ncollections = []\n\n[data.platforms.mac.ocaml]\nplatform_tools = []\n\n",
        ),
        Profile::Linux => text.push_str(
            "[data.platforms.linux.nix]\npackages = {}\n\n[data.platforms.linux.ocaml]\nplatform_tools = []\n\n",
        ),
        Profile::Windows => text.push_str(
            "[data.platforms.windows.scoop]\napps = [\"jq\"]\nwinget_duplicates = []\n\n[data.platforms.windows.winget]\napps = []\n\n",
        ),
        Profile::Wsl => {}
    }
    for skip in omitted(profile) {
        text.push_str(&format!(
            "[[data.setup.skip]]\nscript = {}\nreason = {}\n\n",
            toml_string(skip.script),
            toml_string(skip.reason)
        ));
    }
    text
}

fn native_home() -> Result<PathBuf> {
    Ok(PathBuf::from(
        std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" })
            .context("native home is unavailable")?,
    ))
}

fn checked(command: &mut Command, what: &str) -> Result<()> {
    let status = command.status().with_context(|| format!("start {what}"))?;
    ensure!(status.success(), "{what} failed with {status}");
    Ok(())
}

/// Helper binaries owned by a different cargo package, as the earlier repositories left them.
fn seed_foreign_helpers(home: &Path, scope: &Path) -> Result<()> {
    let package = scope.join("earlier-dotfiles-tools");
    fs::create_dir_all(package.join("src/bin"))?;
    fs::write(
        package.join("Cargo.toml"),
        "[package]\nname = \"earlier-dotfiles-tools\"\nversion = \"0.1.0\"\nedition = \"2024\"\n\n[workspace]\n",
    )?;
    fs::write(package.join("src/main.rs"), "fn main() {}\n")?;
    for binary in ["skill-ops", "pr-workflow"] {
        fs::write(
            package.join(format!("src/bin/{binary}.rs")),
            "fn main() {}\n",
        )?;
    }
    checked(
        Command::new("cargo")
            .args(["install", "--path"])
            .arg(&package)
            .arg("--root")
            .arg(home.join(".local")),
        "seed earlier helper binaries",
    )
}

/// Review and maintenance history that installation must keep byte for byte.
fn seed_history(home: &Path) -> Result<Vec<(PathBuf, Vec<u8>)>> {
    let mut preserved = Vec::new();
    for (relative, contents) in [
        (".local/state/coderabbit-guard/initialized", ""),
        (".local/state/coderabbit-guard/reviews.log", "1000\n"),
        (".local/state/coderabbit-guard/usage-initialized", ""),
        (".local/state/coderabbit-guard/usage.log", "1000\n"),
    ] {
        let path = home.join(relative);
        fs::create_dir_all(path.parent().context("history parent is missing")?)?;
        fs::write(&path, contents)?;
        preserved.push((path, contents.as_bytes().to_vec()));
    }
    Ok(preserved)
}

pub fn run(root: &Path, disposable_host: bool) -> Result<()> {
    ensure!(
        disposable_host && std::env::var("CI").as_deref() == Ok("true"),
        "rehearsal replaces this host's home configuration; run it only on a disposable CI host with --disposable-host"
    );
    let profile = native_profile()?;
    let home = native_home()?;
    let scope = tempfile::Builder::new()
        .prefix("dotfiles-rehearsal")
        .tempdir()?;
    let config = scope.path().join("chezmoi.toml");
    fs::write(&config, configuration(profile, &home))?;
    seed_foreign_helpers(&home, scope.path())?;
    let preserved = seed_history(&home)?;
    let state = scope.path().join("state");
    for round in ["first", "repeated"] {
        operate(
            root,
            NativeAction::Apply,
            &config,
            &home,
            true,
            &state,
            Some(&scope.path().join(format!("backup-{round}"))),
        )
        .with_context(|| format!("{round} rehearsal application"))?;
        operate(
            root,
            NativeAction::Verify,
            &config,
            &home,
            false,
            &state,
            None,
        )
        .with_context(|| format!("{round} rehearsal verification"))?;
        println!(
            "Rehearsal: {round} application of the {} profile verified",
            profile.name()
        );
    }
    for (path, contents) in preserved {
        ensure!(
            fs::read(&path)? == contents,
            "installation changed machine-local history: {}",
            path.display()
        );
    }
    Ok(())
}
