//! Source-bound profiles for the supported GitHub Rust pin readers.

use super::Found;
use super::report::Record;
use std::collections::BTreeSet;
use yaml_rust2::parser::{Event, Parser};
use yaml_rust2::{Yaml, YamlLoader};

const MAX_YAML_BYTES: usize = 128 * 1024;
const MAX_YAML_DEPTH: usize = 64;
const COMMIT_DIGITS: usize = 40;
const RUST_SHORT_VERSION_PARTS: usize = 2;
const RUST_FULL_VERSION_PARTS: usize = 3;
const BASH_READER: &str = include_str!("ci_pin/bash-reader.txt");
const PWSH_READER: &str = include_str!("ci_pin/pwsh-reader.txt");
const CONTROLLER_IDENTITY: &str = include_str!("ci_pin/controller-identity.txt");
const CONTROLLER_CHECKOUTS: &str = include_str!("ci_pin/controller-checkouts.txt");

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Reader {
    CompositeBash,
    ControllerPowerShell,
}

impl Reader {
    /// Checks that the line reader and TOML scalar select the same version.
    pub fn reads(self, manifest: &str, version: &str) -> bool {
        if manifest
            .lines()
            .any(|line| line.starts_with("rust") && !line.is_ascii())
        {
            return false;
        }
        let Ok(table) = toml::from_str::<toml::Table>(manifest) else {
            return false;
        };
        if table
            .get("tools")
            .and_then(toml::Value::as_table)
            .and_then(|tools| tools.get("rust"))
            .and_then(toml::Value::as_str)
            != Some(version)
        {
            return false;
        }
        let mut assignments = manifest.lines().filter_map(|line| {
            let suffix = line.strip_prefix("rust")?;
            suffix
                .trim_start_matches(|character: char| character.is_ascii_whitespace())
                .strip_prefix('=')
        });
        let Some(assignment) = assignments.next() else {
            return false;
        };
        if assignments.next().is_some() {
            return false;
        }
        match self {
            Self::CompositeBash => assignment.split('"').nth(1) == Some(version),
            Self::ControllerPowerShell => {
                let parts: Vec<_> = version.split('.').collect();
                matches!(
                    parts.len(),
                    RUST_SHORT_VERSION_PARTS | RUST_FULL_VERSION_PARTS
                ) && parts
                    .iter()
                    .all(|part| !part.is_empty() && part.bytes().all(|byte| byte.is_ascii_digit()))
                    && assignment.trim_matches(|character: char| character.is_ascii_whitespace())
                        == format!("\"{version}\"")
            }
        }
    }
}

pub(super) fn covers(records: &[Record], record: &Record, found: Option<&Found>) -> bool {
    let alias = &record.dependency;
    if record.manager != "github-actions"
        || alias.dep_name != "rust"
        || alias.package_name.as_deref() != Some("rust")
        || alias.datasource.as_deref() != Some("rust-version")
        || alias.dep_type.as_deref() != Some("uses-with")
        || alias.skip_reason.as_deref() != Some("invalid-value")
        || alias.current_digest.is_some()
        || alias.locked_version.is_some()
        || alias.current_version.is_some()
    {
        return false;
    }
    let mut counterparts = records.iter().filter(|candidate| {
        candidate.manager == "mise"
            && candidate.file == "mise.toml"
            && candidate.dependency.dep_name == "rust"
    });
    let Some(counterpart) = counterparts.next() else {
        return false;
    };
    let literal = &counterpart.dependency;
    if counterparts.next().is_some()
        || !literal.enabled
        || literal.skip_reason.is_some()
        || !literal.warnings.is_empty()
        || literal.datasource.as_deref() != Some("rust-version")
        || literal.package_name.as_deref() != Some("rust")
        || literal.dep_type.as_deref() != Some("tools")
        || !super::mise::source_matches(found, counterpart, literal)
    {
        return false;
    }
    let Some(expression) = alias.current_value.as_deref() else {
        return false;
    };
    let Some(version) = literal.current_value.as_deref() else {
        return false;
    };
    let Some(source) = super::mise::source(found, &record.file) else {
        return false;
    };
    let Some(manifest) = super::mise::source(found, "mise.toml") else {
        return false;
    };
    parse(&source, expression).is_some_and(|readers| {
        readers
            .iter()
            .all(|reader| reader.reads(&manifest, version))
    })
}

/// Recognizes every consumer of one output, refusing unsupported execution profiles.
pub fn parse(source: &str, expression: &str) -> Option<Vec<Reader>> {
    let id = expression
        .strip_prefix("${{")?
        .strip_suffix("}}")?
        .trim_matches(' ')
        .strip_prefix("steps.")?
        .strip_suffix(".outputs.version")?;
    if id.is_empty()
        || !id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
    {
        return None;
    }
    let root = document(source)?;
    if root["defaults"] != Yaml::BadValue || root["env"] != Yaml::BadValue {
        return None;
    }
    let mut readers = Vec::new();
    if root["runs"] == Yaml::BadValue {
        for (_, job) in root["jobs"].as_hash()? {
            if job["defaults"] != Yaml::BadValue || job["env"] != Yaml::BadValue {
                return None;
            }
            if let Some(steps) = job["steps"].as_vec() {
                inspect_steps(steps, id, expression, false, &mut readers)?;
            } else if job["uses"].as_str().is_none() {
                return None;
            }
        }
    } else {
        if root["jobs"] != Yaml::BadValue || root["runs"]["using"].as_str()? != "composite" {
            return None;
        }
        inspect_steps(
            root["runs"]["steps"].as_vec()?,
            id,
            expression,
            true,
            &mut readers,
        )?;
    }
    (!readers.is_empty()).then_some(readers)
}

fn document(source: &str) -> Option<Yaml> {
    if source.len() > MAX_YAML_BYTES || !source.chars().all(rules::yaml_rules::raw_character) {
        return None;
    }
    let mut parser = Parser::new_from_str(source);
    let mut depth = 0usize;
    loop {
        match parser.next_token().ok()?.0 {
            Event::StreamEnd => break,
            Event::Alias(_) => return None,
            Event::Scalar(_, _, anchor, tag) if anchor != 0 || tag.is_some() => return None,
            Event::MappingStart(anchor, tag) | Event::SequenceStart(anchor, tag) => {
                if anchor != 0 || tag.is_some() || depth >= MAX_YAML_DEPTH {
                    return None;
                }
                depth += 1;
            }
            Event::MappingEnd | Event::SequenceEnd => depth = depth.checked_sub(1)?,
            _ => {}
        }
    }
    let mut documents = YamlLoader::load_from_str(source).ok()?;
    if documents.len() != 1 {
        return None;
    }
    let root = documents.pop()?;
    root.as_hash()?;
    Some(root)
}

fn inspect_steps(
    steps: &[Yaml],
    id: &str,
    expression: &str,
    composite: bool,
    readers: &mut Vec<Reader>,
) -> Option<()> {
    let mut ids = BTreeSet::new();
    for step in steps {
        step.as_hash()?;
        if step["id"] != Yaml::BadValue && !ids.insert(step["id"].as_str()?) {
            return None;
        }
    }
    for (index, consumer) in steps.iter().enumerate() {
        if consumer["with"]["toolchain"].as_str() != Some(expression) {
            continue;
        }
        if !pinned_action(consumer["uses"].as_str()?, "dtolnay/rust-toolchain")
            || !only_keys(consumer, &["id", "name", "uses", "with"])
        {
            return None;
        }
        let producer_index = index.checked_sub(1)?;
        let producer = steps.get(producer_index)?;
        if producer["id"].as_str() != Some(id)
            || !only_keys(producer, &["id", "name", "shell", "run"])
        {
            return None;
        }
        let reader = if composite && producer_index == 0 && run_step(producer, "bash", BASH_READER)
        {
            Reader::CompositeBash
        } else if !composite
            && run_step(producer, "pwsh", PWSH_READER)
            && controller_prefix(steps.get(..producer_index)?)
        {
            Reader::ControllerPowerShell
        } else {
            return None;
        };
        readers.push(reader);
    }
    Some(())
}

fn only_keys(value: &Yaml, allowed: &[&str]) -> bool {
    value.as_hash().is_some_and(|mapping| {
        mapping
            .keys()
            .all(|key| key.as_str().is_some_and(|key| allowed.contains(&key)))
    })
}

fn run_step(step: &Yaml, shell: &str, script: &str) -> bool {
    step["shell"].as_str() == Some(shell)
        && step["run"].as_str().is_some_and(|actual| {
            actual.strip_suffix('\n').unwrap_or(actual)
                == script.strip_suffix('\n').unwrap_or(script)
        })
}

fn pinned_action(value: &str, action: &str) -> bool {
    value
        .strip_prefix(action)
        .and_then(|value| value.strip_prefix('@'))
        .is_some_and(|sha| {
            sha.len() == COMMIT_DIGITS
                && sha
                    .bytes()
                    .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        })
}

fn controller_prefix(steps: &[Yaml]) -> bool {
    let Some(checkouts) = steps.last() else {
        return false;
    };
    let verifier_identity = CONTROLLER_IDENTITY
        .replace("Mutation controller must", "Mutation verifier must")
        .replace("reusable workflow are", "verifier workflow are");
    let verifier_checkouts = CONTROLLER_CHECKOUTS.replace(
        "Checkout '$Relative' is dirty before mutation: $($dirty -join '; ')\"",
        "Checkout '$Relative' is dirty during independent verification.\"",
    );
    let identities: Vec<_> = steps
        .iter()
        .enumerate()
        .filter(|(_, step)| {
            run_step(step, "pwsh", CONTROLLER_IDENTITY)
                || run_step(step, "pwsh", &verifier_identity)
        })
        .collect();
    let controllers: Vec<_> = steps
        .iter()
        .enumerate()
        .filter(|(_, step)| checkout(step, "controller", "${{ inputs.controller_sha }}"))
        .collect();
    let targets: Vec<_> = steps
        .iter()
        .enumerate()
        .filter(|(_, step)| checkout(step, "target", "${{ inputs.target_sha }}"))
        .collect();
    let ([(identity_index, identity)], [(controller_index, _)], [(target_index, _)]) = (
        identities.as_slice(),
        controllers.as_slice(),
        targets.as_slice(),
    ) else {
        return false;
    };
    if !(identity_index < controller_index && controller_index < target_index)
        || *target_index >= steps.len() - 1
    {
        return false;
    }
    let identity_env = &identity["env"];
    let checkout_env = &checkouts["env"];
    only_keys(identity, &["name", "shell", "run", "env"])
        && only_keys(checkouts, &["name", "shell", "run", "env"])
        && only_keys(
            identity_env,
            &["CONTROLLER_SHA", "TARGET_SHA", "WORKFLOW_SHA"],
        )
        && only_keys(checkout_env, &["CONTROLLER_SHA", "TARGET_SHA"])
        && identity_env["CONTROLLER_SHA"].as_str() == Some("${{ inputs.controller_sha }}")
        && identity_env["TARGET_SHA"].as_str() == Some("${{ inputs.target_sha }}")
        && identity_env["WORKFLOW_SHA"].as_str() == Some("${{ github.workflow_sha }}")
        && checkout_env["CONTROLLER_SHA"].as_str() == Some("${{ inputs.controller_sha }}")
        && checkout_env["TARGET_SHA"].as_str() == Some("${{ inputs.target_sha }}")
        && (run_step(checkouts, "pwsh", CONTROLLER_CHECKOUTS)
            || run_step(checkouts, "pwsh", &verifier_checkouts))
}

fn checkout(step: &Yaml, path: &str, reference: &str) -> bool {
    only_keys(step, &["name", "uses", "with"])
        && step["uses"]
            .as_str()
            .is_some_and(|uses| pinned_action(uses, "actions/checkout"))
        && only_keys(&step["with"], &["persist-credentials", "ref", "path"])
        && step["with"]["persist-credentials"].as_bool() == Some(false)
        && step["with"]["path"].as_str() == Some(path)
        && step["with"]["ref"].as_str() == Some(reference)
}

#[cfg(test)]
mod tests {
    #[cfg(unix)]
    use super::BASH_READER;
    use super::{MAX_YAML_BYTES, MAX_YAML_DEPTH, PWSH_READER, Reader, document, parse};

    fn execute_reader(program: &str, arguments: &[&str], script: &str) {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path();
        std::fs::create_dir(root.join("controller")).unwrap();
        let script_file = root.join("reader.ps1");
        let output_file = root.join("output");
        std::fs::write(&script_file, script).unwrap();
        for (manifest, successful) in [
            ("[tools]\nrust = \"1.97\"\n", true),
            ("[tools]\nother = \"1.97\"\n", false),
        ] {
            std::fs::write(root.join("mise.toml"), manifest).unwrap();
            std::fs::write(root.join("controller/mise.toml"), manifest).unwrap();
            std::fs::write(&output_file, "").unwrap();
            let output = platform::process::command(program)
                .args(arguments)
                .arg(&script_file)
                .current_dir(root)
                .env("GITHUB_OUTPUT", &output_file)
                .output()
                .unwrap();
            assert_eq!(
                output.status.success(),
                successful,
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
            assert_eq!(
                std::fs::read_to_string(&output_file).unwrap().trim_end(),
                if successful { "version=1.97" } else { "" }
            );
        }
    }

    #[test]
    fn the_pinned_powershell_executes_the_registered_reader_and_its_failure_guard() {
        let selected = crate::locate::child("mise")
            .args(["which", "pwsh"])
            .current_dir(env!("CARGO_MANIFEST_DIR"))
            .output()
            .unwrap();
        assert!(
            selected.status.success(),
            "{}",
            String::from_utf8_lossy(&selected.stderr)
        );
        let program = String::from_utf8(selected.stdout).unwrap();
        execute_reader(
            program.trim(),
            &["-NoLogo", "-NoProfile", "-NonInteractive", "-File"],
            PWSH_READER,
        );
    }

    #[cfg(unix)]
    #[test]
    fn native_bash_executes_the_registered_reader_and_its_failure_guard() {
        execute_reader("bash", &[], BASH_READER);
    }

    #[test]
    fn real_composite_and_controller_fixtures_bind_every_consumer() {
        assert_eq!(
            parse(
                include_str!("../../../../fuzz/corpus/ci_pin/composite.yml"),
                "${{ steps.pin.outputs.version }}"
            ),
            Some(vec![Reader::CompositeBash])
        );
        assert_eq!(
            parse(
                include_str!("../../../../fuzz/corpus/ci_pin/controller.yml"),
                "${{ steps.rust-pin.outputs.version }}"
            ),
            Some(vec![Reader::ControllerPowerShell; 3])
        );
    }

    #[test]
    fn document_limits_reject_alias_expansion_and_excessive_nesting_before_loading() {
        for source in [
            "root: &root [1, 2]\ncopy: *root\n".to_owned(),
            "root: !tag value\n".to_owned(),
            "root: a\nroot: b\n".to_owned(),
            format!(
                "root: {}0{}",
                "[".repeat(MAX_YAML_DEPTH),
                "]".repeat(MAX_YAML_DEPTH)
            ),
            format!("root: {}", "x".repeat(MAX_YAML_BYTES)),
        ] {
            assert!(document(&source).is_none());
        }
        assert!(document("root: [1, 2]\n").is_some());
    }

    #[test]
    fn zero_bytes_never_hide_trailing_documents_or_invalid_consumers() {
        let composite = include_str!("../../../../fuzz/corpus/ci_pin/composite.yml");
        for hidden in [
            format!("{composite}\0\n---\njobs: {{}}\n"),
            format!(
                "{composite}\0    - uses: dtolnay/rust-toolchain@stable\n      with:\n        toolchain: ${{{{ steps.pin.outputs.version }}}}\n"
            ),
        ] {
            assert!(parse(&hidden, "${{ steps.pin.outputs.version }}").is_none());
        }
        assert!(document("value: \"\\0\"\n").is_some());
    }

    #[test]
    fn line_readers_and_toml_must_select_one_identical_literal() {
        for reader in [Reader::CompositeBash, Reader::ControllerPowerShell] {
            assert!(reader.reads("[tools]\nrust = \"1.97\"\n", "1.97"));
            assert!(reader.reads("# A\u{2192}B\n[tools]\nrust = \"1.97\"\n", "1.97"));
            assert!(reader.reads("[tools]\nrust = \"1.97.1\"\n", "1.97.1"));
            for manifest in [
                "[tools]\n\"rust\" = \"1.97\"\n",
                "[tools]\nrust = '1.97'\n",
                "[tools]\nrust = \"1.97\"\n[other]\nrust = \"1.96\"\n",
                "raw = '''\nrust = \"1.96\"\n'''\n[tools]\nrust = \"1.97\"\n",
                "[tools]\nrust = { version = \"1.97\" }\n",
                "[tools]\nrust\u{a0}= \"1.97\"\n",
            ] {
                assert!(!reader.reads(manifest, "1.97"), "{manifest}");
            }
        }
        assert!(Reader::CompositeBash.reads("[tools]\nrust = \"1.97\" # pin\n", "1.97"));
        assert!(!Reader::ControllerPowerShell.reads("[tools]\nrust = \"1.97\" # pin\n", "1.97"));
    }
}
