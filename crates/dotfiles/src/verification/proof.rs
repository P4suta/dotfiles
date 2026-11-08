use super::plan::Check;
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeSet,
    fmt::Write as _,
    path::{Path, PathBuf},
};

const RECEIPT_FORMAT: u32 = 3;
const COUNTEREXAMPLE_EXIT: i32 = 1;
const SHA256_HEX_BYTES: usize = 64;
const PHASE_PREFIX: &str = "phase=verification ";
const RECEIPT_DIRECTORY: &str = "verification/results";

#[derive(Debug)]
pub(super) struct Reference {
    pub path: PathBuf,
    digest: String,
    arguments: Vec<String>,
}

fn digest(value: &str) -> bool {
    value.len() == SHA256_HEX_BYTES && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

pub(super) fn references(text: &str) -> Result<Vec<Reference>> {
    let mut references = Vec::new();
    let mut paths = BTreeSet::new();
    for line in text.lines().filter(|line| line.starts_with(PHASE_PREFIX)) {
        let value = line
            .strip_prefix(PHASE_PREFIX)
            .context("missing verification phase")?;
        let (name, value) = value
            .strip_prefix("name=")
            .and_then(|value| value.split_once(" args="))
            .context("invalid proof phase name")?;
        let (arguments, hash) = value
            .rsplit_once(" source_sha256=")
            .context("proof phase has no source digest")?;
        ensure!(
            !name.is_empty()
                && name
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
                && digest(hash),
            "unsafe proof phase identity"
        );
        let array: syn::ExprArray =
            syn::parse_str(arguments).context("invalid proof phase arguments")?;
        let arguments = array
            .elems
            .iter()
            .map(|value| match value {
                syn::Expr::Lit(value) if value.attrs.is_empty() => match &value.lit {
                    syn::Lit::Str(value) => Ok(value.value()),
                    _ => anyhow::bail!("proof arguments must be string literals"),
                },
                _ => anyhow::bail!("proof arguments must be string literals"),
            })
            .collect::<Result<Vec<_>>>()?;
        let path = Path::new(RECEIPT_DIRECTORY).join(format!("{name}.json"));
        ensure!(paths.insert(path.clone()), "duplicate proof phase");
        references.push(Reference {
            path,
            digest: hash.into(),
            arguments,
        });
    }
    Ok(references)
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Receipt {
    version: u32,
    command: Vec<String>,
    source_sha256: String,
    artifact_sha256: Option<String>,
    container_image: Option<String>,
    exit: Option<i32>,
    stdout: String,
    stderr: String,
}

fn receipt(text: &str) -> Result<Receipt> {
    ensure!(
        text.len() as u64 <= super::MAX_OUTPUT_BYTES,
        "proof receipt exceeds its byte bound"
    );
    let receipt: Receipt = serde_json::from_str(text)?;
    ensure!(
        receipt.version == RECEIPT_FORMAT
            && digest(&receipt.source_sha256)
            && receipt.artifact_sha256.is_none(),
        "unsupported proof receipt format or source"
    );
    ensure!(
        receipt
            .command
            .first()
            .is_some_and(|value| value == "cargo")
            && receipt.command.get(1).is_some_and(|value| value == "kani"),
        "receipt did not run cargo kani"
    );
    ensure!(
        receipt
            .container_image
            .as_deref()
            .and_then(|value| value.strip_prefix("sha256:"))
            .is_some_and(digest),
        "proof image is not immutable"
    );
    Ok(receipt)
}

fn checker_counts(line: &str) -> Result<Option<(u64, u64, u64)>> {
    let fields = line.split_ascii_whitespace().collect::<Vec<_>>();
    let ["**", failed, "of", total, "failed", rest @ ..] = fields.as_slice() else {
        return Ok(None);
    };
    let unreachable = match rest {
        [] => 0,
        [count, "unreachable)"] => count
            .strip_prefix('(')
            .context("invalid unreachable count")?
            .parse::<u64>()?,
        _ => anyhow::bail!("invalid native checker counts"),
    };
    Ok(Some((
        total.parse::<u64>()?,
        unreachable,
        failed.parse::<u64>()?,
    )))
}

fn completed_counts(line: &str) -> Result<(u64, u64, u64)> {
    let fields = line.split_ascii_whitespace().collect::<Vec<_>>();
    let [
        "Complete",
        "-",
        passed,
        "successfully",
        "verified",
        "harnesses,",
        failed,
        "failures,",
        total,
        "total.",
    ] = fields.as_slice()
    else {
        anyhow::bail!("invalid native Kani summary");
    };
    Ok((
        passed.parse::<u64>()?,
        failed.parse::<u64>()?,
        total.parse::<u64>()?,
    ))
}

fn validate_cover_line(line: &str) -> Result<()> {
    if !line.starts_with("**") || !line.contains("cover properties") {
        return Ok(());
    }
    let fields = line.split_ascii_whitespace().collect::<Vec<_>>();
    let [
        "**",
        satisfied,
        "of",
        total,
        "cover",
        "properties",
        "satisfied",
    ] = fields.as_slice()
    else {
        anyhow::bail!("invalid native Kani cover summary");
    };
    let total: u64 = total.parse()?;
    let satisfied: u64 = satisfied.parse()?;
    let missing = total
        .checked_sub(satisfied)
        .context("cover count exceeds its inventory")?;
    ensure!(
        rules::verification_rules::native_assertion_witness(total, 0, missing, true),
        "native Kani cover properties are incomplete"
    );
    Ok(())
}

fn native_report(receipt: &Receipt) -> Result<(bool, BTreeSet<String>)> {
    let output = format!("{}\n{}", receipt.stdout, receipt.stderr);
    let mut names = BTreeSet::new();
    let mut summary = None;
    let mut witness = None;
    let mut verified = 0usize;
    for line in output.lines().map(str::trim) {
        if receipt.exit == Some(0) {
            validate_cover_line(line)?;
        }
        if let Some(name) = line
            .strip_prefix("Checking harness ")
            .and_then(|value| value.strip_suffix("..."))
        {
            ensure!(
                !name.is_empty()
                    && verified == names.len()
                    && witness.is_none()
                    && names.insert(name.to_owned()),
                "duplicate or empty Kani harness"
            );
        }
        if let Some(counts) = checker_counts(line)? {
            ensure!(
                witness.is_none() && names.len() == verified + 1,
                "duplicate or unrelated checker counts"
            );
            witness = Some(counts);
        }
        if matches!(line, "VERIFICATION:- SUCCESSFUL" | "VERIFICATION:- FAILED") {
            let (total, unreachable, failed) = witness
                .take()
                .context("native result has no checker witness")?;
            ensure!(
                rules::verification_rules::native_assertion_witness(
                    total,
                    unreachable,
                    failed,
                    line == "VERIFICATION:- SUCCESSFUL"
                ),
                "native result has no reachable consistent checker witness"
            );
            verified += 1;
        }
        if line.starts_with("Complete - ") {
            ensure!(summary.is_none(), "duplicate native Kani summary");
            summary = Some(completed_counts(line)?);
        }
    }
    let (passed, failed, total) = summary.context("missing completed native Kani summary")?;
    ensure!(
        !names.is_empty()
            && verified == names.len()
            && witness.is_none()
            && total == u64::try_from(names.len())?
            && passed.checked_add(failed) == Some(total),
        "native Kani inventory is incomplete"
    );
    let positive = receipt.exit == Some(0);
    if positive {
        ensure!(
            failed == 0
                && passed == total
                && output
                    .lines()
                    .filter(|line| line.trim() == "VERIFICATION:- SUCCESSFUL")
                    .count()
                    == names.len(),
            "native Kani proof did not succeed"
        );
    } else {
        ensure!(
            receipt.exit == Some(COUNTEREXAMPLE_EXIT)
                && passed == 0
                && failed == total
                && output
                    .lines()
                    .filter(|line| line.trim() == "VERIFICATION:- FAILED")
                    .count()
                    == names.len()
                && output
                    .lines()
                    .filter(|line| line.trim().starts_with("Failed Checks: assertion failed"))
                    .count()
                    == names.len(),
            "counterexample was not rejected by its assertion"
        );
        for name in &names {
            ensure!(
                output
                    .lines()
                    .any(|line| line.trim() == format!("Verification failed for - {name}")),
                "counterexample has no failed native harness"
            );
        }
    }
    Ok((positive, names))
}

/// # Errors
/// Refuses malformed receipts and incomplete native verification outcomes.
pub fn inspect_receipt(text: &str) -> Result<String> {
    let receipt = receipt(text)?;
    if receipt
        .command
        .iter()
        .any(|argument| argument == "--version")
    {
        ensure!(receipt.exit == Some(0), "Kani version command failed");
    } else {
        native_report(&receipt)?;
    }
    Ok(serde_json::to_string(&receipt)?)
}

/// # Errors
/// Refuses unsafe names, digests, nonliteral arguments, and duplicated phases.
pub fn inspect_phases(text: &str) -> Result<String> {
    let mut output = String::new();
    for reference in references(text)? {
        let name = reference
            .path
            .file_stem()
            .and_then(|value| value.to_str())
            .context("proof phase has no file stem")?;
        writeln!(
            output,
            "{PHASE_PREFIX}name={name} args={:?} source_sha256={}",
            reference.arguments, reference.digest
        )?;
    }
    Ok(output)
}

fn required(expected: &[String], observed: &BTreeSet<String>) -> Result<()> {
    for expected in expected {
        ensure!(
            observed
                .iter()
                .filter(
                    |name| *name == expected || name.rsplit("::").next() == Some(expected.as_str())
                )
                .count()
                == 1,
            "required Kani harness is missing or ambiguous: {expected}"
        );
    }
    Ok(())
}

pub(super) fn validate(check: &Check, references: &[Reference], root: &Path) -> Result<()> {
    let mut passed = BTreeSet::new();
    let mut rejected = BTreeSet::new();
    for reference in references {
        let path = super::source::evidence_path(root, &reference.path)?;
        ensure!(
            std::fs::metadata(&path)?.len() <= super::MAX_OUTPUT_BYTES,
            "proof receipt exceeds its byte bound"
        );
        let receipt = receipt(&std::fs::read_to_string(path)?)?;
        ensure!(
            receipt.source_sha256 == reference.digest
                && receipt.command[1..] == reference.arguments,
            "proof receipt differs from its executed phase"
        );
        if receipt
            .command
            .iter()
            .any(|argument| argument == "--version")
        {
            ensure!(receipt.exit == Some(0), "Kani version command failed");
            continue;
        }
        let (positive, names) = native_report(&receipt)?;
        let destination = if positive { &mut passed } else { &mut rejected };
        for name in names {
            ensure!(
                destination.insert(name),
                "duplicate Kani harness across phases"
            );
        }
    }
    required(&check.expected, &passed)?;
    required(&check.rejecting, &rejected)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn positive() -> serde_json::Value {
        serde_json::json!({"version":3,"command":["cargo","kani"],"source_sha256":"a".repeat(64),"artifact_sha256":null,"container_image":format!("sha256:{}", "b".repeat(64)),"exit":0,"stdout":"Checking harness proofs::bounds...\n ** 0 of 4 failed (1 unreachable)\nVERIFICATION:- SUCCESSFUL\nComplete - 1 successfully verified harnesses, 0 failures, 1 total.\n","stderr":""})
    }

    #[test]
    fn native_receipts_and_phases_round_trip_with_strict_identities() -> Result<()> {
        let value = positive();
        let canonical = inspect_receipt(&value.to_string())?;
        assert_eq!(inspect_receipt(&canonical)?, canonical);
        for (field, invalid) in [
            ("version", 4.into()),
            ("source_sha256", "x".repeat(64).into()),
            ("source_sha256", "a".repeat(63).into()),
            ("command", serde_json::json!(["cargo", "other"])),
            ("command", serde_json::json!(["kani"])),
            ("container_image", "tag".into()),
            ("artifact_sha256", "artifact".into()),
            ("extra", true.into()),
        ] {
            let mut changed = value.clone();
            changed[field] = invalid;
            assert!(inspect_receipt(&changed.to_string()).is_err(), "{field}");
        }
        let mut version = value;
        version["command"] = serde_json::json!(["cargo", "kani", "--version"]);
        version["stdout"] = "Kani Rust Verifier 0.68.0".into();
        inspect_receipt(&version.to_string())?;
        version["exit"] = 1.into();
        assert!(inspect_receipt(&version.to_string()).is_err());
        let phases = format!(
            "ordinary log\nphase=verification name=proof_1-ok args=[\"kani\", \"quoted\\\"name\"] source_sha256={}\n",
            "a".repeat(64)
        );
        let canonical = inspect_phases(&phases)?;
        assert_eq!(inspect_phases(&canonical)?, canonical);
        assert_eq!(references(&canonical)?.len(), 1);
        for invalid in [
            phases.replace("proof_1-ok", ""),
            phases.replace("proof_1-ok", "../escape"),
            phases.replace("args=[", "args=#[cfg(test)] ["),
            phases.replace("args=[", "args=[#[cfg(test)] "),
            phases.replace("[\"kani\", \"quoted\\\"name\"]", "[true]"),
            phases.replace("source_sha256=", "digest="),
            format!("{phases}{phases}"),
        ] {
            assert!(inspect_phases(&invalid).is_err(), "{invalid}");
        }
        assert_eq!(inspect_phases("no verification phases")?, "");
        Ok(())
    }

    #[test]
    fn native_checker_counts_and_names_must_be_complete_and_unambiguous() -> Result<()> {
        let original = positive();
        inspect_receipt(&original.to_string())?;
        let stdout = original["stdout"].as_str().unwrap();
        for (from, to) in [
            ("0 of 4 failed (1 unreachable)", "0 of 4 failed"),
            ("0 of 4 failed (1 unreachable)", "0 of 1 failed"),
        ] {
            let mut value = original.clone();
            value["stdout"] = stdout.replace(from, to).into();
            inspect_receipt(&value.to_string())?;
        }
        for (from, to) in [
            ("Checking harness proofs::bounds...", "Checking harness ..."),
            (
                "Checking harness proofs::bounds...",
                "Checking harness proofs::bounds...\nChecking harness proofs::bounds...",
            ),
            (
                "0 of 4 failed (1 unreachable)",
                "0 of 4 failed (5 unreachable)",
            ),
            (
                "0 of 4 failed (1 unreachable)",
                "0 of 4 failed (x unreachable)",
            ),
            ("0 of 4 failed (1 unreachable)", "0 of 4 failed extra"),
            (
                " ** 0 of 4 failed (1 unreachable)",
                " ** 0 of 4 failed\n ** 0 of 4 failed",
            ),
            (
                "VERIFICATION:- SUCCESSFUL",
                "VERIFICATION:- SUCCESSFUL\nVERIFICATION:- SUCCESSFUL",
            ),
            (
                "1 total.",
                "1 total.\nComplete - 1 successfully verified harnesses, 0 failures, 1 total.",
            ),
            ("Complete - 1", "Complete - 0"),
            ("Complete - 1", "Incomplete - 1"),
        ] {
            let mut value = original.clone();
            value["stdout"] = stdout.replace(from, to).into();
            assert!(inspect_receipt(&value.to_string()).is_err(), "{to}");
        }
        let names = BTreeSet::from(["proofs::bounds".into(), "proofs::state".into()]);
        required(&["bounds".into(), "proofs::state".into()], &names)?;
        assert!(required(&["absent".into()], &names).is_err());
        let names = BTreeSet::from(["one::bounds".into(), "two::bounds".into()]);
        assert!(required(&["bounds".into()], &names).is_err());
        Ok(())
    }

    #[test]
    fn native_success_requires_every_declared_cover_property() {
        let original = positive();
        let stdout = original["stdout"].as_str().unwrap();
        for (cover, admitted) in [
            (" ** 1 of 1 cover properties satisfied", true),
            (" ** 2 of 2 cover properties satisfied", true),
            (" ** 0 of 1 cover properties satisfied", false),
            (" ** 1 of 2 cover properties satisfied", false),
            (" ** 0 of 0 cover properties satisfied", false),
            (" ** 2 of 1 cover properties satisfied", false),
            (" ** x of 1 cover properties satisfied", false),
        ] {
            let mut receipt = original.clone();
            receipt["stdout"] = stdout
                .replace("VERIFICATION:-", &format!("{cover}\nVERIFICATION:-"))
                .into();
            assert_eq!(
                inspect_receipt(&receipt.to_string()).is_ok(),
                admitted,
                "{cover}"
            );
        }
    }

    #[test]
    fn counterexamples_require_the_verifiers_assertion_failure_exit() {
        let mut receipt = positive();
        receipt["stdout"] = "Checking harness proofs::bounds...\n ** 1 of 1 failed\nFailed Checks: assertion failed: false\nVERIFICATION:- FAILED\nVerification failed for - proofs::bounds\nComplete - 0 successfully verified harnesses, 1 failures, 1 total.\n".into();
        for exit in [
            None,
            Some(i32::MIN),
            Some(-1),
            Some(0),
            Some(1),
            Some(2),
            Some(10),
            Some(101),
            Some(124),
            Some(137),
            Some(143),
            Some(i32::MAX),
        ] {
            receipt["exit"] = serde_json::json!(exit);
            assert_eq!(
                inspect_receipt(&receipt.to_string()).is_ok(),
                exit == Some(1),
                "{exit:?}"
            );
        }
    }
}
