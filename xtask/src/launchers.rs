//! The setup launchers under `home/.chezmoiscripts/`, which `just gen` writes from the setup steps.
//! Each launcher's name carries its position in the run order, so chezmoi's lexical order runs every step after what it relies on.

use crate::profile_rules::Profile;
use anyhow::{Context, Result, ensure};
use platform::step::{Condition, Family, Launch, Phase, Step, sequence};
use serde_json::Value;
use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::fs;
use std::path::Path;

/// The launcher directory, relative to the checkout.
pub const DIRECTORY: &str = "home/.chezmoiscripts";
/// The data file that restricts each launcher to its profiles, relative to the checkout.
pub const SCRIPTS: &str = "home/.chezmoidata/scripts.toml";

/// One launcher: the file `just gen` writes and the target chezmoi runs.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Launcher {
    pub family: Family,
    pub step: Step,
    /// The source filename under the family's directory.
    pub file: String,
    /// The script target, relative to the destination.
    pub target: String,
    pub text: String,
}

fn file_name(step: Step, family: Family, position: usize) -> String {
    let condition = match step.condition(family) {
        Condition::Always => "",
        Condition::OnChange => "onchange_",
    };
    let phase = match step.phase() {
        Phase::Before => "before",
        Phase::After => "after",
    };
    format!(
        "run_{condition}{phase}_{position:02}-{}.{}.tmpl",
        step.name(),
        family.extension()
    )
}

fn inputs_call(terms: &[&str]) -> String {
    format!(
        "{{{{ includeTemplate \"partials/inputs\" (list{}) }}}}",
        terms.iter().fold(String::new(), |mut text, term| {
            let _ = write!(text, " {term}");
            text
        })
    )
}

/// The line that hashes the step's inputs, chosen per profile when the profiles of the family hash different inputs.
fn inputs(step: Step, family: Family) -> String {
    let profiles: Vec<Profile> = family
        .profiles()
        .iter()
        .copied()
        .filter(|profile| step.runs_on(*profile))
        .collect();
    let first = step.inputs(profiles[0]);
    if profiles
        .iter()
        .all(|profile| step.inputs(*profile) == first)
    {
        return format!("{}\n", inputs_call(first));
    }
    let mut text = String::new();
    for (index, profile) in profiles.iter().enumerate() {
        let keyword = if index == 0 { "if" } else { "else if" };
        let _ = write!(
            text,
            "{{{{ {keyword} eq .profile \"{}\" }}}}{}",
            profile.name(),
            inputs_call(step.inputs(*profile))
        );
    }
    text.push_str("{{ end }}\n");
    text
}

fn unix(step: Step) -> String {
    let mut text = String::from("#!/bin/sh\nset -eu\n");
    if step.condition(Family::Unix) == Condition::OnChange {
        text.push_str(&inputs(step, Family::Unix));
    }
    match step.launch() {
        Launch::PinnedMise => text.push_str("{{ includeTemplate \"partials/install-mise-sh\" . }}\n"),
        Launch::SourceRuntime => text.push_str(concat!(
            "mise=\"$HOME/.local/bin/mise\"\n",
            "\"$mise\" trust --quiet \"$CHEZMOI_WORKING_TREE/mise.toml\"\n",
            "exec \"$mise\" -C \"$CHEZMOI_WORKING_TREE\" x -- cargo run --locked --manifest-path \"$CHEZMOI_WORKING_TREE/crates/dotfiles/Cargo.toml\" -- --root \"$CHEZMOI_WORKING_TREE\" setup runtime --config \"$CHEZMOI_CONFIG_FILE\" --live\n",
        )),
        Launch::InstalledRuntime => {
            let _ = writeln!(
                text,
                "exec \"$HOME/.local/bin/dotfiles-xtask\" --root \"$CHEZMOI_WORKING_TREE\" setup {} --config \"$CHEZMOI_CONFIG_FILE\" --live",
                step.name()
            );
        }
    }
    text
}

fn windows(step: Step) -> String {
    let mut text = String::from("$ErrorActionPreference = 'Stop'\n");
    if step.condition(Family::Windows) == Condition::OnChange {
        text.push_str(&inputs(step, Family::Windows));
    }
    match step.launch() {
        Launch::PinnedMise => text.push_str(
            "{{ includeTemplate \"partials/install-mise-ps1\" . }}\nexit 0\n",
        ),
        Launch::SourceRuntime => text.push_str(concat!(
            "$mise = Join-Path $env:USERPROFILE '.local\\bin\\mise.exe'\n",
            "& $mise trust --quiet (Join-Path $env:CHEZMOI_WORKING_TREE 'mise.toml')\n",
            "if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }\n",
            "& $mise -C $env:CHEZMOI_WORKING_TREE x -- cargo run --locked --manifest-path (Join-Path $env:CHEZMOI_WORKING_TREE 'crates\\dotfiles\\Cargo.toml') -- --root $env:CHEZMOI_WORKING_TREE setup runtime --config $env:CHEZMOI_CONFIG_FILE --live\n",
            "exit $LASTEXITCODE\n",
        )),
        Launch::InstalledRuntime => {
            let _ = writeln!(
                text,
                "& (Join-Path $env:USERPROFILE '.local\\bin\\dotfiles-xtask.exe') --root $env:CHEZMOI_WORKING_TREE setup {} --config $env:CHEZMOI_CONFIG_FILE --live",
                step.name()
            );
            text.push_str("exit $LASTEXITCODE\n");
        }
    }
    text
}

/// Every launcher of every family, in run order.
///
/// # Errors
///
/// Returns an error when a family's steps form a cycle or a step before the files relies on one after them.
pub fn launchers() -> Result<Vec<Launcher>> {
    let mut found = Vec::new();
    for family in Family::ALL {
        let steps = sequence(family).map_err(|disorder| {
            anyhow::anyhow!("{family:?} steps can't run in order: {disorder:?}")
        })?;
        for (position, step) in steps.into_iter().enumerate() {
            found.push(Launcher {
                family,
                step,
                file: file_name(step, family, position),
                target: format!(
                    ".chezmoiscripts/{}/{position:02}-{}.{}",
                    family.directory(),
                    step.name(),
                    family.extension()
                ),
                text: match family {
                    Family::Unix => unix(step),
                    Family::Windows => windows(step),
                },
            });
        }
    }
    Ok(found)
}

/// Whether `target` names a launcher that runs before the installed runtime exists.
/// Only the generator writes these, and `gen --check` refuses any other contents, so the gate lets them hold the shell that installs mise and builds the runtime.
#[must_use]
pub fn bootstraps(target: &str) -> bool {
    launchers().is_ok_and(|all| {
        all.iter().any(|launcher| {
            launcher.target == target && launcher.step.launch() != Launch::InstalledRuntime
        })
    })
}

/// The set of `profiles` that holds exactly `wanted`.
fn set_named(profiles: &Value, wanted: &[Profile]) -> Option<String> {
    profiles.as_object()?.iter().find_map(|(name, members)| {
        let members: Vec<&str> = members
            .as_array()?
            .iter()
            .filter_map(Value::as_str)
            .collect();
        (members.len() == wanted.len()
            && wanted
                .iter()
                .all(|profile| members.contains(&profile.name())))
        .then(|| name.clone())
    })
}

/// The data file that restricts each launcher whose step runs on only some profiles of its family.
///
/// # Errors
///
/// Returns an error when no set of `profiles.toml` holds exactly a launcher's profiles.
pub fn scripts(profiles: &Value, launchers: &[Launcher]) -> Result<String> {
    let mut entries = BTreeMap::new();
    for launcher in launchers {
        let family = launcher.family.profiles();
        let runs: Vec<Profile> = family
            .iter()
            .copied()
            .filter(|profile| launcher.step.runs_on(*profile))
            .collect();
        if runs.len() == family.len() {
            continue;
        }
        let set = set_named(profiles, &runs).with_context(|| {
            format!(
                "no set of profiles.toml holds exactly {runs:?}, which run {}",
                launcher.target
            )
        })?;
        entries.insert(launcher.target.clone(), set);
    }
    let mut text = String::from(
        "# `just gen` writes this file from the setup steps in crates/platform/src/step.rs.\n# Each launcher runs only on the profiles of the named set of profiles.toml.\n\n[scripts]\n",
    );
    for (target, set) in entries {
        let _ = writeln!(text, "{target:?} = {set:?}");
    }
    Ok(text)
}

/// Writes the launchers and the data file that restricts them, or with `check` refuses any that differ.
/// Writing removes every launcher no step declares.
///
/// # Errors
///
/// Returns an error when the steps can't run in order, a launcher's profiles form no named set, or reading or writing a file fails.
/// With `check`, it also fails when a launcher or the data file differs from what the steps declare, or a launcher no step declares remains.
pub fn generate(root: &Path, check: bool) -> Result<()> {
    let launchers = launchers()?;
    let data = model::data::load(&crate::source_state(root).join(model::data::DIRECTORY))
        .map_err(|error| anyhow::anyhow!("{error}"))?;
    let scripts = scripts(&data["profiles"], &launchers)?;
    let mut stale = Vec::new();
    for family in Family::ALL {
        let directory = root.join(DIRECTORY).join(family.directory());
        fs::create_dir_all(&directory)?;
        for entry in fs::read_dir(&directory)? {
            let name = entry?.file_name().to_string_lossy().into_owned();
            if !launchers
                .iter()
                .any(|launcher| launcher.family == family && launcher.file == name)
            {
                stale.push(directory.join(name));
            }
        }
    }
    let mut written = vec![(root.join(SCRIPTS), scripts)];
    for launcher in launchers {
        written.push((
            root.join(DIRECTORY)
                .join(launcher.family.directory())
                .join(&launcher.file),
            launcher.text,
        ));
    }
    if check {
        let mut differing: Vec<String> = stale
            .iter()
            .map(|path| format!("{} declares no step", path.display()))
            .collect();
        for (path, wanted) in &written {
            if !fs::read_to_string(path).is_ok_and(|current| current == *wanted) {
                differing.push(format!("{} differs from its step", path.display()));
            }
        }
        ensure!(
            differing.is_empty(),
            "the setup launchers disagree with the steps in crates/platform/src/step.rs; run `just gen` and commit the result:\n{}",
            differing.join("\n")
        );
    } else {
        for path in stale {
            fs::remove_file(path)?;
        }
        for (path, text) in written {
            fs::write(path, text)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_launcher_name_carries_its_position_and_its_attributes() {
        let launchers = launchers().unwrap();
        let unix: Vec<&Launcher> = launchers
            .iter()
            .filter(|launcher| launcher.family == Family::Unix)
            .collect();
        assert_eq!(unix[0].file, "run_before_00-mise.sh.tmpl");
        assert_eq!(unix[1].file, "run_before_01-runtime.sh.tmpl");
        assert_eq!(unix[2].file, "run_onchange_before_02-github.sh.tmpl");
        assert!(unix.last().unwrap().file.ends_with("-adapters.sh.tmpl"));
        let mut names: Vec<&str> = unix
            .iter()
            .map(|launcher| launcher.target.as_str())
            .collect();
        let order = names.clone();
        names.sort_unstable();
        assert_eq!(names, order, "lexical order is the run order");
    }

    #[test]
    fn only_the_launchers_before_the_runtime_bootstrap() {
        assert!(bootstraps(".chezmoiscripts/unix/00-mise.sh"));
        assert!(bootstraps(".chezmoiscripts/windows/01-runtime.ps1"));
        assert!(!bootstraps(".chezmoiscripts/unix/02-github.sh"));
        assert!(!bootstraps(".chezmoiscripts/unix/99-mise.sh"));
        for launcher in launchers().unwrap() {
            if launcher.family == Family::Unix && !bootstraps(&launcher.target) {
                let body = launcher
                    .text
                    .lines()
                    .filter(|line| !line.starts_with("{{"))
                    .collect::<Vec<_>>()
                    .join("\n");
                assert!(crate::quality::single_exec(&body), "{}", launcher.target);
            }
        }
        assert!(!crate::quality::single_exec("set -eu\ncurl x\nexec y\n"));
        assert!(!crate::quality::single_exec("exec y\nexec z\n"));
        assert!(!crate::quality::single_exec("set -eu\n"));
    }

    #[test]
    fn a_launcher_starts_strict_and_runs_its_step_with_the_chezmoi_configuration() {
        for launcher in launchers().unwrap() {
            match launcher.family {
                Family::Unix => assert!(launcher.text.starts_with("#!/bin/sh\nset -eu\n")),
                Family::Windows => {
                    assert!(
                        launcher
                            .text
                            .starts_with("$ErrorActionPreference = 'Stop'\n")
                    );
                }
            }
            if launcher.step.launch() != Launch::PinnedMise {
                assert!(
                    launcher.text.contains("CHEZMOI_CONFIG_FILE"),
                    "{}",
                    launcher.file
                );
                assert!(
                    launcher
                        .text
                        .contains(&format!("setup {} --config", launcher.step.name())),
                    "{}",
                    launcher.file
                );
            }
        }
    }

    #[test]
    fn profiles_with_different_inputs_hash_their_own() {
        let tools = inputs(Step::Tools, Family::Unix);
        assert!(tools.starts_with("{{ if eq .profile \"mac\" }}"), "{tools}");
        assert!(
            tools.contains("{{ else if eq .profile \"wsl\" }}"),
            "{tools}"
        );
        assert_eq!(
            inputs(Step::Ocaml, Family::Unix),
            "{{ includeTemplate \"partials/inputs\" (list .ocaml) }}\n"
        );
    }

    #[test]
    fn a_launcher_for_some_profiles_names_their_set() {
        let profiles = serde_json::json!({"linux": ["linux"], "desktop": ["mac", "linux"]});
        let launchers = launchers().unwrap();
        let text = scripts(&profiles, &launchers);
        let error = text.unwrap_err().to_string();
        assert!(error.contains("no set of profiles.toml"), "{error}");
        let profiles = serde_json::json!({
            "mac": ["mac"], "linux": ["linux"], "desktop": ["mac", "linux"],
        });
        let text = scripts(&profiles, &launchers).unwrap();
        assert!(text.contains("-nix.sh\" = \"linux\""), "{text}");
        assert!(!text.contains("-tools.sh"), "{text}");
    }

    #[test]
    fn the_check_refuses_a_renamed_or_unknown_launcher() {
        let scope = tempfile::tempdir().unwrap();
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
        let data = scope.path().join("home/.chezmoidata");
        fs::create_dir_all(&data).unwrap();
        fs::copy(
            root.join("home/.chezmoidata/profiles.toml"),
            data.join("profiles.toml"),
        )
        .unwrap();
        generate(scope.path(), false).unwrap();
        generate(scope.path(), true).unwrap();
        let directory = scope.path().join(DIRECTORY).join("unix");
        fs::rename(
            directory.join("run_before_00-mise.sh.tmpl"),
            directory.join("run_before_09-mise.sh.tmpl"),
        )
        .unwrap();
        let error = generate(scope.path(), true).unwrap_err().to_string();
        assert!(
            error.contains("09-mise.sh.tmpl declares no step"),
            "{error}"
        );
        assert!(error.contains("00-mise.sh.tmpl differs"), "{error}");
        generate(scope.path(), false).unwrap();
        generate(scope.path(), true).unwrap();
    }
}
