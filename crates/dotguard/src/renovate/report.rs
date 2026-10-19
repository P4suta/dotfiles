//! Validated Renovate evidence, independent of the dependency manager.

use super::Update;
use rules::freshness_rules::{
    LookupEvidence, LookupFacts, LookupSkip, ProblemSeverity, VersionSelection, lookup_evidence,
    problem_severity,
};
use serde::de::{Deserialize, Deserializer, Error, MapAccess, SeqAccess, Visitor};
use serde_json::{Map, Number, Value};
use std::fmt;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CheckedReport {
    pub(super) updates: Vec<Update>,
    pub(super) notices: usize,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReportError {
    location: String,
    reason: &'static str,
}

impl fmt::Display for ReportError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}: {}", self.location, self.reason)
    }
}

impl std::error::Error for ReportError {}

fn invalid(location: impl Into<String>, reason: &'static str) -> ReportError {
    ReportError {
        location: location.into(),
        reason,
    }
}

#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct Dependency {
    pub(super) dep_name: String,
    pub(super) updates: Vec<Candidate>,
    #[serde(default)]
    pub(super) warnings: Vec<Value>,
    #[serde(default = "enabled")]
    pub(super) enabled: bool,
    pub(super) current_value: Option<String>,
    pub(super) current_digest: Option<String>,
    pub(super) locked_version: Option<String>,
    pub(super) current_version: Option<String>,
    replace_string: Option<String>,
    pub(super) skip_reason: Option<String>,
    pub(super) datasource: Option<String>,
    pub(super) package_name: Option<String>,
    pub(super) dep_type: Option<String>,
    pub(super) target: Option<String>,
    pub(super) registry_urls: Option<Vec<String>>,
}

const fn enabled() -> bool {
    true
}

#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct Candidate {
    #[serde(default, deserialize_with = "update_kind")]
    update_type: Option<String>,
    #[serde(default, deserialize_with = "update_kind")]
    bucket: Option<String>,
    #[serde(default)]
    is_breaking: bool,
    #[serde(default)]
    pending_checks: bool,
    #[serde(default)]
    is_lockfile_update: bool,
    new_value: Option<String>,
    new_version: Option<String>,
    new_digest: Option<String>,
}

fn update_kind<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Option<String>, D::Error> {
    let kind = String::deserialize(deserializer)?;
    if kind.is_empty() {
        return Err(D::Error::custom("empty update classification"));
    }
    Ok(Some(kind))
}

#[derive(Debug)]
pub(super) struct Record {
    pub(super) manager: String,
    pub(super) file: String,
    pub(super) dependency: Dependency,
    pub(super) lock_files: Option<Vec<String>>,
}

impl Record {
    fn location(&self) -> String {
        format!(
            "{:?} / {:?} / {:?}",
            self.manager, self.file, self.dependency.dep_name
        )
    }
}

/// Validate every record before applying exclusions or update-type filters.
///
/// # Errors
/// Rejects malformed reports, error-level problems, and unverified dependency lookups.
pub fn parse(bytes: &[u8], types: &str) -> Result<CheckedReport, ReportError> {
    parse_inner(bytes, types, None)
}

pub(super) fn parse_for_push(
    bytes: &[u8],
    types: &str,
    found: &super::Found,
) -> Result<CheckedReport, ReportError> {
    parse_inner(bytes, types, Some(found))
}

fn parse_inner(
    bytes: &[u8],
    types: &str,
    found: Option<&super::Found>,
) -> Result<CheckedReport, ReportError> {
    let Document(document) = serde_json::from_slice(bytes)
        .map_err(|_| invalid("report", "invalid JSON or duplicate fields"))?;
    let root = document
        .as_object()
        .ok_or_else(|| invalid("report", "expected an object"))?;
    let local = root
        .get("repositories")
        .and_then(Value::as_object)
        .and_then(|repositories| repositories.get("local"))
        .and_then(Value::as_object)
        .ok_or_else(|| invalid("repositories.local", "missing repository evidence"))?;
    let notices = problems(root.get("problems"), "problems")?
        + problems(local.get("problems"), "repositories.local.problems")?;
    let managers = local
        .get("packageFiles")
        .and_then(Value::as_object)
        .ok_or_else(|| invalid("repositories.local.packageFiles", "expected an object"))?;
    let records = dependency_records(managers)?;
    let bindings =
        super::cargo::bind(&records, found).map_err(|reason| invalid("Cargo workspace", reason))?;
    let mut updates = Vec::new();
    for (index, record) in records.iter().enumerate() {
        if bindings.inherited.contains(&index) {
            continue;
        }
        let dependency = &record.dependency;
        let skip = skip(dependency);
        let facts = LookupFacts {
            skip,
            selection: if dependency.current_value.is_some()
                || dependency.current_digest.is_some()
                || dependency.locked_version.is_some()
            {
                VersionSelection::Present
            } else {
                VersionSelection::Absent
            },
            updates: !dependency.updates.is_empty(),
            warnings: !dependency.warnings.is_empty(),
            covered: covered(&records, record),
        };
        match lookup_evidence(facts) {
            LookupEvidence::Missing => {
                return Err(invalid(record.location(), lookup_failure(dependency)));
            }
            LookupEvidence::NotApplicable => continue,
            LookupEvidence::Verified => {}
        }
        for candidate in &dependency.updates {
            let kind = candidate
                .update_type
                .as_deref()
                .or(candidate.bucket.as_deref())
                .ok_or_else(|| invalid(record.location(), "missing update classification"))?;
            if candidate.pending_checks
                || !types.split_whitespace().any(|blocking| blocking == kind)
            {
                continue;
            }
            updates.push(Update {
                package_file: record.file.clone(),
                dep_name: dependency.dep_name.clone(),
                current_value: dependency.current_value.clone().unwrap_or_default(),
                resolved_version: if candidate.is_lockfile_update
                    || dependency.current_value.is_none()
                {
                    dependency
                        .locked_version
                        .clone()
                        .or_else(|| dependency.current_version.clone())
                        .unwrap_or_default()
                } else {
                    String::new()
                },
                current_digest: dependency.current_digest.clone().unwrap_or_default(),
                replace_string: dependency.replace_string.clone().unwrap_or_default(),
                kind: kind.to_owned(),
                new_value: if candidate.is_lockfile_update {
                    candidate
                        .new_version
                        .clone()
                        .or_else(|| candidate.new_value.clone())
                } else {
                    candidate
                        .new_value
                        .clone()
                        .or_else(|| candidate.new_version.clone())
                }
                .unwrap_or_default(),
                new_digest: candidate.new_digest.clone().unwrap_or_default(),
                breaking: candidate.is_breaking,
                inherited_touched: bindings.touched.contains(&index),
            });
        }
    }
    Ok(CheckedReport { updates, notices })
}

fn dependency_records(managers: &Map<String, Value>) -> Result<Vec<Record>, ReportError> {
    let mut records = Vec::new();
    for (manager, files) in managers {
        let files = files
            .as_array()
            .ok_or_else(|| invalid(format!("manager {manager:?}"), "expected package files"))?;
        for file in files {
            let file = file
                .as_object()
                .ok_or_else(|| invalid(format!("manager {manager:?}"), "invalid package file"))?;
            let name = file
                .get("packageFile")
                .and_then(Value::as_str)
                .filter(|name| !name.is_empty())
                .ok_or_else(|| invalid(format!("manager {manager:?}"), "missing packageFile"))?;
            let dependencies = file
                .get("deps")
                .and_then(Value::as_array)
                .ok_or_else(|| invalid(format!("{manager} / {name:?}"), "expected dependencies"))?;
            for dependency in dependencies {
                let name_hint = dependency
                    .get("depName")
                    .and_then(Value::as_str)
                    .unwrap_or("<missing>");
                let location = format!("{manager} / {name:?} / {name_hint:?}");
                let dependency: Dependency = serde_json::from_value(dependency.clone())
                    .map_err(|_| invalid(location.clone(), "invalid dependency field types"))?;
                if dependency.dep_name.is_empty()
                    || dependency.updates.iter().any(|candidate| {
                        candidate.update_type.is_none() && candidate.bucket.is_none()
                    })
                {
                    return Err(invalid(
                        location,
                        "missing dependency name or update classification",
                    ));
                }
                records.push(Record {
                    manager: manager.clone(),
                    file: name.to_owned(),
                    dependency,
                    lock_files: file
                        .get("lockFiles")
                        .map(|value| serde_json::from_value(value.clone()))
                        .transpose()
                        .map_err(|_| invalid(location, "invalid lockfile ownership"))?,
                });
            }
        }
    }
    Ok(records)
}

fn problems(value: Option<&Value>, location: &str) -> Result<usize, ReportError> {
    let Some(value) = value else {
        return Ok(0);
    };
    let problems = value
        .as_array()
        .ok_or_else(|| invalid(location, "expected problem records"))?;
    for problem in problems {
        let level = problem
            .as_object()
            .and_then(|problem| problem.get("level"))
            .and_then(Value::as_u64)
            .ok_or_else(|| invalid(location, "invalid problem severity"))?;
        match problem_severity(level) {
            ProblemSeverity::Notice => {}
            ProblemSeverity::Error => return Err(invalid(location, "Renovate reported an error")),
            ProblemSeverity::Invalid => return Err(invalid(location, "unknown problem severity")),
        }
    }
    Ok(problems.len())
}

fn skip(dependency: &Dependency) -> LookupSkip {
    if !dependency.enabled
        || matches!(
            dependency.skip_reason.as_deref(),
            Some(
                "path-dependency"
                    | "internal-package"
                    | "file-dependency"
                    | "file"
                    | "local-dependency"
                    | "local"
                    | "local-chart"
                    | "ignored"
                    | "disabled"
                    | "is-pinned"
            )
        )
    {
        return LookupSkip::Excluded;
    }
    match dependency.skip_reason.as_deref() {
        None => LookupSkip::None,
        Some("invalid-value") => LookupSkip::InvalidValue,
        Some("unspecified-version") => LookupSkip::UnspecifiedVersion,
        Some(_) => LookupSkip::Other,
    }
}

fn lookup_failure(dependency: &Dependency) -> &'static str {
    match dependency.skip_reason.as_deref() {
        Some("invalid-value") => "invalid-value",
        Some("invalid-version") => "invalid-version",
        Some("unspecified-version") => "unspecified-version",
        Some("github-token-required") => "github-token-required",
        Some("unsupported-datasource") => "unsupported-datasource",
        Some("unknown-registry") => "unknown-registry",
        Some("internal-error") => "internal-error",
        Some(_) => "unverified skip reason",
        None => "dependency lookup warnings",
    }
}

fn covered(records: &[Record], record: &Record) -> bool {
    let dependency = &record.dependency;
    if !dependency.updates.is_empty() || !dependency.warnings.is_empty() {
        return false;
    }
    let hosted = dependency.datasource.as_deref() == Some("github-runners")
        && dependency.dep_type.as_deref() == Some("github-runner")
        && matches!(dependency.dep_name.as_str(), "ubuntu" | "windows")
        && dependency.current_value.as_deref() == Some("latest")
        && dependency.skip_reason.as_deref() == Some("invalid-version");
    let matrix = record.manager == "github-actions"
        && dependency.dep_type.as_deref() == Some("uses-with")
        && dependency.skip_reason.as_deref() == Some("invalid-value")
        && dependency
            .current_value
            .as_deref()
            .is_some_and(matrix_input);
    hosted || matrix || covered_backend(records, record) || covered_ci_alias(records, record)
}

fn matrix_input(value: &str) -> bool {
    value
        .strip_prefix("${{")
        .and_then(|value| value.strip_suffix("}}"))
        .map(|value| value.trim_matches(' '))
        .and_then(|value| value.strip_prefix("matrix."))
        .is_some_and(|name| {
            !name.is_empty()
                && name
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
        })
}

fn counterpart<'a>(records: &'a [Record], file: &str, name: &str) -> Option<&'a Dependency> {
    let mut matches = records.iter().filter(|record| {
        record.manager == "regex" && record.file == file && record.dependency.dep_name == name
    });
    let found = &matches.next()?.dependency;
    if matches.next().is_some() {
        return None;
    }
    let nonempty = |value: &Option<String>| value.as_deref().is_some_and(|value| !value.is_empty());
    (nonempty(&found.current_value)
        && nonempty(&found.current_version)
        && found.skip_reason.is_none()
        && skip(found) != LookupSkip::Excluded
        && found.warnings.is_empty())
    .then_some(found)
}

fn covered_backend(records: &[Record], record: &Record) -> bool {
    let dependency = &record.dependency;
    if record.manager != "mise"
        || dependency.skip_reason.as_deref() != Some("unsupported-datasource")
    {
        return false;
    }
    let provider = match dependency.dep_name.as_str() {
        "nu" => ("github-releases", "nushell/nushell"),
        "conda:zsh" => ("conda", "conda-forge/zsh"),
        "pypi:ansible-core" => ("pypi", "ansible-core"),
        "cargo-deny" => ("crate", "cargo-deny"),
        _ => return false,
    };
    let Some(verified) = counterpart(records, &record.file, &dependency.dep_name) else {
        return false;
    };
    dependency
        .locked_version
        .as_deref()
        .is_some_and(|version| !version.is_empty())
        && verified.current_value == dependency.locked_version
        && verified.datasource.as_deref() == Some(provider.0)
        && verified.package_name.as_deref() == Some(provider.1)
}

fn covered_ci_alias(records: &[Record], record: &Record) -> bool {
    let alias = &record.dependency;
    if record.manager != "github-actions"
        || record.file != ".github/workflows/ci.yml"
        || alias.datasource.as_deref() != Some("github-release-attachments")
        || alias.dep_type.as_deref() != Some("uses-with")
        || alias.dep_name != "jdx/mise"
        || alias.package_name.as_deref() != Some("jdx/mise")
        || alias.current_value.as_deref() != Some("${{ steps.mise.outputs.version }}")
        || alias.skip_reason.as_deref() != Some("invalid-value")
    {
        return false;
    }
    counterpart(records, "home/.chezmoidata/versions.toml", "jdx/mise").is_some_and(|verified| {
        verified.datasource.as_deref() == Some("github-releases")
            && verified.package_name.as_deref() == Some("jdx/mise")
    })
}

/// Duplicate keys must not replace evidence that would refuse a push.
struct Document(Value);

impl<'de> Deserialize<'de> for Document {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserializer.deserialize_any(DocumentVisitor)
    }
}

struct DocumentVisitor;

impl<'de> Visitor<'de> for DocumentVisitor {
    type Value = Document;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("a JSON document with unique fields")
    }

    fn visit_bool<E: Error>(self, value: bool) -> Result<Document, E> {
        Ok(Document(Value::Bool(value)))
    }
    fn visit_i64<E: Error>(self, value: i64) -> Result<Document, E> {
        Ok(Document(Value::Number(value.into())))
    }
    fn visit_u64<E: Error>(self, value: u64) -> Result<Document, E> {
        Ok(Document(Value::Number(value.into())))
    }
    fn visit_f64<E: Error>(self, value: f64) -> Result<Document, E> {
        Number::from_f64(value)
            .map(|number| Document(Value::Number(number)))
            .ok_or_else(|| E::custom("invalid number"))
    }
    fn visit_str<E: Error>(self, value: &str) -> Result<Document, E> {
        Ok(Document(Value::String(value.to_owned())))
    }
    fn visit_string<E: Error>(self, value: String) -> Result<Document, E> {
        Ok(Document(Value::String(value)))
    }
    fn visit_none<E: Error>(self) -> Result<Document, E> {
        Ok(Document(Value::Null))
    }
    fn visit_unit<E: Error>(self) -> Result<Document, E> {
        Ok(Document(Value::Null))
    }
    fn visit_seq<A: SeqAccess<'de>>(self, mut access: A) -> Result<Document, A::Error> {
        let mut values = Vec::new();
        while let Some(Document(value)) = access.next_element()? {
            values.push(value);
        }
        Ok(Document(Value::Array(values)))
    }
    fn visit_map<A: MapAccess<'de>>(self, mut access: A) -> Result<Document, A::Error> {
        let mut values = Map::new();
        while let Some((key, Document(value))) = access.next_entry::<String, Document>()? {
            if values.insert(key, value).is_some() {
                return Err(A::Error::custom("duplicate JSON field"));
            }
        }
        Ok(Document(Value::Object(values)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn dependency_report(dependency: Value) -> Value {
        let mut report = json!({"repositories":{"local":{"packageFiles":{"future-manager":[{"packageFile":"manifest","deps":[]}]}}}});
        report["repositories"]["local"]["packageFiles"]["future-manager"][0]["deps"] =
            Value::Array(vec![dependency]);
        report
    }

    fn check(value: &Value) -> Result<CheckedReport, ReportError> {
        parse(
            &serde_json::to_vec(value).unwrap(),
            "minor patch pin pinDigest",
        )
    }

    #[test]
    fn the_pinned_producers_report_preserves_real_updates_and_nonfatal_notices() {
        let checked = parse(
            include_bytes!("../../tests/fixtures/renovate-local-44.52.json"),
            "minor patch pin pinDigest",
        )
        .unwrap();
        assert_eq!(checked.notices, 1);
        assert_eq!(checked.updates.len(), 1);
        let update = &checked.updates[0];
        assert_eq!(update.dep_name, "bootsnap");
        assert_eq!(update.current_value, "");
        assert_eq!(update.resolved_version, "1.17.0");
        assert_eq!(update.new_value, "1.26.0");
    }

    #[test]
    fn absence_includes_null_but_never_an_empty_or_invalid_selected_version() {
        for reason in ["invalid-value", "unspecified-version"] {
            let base = json!({"depName":"unversioned","skipReason":reason,"updates":[]});
            assert!(check(&dependency_report(base.clone())).is_ok());
            for field in ["currentValue", "currentDigest", "lockedVersion"] {
                let mut dependency = base.clone();
                dependency[field] = Value::Null;
                assert!(check(&dependency_report(dependency.clone())).is_ok());
                for selected in [
                    json!(""),
                    json!("invalid"),
                    json!(false),
                    json!(1),
                    json!([]),
                ] {
                    dependency[field] = selected;
                    assert!(
                        check(&dependency_report(dependency.clone())).is_err(),
                        "{field}"
                    );
                }
            }
        }
    }

    #[test]
    fn duplicate_fields_cannot_replace_a_refusal_with_current_evidence() {
        for bytes in [
            br#"{"problems":[{"level":50}],"problems":[],"repositories":{"local":{"packageFiles":{}}}}"#.as_slice(),
            br#"{"repositories":{"local":{"packageFiles":{"cargo":[{"packageFile":"Cargo.toml","deps":[{"depName":"x","updates":[],"skipReason":"github-token-required","skipReason":null}]}]}}}}"#.as_slice(),
            br#"{"repositories":{"local":{"packageFiles":{"cargo":false,"cargo":[]}}}}"#.as_slice(),
        ] {
            assert!(parse(bytes, "patch").is_err());
        }
    }

    #[test]
    fn malformed_fields_are_checked_before_exclusions_and_pending_or_major_filters() {
        for reason in [Value::Null, json!("path-dependency")] {
            let base = json!({"depName":"x","skipReason":reason,"updates":[{"updateType":"major","pendingChecks":true}]});
            for (field, malformed) in [
                ("updateType", Value::Null),
                ("bucket", json!("")),
                ("pendingChecks", json!("true")),
                ("isBreaking", Value::Null),
                ("newValue", json!({})),
                ("newVersion", json!(false)),
                ("newDigest", json!(1)),
            ] {
                let mut dependency = base.clone();
                dependency["updates"][0][field] = malformed;
                assert!(check(&dependency_report(dependency)).is_err(), "{field}");
            }
        }
    }

    #[test]
    fn all_declared_problem_levels_are_checked_at_both_report_scopes() {
        for global in [false, true] {
            for level in [
                json!(10),
                json!(20),
                json!(30),
                json!(40),
                json!(50),
                json!(60),
                json!(41),
                json!(-1),
                json!(40.0),
                json!("40"),
                Value::Null,
            ] {
                let admitted = matches!(level.as_u64(), Some(10 | 20 | 30 | 40));
                let mut report = dependency_report(json!({"depName":"x","updates":[]}));
                let problems = if global {
                    &mut report["problems"]
                } else {
                    &mut report["repositories"]["local"]["problems"]
                };
                *problems = json!([{"level":level,"msg":"private-payload"}]);
                let result = check(&report);
                assert_eq!(result.is_ok(), admitted);
                if let Err(error) = result {
                    assert!(!error.to_string().contains("private-payload"));
                }
            }
        }
    }

    #[test]
    fn direct_json_preserves_delimiters_and_ignores_unconsumed_metadata() {
        let mut report = dependency_report(
            json!({"depName":"x","currentValue":"1","replaceString":"x\n=\u{1f}1","updates":[{"updateType":"patch","newValue":"2"}]}),
        );
        let before = check(&report).unwrap();
        assert_eq!(before.updates[0].replace_string, "x\n=\u{1f}1");
        report["unused"] = json!({"futureSchema":true});
        assert_eq!(check(&report).unwrap(), before);
    }
}
