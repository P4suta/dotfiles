//! Literal Mise declarations in the exact revision offered to Git.

use super::Found;
use super::report::{Dependency, Record};

const GITHUB_API_REPOSITORIES: &str = "https://api.github.com/repos/";
const GITHUB_LATEST_RELEASE: &str = "/releases/latest";
const GITHUB_REGISTRY: &str = "https://github.com";
const HTTP_FIELDS: &[&str] = &[
    "version",
    "url",
    "checksum",
    "size",
    "os",
    "version_list_url",
    "version_json_path",
];

pub(super) fn covers(found: Option<&Found>, record: &Record, verified: &Dependency) -> bool {
    let dependency = &record.dependency;
    if dependency.dep_type.as_deref() != Some("tools")
        || dependency.package_name.as_deref() != Some(dependency.dep_name.as_str())
        || dependency.current_value.is_some()
        || dependency.current_version.is_some()
        || dependency.current_digest.is_some()
    {
        return false;
    }
    source_matches(found, record, verified)
}

pub(super) fn source_matches(
    found: Option<&Found>,
    record: &Record,
    verified: &Dependency,
) -> bool {
    if verified.current_value != verified.current_version
        || verified.current_value.as_deref().is_none_or(str::is_empty)
    {
        return false;
    }
    let Some(source) = source(found, &record.file) else {
        return false;
    };
    let Ok(manifest) = toml::from_str::<toml::Table>(&source) else {
        return false;
    };
    manifest
        .get("tools")
        .and_then(toml::Value::as_table)
        .and_then(|tools| tools.get(&record.dependency.dep_name))
        .and_then(toml::Value::as_str)
        .is_some_and(|version| verified.current_value.as_deref() == Some(version))
}

pub(super) fn source(found: Option<&Found>, file: &str) -> Option<String> {
    let found = found?;
    let offered = found.offered.as_deref()?;
    let kind = crate::real_git::capture(&[
        "-C",
        &found.repository,
        "ls-tree",
        "--full-tree",
        "--format=%(objectmode) %(objecttype)",
        offered,
        "--",
        &format!(":(literal){file}"),
    ])?;
    if !matches!(kind.trim_end(), "100644 blob" | "100755 blob") {
        return None;
    }
    let object = format!("{offered}:{file}");
    let output = crate::real_git::command()?
        .args(["-C", &found.repository, "show", &object])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    String::from_utf8(output.stdout).ok()
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReleasePin {
    version: String,
    package: String,
}

impl ReleasePin {
    pub fn version(&self) -> &str {
        &self.version
    }

    pub fn package(&self) -> &str {
        &self.package
    }
}

/// Decodes a mise `http:` declaration with a GitHub release source.
pub fn parse_http_release(source: &str, name: &str) -> Option<ReleasePin> {
    if name.strip_prefix("http:")?.is_empty() {
        return None;
    }
    let manifest = toml::from_str::<toml::Table>(source).ok()?;
    let entry = manifest.get("tools")?.get(name)?.as_table()?;
    if entry.keys().any(|key| !HTTP_FIELDS.contains(&key.as_str())) {
        return None;
    }
    if entry.get("version_json_path")?.as_str()? != ".tag_name" {
        return None;
    }
    let package = entry
        .get("version_list_url")?
        .as_str()?
        .strip_prefix(GITHUB_API_REPOSITORIES)?
        .strip_suffix(GITHUB_LATEST_RELEASE)?;
    let (owner, repository) = package.split_once('/')?;
    if owner.is_empty()
        || !owner
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
        || matches!(repository, "" | "." | "..")
        || !repository
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-' | b'.'))
    {
        return None;
    }
    let version = entry.get("version")?.as_str()?;
    if version.is_empty() {
        return None;
    }
    Some(ReleasePin {
        version: version.to_owned(),
        package: package.to_owned(),
    })
}

pub(super) fn http_covers(found: Option<&Found>, record: &Record, verified: &Dependency) -> bool {
    let dependency = &record.dependency;
    if dependency.dep_type.as_deref() != Some("tools")
        || dependency.package_name.as_deref() != Some(dependency.dep_name.as_str())
        || dependency.current_value.is_some()
        || dependency.current_version.is_some()
        || dependency.current_digest.is_some()
        || verified.datasource.as_deref() != Some("github-releases")
        || verified
            .registry_url
            .as_deref()
            .is_some_and(|url| url != GITHUB_REGISTRY)
        || verified
            .registry_urls
            .as_ref()
            .is_some_and(|urls| urls.as_slice() != [GITHUB_REGISTRY])
    {
        return false;
    }
    let Some(source) = source(found, &record.file) else {
        return false;
    };
    let Some(pin) = parse_http_release(&source, &dependency.dep_name) else {
        return false;
    };
    verified.current_value.as_deref() == Some(pin.version())
        && verified.current_version.as_deref().is_some_and(|version| {
            version == pin.version() || version.strip_prefix('v') == Some(pin.version())
        })
        && verified.package_name.as_deref() == Some(pin.package())
        && dependency
            .locked_version
            .as_deref()
            .is_none_or(|version| version == pin.version())
}

pub(super) fn http_changed(found: Option<&Found>, record: &Record) -> bool {
    let Some(found) = found else {
        return false;
    };
    if record.manager != "jsonata"
        || !record.dependency.dep_name.starts_with("http:")
        || !found.changed_files.contains(&record.file)
    {
        return false;
    }
    let Some(before_revision) = found.baselines_by_file.get(&record.file) else {
        return found.scope_unknown;
    };
    let before = crate::real_git::capture(&[
        "-C",
        &found.repository,
        "show",
        &format!("{before_revision}:{}", record.file),
    ]);
    let after = source(Some(found), &record.file);
    let declaration = |source: Option<String>| {
        let manifest = toml::from_str::<toml::Table>(&source?).ok()?;
        manifest
            .get("tools")?
            .get(&record.dependency.dep_name)
            .cloned()
    };
    declaration(before) != declaration(after)
}
