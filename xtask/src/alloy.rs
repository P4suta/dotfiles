//! The Alloy obligations.
//! Each assertion finds no counterexample within its scope, each instance exists, and each mutated fact yields a counterexample.

use crate::obligations::{self, AlloyModel, Obligation};
use crate::phase_evidence::RawResult;
use crate::tool::Tool;
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::ffi::OsString;
use std::fs;
use std::path::Path;

pub use crate::container::{
    ASSET_REPORT_VARIABLE, AssetControlResult, CORRUPT_ASSETS, CorruptAsset, write_asset_results,
};

/// The Alloy release that the image's Dockerfile pins by digest.
pub const ALLOY_VERSION: &str = "6.2.0";

/// The directory that defines the image of the Alloy obligations.
pub const IMAGE_DIRECTORY: &str = "xtask/proofs/alloy";

/// Where the container mounts the read-only checkout and the writable output directory.
const SOURCE: &str = "/source";
const OUTPUT: &str = "/out";

/// The jar the image installs.
pub(crate) const JAR: &str = "/opt/alloy/alloy.jar";

/// The digest bytes an image name carries.
const DIGEST_BYTES: usize = 8;
const COMMANDS_PER_ASSERTION: usize = 3;

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
    let platform = crate::container::native_platform()?;
    if crate::container::image_present(image, &platform)? {
        return Ok(());
    }
    let mut command = Tool::Docker.command();
    command
        .args(["build", "--platform", &platform, "--tag", image])
        .arg(root.join(IMAGE_DIRECTORY));
    let output = crate::phase_evidence::output(&mut command, "build the pinned Alloy image")?;
    ensure!(
        output.status.success(),
        "cannot build the pinned Alloy image: {}\n{}{}",
        output.status,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
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

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Reports {
    inputs_sha256: String,
    manifest_sha256: String,
    image_tag: String,
    image_id: String,
    platform: String,
    asset_controls: crate::container::ImageAssetReports,
    commands: Vec<CommandReport>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct CommandReport {
    spec: String,
    solver: String,
    command: String,
    result: RawResult,
    receipt: Option<Vec<u8>>,
}

fn run(
    root: &Path,
    output: &Path,
    image: &str,
    model: &AlloyModel,
    command: &str,
) -> Result<CommandReport> {
    let mut process = Tool::Docker.command();
    process.args(run_arguments(
        root,
        output,
        image,
        &model.spec,
        model.solver.argument(),
        command,
    ));
    let ran = crate::phase_evidence::output(&mut process, "Alloy model command")?;
    let receipt = output.join(command).join("receipt.json");
    let receipt = if receipt.try_exists()? {
        Some(fs::read(receipt)?)
    } else {
        None
    };
    Ok(CommandReport {
        spec: model.spec.clone(),
        solver: model.solver.argument().to_owned(),
        command: command.to_owned(),
        result: ran.into(),
        receipt,
    })
}

pub(crate) fn collect(root: &Path, manifest: &obligations::Manifest) -> Result<Reports> {
    let root = crate::canonical(root)?;
    let asset_controls = crate::container::verify_image_assets(&root)?;
    let image_tag = image(&root)?;
    build(&root, &image_tag)?;
    let platform = crate::container::native_platform()?;
    let identity = crate::verification_env::inspect(&image_tag, &platform)?;
    let mut reports = Reports {
        inputs_sha256: crate::phase_evidence::model_inputs(&root)?,
        manifest_sha256: crate::build_inputs::hex(&Sha256::digest(format!("{manifest:?}"))),
        image_tag,
        image_id: identity.id,
        platform,
        asset_controls,
        commands: Vec::new(),
    };
    for obligation in &manifest.obligations {
        if let Obligation::Alloy(model) = obligation {
            for check in &model.checks {
                for command in [&check.assertion, &check.instance, &check.counterexample] {
                    let output = tempfile::tempdir()?;
                    reports.commands.push(run(
                        &root,
                        output.path(),
                        &reports.image_id,
                        model,
                        command,
                    )?);
                }
            }
        }
    }
    Ok(reports)
}

pub(crate) fn validate(
    root: &Path,
    manifest: &obligations::Manifest,
    reports: &Reports,
) -> Result<()> {
    crate::container::validate_image_assets(root, &reports.asset_controls)?;
    ensure!(
        reports.manifest_sha256
            == crate::build_inputs::hex(&Sha256::digest(format!("{manifest:?}"))),
        "Alloy report manifest differs"
    );
    ensure!(
        reports.inputs_sha256 == crate::phase_evidence::model_inputs(root)?,
        "Alloy report source inputs differ"
    );
    ensure!(
        reports.image_tag == image(root)?,
        "Alloy report image definition differs"
    );
    ensure!(
        matches!(reports.platform.as_str(), "linux/amd64" | "linux/arm64"),
        "Alloy report platform is unsupported"
    );
    let (_, architecture) = reports
        .platform
        .split_once('/')
        .context("Alloy report target missing")?;
    crate::verification_env::parse_image(
        &serde_json::to_string(&serde_json::json!({
            "Id":reports.image_id, "Os":"linux", "Architecture":architecture
        }))?,
        &reports.platform,
    )?;
    let expected = manifest
        .obligations
        .iter()
        .filter_map(|obligation| {
            if let Obligation::Alloy(model) = obligation {
                Some(model)
            } else {
                None
            }
        })
        .flat_map(|model| {
            model.checks.iter().flat_map(move |check| {
                [
                    (model, check.assertion.as_str(), false),
                    (model, check.instance.as_str(), true),
                    (model, check.counterexample.as_str(), true),
                ]
            })
        })
        .collect::<Vec<_>>();
    ensure!(
        reports.commands.len() == expected.len(),
        "Alloy report inventory is incomplete or unexpected"
    );
    for (report, (model, command, solution)) in reports.commands.iter().zip(&expected) {
        ensure!(
            report.spec == model.spec
                && report.solver == model.solver.argument()
                && report.command == *command,
            "Alloy report operation differs from the current manifest"
        );
        ensure!(
            report.result.success(),
            "Alloy failed on {command} of {}: {:?}\n{}",
            model.spec,
            report.result.status,
            report.result.stderr()?
        );
        let receipt: Value = serde_json::from_slice(
            report
                .receipt
                .as_deref()
                .context("Alloy wrote no receipt")?,
        )?;
        ensure!(
            solved(&receipt, command)? == *solution,
            "Alloy command {command} did not meet its required outcome"
        );
    }
    println!(
        "Checked all {} Alloy assertions offline with Alloy {ALLOY_VERSION}: each holds within its scope, has an instance, and fails on its mutated fact",
        expected.len() / COMMANDS_PER_ASSERTION
    );
    Ok(())
}

/// Discharges every Alloy obligation of the manifest in the pinned image, offline.
///
/// # Errors
///
/// Returns an error when Docker can't build or run the image, or a command ends otherwise than its obligation requires.
pub fn verify(root: &Path, manifest: &obligations::Manifest) -> Result<()> {
    validate(root, manifest, &collect(root, manifest)?)
}

#[cfg(test)]
mod report_tests {
    use super::*;
    const IMAGE_HEX_DIGITS: usize = 64;

    fn fixture() -> Result<(std::path::PathBuf, obligations::Manifest, Reports)> {
        let root = crate::canonical(
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .parent()
                .context("workspace root")?,
        )?;
        let manifest = obligations::manifest(&root)?;
        let mut commands = Vec::new();
        for obligation in &manifest.obligations {
            if let Obligation::Alloy(model) = obligation {
                for check in &model.checks {
                    for (command, solution) in [
                        (&check.assertion, false),
                        (&check.instance, true),
                        (&check.counterexample, true),
                    ] {
                        let mut entry = serde_json::json!({"type":"check"});
                        if solution {
                            entry["solution"] = serde_json::json!([{}]);
                        }
                        commands.push(CommandReport {
                            spec: model.spec.clone(),
                            solver: model.solver.argument().to_owned(),
                            command: command.clone(),
                            result: RawResult {
                                status: Some(0),
                                stdout: Vec::new(),
                                stderr: Vec::new(),
                            },
                            receipt: Some(serde_json::to_vec(
                                &serde_json::json!({"commands":{command:entry}}),
                            )?),
                        });
                    }
                }
            }
        }
        ensure!(!commands.is_empty(), "Alloy fixture inventory missing");
        let reports = Reports {
            inputs_sha256: crate::phase_evidence::model_inputs(&root)?,
            manifest_sha256: crate::build_inputs::hex(&Sha256::digest(format!("{manifest:?}"))),
            image_tag: image(&root)?,
            image_id: format!("sha256:{}", "a".repeat(IMAGE_HEX_DIGITS)),
            platform: "linux/amd64".to_owned(),
            asset_controls: crate::container::fixture_image_assets(&root)?,
            commands,
        };
        Ok((root, manifest, reports))
    }

    #[test]
    fn alloy_reports_require_complete_current_operations_and_actual_outcomes() -> Result<()> {
        let (root, manifest, reports) = fixture()?;
        let encoded = serde_json::to_vec(&reports)?;
        let decoded: Reports = serde_json::from_slice(&encoded)?;
        validate(&root, &manifest, &decoded)?;
        let mut legacy = serde_json::to_value(&reports)?;
        legacy.as_object_mut().unwrap().remove("asset_controls");
        assert!(serde_json::from_value::<Reports>(legacy).is_err());
        let mut omitted = serde_json::to_value(&reports)?;
        omitted["asset_controls"]["controls"] = serde_json::json!([]);
        let omitted: Reports = serde_json::from_value(omitted)?;
        assert!(validate(&root, &manifest, &omitted).is_err());
        for mutation in 0..8 {
            let mut changed = reports.clone();
            match mutation {
                0 => {
                    changed.commands.pop();
                }
                1 => changed.commands.push(changed.commands[0].clone()),
                2 => changed.commands[0].command.push_str("_other"),
                3 => changed.commands[0].result.status = None,
                4 => changed.commands[0].receipt = None,
                5 => changed.inputs_sha256.clear(),
                6 => changed.image_id.clear(),
                _ => changed.manifest_sha256.clear(),
            }
            assert!(
                validate(&root, &manifest, &changed).is_err(),
                "accepted mutation {mutation}"
            );
        }
        for index in 0..3 {
            let mut changed = reports.clone();
            let command = &changed.commands[index].command;
            let entry = if index == 0 {
                serde_json::json!({"type":"check","solution":[{}]})
            } else {
                serde_json::json!({"type":"check"})
            };
            changed.commands[index].receipt = Some(serde_json::to_vec(
                &serde_json::json!({"commands":{command:entry}}),
            )?);
            assert!(
                validate(&root, &manifest, &changed).is_err(),
                "accepted wrong outcome {index}"
            );
        }
        Ok(())
    }
}
