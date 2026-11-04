//! The Japanese word rules of `japanese.toml`, which Vale never reads.
//! Each rule refuses a line in Japanese script that holds one of its tokens outside code.

use crate::{Bundle, Finding, masked};
use anyhow::{Context, Result};
use serde::Deserialize;
use std::collections::BTreeMap;
use std::fs;

/// The file in the prose configuration that holds the Japanese word rules.
pub const RULES: &str = "japanese.toml";

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct File {
    rules: BTreeMap<String, Rule>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Rule {
    message: String,
    tokens: Vec<String>,
    /// Longer words that contain a token and remain allowed, such as a conjunction that starts with a refused particle.
    #[serde(default)]
    except: Vec<String>,
}

/// The Japanese word rules of one prose configuration.
#[derive(Debug)]
pub struct WordRules(BTreeMap<String, Rule>);

impl WordRules {
    /// Reads the rules of an installed or source prose configuration.
    ///
    /// # Errors
    ///
    /// Returns an error when reading or parsing the file fails.
    pub fn read(bundle: &Bundle) -> Result<Self> {
        let path = bundle.config.join(RULES);
        let text = fs::read_to_string(&path)
            .with_context(|| format!("read the Japanese word rules at {}", path.display()))?;
        let file: File = toml::from_str(&text)
            .with_context(|| format!("parse the Japanese word rules at {}", path.display()))?;
        Ok(Self(file.rules))
    }

    /// One finding for each token outside code on each line in Japanese script.
    #[must_use]
    pub fn findings(&self, path: &str, text: &str) -> Vec<Finding> {
        let prose = masked(text);
        let mut findings = Vec::new();
        for (index, line) in prose.lines().enumerate() {
            if !line.chars().any(japanese) {
                continue;
            }
            for (name, rule) in &self.0 {
                for token in &rule.tokens {
                    for (start, _) in line.match_indices(token.as_str()) {
                        if excepted(line, start, token, &rule.except) {
                            continue;
                        }
                        findings.push(Finding {
                            path: path.to_owned(),
                            line: index + 1,
                            column: line[..start].chars().count() + 1,
                            rule: name.clone(),
                            message: rule.message.clone(),
                            sentence: line.trim().to_owned(),
                        });
                    }
                }
            }
        }
        findings
    }
}

/// Whether the token at `start` lies inside one of the allowed longer words.
fn excepted(line: &str, start: usize, token: &str, except: &[String]) -> bool {
    except.iter().any(|word| {
        line.match_indices(word.as_str()).any(|(word_start, _)| {
            word_start <= start && start + token.len() <= word_start + word.len()
        })
    })
}

/// Hiragana, Katakana, and the unified Han ideographs that Japanese prose uses.
fn japanese(character: char) -> bool {
    matches!(character, '\u{3040}'..='\u{30FF}' | '\u{4E00}'..='\u{9FFF}')
}

#[cfg(test)]
mod tests {
    use super::*;

    const RULES_TEXT: &str = "[rules.\"Dotfiles.Restriction\"]\nmessage = \"State the scope affirmatively.\"\ntokens = [\"\u{3060}\u{3051}\"]\nexcept = [\"\u{3060}\u{3051}\u{3069}\"]\n";

    fn rules(text: &str) -> Result<WordRules> {
        let config = tempfile::tempdir()?;
        fs::write(config.path().join(RULES), text)?;
        WordRules::read(&Bundle {
            config: config.path().to_path_buf(),
            textlint: std::path::PathBuf::new(),
            vale: None,
        })
    }

    #[test]
    fn a_token_on_a_japanese_line_is_refused_outside_code_and_its_exceptions() {
        let rules = rules(RULES_TEXT).unwrap();
        let particle = "\u{3060}\u{3051}";
        let text = format!(
            "\u{3053}\u{308c}{particle}\u{3067}\u{3059}\u{3002}\n\u{305d}\u{3046}{particle}\u{3069}\u{3001}\u{3088}\u{3044}\u{3002}\n`{particle}` \u{3092}\u{898b}\u{308b}\u{3002}\nOnly English here.\n"
        );
        let findings = rules.findings("reply", &text);
        assert_eq!(findings.len(), 1, "{findings:?}");
        assert_eq!(findings[0].line, 1);
        assert_eq!(findings[0].column, 3);
        assert_eq!(findings[0].rule, "Dotfiles.Restriction");
    }

    #[test]
    fn a_file_with_an_unknown_field_is_refused() {
        assert!(rules("[rules.\"X\"]\nmessage = \"m\"\ntokens = []\nother = 1\n").is_err());
    }
}
