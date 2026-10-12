//! Native `profile apply` rehearsal on a disposable host.
//!
//! The profile gate's owned fixtures never exercise setup scripts, native verification, rollback, or the takeover of a machine that earlier repositories provisioned.
//! This module seeds earlier provisioning and runs the real `profile apply` on the host's own home three times, the second rerunning every script.

use crate::profile_rules::Profile;
use crate::profiles::{
    NativeAction, Store, native_profile, operate, pending_onchange_scripts,
    reset_entry_and_script_state,
};
use crate::tool::Tool;
use anyhow::{Context, Result, ensure};
use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

/// A setup script the rehearsal host skips, with the reason recorded in the generated configuration.
#[derive(Debug)]
pub struct Omitted {
    pub script: &'static str,
    pub reason: &'static str,
}

const TOOLCHAIN: &str = "builds a full language toolchain that the rehearsal host does not need to prove orchestration; native application exercises it";
const PRIVATE_SOURCE: &str = "clones a private repository with the owner's GitHub credentials, which a disposable host must not hold";

/// The setup scripts the rehearsal host of `profile` skips.
#[must_use]
pub fn omitted(profile: Profile) -> Vec<Omitted> {
    match profile {
        Profile::Mac => vec![
            Omitted {
                script: "ocaml.sh",
                reason: TOOLCHAIN,
            },
            Omitted {
                script: "haskell.sh",
                reason: TOOLCHAIN,
            },
            Omitted {
                script: "racket.sh",
                reason: TOOLCHAIN,
            },
            Omitted {
                script: "fleet.sh",
                reason: PRIVATE_SOURCE,
            },
        ],
        Profile::Linux => vec![Omitted {
            script: "ocaml.sh",
            reason: TOOLCHAIN,
        }],
        Profile::Wsl | Profile::Windows => Vec::new(),
    }
}

fn toml_string(value: &str) -> String {
    serde_json::Value::String(value.to_owned()).to_string()
}

/// The machine-local configuration for the rehearsal: an anonymous identity, no secrets, and one representative entry in each provisioning list.
///
/// # Errors
///
/// Returns an error when loading the source data fails, or when rendering the profile to find its required packages fails.
/// Returns an error for the Linux profile when the data has no Nix package table.
pub fn configuration(root: &Path, profile: Profile, home: &Path) -> Result<String> {
    let data = crate::profiles::source_data(root)?;
    let mut answers = crate::profiles::Answers::fixture(
        profile,
        home.join("projects").to_string_lossy().into_owned(),
    );
    answers.name = "Rehearsal";
    answers.email = "rehearsal@example.invalid";
    answers.doppler = "";
    let mut identity = crate::profiles::configuration(root, &answers)?;
    identity.push_str("\n\n");
    for skip in omitted(profile) {
        write!(
            identity,
            "[[data.setup.ignore]]\nscript = {}\nreason = {}\n\n",
            toml_string(skip.script),
            toml_string(skip.reason)
        )?;
    }
    let kept = required_packages(root, profile, home, &identity)?;
    let keep = |pointer: &str| -> Vec<String> {
        kept.iter()
            .filter(|(list, _)| *list == pointer)
            .map(|(_, entry)| toml_string(entry))
            .collect()
    };
    let list = |pointer: &str| {
        std::iter::once(toml_string("jq"))
            .chain(keep(pointer))
            .collect::<Vec<_>>()
            .join(", ")
    };
    let mut text = identity;
    let name = profile.name();
    write!(
        text,
        "[data.platforms.{name}.tools]\ncommon = [{}]\npersonal = []\nwork = []\n\n",
        list("/tools/common")
    )?;
    match profile {
        Profile::Mac => {
            write!(
                text,
                "[data.platforms.mac.brew]\nformulae = [{}]\ncasks = []\n\n[data.platforms.mac.racket]\ncollections = []\n\n[data.ocaml]\nplatform_tools = []\n\n",
                list("/brew/formulae")
            )?;
        }
        Profile::Linux => {
            text.push_str("[data.ocaml]\nplatform_tools = []\n\n");
            text.push_str("[data.platforms.linux.nix.packages]\n");
            let packages = data
                .pointer("/platforms/linux/nix/packages")
                .and_then(serde_json::Value::as_object)
                .context("Linux Nix packages are missing")?;
            let required = keep(crate::tool::NIX_PACKAGES);
            for (package, spec) in packages {
                let spec = if required.contains(&toml_string(package)) {
                    spec.as_str().unwrap_or_default()
                } else {
                    ""
                };
                writeln!(text, "{} = {}", toml_string(package), toml_string(spec))?;
            }
            text.push('\n');
        }
        Profile::Windows => {
            write!(
                text,
                "[data.platforms.windows.scoop]\napps = [{}]\nwinget_duplicates = []\n\n[data.platforms.windows.winget]\napps = []\n\n",
                list("/scoop/apps")
            )?;
        }
        Profile::Wsl => {}
    }
    Ok(text)
}

/// Package-list entries for the tools that this profile's scripts and installed entry points require, read from the profile rendered with full data.
fn required_packages(
    root: &Path,
    profile: Profile,
    home: &Path,
    identity: &str,
) -> Result<Vec<(&'static str, &'static str)>> {
    let scope = tempfile::tempdir()?;
    let config = scope.path().join("chezmoi.toml");
    fs::write(&config, identity)?;
    let host = crate::profiles::fixture_host(
        profile,
        &crate::profiles::chezmoi_path(root),
        &crate::profiles::chezmoi_path(home),
    );
    let rendered = crate::profiles::chezmoi(root, scope.path(), &config, home)
        .arg("--override-data")
        .arg(serde_json::json!({"chezmoi": host}).to_string())
        .args(["dump", "--include", "files,scripts", "--format", "json"])
        .output()?;
    ensure!(
        rendered.status.success(),
        "render rehearsal scripts: {}",
        String::from_utf8_lossy(&rendered.stderr)
    );
    let dump: serde_json::Value = serde_json::from_slice(&rendered.stdout)?;
    let targets = dump.as_object().context("profile dump must be a mapping")?;
    let contents = |scripts: bool| -> String {
        targets
            .values()
            .filter(|entry| (entry["type"] == "script") == scripts)
            .filter_map(|entry| entry["contents"].as_str())
            .collect::<Vec<_>>()
            .join("\n")
    };
    let mut required = dotfiles::setup::scripted_requirements(profile, &contents(true));
    required.extend(dotfiles::setup::entry_requirements(&contents(false)));
    required.sort();
    required.dedup();
    Ok(required
        .into_iter()
        .filter_map(|tool| tool.package(profile))
        .collect())
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
        Tool::Cargo
            .command()
            .args(["install", "--path"])
            .arg(&package)
            .arg("--root")
            .arg(home.join(".local")),
        "seed earlier helper binaries",
    )
}

/// Machine-local history, credentials, approvals, and unrelated settings that installation must keep byte for byte.
pub(crate) fn seed_history(home: &Path) -> Result<Vec<(PathBuf, Vec<u8>)>> {
    let mut preserved = Vec::new();
    for (relative, contents) in [
        (".local/state/coderabbit-guard/initialized", ""),
        (".local/state/coderabbit-guard/reviews.log", "1000\n"),
        (".local/state/coderabbit-guard/usage-initialized", ""),
        (".local/state/coderabbit-guard/usage.log", "1000\n"),
        (
            ".local/state/skill-ops/private-history.txt",
            "owner history\n",
        ),
        (".claude/projects/fixture/session.jsonl", "{}\n"),
        (".codex/sessions/fixture.jsonl", "{}\n"),
        (".codex/auth.json", "{}\n"),
        (".codex/owner-approvals.json", "{\"owner\":true}\n"),
        (".config/opencode/unrelated.json", "{\"owner\":true}\n"),
        (".config/unrelated/config.toml", "owner = true\n"),
    ] {
        let path = home.join(relative);
        fs::create_dir_all(path.parent().context("history parent is missing")?)?;
        fs::write(&path, contents)?;
        preserved.push((path, contents.as_bytes().to_vec()));
    }
    Ok(preserved)
}

/// Applies and verifies this host's profile three times with the rehearsal configuration, rerunning every script in the middle round.
///
/// # Errors
///
/// Returns an error unless the caller sets `disposable_host` and either `CI` holds `true` or `DOTFILES_DISPOSABLE_HOST` holds `1` on a local host the owner gave up.
/// Returns an error when an apply or verification round fails, or when installation changed the seeded machine-local history.
/// Returns an error when the repeated round would rerun a script whose inputs stayed the same.
pub fn run(root: &Path, disposable_host: bool) -> Result<()> {
    let disposable = std::env::var("CI").as_deref() == Ok("true")
        || std::env::var("DOTFILES_DISPOSABLE_HOST").as_deref() == Ok("1");
    ensure!(
        disposable_host && disposable,
        "rehearsal replaces this host's home configuration; run it only on a disposable host with --disposable-host, where CI is true or DOTFILES_DISPOSABLE_HOST is 1"
    );
    let profile = native_profile()?;
    let home = native_home()?;
    let scope = tempfile::Builder::new()
        .prefix("dotfiles-rehearsal")
        .tempdir()?;
    let config = scope.path().join("chezmoi.toml");
    fs::write(&config, configuration(root, profile, &home)?)?;
    seed_foreign_helpers(&home, scope.path())?;
    let preserved = seed_history(&home)?;
    // The rehearsal keeps chezmoi's default database beside its configuration, as `just apply` does.
    let store = Store::beside(&config)?;
    // The middle round reruns every script on the host the first round provisioned, as `profile apply` does when it takes over an earlier repository's machine or a script changes.
    // The repeated round must find nothing to rerun, because no script's inputs changed.
    for (round, rerun_scripts) in [
        ("first", false),
        ("reprovisioning", true),
        ("repeated", false),
    ] {
        if rerun_scripts {
            reset_entry_and_script_state(root, &store, &config, &home)?;
        } else if round == "repeated" {
            let pending = pending_onchange_scripts(root, &store, &config, &home)?;
            ensure!(
                pending.is_empty(),
                "the repeated application would rerun scripts whose inputs stayed the same: {pending:?}"
            );
        }
        operate(
            root,
            NativeAction::Apply,
            &config,
            &home,
            true,
            None,
            Some(&scope.path().join(format!("backup-{round}"))),
        )
        .with_context(|| format!("{round} rehearsal application"))?;
        operate(
            root,
            NativeAction::Verify,
            &config,
            &home,
            false,
            None,
            None,
        )
        .with_context(|| format!("{round} rehearsal verification"))?;
        println!(
            "Rehearsal: {round} application of the {} profile verified",
            profile.name()
        );
    }
    let pending = pending_onchange_scripts(root, &store, &config, &home)?;
    ensure!(
        pending.is_empty(),
        "an application left scripts that would rerun with unchanged inputs: {pending:?}"
    );
    for (path, contents) in preserved {
        ensure!(
            fs::read(&path)? == contents,
            "installation changed machine-local history: {}",
            path.display()
        );
    }
    startup_counts(root, &home)
}

/// Holds each applied shell's startup to the programs `policy/baselines.toml` admits.
fn startup_counts(root: &Path, home: &Path) -> Result<()> {
    let counted = crate::profiles::home_startup_programs(home)?;
    let mut measured = Vec::new();
    for (shell, programs) in &counted {
        println!(
            "Rehearsal: {shell} started {} programs at startup: {}",
            programs.len(),
            programs.join(", ")
        );
        measured.push((*shell, u64::try_from(programs.len())?));
    }
    let baselines = crate::obligations::parse_baselines(&fs::read_to_string(
        root.join(crate::obligations::BASELINES),
    )?)?;
    let refusals = crate::obligations::exec_counts(&baselines, &measured);
    ensure!(refusals.is_empty(), "{}", refusals.join("\n"));
    Ok(())
}
