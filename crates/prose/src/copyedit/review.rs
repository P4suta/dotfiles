use super::{
    Diagnostic, Language, NlpReport, Segment, Severity, content, diagnostics, grammar, nlp,
    project, style,
};
use crate::{Bundle, build_inputs::hex};
use anyhow::{Context, Result, ensure};
use rules::copyedit_rules::{
    Coverage, Engine, GrammarEngine, ReviewBinding, ReviewCoverage, ReviewVerdict, Verdict,
    review_verdict, verdict,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::Path;

const CONFIG_INPUTS: [&str; 14] = [
    "house.toml",
    "textlint/package.json",
    "calibration.toml",
    "vale.ini",
    "japanese.toml",
    "nlp/languagetool.toml",
    "nlp/GrammarAnnotations.java",
    "nlp/pyproject.toml",
    "nlp/uv.lock",
    "nlp/python-version",
    "nlp/annotations.py",
    "textlint/bun.lock",
    "textlint/textlintrc.json",
    "packages.toml",
];
const RULE_INPUTS: [&[u8]; 13] = [
    include_bytes!("../../../rules/src/editorial_rules.rs"),
    include_bytes!("../house.rs"),
    include_bytes!("../../../../home/dot_config/prose/house.toml"),
    include_bytes!("metadata.rs"),
    include_bytes!("content.rs"),
    include_bytes!("calibration.rs"),
    include_bytes!("grammar.rs"),
    include_bytes!("grammar_cache.rs"),
    include_bytes!("../../../rules/src/copyedit_rules.rs"),
    include_bytes!("diagnostics.rs"),
    include_bytes!("projection.rs"),
    include_bytes!("nlp.rs"),
    include_bytes!("style.rs"),
];

#[derive(Clone, Debug)]
pub struct Document {
    pub path: String,
    pub text: String,
}

impl Document {
    /// Reads one explicit source without following a directory or expanding a glob.
    ///
    /// # Errors
    ///
    /// Refuses missing, unreadable, or non-UTF-8 source files.
    pub fn read(path: &Path) -> Result<Self> {
        let path = crate::canonical(path)?;
        let text = fs::read_to_string(&path).with_context(|| format!("read {}", path.display()))?;
        Ok(Self {
            path: path.to_string_lossy().into_owned(),
            text,
        })
    }
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Source {
    pub path: String,
    pub sha256: String,
    pub segments: usize,
    pub tokens: usize,
    pub sentences: usize,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Report {
    pub schema_version: u32,
    pub producer_build_sha256: String,
    pub language: Language,
    pub configuration_sha256: String,
    pub rules_sha256: String,
    pub style_engine: String,
    pub sources: Vec<Source>,
    pub segments: Vec<Segment>,
    pub diagnostics: Vec<Diagnostic>,
    pub nlp: NlpReport,
    pub grammar: Option<grammar::Report>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ReviewDecisions {
    schema_version: u32,
    report_sha256: String,
    decisions: Vec<CandidateDecision>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct CandidateDecision {
    identity: String,
    resolution: Resolution,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(tag = "status", rename_all = "snake_case", deny_unknown_fields)]
enum Resolution {
    Pending,
    Retain { reason: String, evidence: String },
}

impl ReviewDecisions {
    /// Checks exact report binding, candidate coverage, and source quotes.
    ///
    /// # Errors
    ///
    /// Refuses errors, stale or incomplete decisions, duplicate identities, and absent source evidence.
    pub fn validate_for(&self, report: &Report) -> Result<()> {
        let findings: BTreeMap<_, _> = report
            .diagnostics
            .iter()
            .map(|item| (item.identity.as_str(), item))
            .collect();
        let mut identities = BTreeSet::new();
        let mut exact = findings.len() == report.diagnostics.len();
        let mut retained = true;
        let mut evidence = true;
        for item in &self.decisions {
            exact &= identities.insert(item.identity.as_str());
            let Some(finding) = findings.get(item.identity.as_str()) else {
                exact = false;
                continue;
            };
            match &item.resolution {
                Resolution::Pending => retained = false,
                Resolution::Retain {
                    reason,
                    evidence: quote,
                } => {
                    retained &= !reason.trim().is_empty();
                    evidence &= !quote.trim().is_empty()
                        && report.segments.iter().any(|segment| {
                            segment.path == finding.path
                                && segment.text.match_indices(quote).any(|(start, _)| {
                                    let start = segment.text[..start].chars().count();
                                    segment
                                        .source_span(start, start + quote.chars().count())
                                        .is_ok_and(|span| {
                                            span.start <= finding.start && finding.end <= span.end
                                        })
                                })
                        });
                }
            }
        }
        let coverage = ReviewCoverage {
            expected: findings.len(),
            received: self.decisions.len(),
            errors: report
                .diagnostics
                .iter()
                .filter(|item| item.severity == Severity::Error)
                .count(),
            binding: if self.schema_version == nlp::SCHEMA
                && self.report_sha256 == report.fingerprint()?
            {
                ReviewBinding::Current
            } else {
                ReviewBinding::Stale
            },
            exact,
            retained,
            evidence,
        };
        ensure!(
            review_verdict(coverage) == ReviewVerdict::Retained,
            "copyediting decisions are stale, incomplete, unevidenced, or contain errors: {coverage:?}"
        );
        Ok(())
    }
}

impl Report {
    #[must_use]
    pub fn clean(&self) -> bool {
        self.diagnostics.is_empty()
    }

    fn fingerprint(&self) -> Result<String> {
        Ok(hex(&Sha256::digest(serde_json::to_vec(self)?)))
    }

    /// Creates pending decisions bound to this report without retaining any candidate.
    ///
    /// # Errors
    ///
    /// Refuses serialization errors.
    pub fn review_template(&self) -> Result<ReviewDecisions> {
        Ok(ReviewDecisions {
            schema_version: nlp::SCHEMA,
            report_sha256: self.fingerprint()?,
            decisions: self
                .diagnostics
                .iter()
                .map(|finding| CandidateDecision {
                    identity: finding.identity.clone(),
                    resolution: Resolution::Pending,
                })
                .collect(),
        })
    }

    /// Repeats every engine with documented decisions for retained review candidates.
    ///
    /// # Errors
    ///
    /// Refuses errors, unresolved or stale decisions, changed inputs, or a different repeated result.
    pub fn verify_reviewed(&self, bundle: &Bundle, decisions: &ReviewDecisions) -> Result<()> {
        decisions.validate_for(self)?;
        self.verify_inputs_and_repeat(bundle)
    }

    /// Repeats every engine after checking the report's producer, configuration, and current source bytes.
    ///
    /// # Errors
    ///
    /// Refuses unresolved findings, stale inputs or tools, incomplete coverage, or a changed repeated result.
    pub fn verify_current(&self, bundle: &Bundle) -> Result<()> {
        ensure!(
            self.clean(),
            "copyediting report has unresolved diagnostics"
        );
        self.verify_inputs_and_repeat(bundle)
    }

    fn verify_inputs_and_repeat(&self, bundle: &Bundle) -> Result<()> {
        ensure!(
            self.schema_version == nlp::SCHEMA,
            "unsupported copyediting report schema"
        );
        ensure!(
            self.producer_build_sha256 == platform::BUILD_HASH,
            "copyediting report belongs to another source build"
        );
        ensure!(
            self.configuration_sha256 == configuration_hash(bundle)?
                && self.rules_sha256 == rules_hash(),
            "copyediting rules or configuration changed"
        );
        ensure!(!self.sources.is_empty(), "copyediting report is empty");
        let mut documents = Vec::new();
        for source in &self.sources {
            let document = Document::read(Path::new(&source.path))?;
            ensure!(
                hex(&Sha256::digest(document.text.as_bytes())) == source.sha256,
                "copyediting source changed: {}",
                source.path
            );
            documents.push(document);
        }
        let repeated = inspect(bundle, self.language, &documents)?;
        ensure!(
            serde_json::to_value(&self.diagnostics)?
                == serde_json::to_value(&repeated.diagnostics)?
                && serde_json::to_value(&self.sources)? == serde_json::to_value(&repeated.sources)?
                && serde_json::to_value(&self.segments)?
                    == serde_json::to_value(&repeated.segments)?
                && self.style_engine == repeated.style_engine,
            "repeated source coverage or diagnostics differ"
        );
        ensure!(
            serde_json::to_value(&self.nlp)? == serde_json::to_value(&repeated.nlp)?,
            "repeated linguistic annotations differ"
        );
        ensure!(
            same_grammar(self.grammar.as_ref(), repeated.grammar.as_ref())?,
            "repeated grammar annotations differ"
        );
        Ok(())
    }
}

fn same_grammar(first: Option<&grammar::Report>, second: Option<&grammar::Report>) -> Result<bool> {
    match (first, second) {
        (None, None) => Ok(true),
        (Some(first), Some(second)) => Ok(first.engine_version == second.engine_version
            && first.java_version == second.java_version
            && first.language == second.language
            && first.active_rules == second.active_rules
            && serde_json::to_value(&first.annotations)?
                == serde_json::to_value(&second.annotations)?
            && serde_json::to_value(&first.calibration)?
                == serde_json::to_value(&second.calibration)?),
        _ => Ok(false),
    }
}

fn source_findings(
    language: Language,
    segments: &[Segment],
    nlp: &NlpReport,
    texts: &BTreeMap<&String, &String>,
    house: &crate::house::Policy,
) -> Result<Vec<Diagnostic>> {
    let mut findings = content::check(language, segments, nlp, texts)?;
    for (segment, annotation) in segments.iter().zip(&nlp.annotations) {
        let source = texts
            .get(&segment.path)
            .context("projected text lost its source")?;
        findings.extend(house.findings(language, segment, annotation, source)?);
        findings.extend(diagnostics::check(language, segment, annotation, source)?);
    }
    Ok(findings)
}

fn configuration_hash(bundle: &Bundle) -> Result<String> {
    let mut hasher = Sha256::new();
    hasher.update(crate::tree_digest(&bundle.config.join("styles"))?.as_bytes());
    hasher.update([0]);
    for input in CONFIG_INPUTS {
        hasher.update(input.as_bytes());
        hasher.update([0]);
        hasher.update(fs::read(bundle.config.join(input))?);
        hasher.update([0]);
    }
    Ok(hex(&hasher.finalize()))
}

fn rules_hash() -> String {
    let mut hasher = Sha256::new();
    for input in RULE_INPUTS {
        hasher.update(input);
        hasher.update([0]);
    }
    hex(&hasher.finalize())
}

/// Runs every applicable engine and binds its complete annotations and findings to the source bytes.
///
/// # Errors
///
/// Refuses empty or duplicate source sets, invalid Markdown projections, unavailable engines, partial annotations, or invalid external spans.
pub fn inspect(bundle: &Bundle, language: Language, documents: &[Document]) -> Result<Report> {
    ensure!(
        !documents.is_empty(),
        "copyediting needs explicit source files"
    );
    let mut seen = BTreeSet::new();
    let mut texts = BTreeMap::new();
    let mut segments = Vec::new();
    for document in documents {
        ensure!(
            seen.insert(&document.path),
            "duplicate copyediting source {}",
            document.path
        );
        texts.insert(&document.path, &document.text);
        segments.extend(project(&document.path, &document.text)?);
    }
    let configuration_sha256 = configuration_hash(bundle)?;
    let house = crate::house::Policy::configured(bundle)?;
    let nlp = nlp::analyze(bundle, language, &segments)?;
    let (style_engine, mut findings) = style::check(bundle, language, documents)?;
    for document in documents {
        findings.extend(crate::house::layout_findings(
            &document.path,
            &document.text,
        )?);
    }
    let grammar = match language.profile().grammar {
        GrammarEngine::NativeJapanese => None,
        GrammarEngine::LanguageTool => {
            let (report, grammar_findings) =
                grammar::analyze(bundle, language, &segments, &nlp, &texts)?;
            findings.extend(grammar_findings);
            Some(report)
        }
    };
    findings.extend(source_findings(language, &segments, &nlp, &texts, &house)?);
    let coverage = Coverage {
        expected: segments.len(),
        received: nlp.annotations.len(),
        identity: true,
        spans: true,
        language_engine: Engine::Complete,
        diagnostics: !findings.is_empty(),
    };
    ensure!(
        verdict(coverage) != Verdict::Incomplete,
        "copyediting coverage is incomplete"
    );
    findings.sort_by(|left, right| {
        (&left.path, left.start, &left.rule).cmp(&(&right.path, right.start, &right.rule))
    });
    ensure!(
        configuration_hash(bundle)? == configuration_sha256,
        "copyediting configuration changed during the run"
    );
    let sources = documents
        .iter()
        .map(|document| {
            let annotations: Vec<_> = segments
                .iter()
                .zip(&nlp.annotations)
                .filter(|(segment, _)| segment.path == document.path)
                .map(|(_, annotation)| annotation)
                .collect();
            Source {
                path: document.path.clone(),
                sha256: hex(&Sha256::digest(document.text.as_bytes())),
                segments: annotations.len(),
                tokens: annotations
                    .iter()
                    .map(|annotation| annotation.tokens.len())
                    .sum(),
                sentences: annotations
                    .iter()
                    .map(|annotation| annotation.sentences.len())
                    .sum(),
            }
        })
        .collect();
    Ok(Report {
        schema_version: nlp::SCHEMA,
        producer_build_sha256: platform::BUILD_HASH.to_owned(),
        language,
        configuration_sha256,
        rules_sha256: rules_hash(),
        style_engine,
        sources,
        segments,
        diagnostics: findings,
        nlp,
        grammar,
    })
}
