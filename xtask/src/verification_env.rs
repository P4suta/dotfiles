//! Managed environments for the backends the current obligation inventory requires.

use crate::obligations::{Manifest, Obligation};
use crate::tool::Tool;
use anyhow::{Context, Result, ensure};
use clap::{Args, Subcommand, ValueEnum};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
use std::ffi::OsString;
use std::fs;
use std::path::Path;
use std::process::Output;

const SMOKE_DIRECTORY: &str = "xtask/proofs/smoke";
const ALLOY_JAR: &str = "/opt/alloy/alloy.jar";
const JAVA_HEAP: &str = "-Xmx512m";
const QUINT_STATES: u64 = 2;
const LEAN_REJECTION_EXIT: i32 = 1;
const LEAN_FALSE_DIAGNOSTIC: &str =
    "error: Tactic `decide` proved that the proposition\n  1 = 0\nis false";
const TLC_HELP_EXIT: i32 = 1;
const TLC_HELP_IDENTITY: &str =
    "TLC - provides model checking and simulation of TLA+ specifications - Version ";
const TLC_HELP_SECTIONS: [&str; 5] = ["NAME", "SYNOPSIS", "DESCRIPTION", "OPTIONS", "TIPS"];
const TLC_HELP_FLAG_DECLARATION: [&str; 2] = ["-h", "display these help instructions"];
const LEAN_SHELL_COMMAND_ARGUMENTS: usize = 3;
const TLC_METADATA_LENGTH: usize = 17;
const TLC_METADATA_SEPARATORS: [usize; 3] = [4, 7, 10];
const TLC_HELP_END: &str = "When using more than one worker, the reported depth might differ across runs. For small models, use a single worker. For large models, the diameter will almost always appear deterministic.";
const IMAGE_IDENTITY_TEMPLATE: &str =
    r#"{"Id":{{json .Id}},"Os":{{json .Os}},"Architecture":{{json .Architecture}}}"#;
const DRIVER_INPUTS: [&str; 12] = [
    "crates/dotfiles/src/bounded.rs",
    "crates/dotfiles/src/bounded/guard.rs",
    "home/dot_config/dotfiles/bounded-tools.toml",
    "xtask/src/verification_env.rs",
    "xtask/src/skill_proofs.rs",
    "xtask/src/lean.rs",
    "xtask/src/alloy.rs",
    "xtask/src/quint.rs",
    "xtask/src/container.rs",
    "xtask/src/phase_evidence.rs",
    "mise.toml",
    "mise.lock",
];

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, ValueEnum)]
#[serde(rename_all = "kebab-case")]
pub enum Backend {
    Kani,
    Lean,
    Alloy,
    Quint,
}

#[derive(Debug, Subcommand)]
pub enum Action {
    Prepare,
    PrepareImage(crate::proof_image_prepare::Request),
    Doctor,
    Smoke,
}

#[derive(Args, Debug)]
pub struct Request {
    #[arg(long, global = true)]
    pub backend: Option<Backend>,
    #[command(subcommand)]
    pub action: Action,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ImageIdentity {
    #[serde(rename(deserialize = "Id"))]
    pub id: String,
    #[serde(rename(deserialize = "Os"))]
    pub os: String,
    #[serde(rename(deserialize = "Architecture"))]
    pub architecture: String,
}

#[derive(Debug, Serialize)]
pub struct Component {
    pub tool: String,
    pub expected: String,
    pub actual: Option<String>,
    pub failure: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct Environment {
    pub backend: Backend,
    pub route: String,
    pub required_platform: String,
    pub image: Option<ImageIdentity>,
    pub components: Vec<Component>,
    pub failure: Option<String>,
}

impl Environment {
    fn ready(&self) -> bool {
        self.failure.is_none()
            && !self.components.is_empty()
            && self
                .components
                .iter()
                .all(|component| component.actual.is_some() && component.failure.is_none())
    }
}

#[derive(Debug, Serialize)]
pub struct Report {
    pub inputs_sha256: String,
    pub environments: Vec<Environment>,
}

/// Selects required backends.
/// A bundled dependency doesn't count as another installation.
#[must_use]
pub fn required(manifest: &Manifest) -> BTreeSet<Backend> {
    manifest
        .obligations
        .iter()
        .filter_map(|obligation| match obligation {
            Obligation::Kani { .. } => Some(Backend::Kani),
            Obligation::Lean(_) => Some(Backend::Lean),
            Obligation::Alloy(_) => Some(Backend::Alloy),
            Obligation::Quint(_) => Some(Backend::Quint),
            _ => None,
        })
        .collect()
}

fn selected(root: &Path, requested: Option<Backend>) -> Result<BTreeSet<Backend>> {
    let mut needed = required(&crate::obligations::manifest(root)?);
    ensure!(
        !needed.is_empty(),
        "the obligation inventory requires no supported verification environment"
    );
    if let Some(requested) = requested {
        ensure!(
            needed.contains(&requested),
            "{requested:?} has no execution obligation in the current inventory"
        );
        needed = BTreeSet::from([requested]);
    }
    Ok(needed)
}

fn image(root: &Path, backend: Backend) -> Result<String> {
    match backend {
        Backend::Kani => crate::skill_proofs::proof_image(root),
        Backend::Lean => crate::lean::image(root),
        Backend::Alloy => crate::alloy::image(root),
        Backend::Quint => crate::quint::image(root),
    }
}

/// Validates the inspected immutable image identity and target semantics for the selected environment.
///
/// # Errors
///
/// Refuses malformed identities or an image for another operating system or architecture.
pub fn parse_image(text: &str, expected_platform: &str) -> Result<ImageIdentity> {
    const HEX_DIGEST: usize = 64;
    let identity: ImageIdentity = serde_json::from_str(text)?;
    let digest = identity
        .id
        .strip_prefix("sha256:")
        .context("image has no content identity")?;
    ensure!(
        digest.len() == HEX_DIGEST && digest.bytes().all(|byte| byte.is_ascii_hexdigit()),
        "invalid Docker image identity"
    );
    ensure!(
        format!("{}/{}", identity.os, identity.architecture) == expected_platform,
        "image target differs from {expected_platform}"
    );
    Ok(identity)
}

pub(crate) fn inspect(tag: &str, expected_platform: &str) -> Result<ImageIdentity> {
    let mut command = Tool::Docker.command();
    command.args(["image", "inspect", "--format", IMAGE_IDENTITY_TEMPLATE, tag]);
    let output = crate::phase_evidence::output(&mut command, "verification image identity")?;
    ensure!(
        output.status.success(),
        "the current pinned image {tag} is absent; run verification-env prepare"
    );
    parse_image(&String::from_utf8(output.stdout)?, expected_platform)
}

fn bounded(arguments: &mut Vec<OsString>) {
    arguments.splice(
        1..1,
        ["--pull", "never", "--env", "JVM_ARGS=-Xmx512m"].map(OsString::from),
    );
}

fn image_command(
    identity: &ImageIdentity,
    platform: &str,
    backend: Backend,
    command: &[&str],
) -> Result<Output> {
    let mut arguments = crate::container::run_arguments();
    bounded(&mut arguments);
    if backend == Backend::Kani {
        // The production campaign uses the prepared image's root home without a host-home mount.
        arguments.extend(["--user", "0:0"].map(OsString::from));
    }
    arguments.extend(["--platform", platform, &identity.id].map(OsString::from));
    arguments.extend(command.iter().map(OsString::from));
    let mut command = Tool::Docker.command();
    command.args(arguments);
    crate::phase_evidence::output(&mut command, "verification environment version")
}

fn component(tool: &str, expected: &str, output: Result<Output>) -> Component {
    component_with(tool, expected, output, version_matches)
}

fn kani_component(output: Result<Output>) -> Component {
    component_with(
        "Kani",
        crate::skill_proofs::KANI_VERSION,
        output,
        |text, _| crate::skill_proofs::kani_environment_version_matches(text),
    )
}

fn apalache_component(output: Result<Output>) -> Component {
    component_with(
        "Apalache",
        crate::quint::APALACHE_VERSION,
        output,
        |text, expected| text == expected,
    )
}

fn component_with(
    tool: &str,
    expected: &str,
    output: Result<Output>,
    matches: impl FnOnce(&str, &str) -> bool,
) -> Component {
    let result = output.and_then(|output| {
        let text = format!(
            "{}{}",
            String::from_utf8(output.stdout)?,
            String::from_utf8(output.stderr)?
        )
        .trim()
        .to_owned();
        ensure!(
            output.status.success() && matches(&text, expected),
            "{tool} did not report its required version {expected}: {}; observed {text:?}",
            output.status
        );
        Ok(text)
    });
    component_result(tool, expected, result)
}

fn component_result(tool: &str, expected: &str, result: Result<String>) -> Component {
    match result {
        Ok(actual) => Component {
            tool: tool.to_owned(),
            expected: expected.to_owned(),
            actual: Some(actual),
            failure: None,
        },
        Err(error) => Component {
            tool: tool.to_owned(),
            expected: expected.to_owned(),
            actual: None,
            failure: Some(format!("{error:#}")),
        },
    }
}

fn tlc_component(output: Result<Output>) -> Component {
    let expected = format!(
        "TLC in the Apalache {} bundle",
        crate::quint::APALACHE_VERSION
    );
    let result = output.and_then(|output| {
        let text = String::from_utf8(output.stdout)?;
        let stderr = String::from_utf8(output.stderr)?;
        ensure!(
            tlc_help_metadata(output.status.code(), &text, &stderr),
            "TLC did not report the pinned bundle's supported metadata protocol"
        );
        Ok(text)
    });
    component_result("TLC", &expected, result)
}

fn tlc_help_metadata(status: Option<i32>, stdout: &str, stderr: &str) -> bool {
    if status != Some(TLC_HELP_EXIT) || !stderr.is_empty() {
        return false;
    }
    let plain = ["\u{1b}[0m", "\u{1b}[1m", "\u{1b}[3m"]
        .into_iter()
        .fold(stdout.to_owned(), |text, style| text.replace(style, ""));
    let lines = plain
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>();
    if lines.first() != Some(&TLC_HELP_SECTIONS[0])
        || lines.last() != Some(&TLC_HELP_END)
        || lines.iter().any(|line| {
            let lower = line.to_ascii_lowercase();
            lower.starts_with("error:")
                || lower.starts_with("fatal:")
                || lower.starts_with("exception ")
                || lower.contains("unrecognized option:")
        })
    {
        return false;
    }
    let Some(metadata) = lines
        .get(1)
        .and_then(|line| line.strip_prefix(TLC_HELP_IDENTITY))
    else {
        return false;
    };
    if metadata.len() != TLC_METADATA_LENGTH
        || !metadata.bytes().enumerate().all(|(index, byte)| {
            if TLC_METADATA_SEPARATORS.contains(&index) {
                byte == b'.'
            } else {
                byte.is_ascii_digit()
            }
        })
    {
        return false;
    }
    let sections = lines
        .iter()
        .copied()
        .filter(|line| TLC_HELP_SECTIONS.contains(line))
        .collect::<Vec<_>>();
    sections == TLC_HELP_SECTIONS
        && lines.iter().any(|line| line.starts_with("TLC ["))
        && lines.iter().any(|line| {
            line.starts_with(
                "The model checker (TLC) provides the functionalities of model checking",
            )
        })
        && lines
            .windows(TLC_HELP_FLAG_DECLARATION.len())
            .any(|pair| pair == TLC_HELP_FLAG_DECLARATION)
}

fn lean_rejected(status: Option<i32>, text: &str) -> bool {
    status == Some(LEAN_REJECTION_EXIT) && text.contains(LEAN_FALSE_DIAGNOSTIC)
}

fn version_matches(text: &str, expected: &str) -> bool {
    text.match_indices(expected).any(|(index, matched)| {
        let before = text[..index].chars().next_back();
        let after = text[index + matched.len()..].chars().next();
        before.is_none_or(|character| !character.is_ascii_alphanumeric())
            && if expected.ends_with('.') {
                after.is_some_and(|character| character.is_ascii_digit())
            } else {
                after.is_none_or(|character| !character.is_ascii_alphanumeric())
            }
    })
}

fn apalache_jar() -> String {
    format!(
        "{}/apalache-dist-{}/apalache/lib/apalache.jar",
        crate::quint::QUINT_HOME,
        crate::quint::APALACHE_VERSION
    )
}

fn doctor_backend(root: &Path, backend: Backend) -> Environment {
    if backend == Backend::Kani
        && cfg!(any(target_os = "macos", target_os = "linux"))
        && !dotfiles::bounded::requires_container()
    {
        let driver = crate::skill_proofs::kani_environment_driver();
        let failure = driver.as_ref().err().map(|error| format!("{error:#}"));
        let version = crate::skill_proofs::kani_environment_version(root);
        return Environment {
            backend,
            route: driver.map_or_else(|_| String::new(), |path| path.display().to_string()),
            required_platform: format!("{}/{}", std::env::consts::OS, std::env::consts::ARCH),
            image: None,
            components: vec![match version {
                Ok(actual) => Component {
                    tool: "Kani".to_owned(),
                    expected: crate::skill_proofs::KANI_VERSION.to_owned(),
                    actual: Some(actual),
                    failure: None,
                },
                Err(error) => Component {
                    tool: "Kani".to_owned(),
                    expected: crate::skill_proofs::KANI_VERSION.to_owned(),
                    actual: None,
                    failure: Some(format!("{error:#}")),
                },
            }],
            failure,
        };
    }
    let mut environment = Environment {
        backend,
        route: String::new(),
        required_platform: String::new(),
        image: None,
        components: Vec::new(),
        failure: None,
    };
    let read = (|| -> Result<()> {
        environment.route = image(root, backend)?;
        environment.required_platform = match backend {
            Backend::Kani | Backend::Lean => crate::container::LINUX_AMD64.to_owned(),
            Backend::Alloy | Backend::Quint => crate::container::native_platform()?,
        };
        let identity = inspect(&environment.route, &environment.required_platform)?;
        let command = |arguments: &[&str]| {
            image_command(
                &identity,
                &environment.required_platform,
                backend,
                arguments,
            )
        };
        environment.components = match backend {
            Backend::Kani => {
                let arguments = crate::skill_proofs::container_kani_version_arguments();
                vec![kani_component(command(
                    &arguments.each_ref().map(String::as_str),
                ))]
            }
            Backend::Lean => vec![component(
                "Lean",
                crate::lean::LEAN_VERSION,
                command(&["lean", "--version"]),
            )],
            Backend::Alloy => vec![
                component("Java", "21.", command(&["java", "--version"])),
                component(
                    "Alloy",
                    crate::alloy::ALLOY_VERSION,
                    command(&["java", "-jar", ALLOY_JAR, "version"]),
                ),
            ],
            Backend::Quint => {
                let jar = apalache_jar();
                vec![
                    component("Node", "v22.", command(&["node", "--version"])),
                    component("Java", "21.", command(&["java", "--version"])),
                    component(
                        "Quint",
                        crate::quint::QUINT_VERSION,
                        command(&["quint", "--version"]),
                    ),
                    apalache_component(command(&["java", JAVA_HEAP, "-jar", &jar, "version"])),
                    tlc_component(command(&[
                        "java", JAVA_HEAP, "-cp", &jar, "tlc2.TLC", "-help",
                    ])),
                ]
            }
        };
        environment.image = Some(identity);
        Ok(())
    })();
    if let Err(error) = read {
        environment.failure = Some(format!("{error:#}"));
    }
    environment
}

fn inputs(root: &Path, backends: &BTreeSet<Backend>) -> Result<String> {
    let mut digest = Sha256::new();
    for path in DRIVER_INPUTS {
        digest.update(path.as_bytes());
        digest.update(fs::read(root.join(path))?);
    }
    digest.update(fs::read(root.join(crate::obligations::MANIFEST))?);
    let mut fixtures: Vec<_> =
        fs::read_dir(root.join(SMOKE_DIRECTORY))?.collect::<std::io::Result<_>>()?;
    fixtures.sort_by_key(std::fs::DirEntry::file_name);
    for fixture in fixtures {
        ensure!(
            fixture.file_type()?.is_file(),
            "verification smoke inputs must be regular files"
        );
        digest.update(fixture.file_name().as_encoded_bytes());
        digest.update(fs::read(fixture.path())?);
    }
    for backend in backends {
        digest.update(image(root, *backend)?.as_bytes());
    }
    Ok(crate::build_inputs::hex(&digest.finalize()))
}

/// Reads existing images and native bundles without preparation, setup, pulls, or compilation.
///
/// # Errors
///
/// Returns an error if reading current declarations fails.
pub fn doctor(root: &Path, backends: &BTreeSet<Backend>) -> Result<Report> {
    Ok(Report {
        inputs_sha256: inputs(root, backends)?,
        environments: backends
            .iter()
            .map(|backend| doctor_backend(root, *backend))
            .collect(),
    })
}

fn prepare(root: &Path, backend: Backend) -> Result<()> {
    match backend {
        Backend::Kani => crate::skill_proofs::prepare_kani_environment(root),
        Backend::Lean => crate::lean::build(root, &image(root, backend)?),
        Backend::Alloy => crate::alloy::build(root, &image(root, backend)?),
        Backend::Quint => crate::quint::build(root, &image(root, backend)?),
    }
}

fn successful(output: Output, operation: &str) -> Result<String> {
    let text = format!(
        "{}{}",
        String::from_utf8(output.stdout)?,
        String::from_utf8(output.stderr)?
    );
    ensure!(output.status.success(), "{operation} failed: {text}");
    Ok(text)
}

fn smoke_lean(root: &Path, environment: &Environment) -> Result<()> {
    let identity = environment
        .image
        .as_ref()
        .context("Lean image is unavailable")?;
    let execute = |name: &str| {
        let file = format!("/source/{SMOKE_DIRECTORY}/{name}");
        let mut arguments = crate::lean::run_arguments(root, &identity.id, "");
        arguments.truncate(arguments.len() - LEAN_SHELL_COMMAND_ARGUMENTS);
        bounded(&mut arguments);
        arguments.extend(["lean", &file].map(OsString::from));
        let mut command = Tool::Docker.command();
        command.args(arguments);
        crate::phase_evidence::output(&mut command, "Lean environment smoke")
    };
    let positive = successful(execute("lean-valid.lean")?, "Lean smoke")?;
    ensure!(
        positive.trim().starts_with("'verification_smoke' ")
            && crate::lean::axiom_refusals(&positive).is_empty(),
        "Lean smoke produced no valid theorem report"
    );
    let negative = execute("lean-reject.lean")?;
    let text = format!(
        "{}{}",
        String::from_utf8(negative.stdout)?,
        String::from_utf8(negative.stderr)?
    );
    ensure!(
        lean_rejected(negative.status.code(), &text),
        "Lean did not reject the false smoke proposition"
    );
    Ok(())
}

fn smoke_alloy(root: &Path, environment: &Environment) -> Result<()> {
    let identity = environment
        .image
        .as_ref()
        .context("Alloy image is unavailable")?;
    let directory = tempfile::tempdir()?;
    let spec = format!("{SMOKE_DIRECTORY}/alloy.als");
    for (name, expected_type, solution) in [
        ("Holds", "check", false),
        ("Exists", "run", true),
        ("Breaks", "check", true),
    ] {
        let mut arguments =
            crate::alloy::run_arguments(root, directory.path(), &identity.id, &spec, "sat4j", name);
        bounded(&mut arguments);
        let mut command = Tool::Docker.command();
        command.args(arguments);
        successful(
            crate::phase_evidence::output(&mut command, "Alloy environment smoke")?,
            "Alloy smoke",
        )?;
        let receipt: serde_json::Value =
            serde_json::from_slice(&fs::read(directory.path().join(name).join("receipt.json"))?)?;
        ensure!(
            receipt["commands"][name]["type"] == expected_type
                && crate::alloy::solved(&receipt, name)? == solution,
            "Alloy smoke did not meet {name}'s required outcome"
        );
    }
    Ok(())
}

fn smoke_quint(root: &Path, environment: &Environment) -> Result<()> {
    let identity = environment
        .image
        .as_ref()
        .context("Quint image is unavailable")?;
    let directory = tempfile::tempdir()?;
    fs::write(
        directory.path().join("tlc.json"),
        "{\"workers\":1,\"maxHeap\":\"-Xmx512m\",\"stackSize\":\"-Xss1m\"}",
    )?;
    let model = format!("/source/{SMOKE_DIRECTORY}/quint.qnt");
    let execute = |command: &[&str]| -> Result<Output> {
        let mut arguments =
            crate::quint::run_arguments(root, directory.path(), &identity.id, command);
        bounded(&mut arguments);
        let mut command = Tool::Docker.command();
        command.args(arguments);
        crate::phase_evidence::output(&mut command, "Quint environment smoke")
    };
    successful(execute(&["typecheck", &model])?, "Quint smoke typecheck")?;
    for backend in ["tlc", "apalache"] {
        let extra: &[&str] = if backend == "tlc" {
            &["--tlc-config", "/out/tlc.json", "--verbosity", "3"]
        } else {
            &["--inductive-invariant", "Inv"]
        };
        let command = |main| {
            [
                vec![
                    "verify",
                    &model,
                    "--main",
                    main,
                    "--backend",
                    backend,
                    "--invariant",
                    "Inv",
                ],
                extra.to_vec(),
            ]
            .concat()
        };
        let positive = successful(execute(&command("valid_smoke"))?, "Quint positive smoke")?;
        if backend == "tlc" {
            ensure!(
                crate::quint::distinct_states(&positive) == Some(QUINT_STATES),
                "TLC smoke has an unexpected reachable state set"
            );
        }
        let negative = execute(&command("rejected_smoke"))?;
        let text = format!(
            "{}{}",
            String::from_utf8(negative.stdout)?,
            String::from_utf8(negative.stderr)?
        );
        ensure!(
            !negative.status.success() && text.contains("found a counterexample"),
            "{backend} smoke failed without the required invariant counterexample"
        );
    }
    Ok(())
}

/// Runs a bounded valid example and a semantic rejecting control in each already prepared environment.
///
/// # Errors
///
/// Refuses unavailable environments, tool errors, or missing valid/negative outcomes.
pub fn smoke(root: &Path, report: &Report) -> Result<()> {
    let backends: BTreeSet<_> = report
        .environments
        .iter()
        .map(|environment| environment.backend)
        .collect();
    ensure!(
        !backends.is_empty() && backends.len() == report.environments.len(),
        "environment smoke inventory is empty or duplicated"
    );
    let needed = required(&crate::obligations::manifest(root)?);
    ensure!(
        backends.is_subset(&needed) && report.inputs_sha256 == inputs(root, &backends)?,
        "environment readiness belongs to different current inputs or required backends"
    );
    let mut checks = crate::checks::Checks::default();
    for environment in &report.environments {
        checks.run(
            &format!("{:?} environment smoke", environment.backend),
            || {
                ensure!(
                    environment.ready(),
                    "the required environment is unavailable; run verification-env doctor"
                );
            match environment.backend {
                    Backend::Kani => crate::skill_proofs::smoke_kani_environment(root),
                    Backend::Lean => smoke_lean(root, environment),
                    Backend::Alloy => smoke_alloy(root, environment),
                Backend::Quint => smoke_quint(root, environment),
            }?;
            println!("{:?} environment smoke passed its fixed valid example and semantic rejecting control; this is environment readiness, not project proof coverage", environment.backend);
            Ok(())
            },
        );
    }
    checks.finish()
}

/// Prepares only required backends, diagnoses them without installing, or runs the fixed smoke controls.
///
/// # Errors
///
/// Returns every preparation, readiness, and smoke failure after checking independent environments.
pub fn run(root: &Path, request: &Request) -> Result<()> {
    if let Action::PrepareImage(image) = &request.action {
        ensure!(
            request.backend.is_none() || request.backend == Some(Backend::Kani),
            "proof image preparation belongs to the Kani environment"
        );
        return crate::proof_image_prepare::run(root, image);
    }
    let selected = selected(root, request.backend)?;
    if matches!(request.action, Action::Prepare) {
        crate::phase_evidence::run(
            root,
            "verification-environment-prepare",
            selected.len(),
            || {
                let mut checks = crate::checks::Checks::default();
                for backend in &selected {
                    checks.run(&format!("prepare {backend:?}"), || prepare(root, *backend));
                }
                checks.finish()
            },
        )?;
    }
    let report = doctor(root, &selected)?;
    println!("{}", serde_json::to_string_pretty(&report)?);
    ensure!(
        report.environments.iter().all(Environment::ready),
        "required verification environments are not ready; no verification coverage is claimed"
    );
    if matches!(request.action, Action::Smoke) {
        crate::phase_evidence::run(
            root,
            "verification-environment-smoke",
            report.environments.len(),
            || smoke(root, &report),
        )?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const LEAN_ACTUAL_REJECTION: &str = "/source/xtask/proofs/smoke/lean-reject.lean:1:52: error: Tactic `decide` proved that the proposition\n  1 = 0\nis false\n";

    fn tlc_help_fixture(metadata: &str) -> String {
        format!(
            "NAME\n\n{TLC_HELP_IDENTITY}{metadata}\n\nSYNOPSIS\nTLC [-h] SPEC\n\nDESCRIPTION\nThe model checker (TLC) provides the functionalities of model checking\n\nOPTIONS\n-h\ndisplay these help instructions\n\nTIPS\n{TLC_HELP_END}\n"
        )
    }

    #[test]
    fn lean_semantic_rejection_requires_the_actual_false_proposition_and_natural_exit() {
        assert!(lean_rejected(
            Some(LEAN_REJECTION_EXIT),
            LEAN_ACTUAL_REJECTION
        ));
        for status in [None, Some(0), Some(2), Some(-1)] {
            assert!(!lean_rejected(status, LEAN_ACTUAL_REJECTION));
        }
        for invalid in [
            "error: unknown identifier",
            "'verification_smoke' does not depend on any axioms",
            "error: Tactic `decide` proved that the proposition\n  0 = 0\nis false",
            "error: Tactic `decide` proved that the proposition\n  1 = 0\nis true",
            "error: Tactic `decide` proved that the proposition\n  1 = 0",
        ] {
            assert!(!lean_rejected(Some(LEAN_REJECTION_EXIT), invalid));
        }
    }

    #[test]
    fn apalache_metadata_requires_its_exact_pinned_version_response() {
        for text in [
            "0.62.1\n",
            "0.62.10",
            "0.62.1-SNAPSHOT",
            "Usage\n0.62.1",
            "",
        ] {
            let result = apalache_component(Ok(Output {
                status: std::process::ExitStatus::default(),
                stdout: text.as_bytes().to_vec(),
                stderr: Vec::new(),
            }));
            assert_eq!(result.actual.is_some(), text == "0.62.1\n");
            assert_eq!(result.failure.is_none(), text == "0.62.1\n");
        }
    }

    #[test]
    fn tlc_bundle_metadata_accepts_only_its_complete_supported_help_protocol() {
        let complete = tlc_help_fixture("2026.10.13.133355");
        assert!(tlc_help_metadata(Some(TLC_HELP_EXIT), &complete, ""));
        let another_emission = tlc_help_fixture("2026.10.13.135017");
        assert!(tlc_help_metadata(
            Some(TLC_HELP_EXIT),
            &another_emission,
            ""
        ));
        let styled = complete
            .replace("NAME", "\u{1b}[1mNAME\u{1b}[0m")
            .replace("[-h]", "[\u{1b}[1m-h\u{1b}[0m]");
        assert!(tlc_help_metadata(Some(TLC_HELP_EXIT), &styled, ""));
        for status in [None, Some(0), Some(2), Some(-1)] {
            assert!(!tlc_help_metadata(status, &complete, ""));
        }
        for invalid in [
            complete.replace("TLC - provides", "Other - provides"),
            complete.replace("2026.10.13.133355", "0.62.1"),
            complete.replace("2026.10.13.133355", "2026.10.13"),
            complete.replace("2026.10.13.133355", "2026.10.13.abcdef"),
            complete.replace("SYNOPSIS", "OTHER"),
            complete.replace("TLC [-h] SPEC", "other --help"),
            complete.replace("The model checker (TLC)", "The unrelated checker"),
            complete.replace("display these help instructions", "unrelated help"),
            complete.replace(TLC_HELP_END, "partial help"),
            format!("Error: unknown command\n{complete}"),
            format!("{complete}\nError: missing input file"),
            complete.replace("TIPS", "Error: unrecognized option: -version\nTIPS"),
            "TLC2 Version 2026.10.13.133355\nError: unrecognized option: -version".to_owned(),
        ] {
            assert!(!tlc_help_metadata(Some(TLC_HELP_EXIT), &invalid, ""));
        }
        assert!(!tlc_help_metadata(
            Some(TLC_HELP_EXIT),
            &complete,
            "JVM failed"
        ));
    }

    #[test]
    fn kani_container_readiness_requires_the_complete_standalone_driver_identity() {
        let valid = format!(
            "Kani Rust Verifier {} (standalone)\nCBMC {}\n",
            crate::skill_proofs::KANI_VERSION,
            crate::skill_proofs::KANI_CBMC_VERSION
        );
        for text in [
            &valid,
            "Kani Rust Verifier 0.68.0",
            "Kani Rust Verifier 0.68.0 (standalone)\nCBMC wrong",
            "Kani Rust Verifier 0.68.0 (cargo)\nCBMC 6.11.0",
            "Kani Rust Verifier 0.68.0 (standalone)\nCBMC 6.11.0\nunexpected",
        ] {
            let actual = kani_component(Ok(Output {
                status: std::process::ExitStatus::default(),
                stdout: text.as_bytes().to_vec(),
                stderr: Vec::new(),
            }));
            assert_eq!(actual.actual.is_some(), text == valid);
            assert_eq!(actual.failure.is_none(), text == valid);
        }
    }

    #[test]
    fn version_identity_is_not_an_embedded_digit_match() {
        for (output, expected) in [
            (
                "Kani Rust Verifier 0.68.0",
                crate::skill_proofs::KANI_VERSION,
            ),
            ("Lean (version 4.34.1, x86_64)", crate::lean::LEAN_VERSION),
            ("v22.1.0", "v22."),
            ("openjdk 21.0.5", "21."),
        ] {
            assert!(version_matches(output, expected));
        }
        for (output, expected) in [
            (
                "Kani Rust Verifier 0.68.00",
                crate::skill_proofs::KANI_VERSION,
            ),
            ("140.33.0", crate::quint::QUINT_VERSION),
            ("v22.wrong", "v22."),
            ("TLC20", "TLC2"),
            ("", crate::lean::LEAN_VERSION),
        ] {
            assert!(!version_matches(output, expected));
        }
    }

    #[test]
    fn an_unavailable_component_cannot_be_ready_or_become_a_semantic_counterexample() {
        let missing = component(
            "Quint",
            crate::quint::QUINT_VERSION,
            Err(anyhow::anyhow!("cannot start")),
        );
        assert!(missing.actual.is_none());
        assert!(missing.failure.is_some());
        let report = Environment {
            backend: Backend::Quint,
            route: "absent".to_owned(),
            required_platform: "linux/arm64".to_owned(),
            image: None,
            components: vec![missing],
            failure: None,
        };
        assert!(!report.ready());
        assert!(
            smoke(
                Path::new("unused"),
                &Report {
                    inputs_sha256: String::new(),
                    environments: vec![report]
                }
            )
            .is_err()
        );
    }
}
