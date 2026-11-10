use super::super::report::{Document, Record};
use super::super::{Found, mise};
use serde_json::Value;

const PACKAGE_MANIFEST: &str = "package.json";
const PACKAGE_LOCKFILE: &str = "package-lock.json";
const SHRINKWRAP_LOCKFILE: &str = "npm-shrinkwrap.json";
const LOCKFILE_NAMES: &[&str] = &[PACKAGE_LOCKFILE, SHRINKWRAP_LOCKFILE];
const TREE_LOCKFILE_VERSIONS: &[u64] = &[2, 3];
const NODE_MODULES: &str = "node_modules";

fn directory<'a>(path: &'a str, names: &[&str]) -> Option<&'a str> {
    let (parent, name) = path.rsplit_once('/').unwrap_or(("", path));
    if path.starts_with('/')
        || !names.contains(&name)
        || (!parent.is_empty()
            && parent
                .split('/')
                .any(|part| part.is_empty() || matches!(part, "." | "..") || part.contains('\\')))
    {
        return None;
    }
    Some(parent)
}

pub(super) fn version(found: Option<&Found>, record: &Record, manifest: &str) -> Option<String> {
    if record.manager != "npm" {
        return None;
    }
    let url = record.dependency.current_value.as_deref()?;
    let version = super::official_tarball_version(&record.dependency.dep_name, url)?;
    let [lockfile] = record.lock_files.as_deref()? else {
        return None;
    };
    let root = directory(lockfile, LOCKFILE_NAMES)?;
    if lockfile.rsplit('/').next() == Some(PACKAGE_LOCKFILE) {
        let found = found?;
        let shrinkwrap = if root.is_empty() {
            SHRINKWRAP_LOCKFILE.into()
        } else {
            format!("{root}/{SHRINKWRAP_LOCKFILE}")
        };
        let present = crate::real_git::capture(&[
            "-C",
            &found.repository,
            "ls-tree",
            "--full-tree",
            "--name-only",
            found.offered.as_deref()?,
            "--",
            &format!(":(literal){shrinkwrap}"),
        ])?;
        if !present.is_empty() {
            return None;
        }
    }
    let package = directory(&record.file, &[PACKAGE_MANIFEST])?;
    let relative = if root.is_empty() {
        package
    } else if package == root {
        ""
    } else {
        package.strip_prefix(&format!("{root}/"))?
    };
    let lock = mise::source(found, lockfile)?;
    locked_version(&lock, manifest, relative, record, url, version)
}

fn locked_version(
    lock: &str,
    manifest: &str,
    relative: &str,
    record: &Record,
    url: &str,
    version: &str,
) -> Option<String> {
    let Document(lock) = serde_json::from_str(lock).ok()?;
    if !TREE_LOCKFILE_VERSIONS.contains(&lock.get("lockfileVersion")?.as_u64()?) {
        return None;
    }
    let Document(manifest) = serde_json::from_str(manifest).ok()?;
    let section = record.dependency.dep_type.as_deref()?;
    if !super::DEPENDENCY_SECTIONS.contains(&section) {
        return None;
    }
    let packages = lock.get("packages")?.as_object()?;
    if packages
        .get(relative)?
        .get(section)?
        .get(&record.dependency.dep_name)?
        .as_str()?
        != url
    {
        return None;
    }
    if !relative.is_empty() {
        let name = manifest.get("name")?.as_str()?;
        let link = packages.get(&format!("{NODE_MODULES}/{name}"))?;
        if !link.get("link")?.as_bool()? || link.get("resolved")?.as_str()? != relative {
            return None;
        }
    }
    let mut parent = relative;
    loop {
        if parent.rsplit('/').next() != Some(NODE_MODULES) {
            let prefix = if parent.is_empty() {
                String::new()
            } else {
                format!("{parent}/")
            };
            if let Some(entry) = packages.get(&format!(
                "{prefix}{NODE_MODULES}/{}",
                record.dependency.dep_name
            )) {
                return (entry.get("version")?.as_str()? == version
                    && entry.get("resolved")?.as_str()? == url
                    && entry
                        .get("link")
                        .is_none_or(|link| link == &Value::Bool(false)))
                .then(|| version.to_owned());
            }
        }
        if parent.is_empty() {
            return None;
        }
        parent = parent.rsplit_once('/').map_or("", |(parent, _)| parent);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const URL: &str = "https://cdn.sheetjs.com/xlsx-0.20.3/xlsx-0.20.3.tgz";
    const VERSION: &str = "0.20.3";

    fn record() -> Record {
        Record {
            manager: "npm".into(),
            file: "data/package.json".into(),
            dependency: serde_json::from_value(json!({
                "depName":"xlsx", "currentValue":URL, "depType":"dependencies",
                "skipReason":"unspecified-version", "updates":[]
            }))
            .unwrap(),
            lock_files: None,
        }
    }

    fn lock() -> Value {
        json!({"lockfileVersion":3,"packages":{
            "":{"dependencies":{"xlsx":URL}},
            "data":{"dependencies":{"xlsx":URL}},
            "node_modules/@fixture/data":{"resolved":"data","link":true},
            "node_modules/xlsx":{"version":VERSION,"resolved":URL}
        }})
    }

    fn check(lock: &Value, relative: &str) -> Option<String> {
        locked_version(
            &lock.to_string(),
            r#"{"name":"@fixture/data"}"#,
            relative,
            &record(),
            URL,
            VERSION,
        )
    }

    #[test]
    fn root_and_workspace_declarations_follow_supported_lock_graphs() {
        for format in TREE_LOCKFILE_VERSIONS {
            let mut lock = lock();
            lock["lockfileVersion"] = json!(format);
            for relative in ["", "data"] {
                assert_eq!(check(&lock, relative).as_deref(), Some(VERSION));
            }
        }
    }

    #[test]
    fn changed_versions_origins_and_workspace_links_never_substitute_a_resolution() {
        for (path, replacement) in [
            ("/lockfileVersion", json!(1)),
            ("/lockfileVersion", json!("3")),
            ("/packages", Value::Null),
            ("/packages/data/dependencies/xlsx", json!("different")),
            ("/packages/node_modules~1xlsx/version", json!("0.20.2")),
            (
                "/packages/node_modules~1xlsx/resolved",
                json!("https://attacker.invalid/xlsx.tgz"),
            ),
            ("/packages/node_modules~1@fixture~1data/link", json!(false)),
            ("/packages/node_modules~1@fixture~1data/link", Value::Null),
            (
                "/packages/node_modules~1@fixture~1data/resolved",
                json!("other"),
            ),
        ] {
            let mut lock = lock();
            *lock.pointer_mut(path).unwrap() = replacement;
            assert_eq!(check(&lock, "data"), None, "{path}");
        }
        let mut linked = lock();
        linked["packages"]["node_modules/xlsx"]["link"] = json!(true);
        assert_eq!(check(&linked, "data"), None);
    }

    #[test]
    fn nearest_dependency_entries_shadow_ancestor_entries() {
        let mut lock = lock();
        lock["packages"]["data/node_modules/xlsx"] = json!({"version":"0.20.2","resolved":URL});
        assert_eq!(check(&lock, "data"), None);
        lock["packages"]["data/node_modules/xlsx"]["version"] = json!(VERSION);
        lock["packages"]["node_modules/xlsx"]["version"] = json!("0.20.2");
        assert_eq!(check(&lock, "data").as_deref(), Some(VERSION));
    }

    #[test]
    fn malformed_and_duplicated_lock_members_fail_closed() {
        for lock in [
            "not JSON".into(),
            lock().to_string().replacen(
                "\"lockfileVersion\":3",
                "\"lockfileVersion\":3,\"lockfileVersion\":3",
                1,
            ),
        ] {
            assert_eq!(
                locked_version(
                    &lock,
                    r#"{"name":"@fixture/data"}"#,
                    "data",
                    &record(),
                    URL,
                    VERSION,
                ),
                None,
            );
        }
    }

    #[test]
    fn lock_paths_are_relative_canonical_and_name_a_supported_manifest() {
        for path in [
            "/package-lock.json",
            "../package-lock.json",
            "data/../package-lock.json",
            "data//package-lock.json",
            "data\\package-lock.json",
            "data/yarn.lock",
        ] {
            assert_eq!(directory(path, LOCKFILE_NAMES), None, "{path}");
        }
        assert_eq!(directory("package-lock.json", LOCKFILE_NAMES), Some(""));
        assert_eq!(
            directory("data/npm-shrinkwrap.json", LOCKFILE_NAMES),
            Some("data")
        );
    }
}
