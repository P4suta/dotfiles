use super::{Diagnostic, Document, Language, Severity};
use crate::{Bundle, Channel};
use anyhow::{Context, Result, ensure};
use rules::copyedit_rules::StyleEngine;
use serde::Deserialize;
use std::fs;
use std::process::Stdio;

const TEXTLINT: &str = "node_modules/textlint/bin/textlint.js";
const CONFIG: &str = "textlint/textlintrc.json";
const FINDINGS: i32 = 1;

#[derive(Deserialize)]
struct TextlintFile {
    #[serde(rename = "filePath")]
    path: String,
    messages: Vec<NativeStyleMessage>,
}

#[derive(Debug, Deserialize)]
pub struct NativeStyleMessage {
    #[serde(rename = "ruleId")]
    rule: String,
    line: usize,
    column: usize,
    range: [usize; 2],
    message: String,
}

fn textlint_span(
    document: &Document,
    message: &NativeStyleMessage,
) -> Result<std::ops::Range<usize>> {
    ensure!(
        message.range[0] <= message.range[1]
            && message.range[1] <= document.text.encode_utf16().count(),
        "invalid textlint range"
    );
    if let Ok(span) = located(document, message.line, message.column) {
        return Ok(span);
    }
    let byte = |units| -> Result<usize> {
        let scalar = super::grammar::offset(&document.text, units)?;
        Ok(document
            .text
            .char_indices()
            .nth(scalar)
            .map_or(document.text.len(), |(index, _)| index))
    };
    let span = byte(message.range[0])?..byte(message.range[1])?;
    let line = document
        .text
        .get(..span.start)
        .context("textlint range start")?
        .bytes()
        .filter(|byte| *byte == b'\n')
        .count()
        + 1;
    ensure!(
        line == message.line,
        "textlint range and source line disagree"
    );
    Ok(span)
}

impl NativeStyleMessage {
    /// Maps textlint's character columns, using its absolute UTF-16 range when a rule returns an invalid column.
    ///
    /// # Errors
    ///
    /// Refuses blank diagnostic content, inverted or out-of-bounds ranges, and unresolvable source positions.
    pub fn diagnostic(&self, document: &Document) -> Result<Diagnostic> {
        ensure!(
            !self.rule.trim().is_empty() && !self.message.trim().is_empty(),
            "textlint returned a blank diagnostic"
        );
        let span = textlint_span(document, self).with_context(|| {
            format!(
                "locate textlint diagnostic {}:{}:{}",
                document.path, self.line, self.column
            )
        })?;
        Ok(Diagnostic::source(
            &document.path,
            &document.text,
            span,
            &self.rule,
            Severity::Error,
            "textlint-ja-technical-writing",
            &self.message,
        ))
    }
}

fn located(document: &Document, line: usize, column: usize) -> Result<std::ops::Range<usize>> {
    ensure!(line > 0 && column > 0, "invalid style diagnostic location");
    let mut offset = 0;
    for (index, text) in document.text.split_inclusive('\n').enumerate() {
        if index + 1 == line {
            let column = text
                .char_indices()
                .nth(column - 1)
                .context("style diagnostic column is outside its source")?;
            return Ok(offset + column.0..offset + column.0 + column.1.len_utf8());
        }
        offset += text.len();
    }
    anyhow::bail!("style diagnostic line is outside its source")
}

pub fn check(
    bundle: &Bundle,
    language: Language,
    documents: &[Document],
) -> Result<(String, Vec<Diagnostic>)> {
    match language.profile().style {
        StyleEngine::Common => Ok(("common linguistic rules".to_owned(), Vec::new())),
        StyleEngine::Vale => {
            let mut findings = Vec::new();
            for document in documents {
                for finding in
                    crate::check(bundle, Channel::Document, &document.path, &document.text)?
                {
                    let span = located(document, finding.line, finding.column)?;
                    findings.push(Diagnostic::source(
                        &document.path,
                        &document.text,
                        span,
                        &finding.rule,
                        Severity::Error,
                        "vale+english-policy",
                        &finding.message,
                    ));
                }
            }
            Ok(("Vale and persisted English policy".to_owned(), findings))
        }
        StyleEngine::Textlint => japanese(bundle, documents),
    }
}

fn japanese(bundle: &Bundle, documents: &[Document]) -> Result<(String, Vec<Diagnostic>)> {
    crate::install_textlint(
        &bundle.config.join("textlint"),
        &bundle.textlint,
        bundle.bun(),
    )?;
    let directory = tempfile::tempdir()?;
    let mut paths = Vec::new();
    for (index, document) in documents.iter().enumerate() {
        let path = directory.path().join(format!("{index}.md"));
        fs::write(&path, &document.text)?;
        paths.push(path);
    }
    let output = bundle
        .bun()
        .current_dir(directory.path())
        .arg(bundle.textlint.join(TEXTLINT))
        .arg("--config")
        .arg(crate::canonical(&bundle.config.join(CONFIG))?)
        .arg("--rules-base-directory")
        .arg(crate::canonical(&bundle.textlint.join("node_modules"))?)
        .args(["--format", "json"])
        .args(&paths)
        .stdin(Stdio::null())
        .output()
        .context("start pinned Japanese textlint rules")?;
    ensure!(
        output.status.success() || output.status.code() == Some(FINDINGS),
        "Japanese textlint failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let records: Vec<TextlintFile> =
        serde_json::from_slice(&output.stdout).context("read the Japanese textlint response")?;
    ensure!(
        records.len() == documents.len(),
        "Japanese textlint returned partial file coverage"
    );
    let mut seen = std::collections::BTreeSet::new();
    let mut findings = Vec::new();
    for record in records {
        let index = paths
            .iter()
            .position(|path| path.to_string_lossy() == record.path)
            .context("textlint returned an unknown file")?;
        ensure!(seen.insert(index), "textlint returned a duplicate file");
        let document = &documents[index];
        for message in record.messages {
            findings.push(message.diagnostic(document)?);
        }
        let input = crate::Input::new(&document.path, &document.text, true);
        for finding in crate::sentence_per_line(&input) {
            findings.push(Diagnostic::source(
                &document.path,
                &document.text,
                located(document, finding.line, finding.column)?,
                &finding.rule,
                Severity::Error,
                "sentence-layout",
                &finding.message,
            ));
        }
    }
    Ok((
        "textlint Japanese technical writing and sentence layout".to_owned(),
        findings,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn document() -> Document {
        Document {
            path: "article.md".to_owned(),
            text: "先頭。\n本文😀です。".to_owned(),
        }
    }

    fn message(range: [usize; 2]) -> Result<NativeStyleMessage> {
        Ok(serde_json::from_value(serde_json::json!({
            "ruleId": "sentence-length", "line": 2, "column": 6021,
            "range": range, "message": "Review this sentence."
        }))?)
    }

    #[test]
    fn textlint_ranges_preserve_unicode_when_a_rule_reports_an_invalid_column() -> Result<()> {
        let document = document();
        let span = textlint_span(&document, &message([8, 10])?)?;
        assert_eq!(&document.text[span.clone()], "です");
        let finding = Diagnostic::source(
            &document.path,
            &document.text,
            span,
            "sentence-length",
            Severity::Error,
            "textlint",
            "Review this sentence.",
        );
        assert_eq!((finding.line, finding.column), (2, 4));
        Ok(())
    }

    #[test]
    fn textlint_ranges_refuse_invalid_boundaries_and_preserve_eof_insertions() -> Result<()> {
        let document = document();
        for range in [[7, 8], [8, 7], [0, usize::MAX]] {
            assert!(textlint_span(&document, &message(range)?).is_err());
        }
        let eof = document.text.encode_utf16().count();
        assert_eq!(
            textlint_span(&document, &message([eof, eof])?)?,
            document.text.len()..document.text.len()
        );
        Ok(())
    }

    #[test]
    fn japanese_register_accepts_consistent_plain_and_polite_prose_but_refuses_mixing() -> Result<()>
    {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(Path::parent)
            .context("workspace root")?;
        let bundle = Bundle::source(root)?;
        for text in ["庭園は静かである。\n", "庭園は静かです。\n"] {
            let (_, findings) = japanese(
                &bundle,
                &[Document {
                    path: "register.md".to_owned(),
                    text: text.to_owned(),
                }],
            )?;
            assert_eq!(
                findings
                    .iter()
                    .map(|finding| finding.rule.as_str())
                    .collect::<Vec<_>>(),
                [] as [&str; 0]
            );
        }
        let (_, findings) = japanese(
            &bundle,
            &[Document {
                path: "register.md".to_owned(),
                text: "庭園は静かである。\n池は広いです。\n".to_owned(),
            }],
        )?;
        assert!(
            findings
                .iter()
                .any(|finding| finding.rule == "ja-technical-writing/no-mix-dearu-desumasu")
        );
        Ok(())
    }
}
