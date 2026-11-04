use super::{Diagnostic, Language, NlpReport, Segment, Severity, calibration, grammar_cache};
use crate::{Bundle, build_inputs::hex};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::fs;
use std::process::Stdio;

const SCHEMA: u32 = 1;
const CLASSES: &str = "classes";
const CLASS: &str = "GrammarAnnotations";
const REQUEST: &str = "request.json";
const RESPONSE: &str = "response.json";
const JAVA_RELEASE: &str = "25";
const MEMORY: &str = "-Xmx512m";
const CPU: &str = "-XX:ActiveProcessorCount=1";

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Match {
    pub start_utf16: usize,
    pub end_utf16: usize,
    pub rule: String,
    pub message: String,
    pub category: String,
    pub issue_type: String,
    pub spelling: bool,
    pub replacements: Vec<String>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Annotation {
    pub id: String,
    pub text_sha256: String,
    pub matches: Vec<Match>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Report {
    pub schema_version: u32,
    pub engine_version: String,
    pub java_version: String,
    pub reference_start_date: String,
    pub reference_end_date: String,
    pub language: String,
    pub active_rules: Vec<String>,
    pub annotations: Vec<Annotation>,
    #[serde(default)]
    pub calibration: Vec<calibration::Applied>,
}

#[derive(Serialize)]
struct Request<'a> {
    schema_version: u32,
    language: &'static str,
    segments: Vec<Input<'a>>,
}

#[derive(Serialize)]
struct Input<'a> {
    id: &'a str,
    text: &'a str,
}

pub(super) fn offset(text: &str, utf16: usize) -> Result<usize> {
    let mut units = 0;
    for (index, character) in text.chars().enumerate() {
        if units == utf16 {
            return Ok(index);
        }
        units += character.len_utf16();
    }
    ensure!(
        units == utf16,
        "grammar offset splits a surrogate pair or exceeds its source"
    );
    Ok(text.chars().count())
}

fn findings(
    report: &mut Report,
    language: Language,
    calibration: &calibration::Calibration,
    segments: &[Segment],
    nlp: &NlpReport,
    sources: &BTreeMap<&String, &String>,
) -> Result<Vec<Diagnostic>> {
    ensure!(
        report.annotations.len() == segments.len(),
        "grammar engine returned partial coverage"
    );
    let mut findings = Vec::new();
    for ((annotation, segment), parsed) in report
        .annotations
        .iter()
        .zip(segments)
        .zip(&nlp.annotations)
    {
        ensure!(
            annotation.id == segment.id
                && annotation.text_sha256 == hex(&Sha256::digest(segment.text.as_bytes())),
            "grammar annotation differs from its source"
        );
        for candidate in &annotation.matches {
            ensure!(
                candidate.start_utf16 <= candidate.end_utf16
                    && !candidate.rule.is_empty()
                    && !candidate.message.is_empty(),
                "invalid grammar match"
            );
            let start = offset(&segment.text, candidate.start_utf16)?;
            let end = offset(&segment.text, candidate.end_utf16)?;
            if start < end
                && let Some(reference) = parsed
                    .tokens
                    .iter()
                    .filter(|token| token.start <= start && end <= token.end)
                    .find_map(|token| {
                        calibration.accepted(language, &candidate.rule, &segment.text, token)
                    })
            {
                report.calibration.push(calibration::Applied {
                    segment: segment.id.clone(),
                    start,
                    end,
                    reference: reference.clone(),
                });
                continue;
            }
            if segment.quoted
                || (candidate.spelling
                    && parsed
                        .entities
                        .iter()
                        .any(|entity| entity.start <= start && end <= entity.end))
            {
                continue;
            }
            let start = if start == end {
                start.min(segment.text.chars().count() - 1)
            } else {
                start
            };
            let end = end.max(start + 1);
            let span = segment.source_span(start, end)?;
            let source = sources
                .get(&segment.path)
                .context("grammar source disappeared")?;
            findings.push(Diagnostic::source(
                &segment.path,
                source,
                span,
                &format!("LanguageTool.{}", candidate.rule),
                Severity::Review,
                "languagetool",
                &candidate.message,
            ));
        }
    }
    Ok(findings)
}

pub fn analyze(
    bundle: &Bundle,
    language: Language,
    segments: &[Segment],
    nlp: &NlpReport,
    sources: &BTreeMap<&String, &String>,
) -> Result<(Report, Vec<Diagnostic>)> {
    let installation = grammar_cache::prepare(bundle)?;
    let temporary = tempfile::tempdir()?;
    let classes = temporary.path().join(CLASSES);
    fs::create_dir(&classes)?;
    let output = installation
        .command("javac")
        .args([
            "-Xlint:all",
            "-Werror",
            "--release",
            JAVA_RELEASE,
            "--class-path",
        ])
        .arg(installation.compiler_classpath()?)
        .arg("-d")
        .arg(&classes)
        .arg(crate::canonical(
            &bundle.config.join(grammar_cache::ADAPTER),
        )?)
        .stdin(Stdio::null())
        .output()
        .context("typecheck the grammar library adapter")?;
    ensure!(
        output.status.success(),
        "grammar adapter typecheck failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let grammar_language = match language {
        Language::English => "en-US",
        Language::Japanese => "ja-JP",
        Language::Chinese => "zh-CN",
    };
    let request = Request {
        schema_version: SCHEMA,
        language: grammar_language,
        segments: segments
            .iter()
            .map(|segment| Input {
                id: &segment.id,
                text: &segment.text,
            })
            .collect(),
    };
    let request_path = temporary.path().join(REQUEST);
    let response_path = temporary.path().join(RESPONSE);
    fs::write(&request_path, serde_json::to_vec(&request)?)?;
    let output = installation
        .command("java")
        .args([
            MEMORY,
            CPU,
            "-Djava.awt.headless=true",
            "-Dfile.encoding=UTF-8",
            "-Duser.language=en",
            "-Duser.country=US",
            "-Duser.timezone=UTC",
            "--class-path",
        ])
        .arg(installation.classpath(Some(&classes))?)
        .arg(CLASS)
        .arg(&request_path)
        .arg(&response_path)
        .stdin(Stdio::null())
        .output()
        .context("run the pinned local grammar engine")?;
    ensure!(
        output.status.success(),
        "grammar engine failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let mut report: Report = serde_json::from_slice(&fs::read(response_path)?)
        .context("read the grammar annotation response")?;
    ensure!(
        report.schema_version == SCHEMA
            && report.engine_version == installation.pin.version
            && report.language == grammar_language
            && !report.java_version.is_empty(),
        "grammar engine identity differs from its pin"
    );
    ensure!(
        !report.active_rules.is_empty() && report.active_rules.iter().all(|rule| !rule.is_empty()),
        "grammar engine loaded no applicable rules"
    );
    let calibration = calibration::Calibration::read(bundle)?;
    let findings = findings(&mut report, language, &calibration, segments, nlp, sources)?;
    Ok((report, findings))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn java_offsets_preserve_supplementary_unicode_and_reject_half_surrogates() -> Result<()> {
        assert_eq!(offset("A🚋京都", 0)?, 0);
        assert_eq!(offset("A🚋京都", 1)?, 1);
        assert!(offset("A🚋京都", 2).is_err());
        assert_eq!(offset("A🚋京都", 3)?, 2);
        assert_eq!(offset("A🚋京都", 5)?, 4);
        assert!(offset("A🚋京都", 6).is_err());
        Ok(())
    }
}
