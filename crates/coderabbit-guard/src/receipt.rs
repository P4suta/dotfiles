use crate::scope::{ReviewBinding, Snapshot, digest, oid, text};
use anyhow::{Context, Result, ensure};
use rules::review_completion_rules::{Completion, Outcome, Process, completed};
use serde::de::{MapAccess, SeqAccess, Visitor};
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::Value;
use std::collections::BTreeSet;
use std::fmt;
use std::fs;
use std::io::Read as _;
use std::path::{Component, Path};

pub const CLI_RECEIPT_LIMIT: u64 = 64 * 1024 * 1024;
pub(crate) const STDOUT_LIMIT: u64 = 8 * 1024 * 1024;
pub(crate) const STDERR_LIMIT: u64 = 2 * 1024 * 1024;
const EVENT_LIMIT: usize = 256 * 1024;
const SCHEMA: u32 = 1;
const BUILD_HASH_LENGTH: usize = 64;
pub(crate) const VENDOR_VERSION: &str = "0.8.2";
const PRODUCER: &str = "coderabbit-guard/committed-review-v1";

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ReviewSeverity {
    Critical,
    Major,
    Minor,
    Trivial,
    Info,
    None,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CliFinding {
    pub id: String,
    pub severity: ReviewSeverity,
    pub path: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CliReview {
    pub findings: Vec<CliFinding>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SignedReviewReceipt {
    pub object: String,
    pub raw: String,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct RecordedReview {
    schema: u32,
    producer: String,
    producer_hash: String,
    vendor_version: String,
    pub(crate) scope: Snapshot,
    arguments: Vec<String>,
    stdout: Vec<u8>,
    stderr: Vec<u8>,
    status: Option<i32>,
    completed_at_ms: u64,
}

impl RecordedReview {
    pub(crate) fn new(
        scope: Snapshot,
        stdout: Vec<u8>,
        stderr: Vec<u8>,
        status: Option<i32>,
        at: u64,
    ) -> Self {
        Self {
            schema: SCHEMA,
            producer: PRODUCER.into(),
            producer_hash: platform::BUILD_HASH.into(),
            vendor_version: VENDOR_VERSION.into(),
            arguments: vendor_arguments(&scope.binding.base),
            scope,
            stdout,
            stderr,
            status,
            completed_at_ms: at,
        }
    }

    pub(crate) fn validate(&self) -> Result<CliReview> {
        ensure!(
            self.producer_hash == platform::BUILD_HASH,
            "CLI receipt producer differs from the current guarded build"
        );
        self.validate_original()
    }

    fn validate_original(&self) -> Result<CliReview> {
        ensure!(
            self.schema == SCHEMA
                && self.producer == PRODUCER
                && self.producer_hash.len() == BUILD_HASH_LENGTH
                && self
                    .producer_hash
                    .bytes()
                    .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
                && self.vendor_version == VENDOR_VERSION
                && self.arguments == vendor_arguments(&self.scope.binding.base)
                && self.completed_at_ms != 0
                && u64::try_from(self.stderr.len())? <= STDERR_LIMIT,
            "CLI receipt producer or invocation differs from the supported guarded contract"
        );
        decode(&self.stdout, self.status, &self.scope)
    }
}

pub(crate) fn vendor_arguments(base: &str) -> Vec<String> {
    ["review", "--agent", "--committed", "--base-commit", base]
        .map(String::from)
        .into()
}

struct UniqueJson;

impl<'de> Deserialize<'de> for UniqueJson {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> std::result::Result<Self, D::Error> {
        struct Unique;
        impl<'de> Visitor<'de> for Unique {
            type Value = UniqueJson;
            fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str("JSON without duplicate object fields")
            }
            fn visit_map<M: MapAccess<'de>>(
                self,
                mut map: M,
            ) -> std::result::Result<Self::Value, M::Error> {
                let mut keys = BTreeSet::new();
                while let Some(key) = map.next_key::<String>()? {
                    if !keys.insert(key) {
                        return Err(serde::de::Error::custom("duplicate JSON field"));
                    }
                    map.next_value::<UniqueJson>()?;
                }
                Ok(UniqueJson)
            }
            fn visit_seq<S: SeqAccess<'de>>(
                self,
                mut seq: S,
            ) -> std::result::Result<Self::Value, S::Error> {
                while seq.next_element::<UniqueJson>()?.is_some() {}
                Ok(UniqueJson)
            }
            fn visit_bool<E: serde::de::Error>(
                self,
                _: bool,
            ) -> std::result::Result<Self::Value, E> {
                Ok(UniqueJson)
            }
            fn visit_i64<E: serde::de::Error>(self, _: i64) -> std::result::Result<Self::Value, E> {
                Ok(UniqueJson)
            }
            fn visit_u64<E: serde::de::Error>(self, _: u64) -> std::result::Result<Self::Value, E> {
                Ok(UniqueJson)
            }
            fn visit_f64<E: serde::de::Error>(self, _: f64) -> std::result::Result<Self::Value, E> {
                Ok(UniqueJson)
            }
            fn visit_str<E: serde::de::Error>(
                self,
                _: &str,
            ) -> std::result::Result<Self::Value, E> {
                Ok(UniqueJson)
            }
            fn visit_unit<E: serde::de::Error>(self) -> std::result::Result<Self::Value, E> {
                Ok(UniqueJson)
            }
        }
        deserializer.deserialize_any(Unique)
    }
}

pub(crate) fn json<T: serde::de::DeserializeOwned>(bytes: &[u8]) -> Result<T> {
    serde_json::from_slice::<UniqueJson>(bytes).context("ambiguous review JSON")?;
    Ok(serde_json::from_slice(bytes)?)
}

fn optional_count(value: &Value, key: &str) -> Result<Option<u64>> {
    value
        .get(key)
        .map(|value| {
            value
                .as_u64()
                .context("review count is not an unsigned integer")
        })
        .transpose()
}

fn finding(value: &Value, scope: &Snapshot) -> Result<CliFinding> {
    let path = value["fileName"]
        .as_str()
        .context("review finding path missing")?;
    let relative = path
        .strip_prefix(&format!("{}/", scope.working_directory))
        .unwrap_or(path);
    ensure!(
        !relative.is_empty()
            && !relative.chars().any(char::is_control)
            && Path::new(relative)
                .components()
                .all(|part| matches!(part, Component::Normal(_)))
            && !relative.contains('\\')
            && scope.paths.iter().any(|path| path == relative),
        "review finding path escapes the repository"
    );
    Ok(CliFinding {
        id: digest(&serde_json::to_vec(value)?),
        severity: serde_json::from_value(value["severity"].clone())?,
        path: relative.into(),
    })
}

fn local_context(event: &Value, scope: &Snapshot) -> Result<()> {
    ensure!(
        event.get("remote").is_none_or(|remote| remote == false),
        "remote review context"
    );
    ensure!(
        event["workingDirectory"].as_str() == Some(scope.working_directory.as_str()),
        "review context names another working directory"
    );
    if let Some(branch) = event.get("currentBranch") {
        ensure!(
            branch.as_str() == Some(scope.branch.as_str()),
            "review context branch differs"
        );
    }
    Ok(())
}

fn status_rejected(event: &Value) -> Result<bool> {
    let status = event["status"].as_str().context("review status missing")?;
    let mut rejected = status == "review_skipped";
    if let Some(outcome) = event.get("outcome") {
        ensure!(
            matches!(
                outcome.as_str(),
                Some("completed" | "completed_with_warnings" | "failed")
            ),
            "unsupported status outcome"
        );
        rejected |= outcome == "failed";
    }
    rejected |= optional_count(event, "unreviewedFileCount")?.is_some_and(|count| count != 0);
    Ok(rejected)
}

pub(crate) fn decode(stdout: &[u8], status: Option<i32>, scope: &Snapshot) -> Result<CliReview> {
    ensure!(
        u64::try_from(stdout.len())? <= STDOUT_LIMIT,
        "review stream exceeds its bound"
    );
    let stream = std::str::from_utf8(stdout)?;
    ensure!(
        !stream.is_empty() && stream.ends_with('\n'),
        "review stream is empty or truncated"
    );
    let mut evidence = Completion {
        process: match status {
            Some(0) => Process::Success,
            Some(_) => Process::Failed,
            None => Process::Interrupted,
        },
        contexts: 0,
        completions: 0,
        rejected: false,
        outcome: Outcome::Unknown,
        unreviewed_files: None,
        findings_match: false,
    };
    let mut findings = Vec::new();
    let mut finding_ids = BTreeSet::new();
    let mut declared_findings = None;
    for line in stream.lines() {
        ensure!(
            !line.is_empty() && line.len() <= EVENT_LIMIT,
            "invalid bounded review event"
        );
        let event: Value = json(line.as_bytes())?;
        ensure!(event.is_object(), "review event must be an object");
        let kind = event["type"]
            .as_str()
            .context("review event type missing")?;
        ensure!(
            evidence.completions == 0,
            "review stream continues after its terminal event"
        );
        match kind {
            "review_context" => {
                evidence.contexts += 1;
                ensure!(evidence.contexts == 1, "ambiguous review context");
                local_context(&event, scope)?;
            }
            "finding" => {
                ensure!(
                    evidence.contexts == 1,
                    "finding precedes the actual review context"
                );
                let finding = finding(&event, scope)?;
                ensure!(
                    finding_ids.insert(finding.id.clone()),
                    "duplicate review finding"
                );
                findings.push(finding);
            }
            "heartbeat" => {}
            "status" => {
                evidence.rejected |= status_rejected(&event)?;
            }
            "complete" => {
                evidence.completions += 1;
                evidence.rejected |= event["status"] != "review_completed";
                evidence.outcome = match event.get("outcome") {
                    None => Outcome::Legacy,
                    Some(value) => match value.as_str() {
                        Some("completed") => Outcome::Completed,
                        Some("completed_with_warnings") => Outcome::Warnings,
                        Some("failed") => Outcome::Failed,
                        _ => Outcome::Unknown,
                    },
                };
                evidence.unreviewed_files = optional_count(&event, "unreviewedFileCount")?;
                declared_findings = optional_count(&event, "findings")?;
                if let Some(message) = event.get("message") {
                    ensure!(message.is_string(), "review message is malformed");
                }
            }
            "error" | "action_required" => evidence.rejected = true,
            _ => anyhow::bail!("unsupported review event type"),
        }
    }
    evidence.findings_match =
        declared_findings.is_none_or(|count| u64::try_from(findings.len()) == Ok(count));
    ensure!(
        completed(evidence),
        "review did not complete its full committed scope"
    );
    Ok(CliReview { findings })
}

pub(crate) fn read_bounded(path: &Path, limit: u64) -> Result<Vec<u8>> {
    ensure!(
        fs::symlink_metadata(path)?.file_type().is_file(),
        "review evidence must be a regular file"
    );
    let file = fs::File::open(path)?;
    ensure!(
        file.metadata()?.len() <= limit,
        "review evidence exceeds its byte bound"
    );
    let mut bytes = Vec::new();
    file.take(limit + 1).read_to_end(&mut bytes)?;
    ensure!(
        u64::try_from(bytes.len())? <= limit,
        "review evidence grew beyond its byte bound"
    );
    Ok(bytes)
}

/// Reads a bounded private signed receipt without accepting unsigned result imports.
///
/// # Errors
/// Rejects links, oversize JSON, ambiguous JSON, and unknown packet fields.
pub fn read_cli_receipt(path: &Path) -> Result<SignedReviewReceipt> {
    json(&read_bounded(path, CLI_RECEIPT_LIMIT)?)
}

/// Authenticates the owner receipt and rechecks its original completion at this exact scope.
///
/// # Errors
/// Refuses invalid signatures, stale producer or scope, mismatched Git closure, or incomplete review output.
pub fn require_cli_evidence(
    root: &Path,
    binding: &ReviewBinding,
    packet: &SignedReviewReceipt,
    trusted_signers: &Path,
) -> Result<CliReview> {
    authenticated(root, binding, packet, trusted_signers)?.validate()
}

/// Reads authenticated historical findings without supplying current review completion credit.
/// The owner signature attests the original guarded producer record.
///
/// # Errors
/// Refuses invalid signatures, another original scope, unsupported producers, or incomplete raw output.
pub fn require_historical_cli_findings(
    root: &Path,
    binding: &ReviewBinding,
    packet: &SignedReviewReceipt,
    trusted_signers: &Path,
) -> Result<Vec<CliFinding>> {
    Ok(authenticated(root, binding, packet, trusted_signers)?
        .validate_original()?
        .findings)
}

fn authenticated(
    root: &Path,
    binding: &ReviewBinding,
    packet: &SignedReviewReceipt,
    trusted_signers: &Path,
) -> Result<RecordedReview> {
    ensure!(
        oid(&packet.object) && u64::try_from(packet.raw.len())? <= CLI_RECEIPT_LIMIT,
        "invalid bounded signed review packet"
    );
    let verified =
        platform::signed_git::authenticate(root, &packet.object, &packet.raw, trusted_signers)?;
    let review: RecordedReview = json(verified.payload.as_bytes())?;
    let actual = ReviewBinding::capture(root, &binding.repository, &binding.base, &binding.head)?;
    ensure!(
        actual == *binding && review.scope.binding == actual,
        "CLI receipt belongs to another repository or committed diff"
    );
    let tree = text(root, &["rev-parse", &format!("{}^{{tree}}", binding.head)])?;
    ensure!(
        verified.tree == tree
            && review.scope.tree == tree
            && verified.parents == [binding.head.clone()],
        "signed CLI receipt has another source tree or parent"
    );
    ensure!(
        review.scope.paths == crate::scope::paths(root, &binding.base, &binding.head)?,
        "signed CLI receipt has another committed path inventory"
    );
    Ok(review)
}

/// Validates bounded agent data without authenticating a review or creating evidence.
///
/// # Errors
/// Refuses ambiguous, malformed, incomplete, or out-of-scope review events.
pub fn validate_agent_stream(
    stdout: &[u8],
    working_directory: &str,
    branch: &str,
    paths: &[String],
) -> Result<()> {
    let scope = Snapshot {
        binding: ReviewBinding {
            repository: String::new(),
            base: String::new(),
            head: String::new(),
            diff_sha256: String::new(),
        },
        tree: String::new(),
        working_directory: working_directory.into(),
        branch: branch.into(),
        paths: paths.to_vec(),
    };
    decode(stdout, Some(0), &scope).map(|_| ())
}

pub(crate) fn sign(root: &Path, review: &RecordedReview) -> Result<SignedReviewReceipt> {
    review.validate()?;
    let signed = platform::signed_git::create(
        root,
        &review.scope.binding.head,
        &review.scope.tree,
        &serde_json::to_vec(review)?,
    )?;
    let packet = SignedReviewReceipt {
        object: signed.object,
        raw: signed.raw,
    };
    ensure!(
        u64::try_from(serde_json::to_vec(&packet)?.len())? <= CLI_RECEIPT_LIMIT,
        "signed CLI receipt exceeds its byte bound"
    );
    Ok(packet)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scope() -> Snapshot {
        Snapshot {
            binding: ReviewBinding {
                repository: "fixture/project".into(),
                base: "a".repeat(40),
                head: "b".repeat(40),
                diff_sha256: digest(b"diff"),
            },
            tree: "c".repeat(40),
            working_directory: "/review".into(),
            branch: "feature".into(),
            paths: vec!["src/main.rs".into()],
        }
    }

    fn stream(events: &[Value]) -> Vec<u8> {
        let mut encoded = Vec::new();
        for event in events {
            serde_json::to_writer(&mut encoded, event).unwrap();
            encoded.push(b'\n');
        }
        encoded
    }

    fn context() -> Value {
        serde_json::json!({"type":"review_context","workingDirectory":"/review"})
    }
    fn terminal() -> Value {
        serde_json::json!({"type":"complete","status":"review_completed","findings":0})
    }

    #[test]
    fn actual_legacy_completion_allows_additive_metadata_and_optional_branch() -> Result<()> {
        let valid = stream(&[
            context(),
            serde_json::json!({"type":"status","status":"informational-progress","metadata":{"percent":50}}),
            terminal(),
        ]);
        assert_eq!(
            decode(&valid, Some(0), &scope())?.findings,
            Vec::<CliFinding>::new()
        );
        let mut complete = terminal();
        complete["outcome"] = serde_json::json!("completed_with_warnings");
        complete["unreviewedFileCount"] = serde_json::json!(0);
        complete["newMetadata"] = serde_json::json!({"nested":true});
        decode(&stream(&[context(), complete]), Some(0), &scope())?;
        Ok(())
    }

    #[test]
    fn zero_exit_cannot_credit_skipped_failed_partial_error_or_interrupted_output() {
        for changed in [
            serde_json::json!({"type":"complete","status":"review_skipped","findings":0}),
            serde_json::json!({"type":"complete","status":"review_completed","outcome":"failed"}),
            serde_json::json!({"type":"complete","status":"review_completed","outcome":"unknown"}),
            serde_json::json!({"type":"complete","status":"review_completed","outcome":null}),
            serde_json::json!({"type":"complete","status":"review_completed","unreviewedFileCount":1}),
            serde_json::json!({"type":"complete","status":"review_completed","unreviewedFileCount":null}),
            serde_json::json!({"type":"complete","status":"review_completed","findings":1}),
            serde_json::json!({"type":"complete","status":null}),
        ] {
            assert!(decode(&stream(&[context(), changed]), Some(0), &scope()).is_err());
        }
        for error in [
            serde_json::json!({"type":"error","errorType":"rate_limit"}),
            serde_json::json!({"type":"action_required"}),
            serde_json::json!({"type":"status","status":"review_skipped"}),
            serde_json::json!({"type":"status","status":"progress","outcome":"failed"}),
            serde_json::json!({"type":"status","status":"progress","unreviewedFileCount":1}),
        ] {
            assert!(decode(&stream(&[context(), error, terminal()]), Some(0), &scope()).is_err());
        }
        for status in [Some(1), None] {
            assert!(decode(&stream(&[context(), terminal()]), status, &scope()).is_err());
        }
        assert!(decode(&stream(&[terminal()]), Some(0), &scope()).is_err());
        assert!(decode(&stream(&[context()]), Some(0), &scope()).is_err());
    }

    #[test]
    fn scope_duplicate_null_and_truncated_events_never_supply_credit() {
        let mut wrong = context();
        wrong["workingDirectory"] = serde_json::json!("/other");
        assert!(decode(&stream(&[wrong, terminal()]), Some(0), &scope()).is_err());
        let mut wrong = context();
        wrong["currentBranch"] = serde_json::json!("another");
        assert!(decode(&stream(&[wrong, terminal()]), Some(0), &scope()).is_err());
        for events in [
            vec![context(), context(), terminal()],
            vec![context(), terminal(), terminal()],
        ] {
            assert!(decode(&stream(&events), Some(0), &scope()).is_err());
        }
        let mut valid = stream(&[context(), terminal()]);
        valid.pop();
        assert!(decode(&valid, Some(0), &scope()).is_err());
        for raw in [
            b"null\n".as_slice(),
            b"{}\n",
            b"{\"type\":\"complete\",\"status\":\"failed\",\"status\":\"review_completed\"}\n",
        ] {
            assert!(decode(raw, Some(0), &scope()).is_err());
        }
        assert!(json::<Value>(b"{\"metadata\":{\"a\":1,\"a\":2}}").is_err());
    }

    #[test]
    fn completed_review_keeps_every_finding_and_requires_actual_committed_paths() -> Result<()> {
        let finding = serde_json::json!({"type":"finding","severity":"major","fileName":"src/main.rs","codegenInstructions":"Inspect the actual path.","suggestions":[]});
        let mut complete = terminal();
        complete["findings"] = serde_json::json!(1);
        let review = decode(
            &stream(&[context(), finding.clone(), complete.clone()]),
            Some(0),
            &scope(),
        )?;
        assert_eq!(
            review.findings,
            vec![CliFinding {
                id: digest(&serde_json::to_vec(&finding)?),
                severity: ReviewSeverity::Major,
                path: "src/main.rs".into()
            }]
        );
        for path in [
            "../foreign",
            "C:foreign",
            "/other/src/main.rs",
            "src/missing.rs",
        ] {
            let mut wrong = finding.clone();
            wrong["fileName"] = serde_json::json!(path);
            assert!(
                decode(
                    &stream(&[context(), wrong, complete.clone()]),
                    Some(0),
                    &scope()
                )
                .is_err()
            );
        }
        let mut unknown = finding.clone();
        unknown["severity"] = serde_json::json!("future");
        assert!(
            decode(
                &stream(&[context(), unknown, complete.clone()]),
                Some(0),
                &scope()
            )
            .is_err()
        );
        assert!(
            decode(
                &stream(&[context(), finding.clone(), finding, complete]),
                Some(0),
                &scope()
            )
            .is_err()
        );
        Ok(())
    }

    struct Fixture {
        _temporary: tempfile::TempDir,
        root: std::path::PathBuf,
        scope: Snapshot,
        record: RecordedReview,
        packet: SignedReviewReceipt,
        signers: std::path::PathBuf,
    }

    fn signed_fixture(root: &Path, record: &RecordedReview) -> Result<SignedReviewReceipt> {
        let signed = platform::signed_git::create(
            root,
            &record.scope.binding.head,
            &record.scope.tree,
            &serde_json::to_vec(record)?,
        )?;
        Ok(SignedReviewReceipt {
            object: signed.object,
            raw: signed.raw,
        })
    }

    fn fixture() -> Result<Fixture> {
        let temporary = tempfile::tempdir()?;
        let root = temporary.path().join("repository");
        fs::create_dir(&root)?;
        let key = temporary.path().join("owner");
        let generated = crate::tool::Tool::SshKeygen
            .command()
            .args(["-q", "-t", "ed25519", "-N", "", "-f"])
            .arg(&key)
            .output()?;
        ensure!(
            generated.status.success(),
            "fixture owner key creation failed"
        );
        crate::scope::git(&root, &["init", "--quiet", "--initial-branch=feature"])?;
        for (name, value) in [
            ("user.name", "Fixture"),
            ("user.email", "fixture@example.com"),
            ("gpg.format", "ssh"),
        ] {
            crate::scope::git(&root, &["config", name, value])?;
        }
        crate::scope::git(
            &root,
            &[
                "config",
                "user.signingkey",
                key.to_str().context("fixture key path")?,
            ],
        )?;
        crate::scope::git(
            &root,
            &[
                "remote",
                "add",
                "origin",
                "https://github.com/fixture/project.git",
            ],
        )?;
        fs::write(root.join("source.txt"), b"base")?;
        crate::scope::git(&root, &["add", "source.txt"])?;
        let tree = text(&root, &["write-tree"])?;
        let base = text(&root, &["commit-tree", &tree, "-m", "fixture base"])?;
        fs::write(root.join("source.txt"), b"head")?;
        crate::scope::git(&root, &["add", "source.txt"])?;
        let tree = text(&root, &["write-tree"])?;
        let head = text(
            &root,
            &["commit-tree", &tree, "-p", &base, "-m", "fixture head"],
        )?;
        crate::scope::git(&root, &["update-ref", "HEAD", &head])?;
        let scope = crate::scope::snapshot(&root, &base)?;
        let context =
            serde_json::json!({"type":"review_context","workingDirectory":scope.working_directory});
        let record = RecordedReview::new(
            scope.clone(),
            stream(&[context, terminal()]),
            Vec::new(),
            Some(0),
            1,
        );
        let packet = sign(&root, &record)?;
        let signers = temporary.path().join("allowed-signers");
        fs::write(
            &signers,
            format!(
                "* namespaces=\"git\" {}",
                fs::read_to_string(key.with_extension("pub"))?
            ),
        )?;
        Ok(Fixture {
            _temporary: temporary,
            root,
            scope,
            record,
            packet,
            signers,
        })
    }

    #[test]
    fn owner_signature_and_exact_git_scope_are_required_for_portable_receipts() -> Result<()> {
        let Fixture {
            root,
            scope,
            packet,
            signers,
            _temporary,
            ..
        } = fixture()?;
        require_cli_evidence(&root, &scope.binding, &packet, &signers)?;
        let mut forged = packet.clone();
        forged.raw.push(' ');
        assert!(require_cli_evidence(&root, &scope.binding, &forged, &signers).is_err());
        let mut different = scope.binding.clone();
        different.repository = "other/project".into();
        assert!(require_cli_evidence(&root, &different, &packet, &signers).is_err());
        let mut different = scope.binding.clone();
        different.diff_sha256 = digest(b"other");
        assert!(require_cli_evidence(&root, &different, &packet, &signers).is_err());
        fs::write(&signers, b"")?;
        assert!(require_cli_evidence(&root, &scope.binding, &packet, &signers).is_err());
        Ok(())
    }

    #[test]
    fn historical_findings_survive_refresh_without_current_completion_credit() -> Result<()> {
        let Fixture {
            root,
            scope,
            mut record,
            signers,
            _temporary,
            ..
        } = fixture()?;
        let finding =
            serde_json::json!({"type":"finding","severity":"major","fileName":"source.txt"});
        let context =
            serde_json::json!({"type":"review_context","workingDirectory":scope.working_directory});
        let mut complete = terminal();
        complete["findings"] = serde_json::json!(1);
        record.stdout = stream(&[context.clone(), finding.clone(), complete]);
        record.producer_hash = digest(b"previous guarded producer");
        let packet = signed_fixture(&root, &record)?;
        assert!(require_cli_evidence(&root, &scope.binding, &packet, &signers).is_err());
        assert_eq!(
            require_historical_cli_findings(&root, &scope.binding, &packet, &signers)?,
            vec![CliFinding {
                id: digest(&serde_json::to_vec(&finding)?),
                severity: ReviewSeverity::Major,
                path: "source.txt".into(),
            }]
        );
        for hash in ["unknown", "", "UPPERCASE"] {
            record.producer_hash = hash.into();
            let packet = signed_fixture(&root, &record)?;
            assert!(
                require_historical_cli_findings(&root, &scope.binding, &packet, &signers).is_err()
            );
        }
        record.producer_hash = digest(b"previous guarded producer");
        for status in [Some(1), None] {
            record.status = status;
            let packet = signed_fixture(&root, &record)?;
            assert!(
                require_historical_cli_findings(&root, &scope.binding, &packet, &signers).is_err()
            );
        }
        record.status = Some(0);
        record.stdout = stream(&[
            context,
            serde_json::json!({"type":"complete","status":"review_completed","outcome":"failed"}),
        ]);
        let packet = signed_fixture(&root, &record)?;
        assert!(require_historical_cli_findings(&root, &scope.binding, &packet, &signers).is_err());
        Ok(())
    }
}
