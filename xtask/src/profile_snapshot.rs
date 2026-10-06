//! Committed manifests of what every profile renders, so a refactor proves that it leaves each host unchanged.

use crate::profile_rules::Profile;
use crate::profiles::{chezmoi, script_order};
use anyhow::{Context, Result, ensure};
use serde_json::{Map, Value, json};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Where the manifests live, relative to the checkout.
pub const DIRECTORY: &str = "xtask/tests/snapshots";

const HOME: &str = "/fixture/home";
const SOURCE: &str = "/fixture/source";
const PUBLIC_KEY: &str =
    "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIFixtureFixtureFixtureFixtureFixtureFixture fixture";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Role {
    Personal,
    Work,
}

impl Role {
    pub const ALL: [Self; 2] = [Self::Personal, Self::Work];

    pub const fn name(self) -> &'static str {
        match self {
            Self::Personal => "personal",
            Self::Work => "work",
        }
    }
}

/// One profile under one signing setting and one role.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Combination {
    pub profile: Profile,
    pub signing: bool,
    pub role: Role,
}

impl Combination {
    pub fn all() -> Vec<Self> {
        let mut combinations = Vec::new();
        for profile in Profile::ALL {
            for signing in [false, true] {
                for role in Role::ALL {
                    combinations.push(Self {
                        profile,
                        signing,
                        role,
                    });
                }
            }
        }
        combinations
    }

    pub fn name(self) -> String {
        format!(
            "{}-{}-{}",
            self.profile.name(),
            if self.signing { "signed" } else { "unsigned" },
            self.role.name()
        )
    }

    fn data(self) -> Value {
        let mut data = crate::profiles::fixture(self.profile, Path::new(SOURCE), Path::new(HOME));
        data["role"] = json!(self.role.name());
        data["paths"]["projects"] = json!(format!("{HOME}/projects"));
        data["chezmoi"]["homeDir"] = json!(HOME);
        data["chezmoi"]["sourceDir"] = json!(SOURCE);
        if self.signing {
            data["signing"] = json!({
                "enabled": true,
                "public_key": PUBLIC_KEY,
                "pubkey": PUBLIC_KEY,
                "key_comment": "fixture",
            });
        }
        data
    }
}

fn sha256(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

/// A chezmoi run with an empty environment and only chezmoi on its search path, so `env` and `lookPath` answer the same on every host.
fn isolated(command: &mut Command, home: &Path) -> Result<()> {
    let name = format!("chezmoi{}", std::env::consts::EXE_SUFFIX);
    let path = std::env::var_os("PATH").context("PATH is unset")?;
    let directory = std::env::split_paths(&path)
        .find(|directory| directory.join(&name).is_file())
        .context("chezmoi is not on PATH")?;
    command.env_clear().env("PATH", directory);
    command.env("HOME", home).env("USERPROFILE", home);
    if let Some(system) = std::env::var_os("SystemRoot") {
        command.env("SystemRoot", system);
    }
    Ok(())
}

/// Every variable chezmoi derives from the host and the command line, replaced by one fixed virtual host.
/// The override also names, as null, each key this host lacks, so two hosts render the same `.` value.
fn virtual_host(real: &Value, combination: Combination, args: Option<&Value>) -> Value {
    let mut host = Map::new();
    let fixed = |key: &str| -> Value {
        match key {
            "arch" => json!("amd64"),
            "args" => args.cloned().unwrap_or_else(|| json!([])),
            "cacheDir" => json!("/fixture/cache"),
            "command" => json!("dump"),
            "commandDir" | "destDir" | "homeDir" | "rawHomeDir" => json!(HOME),
            "configFile" => json!("/fixture/config/chezmoi.json"),
            "executable" => json!("/fixture/bin/chezmoi"),
            "fqdnHostname" => json!("fixture.invalid"),
            "hostname" => json!("fixture"),
            "gid" | "uid" => json!("1000"),
            "group" | "username" => json!("fixture"),
            "os" => json!(combination.profile.os()),
            "pathListSeparator" => json!(":"),
            "pathSeparator" => json!("/"),
            "sourceDir" | "workingTree" => json!(SOURCE),
            "kernel" => combination.data()["chezmoi"]["kernel"].clone(),
            _ => Value::Null,
        }
    };
    for key in real.as_object().into_iter().flat_map(Map::keys) {
        let value = match key.as_str() {
            "config" if args.is_some() => real[key].clone(),
            "config" | "version" => Value::Object(
                real[key]
                    .as_object()
                    .into_iter()
                    .flat_map(Map::keys)
                    .map(|inner| (inner.clone(), Value::Null))
                    .collect(),
            ),
            other => fixed(other),
        };
        host.insert(key.clone(), value);
    }
    for key in ["kernel", "windowsVersion", "osRelease"] {
        host.entry(key).or_insert_with(|| fixed(key));
    }
    Value::Object(host)
}

struct Scope {
    _directory: tempfile::TempDir,
    root: PathBuf,
    config: PathBuf,
    destination: PathBuf,
}

impl Scope {
    fn new(combination: Combination) -> Result<Self> {
        let directory = tempfile::tempdir()?;
        let root = directory.path().to_path_buf();
        // Templates read `.chezmoi.targetFile`, which joins the destination, so the destination stays fixed and unwritten.
        let destination = PathBuf::from(HOME);
        let config = root.join("chezmoi.json");
        fs::write(
            &config,
            serde_json::to_vec(&json!({"umask": 0o022, "data": combination.data()}))?,
        )?;
        Ok(Self {
            _directory: directory,
            root,
            config,
            destination,
        })
    }

    fn run(&self, source: &Path, overrides: &Value, arguments: &[&str]) -> Result<Vec<u8>> {
        let mut command = chezmoi(source, &self.root, &self.config, &self.destination);
        isolated(&mut command, &self.root)?;
        let output = command
            .arg("--override-data")
            .arg(serde_json::to_string(overrides)?)
            .args(arguments)
            .output()?;
        ensure!(
            output.status.success(),
            "chezmoi {arguments:?} failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        Ok(output.stdout)
    }
}

fn overrides(
    source: &Path,
    scope: &Scope,
    combination: Combination,
    args: Option<&Value>,
) -> Result<Value> {
    let mut data = combination.data();
    let real: Value =
        serde_json::from_slice(&scope.run(source, &data, &["data", "--format", "json"])?)?;
    data["chezmoi"] = virtual_host(&real["chezmoi"], combination, args);
    Ok(data)
}

fn mode(entry: &Value) -> Value {
    entry["perm"]
        .as_u64()
        .map_or(Value::Null, |perm| json!(format!("{perm:04o}")))
}

/// The manifest of one combination: every target with its type, mode, and content digest, and every script with its phase, position, and body digest.
pub fn manifest(source: &Path, combination: Combination) -> Result<Value> {
    let scope = Scope::new(combination)?;
    let overrides = overrides(source, &scope, combination, None)?;
    let dump: BTreeMap<String, Value> = serde_json::from_slice(&scope.run(
        source,
        &overrides,
        &[
            "dump",
            "--include",
            "dirs,files,symlinks,scripts",
            "--exclude",
            "externals",
            "--format",
            "json",
        ],
    )?)?;
    let listing = scope.run(
        source,
        &overrides,
        &[
            "managed",
            "--include",
            "scripts",
            "--path-style",
            "all",
            "--format",
            "json",
        ],
    )?;
    let sources: BTreeMap<String, Value> = serde_json::from_slice(&listing)?;
    let order = script_order(&listing)?;
    let mut targets = Vec::new();
    for (path, entry) in &dump {
        let kind = entry["type"]
            .as_str()
            .context("dump entry without a type")?;
        let digest = match kind {
            "file" => entry["contents"]
                .as_str()
                .map(|text| sha256(text.as_bytes())),
            "symlink" => entry["linkname"]
                .as_str()
                .map(|text| sha256(text.as_bytes())),
            _ => None,
        };
        if kind != "script" {
            targets
                .push(json!({"path": path, "type": kind, "mode": mode(entry), "sha256": digest}));
        }
    }
    let mut scripts = Vec::new();
    for (position, target) in order.iter().enumerate() {
        let relative = sources[target]["sourceRelative"]
            .as_str()
            .context("script listing requires sourceRelative")?;
        let name = relative.rsplit('/').next().unwrap_or(relative);
        let attributes: Vec<_> = name.split('_').collect();
        let phase = if attributes.contains(&"before") {
            "before"
        } else {
            "after"
        };
        let condition = if attributes.contains(&"onchange") {
            "onchange"
        } else if attributes.contains(&"once") {
            "once"
        } else {
            "always"
        };
        let body = dump
            .get(target)
            .and_then(|entry| entry["contents"].as_str())
            .unwrap_or_default();
        scripts.push(json!({
            "phase": phase,
            "condition": condition,
            "position": position,
            "target": target,
            "sha256": sha256(body.as_bytes()),
        }));
    }
    Ok(json!({"combination": combination.name(), "targets": targets, "scripts": scripts}))
}

/// chezmoi joins paths with the separator of the host it runs on, so Windows and Unix hosts each keep their own manifests.
pub const fn family() -> &'static str {
    if cfg!(windows) { "windows" } else { "unix" }
}

fn path(root: &Path, combination: Combination) -> PathBuf {
    root.join(DIRECTORY)
        .join(family())
        .join(format!("{}.json", combination.name()))
}

/// One entry per line, so a review shows each changed target as one changed line.
fn text(manifest: &Value) -> Result<String> {
    let mut text = format!(
        "{{\n  \"combination\": {}",
        serde_json::to_string(&manifest["combination"])?
    );
    for section in ["targets", "scripts"] {
        let entries = manifest[section]
            .as_array()
            .into_iter()
            .flatten()
            .map(|entry| Ok(format!("    {}", serde_json::to_string(entry)?)))
            .collect::<Result<Vec<_>>>()?;
        text.push_str(&format!(
            ",\n  \"{section}\": [\n{}\n  ]",
            entries.join(",\n")
        ));
    }
    text.push_str("\n}\n");
    Ok(text)
}

/// The paths and script targets whose entries differ between `committed` and `rendered`.
pub fn differences(committed: &Value, rendered: &Value) -> Vec<String> {
    let mut changed = Vec::new();
    for (section, key) in [("targets", "path"), ("scripts", "target")] {
        let index = |manifest: &Value| -> BTreeMap<String, Value> {
            manifest[section]
                .as_array()
                .into_iter()
                .flatten()
                .map(|entry| {
                    (
                        entry[key].as_str().unwrap_or_default().to_owned(),
                        entry.clone(),
                    )
                })
                .collect()
        };
        let (before, after) = (index(committed), index(rendered));
        for name in before
            .keys()
            .chain(after.keys())
            .collect::<std::collections::BTreeSet<_>>()
        {
            if before.get(name) != after.get(name) {
                changed.push(format!("{section} {name}"));
            }
        }
    }
    changed
}

/// Refuses any combination whose rendering differs from its committed manifest.
pub fn check(root: &Path) -> Result<()> {
    let mut changed = Vec::new();
    for combination in Combination::all() {
        let rendered = manifest(root, combination)?;
        let committed: Value = match fs::read(path(root, combination)) {
            Ok(bytes) => serde_json::from_slice(&bytes)?,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Value::Null,
            Err(error) => return Err(error.into()),
        };
        for difference in differences(&committed, &rendered) {
            changed.push(format!("{}: {difference}", combination.name()));
        }
    }
    ensure!(
        changed.is_empty(),
        "rendered profiles differ from the committed manifests in {DIRECTORY}/{}; review each change, then run `cargo xtask profile-snapshots --write` on a Windows and a Unix host and commit both manifest sets:\n{}",
        family(),
        changed.join("\n")
    );
    println!("Matched {} profile manifests", Combination::all().len());
    Ok(())
}

/// Writes every manifest from the current rendering.
pub fn write(root: &Path) -> Result<()> {
    let directory = root.join(DIRECTORY).join(family());
    fs::create_dir_all(&directory)?;
    for combination in Combination::all() {
        fs::write(
            path(root, combination),
            text(&manifest(root, combination)?)?,
        )?;
    }
    Ok(())
}

/// Scripts whose body changes when only the chezmoi command line changes, which reruns them on every such change.
pub fn command_line_dependent(root: &Path) -> Result<Vec<String>> {
    let mut dependent = Vec::new();
    for profile in Profile::ALL {
        let combination = Combination {
            profile,
            signing: false,
            role: Role::Personal,
        };
        let mut bodies = Vec::new();
        for args in [json!(["apply"]), json!(["apply", "--verbose"])] {
            let scope = Scope::new(combination)?;
            let overrides = overrides(root, &scope, combination, Some(&args))?;
            let dump: BTreeMap<String, Value> = serde_json::from_slice(&scope.run(
                root,
                &overrides,
                &["dump", "--include", "scripts", "--format", "json"],
            )?)?;
            bodies.push(dump);
        }
        for (target, entry) in &bodies[0] {
            if bodies[1].get(target) != Some(entry) {
                dependent.push(format!("{}: {target}", profile.name()));
            }
        }
    }
    Ok(dependent)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sixteen_combinations_have_distinct_names() {
        let names: std::collections::BTreeSet<_> = Combination::all()
            .into_iter()
            .map(Combination::name)
            .collect();
        assert_eq!(names.len(), 16);
        assert!(names.contains("wsl-signed-work"));
    }

    #[test]
    fn the_manifest_text_parses_back_to_the_same_manifest() {
        for manifest in [
            json!({"combination": "mac-signed-work", "targets": [{"path": "a", "mode": "0644"}, {"path": "b"}], "scripts": [{"target": "s"}]}),
            json!({"combination": "wsl-unsigned-personal", "targets": [], "scripts": []}),
        ] {
            let text = text(&manifest).unwrap();
            assert_eq!(serde_json::from_str::<Value>(&text).unwrap(), manifest);
            assert!(
                text.contains("\n    {\"mode\":\"0644\",\"path\":\"a\"},\n")
                    || manifest["targets"] == json!([])
            );
        }
    }

    #[test]
    fn a_changed_digest_and_a_new_target_are_both_differences() {
        let committed = json!({"targets": [{"path": "a", "sha256": "1"}], "scripts": []});
        let rendered = json!({"targets": [{"path": "a", "sha256": "2"}, {"path": "b"}], "scripts": [{"target": "s"}]});
        assert_eq!(
            differences(&committed, &rendered),
            ["targets a", "targets b", "scripts s"]
        );
        assert!(differences(&rendered, &rendered).is_empty());
    }

    #[test]
    fn the_virtual_host_names_every_real_key_and_fixes_the_command_line() {
        let real = json!({"args": ["apply", "/tmp/x"], "config": {"destDir": "/tmp/x"}, "homeDir": "/tmp/h", "windowsVersion": {"build": 1}});
        let combination = Combination {
            profile: Profile::Linux,
            signing: false,
            role: Role::Personal,
        };
        let host = virtual_host(&real, combination, None);
        assert_eq!(host["args"], json!([]));
        assert_eq!(host["config"], json!({"destDir": null}));
        assert_eq!(host["homeDir"], json!(HOME));
        assert_eq!(host["windowsVersion"], Value::Null);
        assert!(
            host.as_object()
                .is_some_and(|keys| keys.contains_key("kernel"))
        );
        let probed = virtual_host(&real, combination, Some(&json!(["apply"])));
        assert_eq!(probed["args"], json!(["apply"]));
        assert_eq!(probed["config"], real["config"]);
    }
}
