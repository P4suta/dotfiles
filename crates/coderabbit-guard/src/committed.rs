use crate::receipt::{
    RecordedReview, STDERR_LIMIT, STDOUT_LIMIT, VENDOR_VERSION, read_bounded, vendor_arguments,
};
use crate::scope::{Snapshot, digest, oid, snapshot, text};
use anyhow::{Context, Result, ensure};
use std::fs::{self, File, OpenOptions};
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::{Duration, Instant};

const COMMAND: &str = "review-committed";
const ARGUMENTS: usize = 5;
const BASE_ARGUMENT: usize = 2;
const RECEIPT_OPTION_ARGUMENT: usize = 3;
const RECEIPT_ARGUMENT: usize = 4;
#[cfg(unix)]
const PRIVATE_DIRECTORY_MODE: u32 = 0o700;
#[cfg(unix)]
const PRIVATE_FILE_MODE: u32 = 0o600;
const POLL_INTERVAL: Duration = Duration::from_millis(200);
const REVIEW_TIMEOUT: Duration = Duration::from_secs(30 * 60);
const RECORDS: &str = ".local/state/coderabbit-guard/completions";
#[cfg(unix)]
const GROUP_OTHER_PERMISSION_BITS: u32 = 6;

#[derive(Debug)]
pub(crate) struct Request {
    base: String,
    receipt: PathBuf,
}

impl Request {
    pub(crate) fn parse(arguments: &[String]) -> Result<Option<Self>> {
        if arguments.first().is_none_or(|argument| argument != COMMAND) {
            return Ok(None);
        }
        ensure!(
            arguments.len() == ARGUMENTS
                && arguments[1] == "--base-commit"
                && oid(&arguments[BASE_ARGUMENT])
                && arguments[RECEIPT_OPTION_ARGUMENT] == "--receipt"
                && !arguments[RECEIPT_ARGUMENT].is_empty()
                && !arguments[RECEIPT_ARGUMENT].starts_with('-'),
            "use review-committed --base-commit FULL_SHA --receipt NEW_PRIVATE_FILE"
        );
        Ok(Some(Self {
            base: arguments[BASE_ARGUMENT].clone(),
            receipt: arguments[RECEIPT_ARGUMENT].clone().into(),
        }))
    }

    pub(crate) fn arguments(&self) -> Vec<String> {
        vendor_arguments(&self.base)
    }

    pub(crate) fn prepare(self, user: &Path, version: &str) -> Result<Prepared> {
        ensure!(
            cfg!(target_os = "macos"),
            "guarded CLI evidence can only be produced on Mac"
        );
        ensure!(
            version == VENDOR_VERSION,
            "committed evidence requires the supported pinned vendor contract"
        );
        for name in ["CODERABBIT_HOST_URL", "CR_CLI_ALLOW_UNTRUSTED_HOST_URL"] {
            ensure!(
                std::env::var_os(name).is_none(),
                "a custom service endpoint cannot supply guarded completion evidence"
            );
        }
        let root = platform::issue_context::checkout(&std::env::current_dir()?)?;
        let scope = snapshot(&root, &self.base)?;
        let arguments = self.arguments();
        ensure!(
            text(&root, &["config", "--get", "gpg.format"])? == "ssh"
                && !text(&root, &["config", "--get", "user.signingkey"])?.is_empty(),
            "owner SSH signing must be configured before a service review"
        );
        let destination = if self.receipt.is_absolute() {
            self.receipt
        } else {
            root.join(self.receipt)
        };
        let parent = platform::paths::canonical(
            destination
                .parent()
                .context("private receipt parent missing")?,
        )?;
        ensure!(
            !parent.starts_with(&root),
            "private raw CLI receipts must live outside the reviewed checkout"
        );
        let destination = parent.join(
            destination
                .file_name()
                .context("private receipt name missing")?,
        );
        let receipt = create_private(&destination)?;
        let records = user.join(RECORDS);
        private_directory(&records)?;
        let directory = records.join(format!("{}-{}", crate::now_ms()?, std::process::id()));
        private_directory(&directory)?;
        let probe = platform::signed_git::create(
            &root,
            &scope.binding.head,
            &scope.tree,
            &serde_json::to_vec(&serde_json::json!({"type":"guard_review_started","scope":scope}))?,
        )?;
        write_new(
            &directory.join("started.json"),
            &serde_json::to_vec(&serde_json::json!({
                "scope":scope, "signing_probe":probe.object, "arguments":arguments,
            }))?,
        )?;
        Ok(Prepared {
            root,
            scope,
            receipt,
            directory,
        })
    }
}

#[derive(Debug)]
pub(crate) struct Prepared {
    pub(crate) root: PathBuf,
    scope: Snapshot,
    receipt: File,
    directory: PathBuf,
}

fn private_directory(path: &Path) -> Result<()> {
    match fs::symlink_metadata(path) {
        Ok(metadata) => {
            ensure!(
                metadata.is_dir() && !metadata.is_symlink(),
                "review records require a regular owned directory"
            );
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt as _;
                ensure!(
                    metadata.permissions().mode().trailing_zeros() >= GROUP_OTHER_PERMISSION_BITS,
                    "review records must remain private"
                );
            }
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            let mut builder = fs::DirBuilder::new();
            builder.recursive(true);
            #[cfg(unix)]
            {
                use std::os::unix::fs::DirBuilderExt as _;
                builder.mode(PRIVATE_DIRECTORY_MODE);
            }
            builder.create(path)?;
        }
        Err(error) => return Err(error.into()),
    }
    Ok(())
}

fn create_private(path: &Path) -> Result<File> {
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt as _;
        options.mode(PRIVATE_FILE_MODE);
    }
    Ok(options.open(path)?)
}

fn write_new(path: &Path, bytes: &[u8]) -> Result<()> {
    let mut file = create_private(path)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    Ok(())
}

impl Prepared {
    pub(crate) fn execute(mut self, vendor: &Path) -> Result<i32> {
        ensure!(
            snapshot(&self.root, &self.scope.binding.base)? == self.scope,
            "committed scope changed before review"
        );
        let stdout_path = self.directory.join("stdout.ndjson");
        let stderr_path = self.directory.join("stderr.log");
        let stdout = create_private(&stdout_path)?;
        let stderr = create_private(&stderr_path)?;
        let launched = crate::tool::external(vendor)
            .current_dir(&self.root)
            .args(vendor_arguments(&self.scope.binding.base))
            .env("CI", "true")
            .stdin(Stdio::null())
            .stdout(Stdio::from(stdout.try_clone()?))
            .stderr(Stdio::from(stderr.try_clone()?))
            .spawn();
        let mut child = match launched {
            Ok(child) => child,
            Err(error) => {
                write_new(
                    &self.directory.join("startup.json"),
                    &serde_json::to_vec(&error.to_string())?,
                )?;
                return Err(error.into());
            }
        };
        let started = Instant::now();
        let watched = (|| -> Result<std::process::ExitStatus> {
            loop {
                ensure!(
                    started.elapsed() < REVIEW_TIMEOUT,
                    "guarded review exceeded its elapsed bound; original private output retained"
                );
                if stdout.metadata()?.len() > STDOUT_LIMIT
                    || stderr.metadata()?.len() > STDERR_LIMIT
                {
                    anyhow::bail!(
                        "review output exceeded its bound; original private output retained"
                    );
                }
                if let Some(status) = child.try_wait()? {
                    break Ok(status);
                }
                std::thread::sleep(POLL_INTERVAL);
            }
        })();
        let status = match watched {
            Ok(status) => status,
            Err(error) => {
                platform::cleanup::report(child.kill(), "stop incomplete guarded review");
                platform::cleanup::report(child.wait(), "wait for incomplete guarded review");
                write_new(
                    &self.directory.join("terminal.json"),
                    &serde_json::to_vec(
                        &serde_json::json!({"status":null,"failure":error.to_string()}),
                    )?,
                )?;
                return Err(error);
            }
        };
        stdout.sync_all()?;
        stderr.sync_all()?;
        write_new(
            &self.directory.join("terminal.json"),
            &serde_json::to_vec(&serde_json::json!({"status":status.code()}))?,
        )?;
        let review = RecordedReview::new(
            self.scope.clone(),
            read_bounded(&stdout_path, STDOUT_LIMIT)?,
            read_bounded(&stderr_path, STDERR_LIMIT)?,
            status.code(),
            crate::now_ms()?,
        );
        write_new(
            &self.directory.join("original.json"),
            &serde_json::to_vec(&review)?,
        )?;
        if let Some(code) = status.code()
            && code != 0
        {
            return Ok(code);
        }
        let validated = review.validate()?;
        ensure!(
            snapshot(&self.root, &self.scope.binding.base)? == self.scope,
            "committed scope changed during review; no receipt created"
        );
        let packet = crate::receipt::sign(&self.root, &review)?;
        let bytes = serde_json::to_vec(&packet)?;
        self.receipt.write_all(&bytes)?;
        self.receipt.sync_all()?;
        write_new(&self.directory.join("receipt.json"), &bytes)?;
        println!(
            "Guarded CLI review completed: {} findings; private receipt sha256:{}",
            validated.findings.len(),
            digest(&bytes)
        );
        Ok(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn committed_entry_has_one_exact_scope_and_no_budget_or_import_flags() {
        let base = "a".repeat(40);
        let arguments = [
            COMMAND,
            "--base-commit",
            &base,
            "--receipt",
            "/private/new.json",
        ]
        .map(String::from);
        let parsed = Request::parse(&arguments).unwrap().unwrap();
        assert_eq!(parsed.arguments(), vendor_arguments(&base));
        for arguments in [
            vec![
                COMMAND,
                "--base-commit",
                "main",
                "--receipt",
                "/private/new.json",
            ],
            vec![COMMAND, "--base-commit", &base, "--receipt", "-"],
            vec![
                COMMAND,
                "--base-commit",
                &base,
                "--receipt",
                "/private/new.json",
                "--use-credits",
            ],
            vec![
                COMMAND,
                "--base-commit",
                &base,
                "--receipt",
                "/private/new.json",
                "--import",
                "old.json",
            ],
        ] {
            assert!(
                Request::parse(&arguments.into_iter().map(String::from).collect::<Vec<_>>())
                    .is_err()
            );
        }
    }
}
