//! Portable dependency locks, independent of the installing host.

use anyhow::{Context, Result, ensure};
use rules::tool_lock_rules::{self, Facts, Graph, Project, Sidecar, Verdict};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

const CONFIG: &str = "mise.toml";
const LOCK: &str = "mise.lock";
const SIDECARS: &str = ".mise/locks";
const GRAPH: &str = "uv.lock";
const PROJECT: &str = "pyproject.toml";
const PYPI: &str = "pypi:";
const HEX_PER_BYTE: usize = 2;
const PATH_PARTS: usize = 2;
const REPAIR: &str = "mise lock --platform linux-x64";

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum Request {
    Version(String),
    Options { version: String },
    Multiple(Vec<Self>),
}

impl Request {
    fn versions(&self) -> Vec<&str> {
        match self {
            Self::Version(version) | Self::Options { version } => vec![version],
            Self::Multiple(requests) => requests.iter().flat_map(Self::versions).collect(),
        }
    }
}

#[derive(Debug, Deserialize)]
struct Config {
    tools: BTreeMap<String, Request>,
}

#[derive(Debug, Deserialize)]
struct Lockfile {
    tools: BTreeMap<String, Vec<Entry>>,
}

#[derive(Debug, Deserialize)]
struct Entry {
    version: String,
    backend: Option<String>,
    uv: Option<toml::Value>,
    aube: Option<toml::Value>,
}

impl Entry {
    fn pypi(&self, tool: &str) -> bool {
        tool.starts_with(PYPI)
            || self
                .backend
                .as_deref()
                .is_some_and(|backend| backend.starts_with(PYPI))
    }

    fn path(&self) -> Option<&str> {
        self.uv.as_ref()?.get("path")?.as_str()
    }
}

#[derive(Debug, Deserialize)]
struct Pyproject {
    project: Dependencies,
}

#[derive(Debug, Deserialize)]
struct Dependencies {
    dependencies: Vec<String>,
}

fn read_optional(path: &Path) -> Result<Option<String>> {
    match fs::read_to_string(path) {
        Ok(text) => Ok(Some(text)),
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error).with_context(|| format!("read {}", path.display())),
    }
}

fn sidecar_path(path: &str) -> Option<&str> {
    let suffix = path.strip_prefix(SIDECARS)?.strip_prefix('/')?;
    let parts: Vec<_> = suffix.split('/').collect();
    (parts.len() == PATH_PARTS
        && parts.iter().all(|part| {
            !part.is_empty() && !matches!(*part, "." | "..") && !part.contains(['\\', ':'])
        }))
    .then_some(path)
}

fn package_name(name: &str) -> String {
    name.to_ascii_lowercase().replace(['_', '.'], "-")
}

fn pinned(project: &Pyproject, package: &str, version: &str) -> bool {
    project.project.dependencies.iter().any(|dependency| {
        let Some((name, pin)) = dependency.split_once("==") else {
            return false;
        };
        let name = name.split_once('[').map_or(name, |(name, _)| name);
        package_name(name.trim()) == package_name(package) && pin.trim() == version
    })
}

fn facts(root: &Path, tool: &str, entry: &Entry) -> Result<Facts> {
    let path = entry.path().and_then(sidecar_path);
    let digest = entry
        .uv
        .as_ref()
        .and_then(|uv| uv.get("digest"))
        .and_then(toml::Value::as_str);
    let digest_valid = digest
        .and_then(|digest| digest.strip_prefix("sha256:"))
        .is_some_and(|hex| {
            hex.len() == Sha256::output_size() * HEX_PER_BYTE
                && hex.bytes().all(|byte| byte.is_ascii_hexdigit())
        });
    let directory = path.map(|path| root.join(path));
    let graph = directory
        .as_ref()
        .map(|path| read_optional(&path.join(GRAPH)))
        .transpose()?
        .flatten();
    let project = directory
        .as_ref()
        .map(|path| read_optional(&path.join(PROJECT)))
        .transpose()?
        .flatten()
        .and_then(|text| toml::from_str::<Pyproject>(&text).ok());
    let package = entry
        .backend
        .as_deref()
        .unwrap_or(tool)
        .split_once(':')
        .map_or(tool, |(_, package)| package);
    Ok(Facts {
        pypi: entry.pypi(tool),
        uv: entry.uv.is_some(),
        locked: true,
        sidecar: Sidecar {
            path: path.is_some(),
            digest: digest_valid,
            directory: directory.is_some_and(|path| path.is_dir()),
            graph: Graph {
                present: graph.is_some(),
                // mise 2026.10.11's src/lockfile/graph.rs hashes only uv.lock, with CRLF normalized to LF.
                checksum: graph.is_some_and(|text| {
                    let hash = Sha256::digest(text.replace("\r\n", "\n").as_bytes());
                    digest == Some(format!("sha256:{}", crate::build_inputs::hex(&hash)).as_str())
                }),
            },
            project: Project {
                present: project.is_some(),
                pin: project.is_some_and(|project| pinned(&project, package, &entry.version)),
            },
        },
    })
}

fn refusal(tool: &str, detail: &str, verdict: Verdict) -> Option<String> {
    let reason = match verdict {
        Verdict::Complete => return None,
        Verdict::MissingEntry => "missing lock entry for the configured version",
        Verdict::MissingUv => "missing uv reference",
        Verdict::MissingPath => "missing or invalid uv path",
        Verdict::MissingDigest => "missing or invalid uv digest",
        Verdict::MissingDirectory => "missing uv directory",
        Verdict::MissingGraph => "missing uv.lock",
        Verdict::DigestMismatch => "uv.lock SHA-256 differs from the uv digest",
        Verdict::MissingProject => "missing or invalid pyproject.toml",
        Verdict::PinMismatch => "pyproject.toml pinned version differs from the lock entry",
        Verdict::Unreferenced => "unreferenced version directory",
    };
    let action = if verdict == Verdict::Unreferenced {
        format!(
            "remove this unreferenced directory and rerun `just tool-locks`; regenerate a configured tool with `{REPAIR} {tool}`"
        )
    } else {
        format!(
            "remove this tool's invalid uv field from mise.lock if present, run `{REPAIR} {tool}`, and include mise.lock and .mise/locks in the change"
        )
    };
    Some(format!(
        "Refused: {tool}{detail}: {reason}.\nNext action: {action}."
    ))
}

fn directories(root: &Path) -> Result<Vec<PathBuf>> {
    match fs::read_dir(root.join(SIDECARS)) {
        Ok(tools) => {
            let mut found = Vec::new();
            for tool in tools {
                let tool = tool?;
                ensure!(
                    tool.file_type()?.is_dir(),
                    "{} is not a lock directory",
                    tool.path().display()
                );
                for version in fs::read_dir(tool.path())? {
                    let version = version?;
                    ensure!(
                        version.file_type()?.is_dir(),
                        "{} is not a version directory",
                        version.path().display()
                    );
                    found.push(version.path());
                }
            }
            found.sort();
            Ok(found)
        }
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(Vec::new()),
        Err(error) => Err(error.into()),
    }
}

/// Checks every configured uv tool and sidecar directory, including tools restricted to another OS.
///
/// # Errors
///
/// Refuses incomplete metadata, absent sidecars, changed hashes or pins, and unreferenced directories.
pub fn check(root: &Path) -> Result<()> {
    let config: Config = toml::from_str(&fs::read_to_string(root.join(CONFIG))?).context(CONFIG)?;
    let lock: Lockfile = toml::from_str(&fs::read_to_string(root.join(LOCK))?).context(LOCK)?;
    let mut failures = Vec::new();
    let mut references = BTreeSet::new();
    let mut names = BTreeMap::new();
    let mut checked = 0;
    for (tool, request) in &config.tools {
        let entries = lock.tools.get(tool).map_or(&[][..], Vec::as_slice);
        let pypi = tool.starts_with(PYPI) || entries.iter().any(|entry| entry.pypi(tool));
        let uv = entries.iter().any(|entry| entry.uv.is_some());
        for version in request.versions() {
            let verdict = tool_lock_rules::entry(Facts {
                pypi,
                uv,
                locked: entries.iter().any(|entry| entry.version == version),
                ..Facts::default()
            });
            if verdict == Verdict::MissingEntry {
                failures.extend(refusal(tool, &format!("@{version}"), verdict));
            }
        }
    }
    for (tool, entries) in &lock.tools {
        for entry in entries {
            for graph in [entry.uv.as_ref(), entry.aube.as_ref()]
                .into_iter()
                .flatten()
            {
                if let Some(path) = graph
                    .get("path")
                    .and_then(toml::Value::as_str)
                    .and_then(sidecar_path)
                {
                    references.insert(root.join(path));
                    names.insert(Path::new(path).parent().map(Path::to_path_buf), tool);
                }
            }
            if entry.pypi(tool) || entry.uv.is_some() {
                let verdict =
                    tool_lock_rules::entry(facts(root, tool, entry).with_context(|| {
                        format!(
                            "{tool}@{}; Next action: run `{REPAIR} {tool}`",
                            entry.version
                        )
                    })?);
                failures.extend(refusal(tool, &format!("@{}", entry.version), verdict));
                checked += 1;
            }
        }
    }
    let directories = directories(root)?;
    for directory in &directories {
        let relative = directory.strip_prefix(root)?;
        let parent = relative.parent().map(Path::to_path_buf);
        let fallback = parent
            .as_ref()
            .and_then(|path| path.file_name())
            .unwrap_or_default()
            .to_string_lossy()
            .replacen("pypi-", PYPI, 1);
        let tool = names
            .get(&parent)
            .copied()
            .map_or(fallback.as_str(), String::as_str);
        let verdict = tool_lock_rules::directory(references.contains(directory));
        failures.extend(refusal(
            tool,
            &format!(" ({})", relative.display()),
            verdict,
        ));
    }
    ensure!(failures.is_empty(), "{}", failures.join("\n"));
    println!(
        "Checked {checked} uv tool locks and {} version directories on all host platforms",
        directories.len()
    );
    Ok(())
}
