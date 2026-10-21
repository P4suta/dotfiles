//! The target semantics and native identity of proof containers.

use crate::tool::Tool;
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::ffi::OsString;
use std::fs;
use std::io::{Read as _, Write as _};
use std::path::Path;
use std::process::{Command, Output, Stdio};

pub(crate) const LINUX_AMD64: &str = "linux/amd64";
pub(crate) const CARGO_REGISTRY_VOLUME: &str = "dotfiles-proofs-registry:/usr/local/cargo/registry";
const CARGO_FETCH_GATE: &str =
    "set -e; for manifest in \"$@\"; do cargo fetch --locked --manifest-path \"$manifest\"; done";
const CARGO_SOURCE: &str = "/source";
const PLATFORM_FORMAT: &str = "{{.Os}}/{{.Architecture}}";
const ASSET_TEST: &str = "proof_images_reject_corrupt_verifier_assets_before_use";
const MAX_ASSET_STREAM_BYTES: usize = 1024 * 1024;
const MAX_REGRESSION_STREAM_BYTES: usize = 8 * MAX_ASSET_STREAM_BYTES;
const MAX_ASSET_REPORT_BYTES: usize = 40 * 1024 * 1024;
const CHECKSUM_REJECTION_EXIT: i32 = 1;
const ASSET_PRODUCER_INPUTS: [&str; 2] = [
    "xtask/tests/container_proofs.rs",
    "xtask/tests/support/incident_fixture.rs",
];
pub const ASSET_REPORT_VARIABLE: &str = "DOTFILES_MODEL_ASSET_REPORT";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CorruptAsset {
    pub directory: &'static str,
    pub target: &'static str,
}

pub const CORRUPT_ASSETS: [CorruptAsset; 4] = [
    CorruptAsset {
        directory: crate::alloy::IMAGE_DIRECTORY,
        target: crate::alloy::JAR,
    },
    CorruptAsset {
        directory: crate::quint::IMAGE_DIRECTORY,
        target: "/tmp/apalache.tgz",
    },
    CorruptAsset {
        directory: crate::lean::IMAGE_DIRECTORY,
        target: "/tmp/elan.tar.gz",
    },
    CorruptAsset {
        directory: crate::lean::IMAGE_DIRECTORY,
        target: "/tmp/lean.tar.zst",
    },
];

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AssetControlResult {
    directory: String,
    target: String,
    result: crate::phase_evidence::RawResult,
}

impl AssetControlResult {
    /// Captures an actual command result as unvalidated control data.
    ///
    /// # Errors
    ///
    /// Refuses an unknown asset or oversize streams.
    pub fn capture(asset: CorruptAsset, output: Output) -> Result<Self> {
        ensure!(CORRUPT_ASSETS.contains(&asset), "unknown corrupted asset");
        let captured = Self {
            directory: asset.directory.to_owned(),
            target: asset.target.to_owned(),
            result: output.into(),
        };
        captured.bounded()?;
        Ok(captured)
    }

    fn bounded(&self) -> Result<()> {
        ensure!(
            CORRUPT_ASSETS
                .iter()
                .any(|asset| { self.directory == asset.directory && self.target == asset.target }),
            "unknown asset control identity"
        );
        ensure!(
            self.result.stdout.len() <= MAX_ASSET_STREAM_BYTES
                && self.result.stderr.len() <= MAX_ASSET_STREAM_BYTES,
            "asset control streams exceed their bound"
        );
        Ok(())
    }

    /// Requires the original checksum refusal for this control's asset.
    ///
    /// # Errors
    ///
    /// Refuses success, interruption, an unrelated exit, or missing checksum evidence.
    pub fn require_rejection(&self) -> Result<()> {
        self.bounded()?;
        let stdout = self.result.stdout()?;
        let stderr = self.result.stderr()?;
        let report = format!("{stdout}{stderr}");
        ensure!(
            self.result.status == Some(CHECKSUM_REJECTION_EXIT)
                && report.contains(&format!("{}: FAILED", self.target))
                && report.contains("computed checksum did NOT match"),
            "the model image did not reject the corrupted {} by its checksum: {:?}\n{report}",
            self.target,
            self.result.status
        );
        Ok(())
    }
}

/// Writes bounded original control data to the existing parent-owned report file when requested.
///
/// # Errors
///
/// Refuses an oversize report or an unavailable output file.
pub fn write_asset_results(results: &[AssetControlResult]) -> Result<()> {
    let Some(path) = std::env::var_os(ASSET_REPORT_VARIABLE) else {
        return Ok(());
    };
    ensure!(
        results.len() <= CORRUPT_ASSETS.len(),
        "too many asset controls"
    );
    for result in results {
        result.bounded()?;
    }
    let bytes = serde_json::to_vec(results)?;
    ensure!(
        bytes.len() <= MAX_ASSET_REPORT_BYTES,
        "asset control report is oversized"
    );
    let mut file = fs::OpenOptions::new()
        .write(true)
        .truncate(true)
        .open(path)?;
    file.write_all(&bytes)?;
    file.sync_all()?;
    Ok(())
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ImageAssetReports {
    inputs_sha256: String,
    regression: crate::phase_evidence::RawResult,
    controls: Vec<AssetControlResult>,
}

fn asset_inputs(root: &Path) -> Result<String> {
    let mut hash = Sha256::new();
    hash.update(crate::phase_evidence::model_inputs(root)?.as_bytes());
    for path in ASSET_PRODUCER_INPUTS {
        let bytes = fs::read(root.join(path))?;
        hash.update(path.as_bytes());
        hash.update(u64::try_from(bytes.len())?.to_le_bytes());
        hash.update(bytes);
    }
    Ok(crate::build_inputs::hex(&hash.finalize()))
}

fn validate_asset_regression(result: &crate::phase_evidence::RawResult) -> Result<()> {
    ensure!(
        result.stdout.len() <= MAX_REGRESSION_STREAM_BYTES
            && result.stderr.len() <= MAX_REGRESSION_STREAM_BYTES,
        "asset regression streams exceed their bound"
    );
    let summaries: Vec<_> = result
        .stdout()?
        .lines()
        .filter(|line| line.starts_with("test result:"))
        .collect();
    ensure!(
        result.success()
            && summaries.len() == 1
            && summaries[0]
                .starts_with("test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured;"),
        "the model image corruption regression did not pass exactly once: {:?}\n{}{}",
        result.status,
        result.stdout()?,
        result.stderr()?
    );
    Ok(())
}

pub(crate) fn validate_image_assets(root: &Path, reports: &ImageAssetReports) -> Result<()> {
    ensure!(
        reports.inputs_sha256 == asset_inputs(root)?,
        "asset control inputs differ"
    );
    validate_asset_regression(&reports.regression)?;
    ensure!(
        reports.controls.len() == CORRUPT_ASSETS.len(),
        "asset control inventory is incomplete or unexpected"
    );
    for (result, expected) in reports.controls.iter().zip(CORRUPT_ASSETS) {
        ensure!(
            result.directory == expected.directory && result.target == expected.target,
            "asset control operation differs from its exact inventory"
        );
        result.require_rejection()?;
    }
    Ok(())
}

pub(crate) fn verify_image_assets(root: &Path) -> Result<ImageAssetReports> {
    let inputs_sha256 = asset_inputs(root)?;
    let mut file = tempfile::NamedTempFile::new()?;
    let mut command = Tool::Cargo.command();
    command
        .current_dir(root)
        .env(ASSET_REPORT_VARIABLE, file.path())
        .args([
            "test",
            "--locked",
            "--package",
            "xtask",
            "--test",
            "container_proofs",
            "--",
            "--ignored",
            "--exact",
            ASSET_TEST,
            "--nocapture",
        ]);
    let output = crate::phase_evidence::output(&mut command, "model image corrupted assets")?;
    let mut bytes = Vec::new();
    file.as_file_mut()
        .take(u64::try_from(MAX_ASSET_REPORT_BYTES)? + 1)
        .read_to_end(&mut bytes)?;
    ensure!(
        bytes.len() <= MAX_ASSET_REPORT_BYTES,
        "asset control report is oversized"
    );
    crate::phase_evidence::retain(
        "model image corrupted assets",
        "original-control-results",
        &bytes,
    )?;
    let regression = output.into();
    validate_asset_regression(&regression)?;
    let reports = ImageAssetReports {
        inputs_sha256,
        regression,
        controls: serde_json::from_slice(&bytes)
            .context("invalid original asset control results")?,
    };
    validate_image_assets(root, &reports)?;
    Ok(reports)
}

#[cfg(test)]
pub(crate) fn fixture_image_assets(root: &Path) -> Result<ImageAssetReports> {
    let controls = CORRUPT_ASSETS
        .iter()
        .map(|asset| AssetControlResult {
            directory: asset.directory.to_owned(),
            target: asset.target.to_owned(),
            result: crate::phase_evidence::RawResult {
                status: Some(CHECKSUM_REJECTION_EXIT),
                stdout: Vec::new(),
                stderr: format!(
                    "{}: FAILED\nsha256sum: WARNING: 1 computed checksum did NOT match\n",
                    asset.target
                )
                .into_bytes(),
            },
        })
        .collect();
    Ok(ImageAssetReports {
        inputs_sha256: asset_inputs(root)?,
        regression: crate::phase_evidence::RawResult {
            status: Some(0),
            stdout: b"test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 9 filtered out; finished in 0.01s\n".to_vec(),
            stderr: Vec::new(),
        },
        controls,
    })
}

pub(crate) fn amd64_image_present(image: &str) -> Result<bool> {
    image_present(image, LINUX_AMD64)
}

pub(crate) fn image_present(image: &str, platform: &str) -> Result<bool> {
    let mut command = Tool::Docker.command();
    command.args(["image", "inspect", "--format", PLATFORM_FORMAT, image]);
    let output = crate::phase_evidence::output(&mut command, "verification image platform")?;
    Ok(output.status.success() && String::from_utf8(output.stdout)?.trim() == platform)
}

pub(crate) fn native_platform() -> Result<String> {
    let mut command = Tool::Docker.command();
    command.args(["version", "--format", "{{.Server.Os}}/{{.Server.Arch}}"]);
    let output = crate::phase_evidence::output(&mut command, "verification Docker platform")?;
    let platform = String::from_utf8(output.stdout)?.trim().to_owned();
    ensure!(
        output.status.success() && matches!(platform.as_str(), "linux/amd64" | "linux/arm64"),
        "verification requires a running supported Linux Docker engine"
    );
    Ok(platform)
}

pub(crate) fn prepare_cargo(root: &Path, image_id: &str) -> Result<()> {
    prepare_cargo_with(root, image_id, |command| {
        crate::phase_evidence::output(command, "verification container Cargo materials")
    })
}

fn prepare_cargo_with(
    root: &Path,
    image_id: &str,
    execute: impl FnOnce(&mut Command) -> Result<Output>,
) -> Result<()> {
    let workload = dotfiles::artifacts::CargoWorkload::capture(root)?;
    let manifests = workload.workspace_manifests()?;
    let mut mount = OsString::from("type=bind,\"source=");
    mount.push(root);
    mount.push(format!("\",target={CARGO_SOURCE},readonly"));
    let mut command = Tool::Docker.command();
    command
        .args([
            "run",
            "--rm",
            "--pull=never",
            "--platform",
            LINUX_AMD64,
            "--mount",
        ])
        .arg(mount)
        .args([
            "--volume",
            CARGO_REGISTRY_VOLUME,
            "--workdir",
            CARGO_SOURCE,
            image_id,
            "bash",
            "-ec",
            CARGO_FETCH_GATE,
            "--",
        ])
        .args(
            manifests
                .iter()
                .map(|path| format!("{CARGO_SOURCE}/{path}")),
        )
        .stdin(Stdio::null());
    let output = execute(&mut command)?;
    ensure!(
        output.status.success(),
        "preparing the container's locked Cargo workspaces failed or was interrupted: {}",
        output.status
    );
    workload.require_current(root)
}

pub(crate) fn run_arguments() -> Vec<OsString> {
    let mut arguments: Vec<OsString> = ["run", "--rm", "--network", "none"]
        .map(OsString::from)
        .into();
    arguments.extend(owner_arguments());
    arguments.extend(dotfiles::bounded::resource_arguments());
    arguments
}

#[cfg(unix)]
fn owner_arguments() -> [OsString; 2] {
    let user = rustix::process::geteuid();
    let group = rustix::process::getegid();
    ["--user".into(), format!("{user}:{group}").into()]
}

#[cfg(not(unix))]
fn owner_arguments() -> [OsString; 0] {
    []
}

#[cfg(test)]
mod cargo_tests {
    use super::*;
    #[cfg(unix)]
    use std::os::unix::process::ExitStatusExt as _;
    #[cfg(windows)]
    use std::os::windows::process::ExitStatusExt as _;

    const SATELLITE: &str = "satellite $literal; (one)";
    const IMAGE: &str = "sha256:fixture";

    fn workspace(root: &Path) -> Result<()> {
        fs::create_dir_all(root)?;
        fs::write(root.join("Cargo.toml"), "[workspace]\nmembers = []\n")?;
        fs::write(root.join("Cargo.lock"), "version = 4\n")?;
        Ok(())
    }

    fn output(code: i32) -> Output {
        #[cfg(unix)]
        let status = std::process::ExitStatus::from_raw(code << 8);
        #[cfg(windows)]
        let status = std::process::ExitStatus::from_raw(code.cast_unsigned());
        Output {
            status,
            stdout: Vec::new(),
            stderr: Vec::new(),
        }
    }

    #[test]
    fn preparation_fetches_every_captured_workspace_as_data_in_the_shared_registry() -> Result<()> {
        let root = tempfile::tempdir()?;
        workspace(root.path())?;
        workspace(&root.path().join(SATELLITE))?;
        prepare_cargo_with(root.path(), IMAGE, |command| {
            let arguments: Vec<_> = command.get_args().collect();
            let gate = arguments
                .iter()
                .position(|arg| *arg == CARGO_FETCH_GATE)
                .unwrap();
            assert_eq!(arguments[gate - 3], IMAGE);
            assert_eq!(arguments[gate - 2], "bash");
            assert_eq!(arguments[gate - 1], "-ec");
            assert_eq!(
                arguments[gate + 1..],
                [
                    "--",
                    "/source/Cargo.toml",
                    &format!("/source/{SATELLITE}/Cargo.toml"),
                ]
            );
            assert!(
                arguments
                    .windows(2)
                    .any(|pair| { pair[0] == "--volume" && pair[1] == CARGO_REGISTRY_VOLUME })
            );
            assert!(arguments.windows(2).any(|pair| {
                pair[0] == "--mount"
                    && pair[1]
                        .to_string_lossy()
                        .ends_with(",target=/source,readonly")
            }));
            assert!(
                arguments
                    .windows(2)
                    .any(|pair| { pair[0] == "--platform" && pair[1] == LINUX_AMD64 })
            );
            assert!(!arguments.iter().any(|arg| {
                matches!(
                    arg.to_str(),
                    Some("--target" | "--offline" | "--network=none")
                )
            }));
            Ok(output(0))
        })
    }

    #[test]
    fn preparation_refuses_failed_or_unavailable_fetches() -> Result<()> {
        let root = tempfile::tempdir()?;
        workspace(root.path())?;
        for code in [1, 137] {
            assert!(prepare_cargo_with(root.path(), IMAGE, |_| Ok(output(code))).is_err());
        }
        assert!(
            prepare_cargo_with(root.path(), IMAGE, |_| {
                Err(std::io::Error::from(std::io::ErrorKind::NotFound).into())
            })
            .is_err()
        );
        Ok(())
    }

    #[test]
    fn preparation_refuses_changed_inputs_after_a_successful_fetch() -> Result<()> {
        for name in ["Cargo.toml", "Cargo.lock"] {
            let root = tempfile::tempdir()?;
            workspace(root.path())?;
            let result = prepare_cargo_with(root.path(), IMAGE, |_| {
                let path = root.path().join(name);
                let mut text = fs::read_to_string(&path)?;
                text.push_str("# changed during preparation\n");
                fs::write(path, text)?;
                Ok(output(0))
            });
            assert!(result.is_err(), "accepted changed {name}");
        }
        Ok(())
    }

    #[test]
    fn preparation_refuses_an_unlocked_workspace_before_starting_docker() -> Result<()> {
        let root = tempfile::tempdir()?;
        workspace(root.path())?;
        fs::create_dir(root.path().join(SATELLITE))?;
        fs::write(
            root.path().join(SATELLITE).join("Cargo.toml"),
            "[workspace]\nmembers = []\n",
        )?;
        let mut invoked = false;
        assert!(
            prepare_cargo_with(root.path(), IMAGE, |_| {
                invoked = true;
                Ok(output(0))
            })
            .is_err()
        );
        assert!(!invoked);
        Ok(())
    }
}

#[cfg(test)]
mod report_tests {
    use super::*;

    fn root() -> &'static Path {
        Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap()
    }

    #[test]
    fn image_asset_reports_require_every_original_control_and_checksum_refusal() -> Result<()> {
        let reports = fixture_image_assets(root())?;
        validate_image_assets(root(), &reports)?;
        for mutation in 0..17 {
            let mut changed = reports.clone();
            match mutation {
                0 => {
                    changed.controls.pop();
                }
                1 => changed.controls[1] = changed.controls[0].clone(),
                2 => changed.controls[0].target.push_str("_other"),
                3 => changed.controls[0].directory.push_str("_other"),
                4 => changed.controls[0].result.status = Some(0),
                5 => changed.controls[0].result.status = None,
                6 => changed.controls[0].result.status = Some(7),
                7 => changed.controls[0].result.status = Some(137),
                8 => changed.controls[0].result.stderr = b"unrelated Docker failure".to_vec(),
                9 => changed.controls[0].result.stderr = reports.controls[1].result.stderr.clone(),
                10 => changed.inputs_sha256.clear(),
                11 => changed.regression.status = None,
                12 => changed.regression.status = Some(1),
                13 => changed.regression.stdout.clear(),
                14 => changed
                    .regression
                    .stdout
                    .extend_from_slice(&reports.regression.stdout),
                15 => changed.controls.swap(0, 1),
                _ => changed.controls[0].result.stderr = vec![u8::MAX],
            }
            assert!(
                validate_image_assets(root(), &changed).is_err(),
                "accepted mutation {mutation}"
            );
        }
        Ok(())
    }

    #[test]
    fn image_asset_transport_is_closed_and_preserves_original_streams() -> Result<()> {
        let reports = fixture_image_assets(root())?;
        let encoded = serde_json::to_vec(&reports.controls)?;
        let decoded: Vec<AssetControlResult> = serde_json::from_slice(&encoded)?;
        for (original, transported) in reports.controls.iter().zip(decoded) {
            assert_eq!(original.result.status, transported.result.status);
            assert_eq!(original.result.stdout, transported.result.stdout);
            assert_eq!(original.result.stderr, transported.result.stderr);
        }
        let mut untrusted = serde_json::to_value(&reports.controls)?;
        untrusted[0]
            .as_object_mut()
            .unwrap()
            .insert("unknown".into(), true.into());
        assert!(serde_json::from_value::<Vec<AssetControlResult>>(untrusted).is_err());
        let mut missing = serde_json::to_value(&reports.controls)?;
        missing[0]["result"]
            .as_object_mut()
            .unwrap()
            .remove("stderr");
        assert!(serde_json::from_value::<Vec<AssetControlResult>>(missing).is_err());
        Ok(())
    }

    #[test]
    fn oversized_asset_streams_supply_no_control_credit() -> Result<()> {
        let mut reports = fixture_image_assets(root())?;
        reports.controls[0]
            .result
            .stderr
            .resize(MAX_ASSET_STREAM_BYTES + 1, b'x');
        assert!(reports.controls[0].require_rejection().is_err());
        reports
            .regression
            .stdout
            .resize(MAX_REGRESSION_STREAM_BYTES + 1, b'x');
        assert!(validate_asset_regression(&reports.regression).is_err());
        Ok(())
    }
}
