//! Literal Mise declarations in the exact revision offered to Git.

use super::Found;
use super::report::{Dependency, Record};

pub(super) fn covers(found: Option<&Found>, record: &Record, verified: &Dependency) -> bool {
    let dependency = &record.dependency;
    if dependency.dep_type.as_deref() != Some("tools")
        || dependency.package_name.as_deref() != Some(dependency.dep_name.as_str())
        || dependency.current_value.is_some()
        || dependency.current_version.is_some()
        || dependency.current_digest.is_some()
        || verified.current_value != verified.current_version
    {
        return false;
    }
    let Some(found) = found else {
        return false;
    };
    let Some(offered) = found.offered.as_deref() else {
        return false;
    };
    let object = format!("{offered}:{}", record.file);
    let Some(source) = crate::real_git::capture(&["-C", &found.repository, "show", &object]) else {
        return false;
    };
    let Ok(manifest) = toml::from_str::<toml::Table>(&source) else {
        return false;
    };
    manifest
        .get("tools")
        .and_then(toml::Value::as_table)
        .and_then(|tools| tools.get(&dependency.dep_name))
        .and_then(toml::Value::as_str)
        .is_some_and(|version| verified.current_value.as_deref() == Some(version))
}
