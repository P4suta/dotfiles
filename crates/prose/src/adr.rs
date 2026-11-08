use crate::{Finding, Input};
use pulldown_cmark::{Event, HeadingLevel, Options, Parser, Tag, TagEnd};
use rules::adr_rules::{
    Content, ContentVerdict, RECORD_CHARACTER_LIMIT, RECORD_WORD_LIMIT, content_verdict,
};
use std::collections::BTreeSet;

const COLLECTIONS: &[&str] = &[
    "/docs/adr/",
    "/docs/adrs/",
    "/docs/decisions/",
    "/doc/adr/",
    "/doc/arch/",
];

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
enum Section {
    Context,
    Decision,
    Alternatives,
    Consequences,
    Confirmation,
}

impl Section {
    const ALL: [Self; 5] = [
        Self::Context,
        Self::Decision,
        Self::Alternatives,
        Self::Consequences,
        Self::Confirmation,
    ];

    fn name(self) -> &'static str {
        match self {
            Self::Context => "Context",
            Self::Decision => "Decision",
            Self::Alternatives => "Alternatives",
            Self::Consequences => "Consequences",
            Self::Confirmation => "Confirmation",
        }
    }

    fn parse(text: &str) -> Option<Self> {
        match text.trim().to_ascii_lowercase().as_str() {
            "context" | "context and problem statement" => Some(Self::Context),
            "decision" | "decision outcome" => Some(Self::Decision),
            "alternatives" | "considered options" => Some(Self::Alternatives),
            "consequences" => Some(Self::Consequences),
            "confirmation" => Some(Self::Confirmation),
            _ => None,
        }
    }
}

#[must_use]
pub fn is_record(path: &str) -> bool {
    let normalized = format!("/{}", path.replace('\\', "/"));
    let Some(file) = normalized.rsplit('/').next() else {
        return false;
    };
    file.to_ascii_lowercase().ends_with(".md")
        && !file.eq_ignore_ascii_case("README.md")
        && COLLECTIONS
            .iter()
            .any(|collection| normalized.contains(collection))
}

fn populated_sections(text: &str) -> BTreeSet<Section> {
    let mut found = BTreeSet::new();
    let mut current = None;
    let mut heading = None;
    let mut label = String::new();
    let mut paragraph = false;
    for event in Parser::new_ext(text, Options::ENABLE_YAML_STYLE_METADATA_BLOCKS) {
        match event {
            Event::Start(Tag::Heading { level, .. }) => {
                heading = Some(level);
                label.clear();
            }
            Event::End(TagEnd::Heading(level)) => {
                let section = Section::parse(&label);
                if section.is_some() || level == HeadingLevel::H2 {
                    current = section;
                }
                heading = None;
            }
            Event::Start(Tag::Paragraph) => paragraph = true,
            Event::End(TagEnd::Paragraph) => paragraph = false,
            Event::Text(value) | Event::Code(value) => {
                if heading.is_some() {
                    label.push_str(&value);
                } else if paragraph
                    && value.chars().any(char::is_alphanumeric)
                    && let Some(section) = current
                {
                    found.insert(section);
                }
            }
            _ => {}
        }
    }
    found
}

#[must_use]
pub fn check(path: &str, text: &str) -> Vec<Finding> {
    if !is_record(path) {
        return Vec::new();
    }
    let populated = populated_sections(text);
    let missing: Vec<_> = Section::ALL
        .into_iter()
        .filter(|section| !populated.contains(section))
        .map(Section::name)
        .collect();
    let content = Content {
        words: text
            .split_whitespace()
            .filter(|word| word.chars().any(char::is_alphanumeric))
            .count(),
        characters: text
            .chars()
            .filter(|character| !character.is_whitespace())
            .count(),
        complete: missing.is_empty(),
    };
    let (rule, message) = match content_verdict(content) {
        ContentVerdict::Admit => return Vec::new(),
        ContentVerdict::WordBudget => (
            "ADR.WordBudget",
            format!(
                "ADR has {} words; the limit is {RECORD_WORD_LIMIT}, including metadata and code. Remove repetition and link supporting evidence.",
                content.words
            ),
        ),
        ContentVerdict::CharacterBudget => (
            "ADR.CharacterBudget",
            format!(
                "ADR has {} non-whitespace characters; the limit is {RECORD_CHARACTER_LIMIT}. Shorten the record.",
                content.characters
            ),
        ),
        ContentVerdict::Incomplete => (
            "ADR.RequiredContent",
            format!(
                "Write substantive content under {}. Empty headings and fenced examples don't supply a decision.",
                missing.join(", ")
            ),
        ),
    };
    vec![Input::new(path, text, true).finding(1, 1, rule, &message)]
}

#[cfg(test)]
mod tests {
    use super::*;

    const VALID: &str = "# Bound the queue\n\n## Context\nA disconnected server can exhaust memory.\n\n## Decision\nRetain one request.\n\n## Alternatives\nAn unlimited queue retains every request but can exhaust memory.\n\n## Consequences\nOlder pending requests are lost.\n\n## Confirmation\nVerify that a disconnected server leaves one request queued.\n";
    const PATH: &str = "docs/adr/bound-the-queue.md";

    #[test]
    fn valid_records_and_existing_equivalent_headings_pass() {
        assert_eq!(check(PATH, VALID), [] as [Finding; 0]);
        let equivalent = VALID
            .replace("## Context", "## Context and Problem Statement")
            .replace("## Decision", "## Decision Outcome")
            .replace("## Alternatives", "## Considered Options")
            .replace("## Consequences", "### Consequences")
            .replace("## Confirmation", "### Confirmation");
        assert_eq!(check(PATH, &equivalent), [] as [Finding; 0]);
    }

    #[test]
    fn headings_inside_code_and_metadata_cannot_supply_required_content() {
        for source in [
            format!("```markdown\n{VALID}\n```"),
            format!("---\nexample: |\n  {}\n---\n", VALID.replace('\n', "\n  ")),
            VALID.replace("Retain one request.", "<!-- no decision -->"),
        ] {
            assert!(
                check(PATH, &source)
                    .iter()
                    .any(|finding| finding.rule == "ADR.RequiredContent"),
                "{source}"
            );
        }
    }

    #[test]
    fn all_source_text_consumes_the_budget() {
        for source in [
            format!(
                "{VALID}\n```\n{}\n```\n",
                "detail ".repeat(RECORD_WORD_LIMIT)
            ),
            format!(
                "---\nnotes: {}\n---\n{VALID}",
                "detail ".repeat(RECORD_WORD_LIMIT)
            ),
            format!("{VALID}\n<!-- {} -->", "detail ".repeat(RECORD_WORD_LIMIT)),
        ] {
            assert!(
                check(PATH, &source)
                    .iter()
                    .any(|finding| finding.rule == "ADR.WordBudget")
            );
        }
        let source = format!("{VALID}{}", "長".repeat(RECORD_CHARACTER_LIMIT));
        assert!(
            check(PATH, &source)
                .iter()
                .any(|finding| finding.rule == "ADR.CharacterBudget")
        );
    }

    #[test]
    fn collection_detection_handles_native_paths_without_claiming_other_documents() {
        for path in [
            PATH,
            "/project/docs/decisions/queue.md",
            "C:\\project\\docs\\adr\\queue.md",
        ] {
            assert!(is_record(path), "{path}");
        }
        for path in [
            "docs/adr/README.md",
            "home/dot_agents/skills/adr/SKILL.md",
            "docs/architecture.md",
            "docs/adr/queue.json",
        ] {
            assert!(!is_record(path), "{path}");
        }
    }
}
