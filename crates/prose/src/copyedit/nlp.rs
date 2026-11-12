use super::{Language, Segment};
use crate::{Bundle, build_inputs::hex};
use anyhow::{Context, Result, ensure};
use rules::copyedit_rules::span_valid;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::Path;
use std::process::Stdio;

pub const SCHEMA: u32 = 1;
const ENGINE: &str = "3.8.16";
const MODEL_VERSION: &str = "3.8.0";
const PACKAGE: &str = "nlp";
const PYTHON: &str = "python-version";
const ADAPTER: &str = "annotations.py";
const LOCK: &str = "uv.lock";
const REQUEST: &str = "request.json";
const RESPONSE: &str = "response.json";

enum EnvironmentSync {
    Ready,
    Unavailable(String),
}

#[cfg(test)]
mod environment_tests {
    use super::*;

    #[test]
    fn a_valid_cached_graph_never_contacts_the_network() -> Result<()> {
        prepare_with(|offline| {
            assert!(offline, "a complete cache must not need the network");
            Ok(EnvironmentSync::Ready)
        })
    }

    #[test]
    fn an_incomplete_cache_requires_successful_locked_online_preparation() -> Result<()> {
        let mut requests = Vec::new();
        prepare_with(|offline| {
            requests.push(offline);
            Ok(if offline {
                EnvironmentSync::Unavailable("missing cached wheel".into())
            } else {
                EnvironmentSync::Ready
            })
        })?;
        assert_eq!(requests, [true, false]);
        let error = prepare_with(|offline| {
            Ok(EnvironmentSync::Unavailable(
                if offline {
                    "missing cached wheel"
                } else {
                    "network unavailable"
                }
                .into(),
            ))
        })
        .unwrap_err();
        assert!(error.to_string().contains("missing cached wheel"));
        assert!(error.to_string().contains("network unavailable"));
        Ok(())
    }

    #[test]
    fn a_missing_sync_tool_stops_before_another_attempt() {
        let mut calls = 0;
        let error = prepare_with(|_| {
            calls += 1;
            anyhow::bail!("uv cannot start")
        })
        .unwrap_err();
        assert_eq!(calls, 1);
        assert!(error.to_string().contains("uv cannot start"));
    }
}

fn prepare_with(mut sync: impl FnMut(bool) -> Result<EnvironmentSync>) -> Result<()> {
    let cached = match sync(true)? {
        EnvironmentSync::Ready => return Ok(()),
        EnvironmentSync::Unavailable(diagnostic) => diagnostic,
    };
    match sync(false)? {
        EnvironmentSync::Ready => Ok(()),
        EnvironmentSync::Unavailable(diagnostic) => anyhow::bail!(
            "locked annotation environment could not sync from cache or network:\ncache: {cached}\nnetwork: {diagnostic}"
        ),
    }
}

fn prepare_environment(
    bundle: &Bundle,
    package: &Path,
    environment: &Path,
    python: &str,
    development: bool,
) -> Result<()> {
    prepare_with(|offline| {
        let mut command = bundle.uv();
        command
            .current_dir(package)
            .env("UV_PROJECT_ENVIRONMENT", environment)
            .args(["sync", "--locked", "--python", python.trim()]);
        if offline {
            command.arg("--offline");
        }
        if !development {
            command.arg("--no-dev");
        }
        let output = command
            .stdin(Stdio::null())
            .output()
            .context("prepare the locked annotation environment")?;
        Ok(if output.status.success() {
            EnvironmentSync::Ready
        } else {
            EnvironmentSync::Unavailable(String::from_utf8_lossy(&output.stderr).into_owned())
        })
    })
}

pub(super) fn verify_adapter(bundle: &Bundle) -> Result<()> {
    let package = crate::canonical(&bundle.config.join(PACKAGE))?;
    let lock = fs::read(package.join(LOCK))?;
    let environment = crate::runtime_directory()?
        .join(format!("nlp-development-{}", hex(&Sha256::digest(&lock))));
    let python = fs::read_to_string(package.join(PYTHON))?;
    prepare_environment(bundle, &package, &environment, &python, true)?;
    for arguments in [
        vec!["mypy"],
        vec!["ruff", "check", "."],
        vec!["ruff", "format", "--check", "."],
        vec!["python", "-m", "unittest", "test_annotations"],
    ] {
        let output = bundle
            .uv()
            .current_dir(&package)
            .env("UV_PROJECT_ENVIRONMENT", &environment)
            .env("PYTHONDONTWRITEBYTECODE", "1")
            .env("MYPY_CACHE_DIR", environment.join("type-cache"))
            .env("RUFF_CACHE_DIR", environment.join("lint-cache"))
            .env("OMP_NUM_THREADS", "1")
            .env("OPENBLAS_NUM_THREADS", "1")
            .env("VECLIB_MAXIMUM_THREADS", "1")
            .args(["run", "--no-sync", "--python", python.trim()])
            .args(&arguments)
            .stdin(Stdio::null())
            .output()
            .context("verify the typed annotation adapter")?;
        ensure!(
            output.status.success(),
            "annotation adapter check {arguments:?} failed: {}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }
    Ok(())
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Span {
    pub start: usize,
    pub end: usize,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Lexeme {
    pub start: usize,
    pub end: usize,
    pub text: String,
    pub lemma: String,
    pub pos: String,
    pub tag: String,
    pub dependency: String,
    pub head: usize,
    pub morphology: String,
    pub entity: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Entity {
    pub start: usize,
    pub end: usize,
    pub label: String,
    pub text: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Annotation {
    pub id: String,
    pub text_sha256: String,
    pub characters: usize,
    pub tokens: Vec<Lexeme>,
    pub sentences: Vec<Span>,
    pub entities: Vec<Entity>,
    pub simplified: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct NlpReport {
    pub schema_version: u32,
    pub engine: String,
    pub model: String,
    pub model_version: String,
    pub python_version: String,
    pub packages: BTreeMap<String, String>,
    pub annotations: Vec<Annotation>,
}

fn character_slice(text: &str, start: usize, end: usize) -> String {
    text.chars().skip(start).take(end - start).collect()
}

impl Annotation {
    /// Checks source identity, complete spans, sentence membership, and rooted dependencies.
    ///
    /// # Errors
    ///
    /// Refuses mismatched text, incomplete annotations, invalid offsets, and cyclic or cross-sentence dependencies.
    pub fn validate(&self, segment: &Segment) -> Result<()> {
        let length = segment.text.chars().count();
        ensure!(
            self.id == segment.id && self.characters == length,
            "annotation identity or text length differs from its request"
        );
        ensure!(
            self.text_sha256 == hex(&Sha256::digest(segment.text.as_bytes())),
            "annotation text hash differs from its source"
        );
        ensure!(
            !self.tokens.is_empty() && !self.sentences.is_empty(),
            "annotation has empty applicable coverage"
        );
        let mut end = 0;
        for (index, token) in self.tokens.iter().enumerate() {
            ensure!(
                span_valid(token.start, token.end, length) && token.start >= end,
                "invalid or overlapping token span"
            );
            ensure!(
                token.text == character_slice(&segment.text, token.start, token.end),
                "token text differs from its source span"
            );
            ensure!(
                character_slice(&segment.text, end, token.start)
                    .chars()
                    .all(char::is_whitespace),
                "token coverage skipped visible text"
            );
            ensure!(
                token.head < self.tokens.len()
                    && !token.pos.is_empty()
                    && !token.dependency.is_empty(),
                "invalid token role or dependency head"
            );
            ensure!(
                (token.pos == "SPACE" && token.text.chars().all(char::is_whitespace))
                    || (token.head == index) == (token.dependency == "ROOT"),
                "dependency root differs at token {index} {:?}: {} -> {} in {}",
                token.text,
                token.dependency,
                token.head,
                self.id
            );
            end = token.end;
        }
        ensure!(
            character_slice(&segment.text, end, length)
                .chars()
                .all(char::is_whitespace),
            "token coverage omitted the source suffix"
        );
        self.validate_sentences(length)?;
        self.validate_dependencies()?;
        for entity in &self.entities {
            ensure!(
                span_valid(entity.start, entity.end, length) && !entity.label.is_empty(),
                "invalid entity span or label"
            );
            ensure!(
                entity.text == character_slice(&segment.text, entity.start, entity.end),
                "entity text differs from its source span"
            );
        }
        Ok(())
    }

    fn validate_sentences(&self, length: usize) -> Result<()> {
        let mut end = 0;
        for sentence in &self.sentences {
            ensure!(
                span_valid(sentence.start, sentence.end, length) && sentence.start >= end,
                "invalid or overlapping sentence span"
            );
            end = sentence.end;
        }
        for token in &self.tokens {
            ensure!(
                self.sentences
                    .iter()
                    .filter(|sentence| sentence.start <= token.start && token.end <= sentence.end)
                    .count()
                    == 1,
                "a token lacks exactly one enclosing sentence"
            );
        }
        Ok(())
    }

    fn validate_dependencies(&self) -> Result<()> {
        let mut complete = BTreeSet::new();
        for index in 0..self.tokens.len() {
            let mut visiting = BTreeSet::new();
            let mut next = index;
            while !complete.contains(&next) {
                ensure!(visiting.insert(next), "dependency graph contains a cycle");
                let token = &self.tokens[next];
                let head = &self.tokens[token.head];
                ensure!(
                    self.sentences
                        .iter()
                        .any(|sentence| sentence.start <= token.start
                            && token.end <= sentence.end
                            && sentence.start <= head.start
                            && head.end <= sentence.end),
                    "dependency crosses a sentence boundary"
                );
                if token.head == next {
                    break;
                }
                next = token.head;
            }
            complete.extend(visiting);
        }
        Ok(())
    }
}

#[derive(Serialize)]
struct Request<'a> {
    schema_version: u32,
    language: &'static str,
    segments: Vec<RequestSegment<'a>>,
}

#[derive(Serialize)]
struct RequestSegment<'a> {
    id: &'a str,
    text: &'a str,
}

#[derive(Deserialize)]
struct Lock {
    package: Vec<Package>,
}

#[derive(Deserialize)]
struct Package {
    name: String,
    version: String,
}

fn normalized(name: &str) -> String {
    name.to_ascii_lowercase().replace('_', "-")
}

fn verify_packages(lock: &str, report: &NlpReport) -> Result<()> {
    let lock: Lock = toml::from_str(lock).context("read the pinned annotation package graph")?;
    let expected: BTreeSet<_> = lock
        .package
        .into_iter()
        .map(|package| (normalized(&package.name), package.version))
        .collect();
    let actual: BTreeMap<_, _> = report
        .packages
        .iter()
        .map(|(name, version)| (normalized(name), version.clone()))
        .collect();
    for (name, version) in &actual {
        ensure!(
            expected.contains(&(name.clone(), version.clone())),
            "annotation package {name} {version} differs from the lock"
        );
    }
    for required in [
        "spacy",
        "thinc",
        "numpy",
        "spacy-pkuseg",
        "sudachipy",
        "sudachidict-core",
        "en-core-web-sm",
        "ja-core-news-sm",
        "zh-core-web-sm",
        "opencc",
    ] {
        ensure!(
            actual.contains_key(required),
            "annotation runtime lacks {required}"
        );
    }
    Ok(())
}

pub fn analyze(bundle: &Bundle, language: Language, segments: &[Segment]) -> Result<NlpReport> {
    ensure!(!segments.is_empty(), "refusing an empty annotation request");
    let package = crate::canonical(&bundle.config.join(PACKAGE))?;
    let lock = fs::read_to_string(package.join(LOCK))?;
    let digest = hex(&Sha256::digest(lock.as_bytes()));
    let environment = crate::runtime_directory()?.join(format!("nlp-{digest}"));
    let python = fs::read_to_string(package.join(PYTHON))?;
    prepare_environment(bundle, &package, &environment, &python, false)?;
    let temporary = tempfile::tempdir()?;
    let request_path = temporary.path().join(REQUEST);
    let response_path = temporary.path().join(RESPONSE);
    let request = Request {
        schema_version: SCHEMA,
        language: language.code(),
        segments: segments
            .iter()
            .map(|segment| RequestSegment {
                id: &segment.id,
                text: &segment.text,
            })
            .collect(),
    };
    fs::write(&request_path, serde_json::to_vec(&request)?)?;
    let output = bundle
        .uv()
        .current_dir(&package)
        .env("UV_PROJECT_ENVIRONMENT", environment)
        .env("PYTHONHASHSEED", "0")
        .env("PYTHONDONTWRITEBYTECODE", "1")
        .env("OMP_NUM_THREADS", "1")
        .env("OPENBLAS_NUM_THREADS", "1")
        .env("VECLIB_MAXIMUM_THREADS", "1")
        .args([
            "run",
            "--no-sync",
            "--python",
            python.trim(),
            "python",
            ADAPTER,
        ])
        .arg(&request_path)
        .arg(&response_path)
        .stdin(Stdio::null())
        .output()
        .context("start the locked local annotation engine")?;
    ensure!(
        output.status.success(),
        "annotation engine failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let report: NlpReport = serde_json::from_slice(&fs::read(response_path)?)
        .context("read the annotation response")?;
    let model = match language {
        Language::English => "en_core_web_sm",
        Language::Japanese => "ja_core_news_sm",
        Language::Chinese => "zh_core_web_sm",
    };
    ensure!(
        report.schema_version == SCHEMA
            && report.engine == ENGINE
            && report.model == model
            && report.model_version == MODEL_VERSION,
        "annotation engine or model identity differs from its pin"
    );
    ensure!(
        report.python_version == python.trim(),
        "annotation Python runtime differs from its pin"
    );
    verify_packages(&lock, &report)?;
    ensure!(
        report.annotations.len() == segments.len(),
        "annotation engine returned partial coverage"
    );
    for (annotation, segment) in report.annotations.iter().zip(segments) {
        ensure!(
            annotation.simplified.is_some()
                == (language.profile().script == rules::copyedit_rules::ScriptEngine::OpenCc),
            "the required script engine didn't run"
        );
        annotation
            .validate(segment)
            .with_context(|| format!("validate {}", segment.id))?;
    }
    Ok(report)
}
