use super::{Language, Lexeme};
use crate::Bundle;
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
use std::fs;

pub const CONFIG: &str = "calibration.toml";
const SCHEMA: u32 = 1;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Calibration {
    schema_version: u32,
    lexeme: Vec<LexicalRecord>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct LexicalRecord {
    language: Language,
    rule: String,
    word: String,
    part_of_speech: String,
    blocked_contexts: Vec<String>,
    source: String,
    reference: String,
    reason: String,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Applied {
    pub segment: String,
    pub start: usize,
    pub end: usize,
    pub reference: LexicalRecord,
}

impl Calibration {
    pub fn read(bundle: &Bundle) -> Result<Self> {
        let policy: Self = toml::from_str(&fs::read_to_string(bundle.config.join(CONFIG))?)?;
        ensure!(
            policy.schema_version == SCHEMA,
            "unsupported linguistic calibration schema"
        );
        for reference in &policy.lexeme {
            ensure!(
                !reference.rule.is_empty()
                    && reference.word.chars().count() > 1
                    && matches!(reference.part_of_speech.as_str(), "NOUN" | "PROPN"),
                "lexical calibration needs a rule and a complete noun or name"
            );
            ensure!(
                reference.source.starts_with("https://")
                    && !reference.reference.is_empty()
                    && !reference.reason.is_empty(),
                "lexical calibration needs its reference and reason"
            );
            ensure!(
                reference
                    .blocked_contexts
                    .iter()
                    .all(|context| !context.is_empty()),
                "lexical counterexamples must not be empty"
            );
        }
        Ok(policy)
    }

    pub fn accepted(
        &self,
        language: Language,
        rule: &str,
        text: &str,
        token: &Lexeme,
    ) -> Option<&LexicalRecord> {
        self.lexeme.iter().find(|reference| {
            reference.language == language
                && reference.rule == rule
                && rules::copyedit_rules::calibrated_word(rules::copyedit_rules::Calibration {
                    word: reference.word == token.text,
                    role: reference.part_of_speech == token.pos,
                    counterexample: reference
                        .blocked_contexts
                        .iter()
                        .any(|context| text.contains(context)),
                })
        })
    }
}
