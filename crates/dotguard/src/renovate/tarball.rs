use super::Found;
use super::report::Record;
use super::report::{Dependency, Document};
use rules::freshness_rules::{
    DeclarationBinding, LookupFacts, TarballFacts, VersionSelection, lookup_evidence,
    verified_tarball,
};

const SHEETJS_CDN: &str = "https://cdn.sheetjs.com/xlsx-";
const SHEETJS_REPOSITORY: &str = "https://git.sheetjs.com/SheetJS/sheetjs.git";
const DEPENDENCY_SECTIONS: &[&str] = &[
    "dependencies",
    "devDependencies",
    "optionalDependencies",
    "peerDependencies",
];
const VERSION_COMPONENTS: usize = 3;
const MAX_VERSION_BYTES: usize = 64;

#[must_use]
pub fn official_tarball_version<'a>(name: &str, url: &'a str) -> Option<&'a str> {
    if name != "xlsx" {
        return None;
    }
    let (version, file) = url.strip_prefix(SHEETJS_CDN)?.split_once("/xlsx-")?;
    if version.len() > MAX_VERSION_BYTES || file.strip_suffix(".tgz")? != version {
        return None;
    }
    let parts: Vec<_> = version.split('.').collect();
    if parts.len() != VERSION_COMPONENTS
        || parts.iter().any(|part| {
            part.is_empty()
                || !part.bytes().all(|byte| byte.is_ascii_digit())
                || (part.len() > 1 && part.starts_with('0'))
                || part.parse::<u64>().is_err()
        })
    {
        return None;
    }
    Some(version)
}

fn replacement_matches(dependency: &Dependency, name: &str, url: &str, source: &str) -> bool {
    let Some(replacement) = dependency.replace_string.as_deref() else {
        return false;
    };
    if source.matches(replacement).count() != 1 {
        return false;
    }
    let Ok(Document(value)) = serde_json::from_str(&format!("{{{replacement}}}")) else {
        return false;
    };
    value.as_object().is_some_and(|object| {
        object.len() == 1 && object.get(name).and_then(serde_json::Value::as_str) == Some(url)
    })
}

pub(super) fn covers(records: &[Record], record: &Record, found: Option<&Found>) -> bool {
    let Some(source) = super::mise::source(found, &record.file) else {
        return false;
    };
    binds(records, record, &source)
}

fn binds(records: &[Record], record: &Record, source: &str) -> bool {
    let dependency = &record.dependency;
    if !dependency.enabled
        || !dependency.warnings.is_empty()
        || !dependency.updates.is_empty()
        || dependency.current_digest.is_some()
    {
        return false;
    }
    if record.manager != "npm"
        || dependency.skip_reason.as_deref() != Some("unspecified-version")
        || dependency.package_name.as_deref() != Some(dependency.dep_name.as_str())
    {
        return false;
    }
    let Some(url) = dependency.current_value.as_deref() else {
        return false;
    };
    let Some(version) = official_tarball_version(&dependency.dep_name, url) else {
        return false;
    };
    let Ok(Document(manifest)) = serde_json::from_str(source) else {
        return false;
    };
    let Some(section) = dependency
        .dep_type
        .as_deref()
        .filter(|section| DEPENDENCY_SECTIONS.contains(section))
    else {
        return false;
    };
    let declared = manifest
        .get(section)
        .and_then(|section| section.get(&dependency.dep_name))
        .and_then(serde_json::Value::as_str)
        == Some(url)
        && DEPENDENCY_SECTIONS
            .iter()
            .filter(|section| {
                manifest
                    .get(**section)
                    .and_then(|section| section.get(&dependency.dep_name))
                    .is_some()
            })
            .count()
            == 1;
    let matching: Vec<_> = records
        .iter()
        .filter(|candidate| {
            candidate.manager == "regex"
                && candidate.file == record.file
                && replacement_matches(&candidate.dependency, &dependency.dep_name, url, source)
        })
        .collect();
    let Some(verified) = matching.first().map(|record| &record.dependency) else {
        return false;
    };
    verified_tarball(TarballFacts {
        declaration: if declared {
            DeclarationBinding::Bound
        } else {
            DeclarationBinding::Unbound
        },
        unique: matching.len() == 1,
        source: verified.datasource.as_deref() == Some("git-tags")
            && verified.package_name.as_deref() == Some(SHEETJS_REPOSITORY),
        version: dependency.locked_version.as_deref() == Some(version)
            && verified.current_value.as_deref() == Some(version)
            && verified.current_version.as_deref() == Some(version),
        lookup: lookup_evidence(LookupFacts {
            skip: super::report::skip(verified),
            selection: VersionSelection::Present,
            updates: !verified.updates.is_empty(),
            warnings: !verified.warnings.is_empty(),
            covered: false,
        }),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::collections::BTreeMap;

    const SOURCE: &str =
        r#"{"devDependencies":{"xlsx":"https://cdn.sheetjs.com/xlsx-0.20.3/xlsx-0.20.3.tgz"}}"#;

    fn record(manager: &str, dependency: serde_json::Value) -> Record {
        Record {
            manager: manager.into(),
            file: "package.json".into(),
            dependency: serde_json::from_value(dependency).unwrap(),
            lock_files: Some(vec!["package-lock.json".into()]),
        }
    }

    fn records() -> Vec<Record> {
        vec![
            record(
                "npm",
                json!({"depName":"xlsx","currentValue":"https://cdn.sheetjs.com/xlsx-0.20.3/xlsx-0.20.3.tgz","lockedVersion":"0.20.3","packageName":"xlsx","depType":"devDependencies","skipReason":"unspecified-version","updates":[]}),
            ),
            record(
                "regex",
                json!({"depName":"sheetjs","packageName":"https://git.sheetjs.com/SheetJS/sheetjs.git","datasource":"git-tags","currentValue":"0.20.3","currentVersion":"0.20.3","replaceString":"\"xlsx\":\"https://cdn.sheetjs.com/xlsx-0.20.3/xlsx-0.20.3.tgz\"","updates":[],"warnings":[]}),
            ),
        ]
    }

    #[test]
    fn a_verified_official_tarball_lookup_binds_its_exact_manifest_declaration() {
        let records = records();
        assert!(binds(&records, &records[0], SOURCE));
        assert!(!covers(&records, &records[0], None));
    }

    #[test]
    fn missing_ambiguous_or_unverified_lookups_never_cover_the_declaration() {
        let baseline = records();
        assert!(!binds(&baseline[..1], &baseline[0], SOURCE));
        let mut duplicate = records();
        duplicate.push(records().remove(1));
        assert!(!binds(&duplicate, &duplicate[0], SOURCE));
        for change in 0..8 {
            let mut records = records();
            let verified = &mut records[1].dependency;
            match change {
                0 => verified.current_version = Some("0.20.2".into()),
                1 => verified.current_value = Some("0.20.4".into()),
                2 => verified.package_name = Some("https://attacker.invalid/sheetjs.git".into()),
                3 => verified.skip_reason = Some("lookup-failed".into()),
                4 => verified.warnings.push(json!("unverified")),
                5 => verified.enabled = false,
                6 => records[1].file = "different/package.json".into(),
                7 => records[0].dependency.locked_version = Some("0.18.5".into()),
                _ => unreachable!(),
            }
            assert!(!binds(&records, &records[0], SOURCE), "control {change}");
        }
    }

    #[test]
    fn source_changes_duplicate_members_and_unrelated_replacements_fail_closed() {
        let records = records();
        for source in [
            SOURCE.replace("0.20.3", "0.20.2"),
            SOURCE.replace("cdn.sheetjs.com", "attacker.invalid"),
            SOURCE.replace("\"devDependencies\"", "\"dependencies\""),
            SOURCE.replace(
                "{\"devDependencies\":",
                "{\"devDependencies\":{},\"devDependencies\":",
            ),
        ] {
            assert!(!binds(&records, &records[0], &source));
        }
        let mut wrong_replacement = self::records();
        wrong_replacement[1].dependency.replace_string =
            Some("\"different\":\"https://cdn.sheetjs.com/xlsx-0.20.3/xlsx-0.20.3.tgz\"".into());
        assert!(!binds(&wrong_replacement, &wrong_replacement[0], SOURCE));
        for url in [
            "https://cdn.sheetjs.com/xlsx-0.20.3/xlsx-0.20.2.tgz",
            "https://cdn.sheetjs.com/xlsx-0.20.3/xlsx-0.20.3.tgz?source=other",
            "https://cdn.sheetjs.com/xlsx-0.20.3/xlsx-0.20.3.tgz#different",
            "https://cdn.sheetjs.com/xlsx-00.20.3/xlsx-00.20.3.tgz",
        ] {
            assert!(official_tarball_version("xlsx", url).is_none());
        }
    }

    #[test]
    fn verified_update_candidates_remain_visible_to_the_freshness_gate() {
        let mut records = records();
        records[1].dependency.updates =
            serde_json::from_value(json!([{"updateType":"patch","newValue":"0.20.4"}])).unwrap();
        assert!(binds(&records, &records[0], SOURCE));
        assert_eq!(records[1].dependency.updates.len(), 1);
    }

    #[test]
    fn push_evidence_reads_the_offered_blob_and_rejects_a_stale_counterpart() {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path().to_str().unwrap();
        let git = |arguments: &[&str]| {
            let mut words = vec!["-C", root];
            words.extend(arguments);
            crate::real_git::capture(&words).unwrap().trim().to_owned()
        };
        git(&["init", "--initial-branch=main", "--template="]);
        git(&["config", "user.name", "Fixture"]);
        git(&["config", "user.email", "fixture@example.invalid"]);
        let manifest = directory.path().join("package.json");
        std::fs::write(&manifest, SOURCE).unwrap();
        git(&["add", "package.json"]);
        let tree = git(&["write-tree"]);
        let offered = git(&["commit-tree", &tree, "-m", "offered"]);
        let found = Found {
            repository: root.to_owned(),
            changed_files: vec!["package.json".to_owned()],
            added_by_file: BTreeMap::default(),
            baselines_by_file: BTreeMap::default(),
            offered: Some(offered),
            scope_unknown: false,
        };
        let dependencies = records();
        let report = json!({"repositories":{"local":{"packageFiles":{
            "npm":[{"packageFile":"package.json","deps":[
                {"depName":"xlsx","currentValue":"https://cdn.sheetjs.com/xlsx-0.20.3/xlsx-0.20.3.tgz","lockedVersion":"0.20.3","packageName":"xlsx","depType":"devDependencies","skipReason":"unspecified-version","updates":[]}
            ]}],
            "regex":[{"packageFile":"package.json","deps":[
                {"depName":"sheetjs","packageName":SHEETJS_REPOSITORY,"datasource":"git-tags","currentValue":"0.20.3","currentVersion":"0.20.3","replaceString":"\"xlsx\":\"https://cdn.sheetjs.com/xlsx-0.20.3/xlsx-0.20.3.tgz\"","updates":[],"warnings":[]}
            ]}]
        }}}});
        let bytes = serde_json::to_vec(&report).unwrap();
        assert!(
            super::super::report::parse_for_push(&bytes, "minor patch pin pinDigest", &found)
                .is_ok()
        );
        std::fs::write(&manifest, SOURCE.replace("0.20.3", "0.20.4")).unwrap();
        git(&["add", "package.json"]);
        assert!(
            super::super::report::parse_for_push(&bytes, "minor patch pin pinDigest", &found)
                .is_ok()
        );
        assert!(covers(&dependencies, &dependencies[0], Some(&found)));
        let changed_tree = git(&["write-tree"]);
        let changed = git(&["commit-tree", &changed_tree, "-m", "changed"]);
        let stale = Found {
            offered: Some(changed),
            ..found
        };
        assert!(
            super::super::report::parse_for_push(&bytes, "minor patch pin pinDigest", &stale)
                .is_err()
        );
    }
}
