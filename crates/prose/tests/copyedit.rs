use anyhow::{Context, Result};
use prose::copyedit::{Document, inspect};
use prose::copyedit::{Language, project};
use sha2::Digest;
use std::path::Path;

#[test]
fn persisted_prose_and_copyediting_reject_the_same_authored_layouts() -> Result<()> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let bundle = prose::Bundle::source(&root)?;
    let source = "読み取り値を保持します。\n\n- 入力値を保存します。\n\n<table><tr><td>値を保存します。</td></tr></table>\n";
    let report = inspect(
        &bundle,
        Language::Japanese,
        &[Document {
            path: "layout.md".to_owned(),
            text: source.to_owned(),
        }],
    )?;
    for rule in [prose::house::LIST, prose::house::TABLE] {
        assert!(
            report
                .diagnostics
                .iter()
                .any(|finding| finding.rule == rule)
        );
    }
    let findings = prose::check(
        &bundle,
        prose::Channel::Document,
        "layout.md",
        "The parser returns a value.\n\n1. The parser reads the input.\n\n| Input | Output |\n| --- | --- |\n| A | B |\n",
    )?;
    for rule in [prose::house::LIST, prose::house::TABLE] {
        assert!(findings.iter().any(|finding| finding.rule == rule));
    }
    Ok(())
}

#[test]
fn persisted_documents_and_source_comments_share_house_policy_and_source_lines() -> Result<()> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let bundle = prose::Bundle::source(&root)?;
    let text = "このメソッドは値を返します。\n";
    let document = prose::check(&bundle, prose::Channel::Document, "document.md", text)?;
    assert!(
        document
            .iter()
            .any(|finding| finding.rule == "House.TechnicalTerm")
    );
    let mut input = prose::Input::new("source.rs", text, false);
    input.lines = vec![42];
    let comments = prose::check_inputs(&bundle, &[input])?;
    for rule in ["House.TechnicalTerm", "House.JapaneseDemonstrative"] {
        assert!(
            comments
                .iter()
                .any(|finding| finding.rule == rule && finding.line == 42)
        );
    }
    Ok(())
}

#[test]
fn japanese_house_style_rejects_demonstratives_and_localized_technical_terms() -> Result<()> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let bundle = prose::Bundle::source(&root)?;
    let source = include_str!("fixtures/copyedit/house-ja-rejected.md");
    let report = inspect(
        &bundle,
        Language::Japanese,
        &[Document {
            path: "house.md".to_owned(),
            text: source.to_owned(),
        }],
    )?;
    let findings: Vec<_> = report
        .diagnostics
        .iter()
        .filter(|finding| finding.rule.starts_with("House."))
        .collect();
    assert_eq!(
        findings
            .iter()
            .filter(|finding| finding.rule == "House.JapaneseDemonstrative")
            .count(),
        3,
        "{:?}",
        report.diagnostics
    );
    assert_eq!(
        findings
            .iter()
            .filter(|finding| finding.rule == "House.TechnicalTerm")
            .count(),
        3,
        "{:?}",
        report.diagnostics
    );
    assert!(
        findings
            .iter()
            .all(|finding| finding.severity == prose::copyedit::Severity::Error)
    );
    for finding in findings {
        assert_eq!(&source[finding.start..finding.end], finding.excerpt);
        assert!(finding.line < 12, "{finding:?}");
    }
    assert!(!report.clean());
    Ok(())
}

#[test]
fn japanese_house_style_preserves_word_senses_and_literal_identifiers() -> Result<()> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let bundle = prose::Bundle::source(&root)?;
    let source = include_str!("fixtures/copyedit/house-ja-accepted.md");
    let report = inspect(
        &bundle,
        Language::Japanese,
        &[Document {
            path: "house-control.md".to_owned(),
            text: source.to_owned(),
        }],
    )?;
    assert!(
        report
            .diagnostics
            .iter()
            .all(|finding| !finding.rule.starts_with("House.")),
        "{:?}",
        report.diagnostics
    );
    Ok(())
}

#[test]
fn reviewed_candidates_require_exact_evidence_and_repeated_engines() -> Result<()> {
    use prose::copyedit::{ReviewDecisions, Severity};

    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let bundle = prose::Bundle::source(&root)?;
    let temporary = tempfile::tempdir()?;
    let path = temporary.path().join("article.md");
    let text = "河童属于日本的民间传说，不同地方的故事与造型各有变化。\n";
    std::fs::write(&path, text)?;
    let report = inspect(&bundle, Language::Chinese, &[Document::read(&path)?])?;
    assert!(!report.clean());
    assert!(
        report
            .diagnostics
            .iter()
            .all(|item| item.severity == Severity::Review)
    );
    let template = report.review_template()?;
    assert!(template.validate_for(&report).is_err());
    let mut reviewed = serde_json::to_value(&template)?;
    for decision in reviewed["decisions"].as_array_mut().expect("decisions") {
        decision["resolution"] = serde_json::json!({
            "status": "retain",
            "reason": "民间 modifies 传说 as a compound; 间 is not a numeral classifier here.",
            "evidence": text.trim(),
        });
    }
    let decisions: ReviewDecisions = serde_json::from_value(reviewed.clone())?;
    let validated = decisions.validate_for(&report);
    assert!(
        validated.is_ok(),
        "{validated:?}: {:?}; {:?}",
        report.diagnostics,
        report.segments
    );
    assert!(report.verify_current(&bundle).is_err());
    report
        .verify_reviewed(&bundle, &decisions)
        .context("repeat reviewed report")?;

    for altered in [
        serde_json::json!([]),
        serde_json::json!([reviewed["decisions"][0], reviewed["decisions"][0]]),
    ] {
        let mut invalid = reviewed.clone();
        invalid["decisions"] = altered;
        let invalid: ReviewDecisions = serde_json::from_value(invalid)?;
        assert!(invalid.validate_for(&report).is_err());
    }
    for (field, value) in [
        ("identity", serde_json::json!("unknown")),
        ("resolution", serde_json::json!({"status": "pending"})),
        (
            "resolution",
            serde_json::json!({"status": "retain", "reason": " ", "evidence": text.trim()}),
        ),
        (
            "resolution",
            serde_json::json!({"status": "retain", "reason": "A reason.", "evidence": "Unrelated source."}),
        ),
    ] {
        let mut invalid = reviewed.clone();
        invalid["decisions"][0][field] = value;
        let invalid: ReviewDecisions = serde_json::from_value(invalid)?;
        assert!(
            invalid.validate_for(&report).is_err(),
            "accepted invalid {field}"
        );
    }
    let mut encoded = serde_json::to_value(&report)?;
    encoded["diagnostics"][0]["severity"] = serde_json::json!("error");
    let error_report: prose::copyedit::Report = serde_json::from_value(encoded.clone())?;
    let mut error_decisions = reviewed.clone();
    error_decisions["report_sha256"] =
        serde_json::to_value(error_report.review_template()?)?["report_sha256"].clone();
    let error_decisions: ReviewDecisions = serde_json::from_value(error_decisions)?;
    assert!(error_decisions.validate_for(&error_report).is_err());

    encoded["diagnostics"][0]["severity"] = serde_json::json!("review");
    encoded["diagnostics"][0]["message"] = serde_json::json!("A forged diagnostic.");
    let altered_report: prose::copyedit::Report = serde_json::from_value(encoded)?;
    assert!(decisions.validate_for(&altered_report).is_err());
    let mut forged = reviewed;
    forged["report_sha256"] =
        serde_json::to_value(altered_report.review_template()?)?["report_sha256"].clone();
    let forged: ReviewDecisions = serde_json::from_value(forged)?;
    forged
        .validate_for(&altered_report)
        .context("validate forged report before engine replay")?;
    assert!(altered_report.verify_reviewed(&bundle, &forged).is_err());
    std::fs::write(path, "変更した本文です。\n")?;
    assert!(report.verify_reviewed(&bundle, &decisions).is_err());
    Ok(())
}

#[test]
fn markdown_projection_keeps_prose_captions_and_locations() -> Result<()> {
    let source = "---\ntitle: metadata\n---\n\n# 京都\n\n本文の[案内](https://example.com/a_b)です。\n\n![庭園の池](../images/garden.jpg)\n\n`let x = 1;`\n\n```rust\nlet x = 1;\n```\n\n| 場所 | 説明 |\n| --- | --- |\n| 京都 | 庭園を歩く。 |\n";
    let segments = project("article.md", source)?;
    let visible = segments
        .iter()
        .map(|segment| segment.text.as_str())
        .collect::<Vec<_>>()
        .join("\n");
    assert!(visible.contains("京都"));
    assert!(visible.contains("本文の案内です。"));
    assert!(visible.contains("庭園の池"));
    assert!(visible.contains("庭園を歩く。"));
    assert!(visible.contains("metadata"));
    for hidden in ["https://", "garden.jpg", "let x"] {
        assert!(!visible.contains(hidden), "{visible}");
    }
    let paragraph = segments
        .iter()
        .find(|segment| segment.text.starts_with("本文"))
        .expect("paragraph");
    let location = paragraph.source_span(3, 5)?;
    assert_eq!(&source[location], "案内");
    Ok(())
}

#[test]
fn human_metadata_decodes_yaml_and_preserves_source_locations() -> Result<()> {
    let source = "---\ntitle: 京都の庭園\nslug: hidden-slug\nfeature_image_caption: \"庭園の池\"\ndescription: >-\n  水辺を歩く。\n  木々を眺める。\nmeta_description: \"庭園\\u3000の案内\"\nsource: secret-path\n---\n\n本文です。\n";
    let segments = project("article.md", source)?;
    let text: Vec<_> = segments
        .iter()
        .map(|segment| segment.text.as_str())
        .collect();
    assert_eq!(
        text,
        [
            "京都の庭園",
            "庭園の池",
            "水辺を歩く。 木々を眺める。",
            "庭園　の案内",
            "本文です。"
        ]
    );
    assert_eq!(&source[segments[0].source_span(0, 2)?], "京都");
    assert_eq!(&source[segments[1].source_span(3, 4)?], "池");
    assert!(source[segments[3].source_span(2, 3)?].contains("\\u3000"));
    for source in [
        "---\ntitle: [invalid, title]\n---\n\nBody.\n",
        "---\ntitle: First\ntitle: Second\n---\n\nBody.\n",
        "---\nother: &name Museum\ntitle: *name\n---\n\nBody.\n",
        "---\ntitle: true\n---\n\nBody.\n",
    ] {
        assert!(project("invalid.md", source).is_err());
    }
    Ok(())
}

#[test]
fn inline_code_and_autolinks_separate_clauses() -> Result<()> {
    let segments = project(
        "guide.md",
        "Read `the the` and <https://example.com/a_b>.\n",
    )?;
    assert!(
        segments
            .iter()
            .all(|segment| !segment.text.contains("the the"))
    );
    assert!(
        segments
            .iter()
            .all(|segment| !segment.text.contains("https://"))
    );
    assert!(
        segments
            .iter()
            .all(|segment| !segment.text.contains("Read  and"))
    );
    Ok(())
}

#[test]
fn tight_list_items_remain_separate_prose_blocks() -> Result<()> {
    let source = "- We walk to the station.\n- They arrive at noon.\n  - The guide opens the gate.\n- We enter the museum.\n";
    let segments = project("list.md", source)?;
    assert_eq!(
        segments
            .iter()
            .map(|segment| segment.text.as_str())
            .collect::<Vec<_>>(),
        [
            "We walk to the station.",
            "They arrive at noon.",
            "The guide opens the gate.",
            "We enter the museum."
        ]
    );
    for segment in &segments {
        assert_eq!(
            &source[segment.source_span(0, segment.text.chars().count())?],
            segment.text
        );
    }
    Ok(())
}

#[test]
fn code_only_empty_and_oversize_documents_never_pass_without_coverage() {
    for source in ["", "```rust\nlet x = 1;\n```\n", "---\nslug: hidden\n---\n"] {
        assert!(project("empty.md", source).is_err());
    }
}

#[test]
fn native_checks_cover_metadata_and_unicode_without_breaking_unit_spacing() -> Result<()> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let bundle = prose::Bundle::source(&root)?;
    let document = Document {
        path: "unicode.md".to_owned(),
        text: "---\ntitle: They walks to the museum.\n---\n\nThe museum\u{2003}opens at noon.\n\nThe train travels 5\u{202f}km.\n\nThe guide\u{200e} walks to the station.\n".to_owned(),
    };
    let report = inspect(&bundle, Language::English, &[document])?;
    for (line, rule) in [
        (2, "Grammar.SubjectVerbAgreement"),
        (5, "Typography.UnusualSpace"),
        (9, "Typography.InvisibleSpace"),
    ] {
        assert!(
            report
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.line == line && diagnostic.rule == rule),
            "{rule}: {:?}",
            report.diagnostics
        );
    }
    assert!(
        report
            .diagnostics
            .iter()
            .all(|diagnostic| diagnostic.line != 7 || !diagnostic.rule.starts_with("Typography."))
    );
    Ok(())
}

#[test]
fn language_selection_is_explicit_and_covers_three_languages() {
    assert_eq!(Language::English.code(), "en");
    assert_eq!(Language::Japanese.code(), "ja");
    assert_eq!(Language::Chinese.code(), "zh");
}

#[test]
fn native_pipelines_detect_errors_and_accept_matched_controls() -> Result<()> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let bundle = prose::Bundle::source(&root)?;
    for (language, name, expected, first_control_line) in [
        (
            Language::English,
            "en.md",
            vec![
                (3, "Grammar.SubjectVerbAgreement"),
                (5, "Grammar.SubjectVerbAgreement"),
                (7, "Grammar.RepeatedFunction"),
                (9, "Grammar.DeterminerNumber"),
                (11, "LanguageTool.EN_A_VS_AN"),
            ],
            13,
        ),
        (
            Language::Japanese,
            "ja.md",
            vec![
                (3, "Grammar.RepeatedFunction"),
                (5, "Typography.RepeatedSpace"),
                (7, "Typography.FullWidthSpace"),
            ],
            9,
        ),
        (
            Language::Chinese,
            "zh.md",
            vec![
                (3, "Grammar.RepeatedFunction"),
                (7, "Chinese.AdverbialMarker"),
                (9, "Typography.RepeatedSpace"),
                (11, "Typography.FullWidthSpace"),
                (13, "Typography.PunctuationSpace"),
            ],
            21,
        ),
    ] {
        let document =
            Document::read(&root.join("crates/prose/tests/fixtures/copyedit").join(name))?;
        let report = inspect(&bundle, language, &[document])?;
        assert_eq!(report.sources.len(), 1);
        assert!(!report.nlp.annotations.is_empty());
        if language == Language::Japanese {
            assert!(report.grammar.is_none());
            assert!(
                report
                    .diagnostics
                    .iter()
                    .any(|diagnostic| diagnostic.rule
                        == "ja-technical-writing/ja-no-successive-word")
            );
        } else {
            assert_ne!(
                report
                    .grammar
                    .as_ref()
                    .expect("qualified grammar engine")
                    .active_rules,
                [] as [String; 0]
            );
            if language == Language::Chinese {
                for line in [15, 17, 19] {
                    assert!(
                        report
                            .diagnostics
                            .iter()
                            .any(|diagnostic| diagnostic.line == line
                                && diagnostic.engine == "languagetool"),
                        "Chinese counterexample on line {line} wasn't detected: {:?}",
                        report.diagnostics
                    );
                }
            }
        }
        for (line, rule) in expected {
            assert!(
                report
                    .diagnostics
                    .iter()
                    .any(|diagnostic| diagnostic.line == line && diagnostic.rule == rule),
                "{name}:{line} needs {rule}: {:?}",
                report.diagnostics
            );
        }
        assert!(
            report
                .diagnostics
                .iter()
                .all(|diagnostic| diagnostic.line < first_control_line),
            "accepted {name} control produced findings: {:?}",
            report.diagnostics
        );
        let encoded = serde_json::to_vec(&report)?;
        let decoded: prose::copyedit::Report = serde_json::from_slice(&encoded)?;
        assert_eq!(decoded.sources[0].sha256, report.sources[0].sha256);
        assert_eq!(decoded.nlp.model, report.nlp.model);
    }
    Ok(())
}

#[test]
fn malformed_native_annotations_fail_before_rule_evaluation() -> Result<()> {
    let segment = project("guide.md", "The museum opens at noon.\n")?.remove(0);
    let valid: prose::copyedit::Annotation = serde_json::from_value(serde_json::json!({
        "id": segment.id, "characters": 25, "simplified": null,
        "text_sha256": prose::build_inputs::hex(&sha2::Sha256::digest(segment.text.as_bytes())),
        "sentences": [{"start":0,"end":25}], "entities": [],
        "tokens": [
            {"start":0,"end":3,"text":"The","lemma":"the","pos":"DET","tag":"DT","dependency":"det","head":1,"morphology":"","entity":""},
            {"start":4,"end":10,"text":"museum","lemma":"museum","pos":"NOUN","tag":"NN","dependency":"nsubj","head":2,"morphology":"Number=Sing","entity":""},
            {"start":11,"end":16,"text":"opens","lemma":"open","pos":"VERB","tag":"VBZ","dependency":"ROOT","head":2,"morphology":"Tense=Pres","entity":""},
            {"start":17,"end":19,"text":"at","lemma":"at","pos":"ADP","tag":"IN","dependency":"prep","head":2,"morphology":"","entity":""},
            {"start":20,"end":24,"text":"noon","lemma":"noon","pos":"NOUN","tag":"NN","dependency":"pobj","head":3,"morphology":"","entity":""},
            {"start":24,"end":25,"text":".","lemma":".","pos":"PUNCT","tag":".","dependency":"punct","head":2,"morphology":"","entity":""}
        ]
    }))?;
    valid.validate(&segment)?;
    let different_whitespace = project("guide.md", "The\u{a0}museum opens at noon.\n")?.remove(0);
    assert!(valid.validate(&different_whitespace).is_err());
    let mut invalid = valid.clone();
    invalid.tokens[0].end = 0;
    assert!(invalid.validate(&segment).is_err());
    let mut invalid = valid.clone();
    invalid.tokens[0].head = valid.tokens.len();
    assert!(invalid.validate(&segment).is_err());
    let mut invalid = valid.clone();
    invalid.tokens[0].text = "Other".to_owned();
    assert!(invalid.validate(&segment).is_err());
    let mut invalid = valid.clone();
    invalid.tokens[0].head = 1;
    invalid.tokens[1].head = 0;
    assert!(invalid.validate(&segment).is_err());
    let mut invalid = valid.clone();
    invalid.tokens.remove(0);
    assert!(invalid.validate(&segment).is_err());
    let mut invalid = valid;
    invalid.sentences[0].end = 24;
    assert!(invalid.validate(&segment).is_err());
    Ok(())
}

#[test]
fn native_japanese_locations_preserve_supplementary_unicode() -> Result<()> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let bundle = prose::Bundle::source(&root)?;
    let mut baseline = None;
    for prefix in ["", "🚃 ", "𠮷 "] {
        let document = Document {
            path: "japanese-locations.md".to_owned(),
            text: format!("{prefix}彼はは駅へ向かいました。\n"),
        };
        let report = inspect(&bundle, Language::Japanese, &[document])?;
        let diagnostic = report
            .diagnostics
            .iter()
            .find(|finding| finding.rule == "ja-technical-writing/ja-no-successive-word")
            .expect("native duplicate-word finding");
        let relative = (diagnostic.start - prefix.len(), diagnostic.excerpt.clone());
        if let Some(expected) = &baseline {
            assert_eq!(&relative, expected);
        } else {
            baseline = Some(relative);
        }
    }
    Ok(())
}

#[test]
fn a_clean_report_repeats_all_checks_and_rejects_changed_sources() -> Result<()> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let bundle = prose::Bundle::source(&root)?;
    let temporary = tempfile::tempdir()?;
    let path = temporary.path().join("article.md");
    std::fs::write(&path, "The museum opens at noon.\n")?;
    let mut report = inspect(&bundle, Language::English, &[Document::read(&path)?])?;
    assert!(report.clean(), "{:?}", report.diagnostics);
    report.verify_current(&bundle)?;
    std::fs::write(&path, "The museum closes at noon.\n")?;
    assert!(
        report
            .verify_current(&bundle)
            .expect_err("stale source")
            .to_string()
            .contains("source changed")
    );
    std::fs::write(&path, "The museum opens at noon.\n")?;
    report.producer_build_sha256 = "another producer".to_owned();
    assert!(
        report
            .verify_current(&bundle)
            .expect_err("stale producer")
            .to_string()
            .contains("another source build")
    );
    Ok(())
}

#[test]
fn parser_adapter_types_and_all_native_engine_controls_pass() -> Result<()> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    prose::copyedit::verify_runtime(&prose::Bundle::source(&root)?)
}

#[test]
fn paragraph_reuse_preserves_distinct_facts_and_document_boundaries() -> Result<()> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let bundle = prose::Bundle::source(&root)?;
    for (language, name) in [
        (Language::English, "content-en.md"),
        (Language::Japanese, "content-ja.md"),
        (Language::Chinese, "content-zh.md"),
    ] {
        let document =
            Document::read(&root.join("crates/prose/tests/fixtures/copyedit").join(name))?;
        let control = Document {
            path: "another-article.md".to_owned(),
            text: document
                .text
                .split("\n\n")
                .nth(1)
                .expect("paragraph")
                .to_owned(),
        };
        let report = inspect(&bundle, language, &[document, control])?;
        let repeated: Vec<_> = report
            .diagnostics
            .iter()
            .filter(|diagnostic| diagnostic.rule.starts_with("Content."))
            .map(|diagnostic| {
                (
                    diagnostic.path.as_str(),
                    diagnostic.line,
                    diagnostic.rule.as_str(),
                )
            })
            .collect();
        let original = report.sources[0].path.as_str();
        let mut expected = vec![(original, 5, "Content.RepeatedParagraph")];
        if language == Language::English {
            expected.push((original, 7, "Content.LemmaOverlap"));
        }
        assert_eq!(repeated, expected);
    }
    Ok(())
}
