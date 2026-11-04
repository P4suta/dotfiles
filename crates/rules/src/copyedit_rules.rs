//! Decisions over validated annotations and visible prose boundaries.

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(kani, derive(kani::Arbitrary))]
pub enum Language {
    English,
    Japanese,
    Chinese,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GrammarEngine {
    LanguageTool,
    NativeJapanese,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StyleEngine {
    Vale,
    Textlint,
    Common,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ScriptEngine {
    OpenCc,
    Unicode,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Profile {
    pub grammar: GrammarEngine,
    pub style: StyleEngine,
    pub script: ScriptEngine,
}

#[must_use]
pub const fn profile(language: Language) -> Profile {
    match language {
        Language::English => Profile {
            grammar: GrammarEngine::LanguageTool,
            style: StyleEngine::Vale,
            script: ScriptEngine::Unicode,
        },
        Language::Japanese => Profile {
            grammar: GrammarEngine::NativeJapanese,
            style: StyleEngine::Textlint,
            script: ScriptEngine::Unicode,
        },
        Language::Chinese => Profile {
            grammar: GrammarEngine::LanguageTool,
            style: StyleEngine::Common,
            script: ScriptEngine::OpenCc,
        },
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(kani, derive(kani::Arbitrary))]
pub enum AgreementClass {
    ThirdSingular,
    Other,
    Unknown,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(kani, derive(kani::Arbitrary))]
pub struct AgreementContext {
    pub present: bool,
    pub coordinated: bool,
    pub protected: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(kani, derive(kani::Arbitrary))]
pub struct Calibration {
    pub word: bool,
    pub role: bool,
    pub counterexample: bool,
}

#[must_use]
pub const fn calibrated_word(facts: Calibration) -> bool {
    facts.word && facts.role && !facts.counterexample
}

const DUPLICATE_CHARACTERS: u32 = 30;
const DUPLICATE_WORDS: u32 = 12;
const SIMILARITY_DENOMINATOR: u64 = 100;
const SIMILARITY_NUMERATOR: u64 = 90;
const ENGLISH_SENTENCE_WORDS: u32 = 45;
const JAPANESE_SENTENCE_WORDS: u32 = 70;
const CHINESE_SENTENCE_WORDS: u32 = 55;
const CHAIN_CLAUSES: u32 = 4;
const CHAIN_DISTANCE: u32 = 24;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(kani, derive(kani::Arbitrary))]
pub struct Reuse {
    pub exact: bool,
    pub facts_match: bool,
    pub characters: u32,
    pub words: u32,
    pub shared: u32,
    pub union: u32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReuseKind {
    None,
    Exact,
    Near,
}

#[must_use]
pub fn duplication(facts: Reuse) -> ReuseKind {
    if facts.characters < DUPLICATE_CHARACTERS {
        ReuseKind::None
    } else if facts.exact {
        ReuseKind::Exact
    } else if facts.facts_match
        && facts.words >= DUPLICATE_WORDS
        && facts.union > 0
        && facts.shared <= facts.union
        && u64::from(facts.shared) * SIMILARITY_DENOMINATOR
            >= u64::from(facts.union) * SIMILARITY_NUMERATOR
    {
        ReuseKind::Near
    } else {
        ReuseKind::None
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(kani, derive(kani::Arbitrary))]
pub struct Complexity {
    pub words: u32,
    pub clauses: u32,
    pub distance: u32,
}

#[must_use]
pub fn complex_sentence(language: Language, facts: Complexity) -> bool {
    let words = match language {
        Language::English => ENGLISH_SENTENCE_WORDS,
        Language::Japanese => JAPANESE_SENTENCE_WORDS,
        Language::Chinese => CHINESE_SENTENCE_WORDS,
    };
    facts.words >= words && facts.clauses >= CHAIN_CLAUSES && facts.distance >= CHAIN_DISTANCE
}

#[must_use]
pub fn disagrees(subject: AgreementClass, verb: AgreementClass, context: AgreementContext) -> bool {
    context.present
        && !context.coordinated
        && !context.protected
        && subject != AgreementClass::Unknown
        && verb != AgreementClass::Unknown
        && subject != verb
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(kani, derive(kani::Arbitrary))]
pub struct Repetition {
    pub same_surface: bool,
    pub role: Role,
    pub adjacent: bool,
    pub protected: bool,
}

#[must_use]
pub fn repeated_function(facts: Repetition) -> bool {
    facts.same_surface && facts.role == Role::Function && facts.adjacent && !facts.protected
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(kani, derive(kani::Arbitrary))]
pub struct Determiner {
    pub singular: bool,
    pub plural_noun: bool,
    pub finite_clause: bool,
    pub protected: Protection,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(kani, derive(kani::Arbitrary))]
pub enum Protection {
    Literal,
    Prose,
}

#[must_use]
pub fn determiner_number(facts: Determiner) -> bool {
    facts.singular
        && facts.plural_noun
        && facts.finite_clause
        && facts.protected == Protection::Prose
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(kani, derive(kani::Arbitrary))]
pub enum ModifierRole {
    Adverbial,
    Other,
}

#[must_use]
pub fn chinese_modifier(
    adverbial_role: ModifierRole,
    attributive_marker: bool,
    protected: bool,
) -> bool {
    adverbial_role == ModifierRole::Adverbial && attributive_marker && !protected
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(kani, derive(kani::Arbitrary))]
pub enum Role {
    Function,
    Content,
}

#[must_use]
pub const fn span_valid(start: usize, end: usize, length: usize) -> bool {
    start < end && end <= length
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(kani, derive(kani::Arbitrary))]
pub enum Script {
    Cjk,
    Latin,
    Digit,
    Opening,
    Closing,
    CjkPunctuation,
    Other,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(kani, derive(kani::Arbitrary))]
pub enum Space {
    Ordinary,
    Repeated,
    FullWidth,
    Invisible,
    Unusual,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SpaceFault {
    None,
    Invisible,
    FullWidth,
    Repeated,
    Punctuation,
    CjkInterior,
    Unusual,
}

#[must_use]
pub fn spacing(left: Script, right: Script, space: Space, protected: bool) -> SpaceFault {
    if protected {
        return SpaceFault::None;
    }
    match space {
        Space::Invisible => SpaceFault::Invisible,
        Space::FullWidth => SpaceFault::FullWidth,
        Space::Repeated => SpaceFault::Repeated,
        Space::Unusual => SpaceFault::Unusual,
        Space::Ordinary
            if left == Script::Opening
                || right == Script::Closing
                || left == Script::CjkPunctuation
                || right == Script::CjkPunctuation =>
        {
            SpaceFault::Punctuation
        }
        Space::Ordinary if left == Script::Cjk && right == Script::Cjk => SpaceFault::CjkInterior,
        Space::Ordinary => SpaceFault::None,
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(kani, derive(kani::Arbitrary))]
pub struct Coverage {
    pub expected: usize,
    pub received: usize,
    pub identity: bool,
    pub spans: bool,
    pub language_engine: Engine,
    pub diagnostics: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Verdict {
    Clean,
    Findings,
    Incomplete,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(kani, derive(kani::Arbitrary))]
pub enum Engine {
    Complete,
    Unavailable,
}

#[must_use]
pub fn verdict(coverage: Coverage) -> Verdict {
    if coverage.expected == 0
        || coverage.expected != coverage.received
        || !coverage.identity
        || !coverage.spans
        || coverage.language_engine != Engine::Complete
    {
        Verdict::Incomplete
    } else if coverage.diagnostics {
        Verdict::Findings
    } else {
        Verdict::Clean
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(kani, derive(kani::Arbitrary))]
pub struct ReviewCoverage {
    pub expected: usize,
    pub received: usize,
    pub errors: usize,
    pub binding: ReviewBinding,
    pub exact: bool,
    pub retained: bool,
    pub evidence: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReviewVerdict {
    Retained,
    Unresolved,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(kani, derive(kani::Arbitrary))]
pub enum ReviewBinding {
    Current,
    Stale,
}

#[must_use]
pub fn review_verdict(facts: ReviewCoverage) -> ReviewVerdict {
    if facts.expected > 0
        && facts.expected == facts.received
        && facts.errors == 0
        && facts.binding == ReviewBinding::Current
        && facts.exact
        && facts.retained
        && facts.evidence
    {
        ReviewVerdict::Retained
    } else {
        ReviewVerdict::Unresolved
    }
}

#[must_use]
pub const fn house_style_violation(japanese: bool, literal: bool, matched: bool) -> bool {
    japanese && !literal && matched
}

#[cfg(kani)]
mod proofs {
    use super::*;

    #[kani::proof]
    fn house_style_refuses_authored_matches_and_preserves_other_languages_and_literals() {
        let japanese: bool = kani::any();
        let literal: bool = kani::any();
        let matched: bool = kani::any();
        let violation = house_style_violation(japanese, literal, matched);
        assert_eq!(violation, japanese && !literal && matched);
        kani::cover!(violation);
        kani::cover!(!violation);
    }

    #[kani::proof]
    fn reviewed_candidates_require_exact_complete_bound_evidence_without_errors() {
        let facts: ReviewCoverage = kani::any();
        let result = review_verdict(facts);
        assert_eq!(
            result == ReviewVerdict::Retained,
            facts.expected > 0
                && facts.expected == facts.received
                && facts.errors == 0
                && facts.binding == ReviewBinding::Current
                && facts.exact
                && facts.retained
                && facts.evidence
        );
        kani::cover!(result == ReviewVerdict::Retained);
        kani::cover!(result == ReviewVerdict::Unresolved && facts.errors > 0);
        kani::cover!(result == ReviewVerdict::Unresolved && facts.binding == ReviewBinding::Stale);
        kani::cover!(result == ReviewVerdict::Unresolved && !facts.retained);
    }

    #[kani::proof]
    fn paragraph_similarity_is_nonempty_bounded_and_overflow_free() {
        let facts: Reuse = kani::any();
        let result = duplication(facts);
        assert_eq!(
            result == ReuseKind::Exact,
            facts.exact && facts.characters >= DUPLICATE_CHARACTERS
        );
        let near = !(facts.exact && facts.characters >= DUPLICATE_CHARACTERS)
            && facts.characters >= DUPLICATE_CHARACTERS
            && facts.facts_match
            && facts.words >= DUPLICATE_WORDS
            && facts.union > 0
            && facts.shared <= facts.union
            && u64::from(facts.shared) * SIMILARITY_DENOMINATOR
                >= u64::from(facts.union) * SIMILARITY_NUMERATOR;
        assert_eq!(result == ReuseKind::Near, near);
        if result == ReuseKind::Near {
            assert!(facts.characters >= DUPLICATE_CHARACTERS);
            assert!(
                facts.union > 0 && facts.shared <= facts.union && facts.words >= DUPLICATE_WORDS
            );
            assert!(
                u64::from(facts.shared) * SIMILARITY_DENOMINATOR
                    >= u64::from(facts.union) * SIMILARITY_NUMERATOR
            );
        }
        kani::cover!(result == ReuseKind::Exact);
        kani::cover!(result == ReuseKind::Near);
        kani::cover!(result == ReuseKind::None);
    }

    #[kani::proof]
    fn clause_chain_review_requires_all_three_complexity_conditions() {
        let language: Language = kani::any();
        let facts: Complexity = kani::any();
        let result = complex_sentence(language, facts);
        let threshold = match language {
            Language::English => ENGLISH_SENTENCE_WORDS,
            Language::Japanese => JAPANESE_SENTENCE_WORDS,
            Language::Chinese => CHINESE_SENTENCE_WORDS,
        };
        assert_eq!(
            result,
            facts.words >= threshold
                && facts.clauses >= CHAIN_CLAUSES
                && facts.distance >= CHAIN_DISTANCE
        );
        kani::cover!(result);
        kani::cover!(!result);
    }

    #[kani::proof]
    fn lexical_calibration_requires_the_word_role_and_unblocked_context() {
        let facts: Calibration = kani::any();
        assert_eq!(
            calibrated_word(facts),
            facts.word && facts.role && !facts.counterexample
        );
        kani::cover!(calibrated_word(facts));
        kani::cover!(!calibrated_word(facts));
    }

    #[kani::proof]
    fn each_language_requires_its_qualified_engines() {
        let language: Language = kani::any();
        let result = profile(language);
        assert_eq!(
            result.style == StyleEngine::Vale,
            language == Language::English
        );
        assert_eq!(
            result.style == StyleEngine::Textlint,
            language == Language::Japanese
        );
        assert_eq!(
            result.style == StyleEngine::Common,
            language == Language::Chinese
        );
        assert_eq!(
            result.grammar == GrammarEngine::NativeJapanese,
            language == Language::Japanese
        );
        assert_eq!(
            result.script == ScriptEngine::OpenCc,
            language == Language::Chinese
        );
        kani::cover!(language == Language::English);
        kani::cover!(language == Language::Japanese);
        kani::cover!(language == Language::Chinese);
    }

    #[kani::proof]
    fn determiners_require_a_finite_unquoted_number_conflict() {
        let facts: Determiner = kani::any();
        assert_eq!(
            determiner_number(facts),
            facts.singular
                && facts.plural_noun
                && facts.finite_clause
                && facts.protected == Protection::Prose
        );
        kani::cover!(determiner_number(facts));
        kani::cover!(!determiner_number(facts));
    }

    #[kani::proof]
    fn chinese_modifier_review_requires_the_adverbial_role() {
        let role: ModifierRole = kani::any();
        let attributive: bool = kani::any();
        let protected: bool = kani::any();
        assert_eq!(
            chinese_modifier(role, attributive, protected),
            role == ModifierRole::Adverbial && attributive && !protected
        );
        kani::cover!(chinese_modifier(role, attributive, protected));
        kani::cover!(!chinese_modifier(role, attributive, protected));
    }

    #[kani::proof]
    fn spans_are_nonempty_ordered_and_bounded() {
        let start: usize = kani::any();
        let end: usize = kani::any();
        let length: usize = kani::any();
        assert_eq!(span_valid(start, end, length), start < end && end <= length);
        kani::cover!(span_valid(start, end, length));
        kani::cover!(!span_valid(start, end, length));
    }

    #[kani::proof]
    fn agreement_requires_known_unprotected_present_roles() {
        let subject: AgreementClass = kani::any();
        let verb: AgreementClass = kani::any();
        let context: AgreementContext = kani::any();
        let result = disagrees(subject, verb, context);
        assert_eq!(
            result,
            context.present
                && !context.coordinated
                && !context.protected
                && !matches!(subject, AgreementClass::Unknown)
                && !matches!(verb, AgreementClass::Unknown)
                && subject != verb
        );
        kani::cover!(result);
        kani::cover!(!result);
    }

    #[kani::proof]
    fn repeated_functions_require_adjacent_unprotected_equal_tokens() {
        let facts: Repetition = kani::any();
        assert_eq!(
            repeated_function(facts),
            facts.same_surface
                && facts.role == Role::Function
                && facts.adjacent
                && !facts.protected
        );
        kani::cover!(repeated_function(facts));
        kani::cover!(!repeated_function(facts));
    }

    #[kani::proof]
    fn spacing_preserves_names_and_semantic_word_separators() {
        let left: Script = kani::any();
        let right: Script = kani::any();
        let space: Space = kani::any();
        assert_eq!(spacing(left, right, space, true), SpaceFault::None);
        if space == Space::Invisible {
            assert_eq!(spacing(left, right, space, false), SpaceFault::Invisible);
        }
        if space == Space::Unusual {
            assert_eq!(spacing(left, right, space, false), SpaceFault::Unusual);
        }
        assert_eq!(
            spacing(Script::Latin, Script::Latin, Space::Ordinary, false),
            SpaceFault::None
        );
        assert_eq!(
            spacing(Script::Digit, Script::Latin, Space::Ordinary, false),
            SpaceFault::None
        );
        assert_eq!(
            spacing(Script::Cjk, Script::Cjk, Space::Ordinary, false),
            SpaceFault::CjkInterior
        );
        kani::cover!(spacing(left, right, space, false) != SpaceFault::None);
        kani::cover!(spacing(left, right, space, false) == SpaceFault::None);
    }

    #[kani::proof]
    fn incomplete_or_empty_work_never_passes() {
        let facts: Coverage = kani::any();
        let result = verdict(facts);
        assert_eq!(
            result == Verdict::Incomplete,
            facts.expected == 0
                || facts.expected != facts.received
                || !facts.identity
                || !facts.spans
                || facts.language_engine != Engine::Complete
        );
        if result != Verdict::Incomplete {
            assert_eq!(result == Verdict::Findings, facts.diagnostics);
        }
        kani::cover!(result == Verdict::Clean);
        kani::cover!(result == Verdict::Findings);
        kani::cover!(result == Verdict::Incomplete);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn retained_findings_never_hide_errors_or_incomplete_decisions() {
        let complete = ReviewCoverage {
            expected: 2,
            received: 2,
            errors: 0,
            binding: ReviewBinding::Current,
            exact: true,
            retained: true,
            evidence: true,
        };
        assert_eq!(review_verdict(complete), ReviewVerdict::Retained);
        for incomplete in [
            ReviewCoverage {
                expected: 0,
                received: 0,
                ..complete
            },
            ReviewCoverage {
                received: 1,
                ..complete
            },
            ReviewCoverage {
                received: 3,
                ..complete
            },
            ReviewCoverage {
                errors: 1,
                ..complete
            },
            ReviewCoverage {
                binding: ReviewBinding::Stale,
                ..complete
            },
            ReviewCoverage {
                exact: false,
                ..complete
            },
            ReviewCoverage {
                retained: false,
                ..complete
            },
            ReviewCoverage {
                evidence: false,
                ..complete
            },
        ] {
            assert_eq!(review_verdict(incomplete), ReviewVerdict::Unresolved);
        }
    }

    #[test]
    fn repeated_and_reworded_paragraphs_require_substantial_overlap() {
        let reused = Reuse {
            exact: true,
            facts_match: true,
            characters: 30,
            words: 12,
            shared: 9,
            union: 10,
        };
        assert_eq!(duplication(reused), ReuseKind::Exact);
        assert_eq!(
            duplication(Reuse {
                exact: false,
                ..reused
            }),
            ReuseKind::Near
        );
        assert_eq!(
            duplication(Reuse {
                exact: false,
                shared: 8,
                ..reused
            }),
            ReuseKind::None
        );
        assert_eq!(
            duplication(Reuse {
                exact: false,
                facts_match: false,
                ..reused
            }),
            ReuseKind::None
        );
        assert_eq!(
            duplication(Reuse {
                exact: false,
                words: 11,
                ..reused
            }),
            ReuseKind::None
        );
        assert_eq!(
            duplication(Reuse {
                exact: false,
                union: 0,
                ..reused
            }),
            ReuseKind::None
        );
        assert_eq!(
            duplication(Reuse {
                exact: false,
                shared: 11,
                ..reused
            }),
            ReuseKind::None
        );
        assert_eq!(
            duplication(Reuse {
                characters: 29,
                words: 0,
                ..reused
            }),
            ReuseKind::None
        );
        assert_eq!(
            duplication(Reuse {
                exact: false,
                shared: u32::MAX,
                union: u32::MAX,
                ..reused
            }),
            ReuseKind::Near
        );
    }

    #[test]
    fn a_long_sentence_alone_does_not_imply_a_clause_chain() {
        for (language, threshold) in [
            (Language::English, 45),
            (Language::Japanese, 70),
            (Language::Chinese, 55),
        ] {
            let complex = Complexity {
                words: threshold,
                clauses: 4,
                distance: 24,
            };
            assert!(complex_sentence(language, complex));
            assert!(!complex_sentence(
                language,
                Complexity {
                    words: threshold - 1,
                    ..complex
                }
            ));
            assert!(!complex_sentence(
                language,
                Complexity {
                    clauses: 3,
                    ..complex
                }
            ));
            assert!(!complex_sentence(
                language,
                Complexity {
                    distance: 23,
                    ..complex
                }
            ));
        }
    }

    #[test]
    fn lexical_references_never_hide_their_counterexamples() {
        let accepted = Calibration {
            word: true,
            role: true,
            counterexample: false,
        };
        assert!(calibrated_word(accepted));
        assert!(!calibrated_word(Calibration {
            counterexample: true,
            ..accepted
        }));
        assert!(!calibrated_word(Calibration {
            word: false,
            ..accepted
        }));
        assert!(!calibrated_word(Calibration {
            role: false,
            ..accepted
        }));
    }

    #[test]
    fn native_japanese_rules_preserve_the_valid_plural_suffix() {
        assert_eq!(
            profile(Language::Japanese),
            Profile {
                grammar: GrammarEngine::NativeJapanese,
                style: StyleEngine::Textlint,
                script: ScriptEngine::Unicode
            }
        );
        assert_eq!(
            profile(Language::English).grammar,
            GrammarEngine::LanguageTool
        );
        assert_eq!(
            profile(Language::Chinese).grammar,
            GrammarEngine::LanguageTool
        );
    }

    #[test]
    fn grammar_conflicts_require_all_their_semantic_conditions() {
        let context = AgreementContext {
            present: true,
            coordinated: false,
            protected: false,
        };
        for (subject, verb, expected) in [
            (AgreementClass::ThirdSingular, AgreementClass::Other, true),
            (AgreementClass::Other, AgreementClass::ThirdSingular, true),
            (
                AgreementClass::ThirdSingular,
                AgreementClass::ThirdSingular,
                false,
            ),
            (AgreementClass::Other, AgreementClass::Other, false),
            (
                AgreementClass::Unknown,
                AgreementClass::ThirdSingular,
                false,
            ),
            (
                AgreementClass::ThirdSingular,
                AgreementClass::Unknown,
                false,
            ),
        ] {
            assert_eq!(disagrees(subject, verb, context), expected);
            for protected_context in [
                AgreementContext {
                    present: false,
                    ..context
                },
                AgreementContext {
                    coordinated: true,
                    ..context
                },
                AgreementContext {
                    protected: true,
                    ..context
                },
            ] {
                assert!(!disagrees(subject, verb, protected_context));
            }
        }
    }

    #[test]
    fn repeated_words_and_modifiers_preserve_their_controls() {
        let duplicate = Repetition {
            same_surface: true,
            role: Role::Function,
            adjacent: true,
            protected: false,
        };
        assert!(repeated_function(duplicate));
        for control in [
            Repetition {
                same_surface: false,
                ..duplicate
            },
            Repetition {
                role: Role::Content,
                ..duplicate
            },
            Repetition {
                adjacent: false,
                ..duplicate
            },
            Repetition {
                protected: true,
                ..duplicate
            },
        ] {
            assert!(!repeated_function(control));
        }
        let determiner = Determiner {
            singular: true,
            plural_noun: true,
            finite_clause: true,
            protected: Protection::Prose,
        };
        assert!(determiner_number(determiner));
        for control in [
            Determiner {
                singular: false,
                ..determiner
            },
            Determiner {
                plural_noun: false,
                ..determiner
            },
            Determiner {
                finite_clause: false,
                ..determiner
            },
            Determiner {
                protected: Protection::Literal,
                ..determiner
            },
        ] {
            assert!(!determiner_number(control));
        }
        assert!(chinese_modifier(ModifierRole::Adverbial, true, false));
        assert!(!chinese_modifier(ModifierRole::Other, true, false));
        assert!(!chinese_modifier(ModifierRole::Adverbial, false, false));
        assert!(!chinese_modifier(ModifierRole::Adverbial, true, true));
    }

    #[test]
    fn spacing_separates_literal_names_punctuation_and_visual_alignment() {
        for (space, expected) in [
            (Space::Invisible, SpaceFault::Invisible),
            (Space::FullWidth, SpaceFault::FullWidth),
            (Space::Repeated, SpaceFault::Repeated),
        ] {
            assert_eq!(
                spacing(Script::Latin, Script::Latin, space, false),
                expected
            );
            assert_eq!(
                spacing(Script::Latin, Script::Latin, space, true),
                SpaceFault::None
            );
        }
        for (left, right) in [
            (Script::Opening, Script::Cjk),
            (Script::Cjk, Script::Closing),
            (Script::CjkPunctuation, Script::Cjk),
            (Script::Cjk, Script::CjkPunctuation),
        ] {
            assert_eq!(
                spacing(left, right, Space::Ordinary, false),
                SpaceFault::Punctuation
            );
        }
        assert_eq!(
            spacing(Script::Cjk, Script::Latin, Space::Ordinary, false),
            SpaceFault::None
        );
        assert!(!span_valid(1, 0, 1));
        assert!(!span_valid(0, 1, 0));
        assert!(span_valid(usize::MAX - 1, usize::MAX, usize::MAX));
    }

    #[test]
    fn empty_stale_missing_and_partial_coverage_refuse() {
        let complete = Coverage {
            expected: 1,
            received: 1,
            identity: true,
            spans: true,
            language_engine: Engine::Complete,
            diagnostics: false,
        };
        assert_eq!(verdict(complete), Verdict::Clean);
        assert_eq!(
            verdict(Coverage {
                diagnostics: true,
                ..complete
            }),
            Verdict::Findings
        );
        for incomplete in [
            Coverage {
                expected: 0,
                received: 0,
                ..complete
            },
            Coverage {
                received: 0,
                ..complete
            },
            Coverage {
                identity: false,
                ..complete
            },
            Coverage {
                spans: false,
                ..complete
            },
            Coverage {
                language_engine: Engine::Unavailable,
                ..complete
            },
        ] {
            assert_eq!(verdict(incomplete), Verdict::Incomplete);
        }
    }

    #[test]
    fn literal_names_units_and_unknown_roles_are_controls() {
        assert_eq!(
            spacing(Script::Cjk, Script::Cjk, Space::Ordinary, true),
            SpaceFault::None
        );
        assert_eq!(
            spacing(Script::Digit, Script::Latin, Space::Ordinary, false),
            SpaceFault::None
        );
        assert_eq!(
            spacing(Script::Cjk, Script::Cjk, Space::Ordinary, false),
            SpaceFault::CjkInterior
        );
        assert!(!disagrees(
            AgreementClass::Unknown,
            AgreementClass::Other,
            AgreementContext {
                present: true,
                coordinated: false,
                protected: false
            }
        ));
        assert!(!repeated_function(Repetition {
            same_surface: true,
            role: Role::Function,
            adjacent: true,
            protected: true
        }));
        assert!(span_valid(0, 1, 1));
        assert!(!span_valid(1, 1, 1));
    }
}
