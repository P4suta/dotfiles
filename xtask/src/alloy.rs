//! The Alloy obligations.
//! Each assertion finds no counterexample within its scope, each instance exists, and each mutated fact yields a counterexample.

use crate::obligations::{self, AlloyModel, Obligation};
use crate::tool::Tool;
use anyhow::{Context, Result, ensure};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::ffi::OsString;
use std::fs;
use std::path::Path;
use std::process::Stdio;

/// The Alloy release that the image's Dockerfile pins by digest.
pub const ALLOY_VERSION: &str = "6.2.0";

/// The directory that defines the image of the Alloy obligations.
pub const IMAGE_DIRECTORY: &str = "xtask/proofs/alloy";

/// Where the container mounts the read-only checkout and the writable output directory.
const SOURCE: &str = "/source";
const OUTPUT: &str = "/out";

/// The jar the image installs.
const JAR: &str = "/opt/alloy/alloy.jar";

/// The digest bytes an image name carries.
const DIGEST_BYTES: usize = 8;

/// The image name for the Dockerfile, which changes with it.
///
/// # Errors
///
/// Returns an error when reading the Dockerfile fails.
pub fn image(root: &Path) -> Result<String> {
    let path = root.join(IMAGE_DIRECTORY).join("Dockerfile");
    let definition = fs::read(&path).with_context(|| path.display().to_string())?;
    Ok(format!(
        "dotfiles-alloy:{}",
        crate::build_inputs::hex(&Sha256::digest(definition)[..DIGEST_BYTES])
    ))
}

/// Builds the pinned image unless it exists.
///
/// # Errors
///
/// Returns an error when Docker can't build it.
pub fn build(root: &Path, image: &str) -> Result<()> {
    let present = Tool::Docker
        .command()
        .args(["image", "inspect", image])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()?;
    if present.success() {
        return Ok(());
    }
    let status = Tool::Docker
        .command()
        .args(["build", "--tag", image])
        .arg(root.join(IMAGE_DIRECTORY))
        .status()
        .context("build the pinned Alloy image")?;
    ensure!(
        status.success(),
        "cannot build the pinned Alloy image: {status}"
    );
    Ok(())
}

/// The arguments that run one Alloy command in the image with no network, the checkout read-only, and `output` writable.
#[must_use]
pub fn run_arguments(
    root: &Path,
    output: &Path,
    image: &str,
    spec: &str,
    solver: &str,
    command: &str,
) -> Vec<OsString> {
    let mount = |source: &Path, target: &str, readonly: bool| {
        // Docker reads `--mount` as one CSV record, so quoting keeps a comma in the path inside the field.
        let mut mount = OsString::from("type=bind,\"source=");
        mount.push(source);
        mount.push(format!("\",target={target}"));
        if readonly {
            mount.push(",readonly");
        }
        mount
    };
    let mut arguments = crate::container::run_arguments();
    arguments.push("--mount".into());
    arguments.push(mount(root, SOURCE, true));
    arguments.push("--mount".into());
    arguments.push(mount(output, OUTPUT, false));
    let receipt = format!("{OUTPUT}/{command}");
    let model = format!("{SOURCE}/{spec}");
    arguments.extend(
        [
            image,
            "java",
            "-jar",
            JAR,
            "exec",
            "--quiet",
            "--force",
            "--solver",
            solver,
            "--command",
            command,
            "--output",
            &receipt,
            "--type",
            "json",
            &model,
        ]
        .map(OsString::from),
    );
    arguments
}

/// Whether a receipt holds a solution for `command`: an instance of a run, or a counterexample of a check.
///
/// # Errors
///
/// Returns an error when the receipt doesn't name the command or holds a malformed outcome.
pub fn solved(receipt: &Value, command: &str) -> Result<bool> {
    let entry = receipt["commands"]
        .get(command)
        .with_context(|| format!("the Alloy receipt names no command {command}"))?;
    ensure!(
        entry.is_object() && matches!(entry["type"].as_str(), Some("check" | "run")),
        "the Alloy receipt has no command outcome for {command}"
    );
    match entry.get("solution") {
        None => Ok(false),
        Some(Value::Array(solutions)) => Ok(!solutions.is_empty()),
        Some(_) => anyhow::bail!("the Alloy receipt has a malformed solution for {command}"),
    }
}

fn run(root: &Path, output: &Path, image: &str, model: &AlloyModel, command: &str) -> Result<bool> {
    let ran = Tool::Docker
        .command()
        .args(run_arguments(
            root,
            output,
            image,
            &model.spec,
            model.solver.argument(),
            command,
        ))
        .output()
        .context("start the Alloy container")?;
    ensure!(
        ran.status.success(),
        "Alloy failed on {command} of {}: {}\n{}",
        model.spec,
        ran.status,
        String::from_utf8_lossy(&ran.stderr)
    );
    let receipt: Value =
        serde_json::from_slice(&fs::read(output.join(command).join("receipt.json"))?)?;
    solved(&receipt, command)
}

/// Discharges every Alloy obligation of the manifest in the pinned image, offline.
///
/// # Errors
///
/// Returns an error when Docker can't build or run the image, or a command ends otherwise than its obligation requires.
pub fn verify(root: &Path, manifest: &obligations::Manifest) -> Result<()> {
    let root = crate::canonical(root)?;
    crate::container::verify_image_assets(&root)?;
    let output = tempfile::tempdir()?;
    let image = image(&root)?;
    build(&root, &image)?;
    let mut checked = 0;
    for obligation in &manifest.obligations {
        if let Obligation::Alloy(model) = obligation {
            for check in &model.checks {
                ensure!(
                    !run(&root, output.path(), &image, model, &check.assertion)?,
                    "Alloy found a counterexample to {} in {} {}",
                    check.assertion,
                    model.spec,
                    check.scope
                );
                ensure!(
                    run(&root, output.path(), &image, model, &check.instance)?,
                    "Alloy found no instance of {} in {}, so {} may hold vacuously",
                    check.instance,
                    model.spec,
                    check.assertion
                );
                ensure!(
                    run(&root, output.path(), &image, model, &check.counterexample)?,
                    "Alloy found no counterexample to {} in {}, so nothing shows {} can fail",
                    check.counterexample,
                    model.spec,
                    check.assertion
                );
                checked += 1;
            }
        }
    }
    println!(
        "Checked all {checked} Alloy assertions offline with Alloy {ALLOY_VERSION}: each holds within its scope, has an instance, and fails on its mutated fact"
    );
    Ok(())
}
