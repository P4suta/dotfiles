use anyhow::{Context, Result};
use serde_json::Value;
use std::fmt::Write;
use std::fs;
use std::path::Path;
use std::process::Output;

#[path = "../../platform/tests/support/fixture_process.rs"]
mod fixture_process;

fn run(directory: &Path, arguments: &[&str]) -> Result<Output> {
    fixture_process::command(env!("CARGO_BIN_EXE_prose"), directory)
        .current_dir(directory)
        .arg("--source")
        .arg(Path::new(env!("CARGO_MANIFEST_DIR")).join("../.."))
        .arg("editorial")
        .args(arguments)
        .output()
        .context("run the editorial checker")
}

fn report(output: &Output) -> Result<Value> {
    serde_json::from_slice(&output.stdout).with_context(|| {
        format!(
            "read the editorial report; stderr: {}",
            String::from_utf8_lossy(&output.stderr)
        )
    })
}

fn rules(value: &Value) -> Result<Vec<&str>> {
    value["sources"]
        .as_array()
        .context("report sources")?
        .iter()
        .flat_map(|source| source["findings"].as_array().into_iter().flatten())
        .map(|finding| finding["rule"].as_str().context("finding rule"))
        .collect()
}

#[test]
fn detailed_reports_refuse_short_prose_despite_literal_padding() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let body = "The decoder rejects invalid input.";
    let literal_padding = "payload ".repeat(4_000);
    fs::write(
        directory.path().join("report.md"),
        format!(
            "---\ntitle: Release behavior\n---\n# Report\n\n{body}\n\n```text\n{literal_padding}\n```\n\n> {literal_padding}\n\nSource: [Record](https://example.com/record).\n"
        ),
    )?;
    let output = run(
        directory.path(),
        &[
            "--profile",
            "detailed-report",
            "--format",
            "json",
            "report.md",
        ],
    )?;
    assert_eq!(output.status.code(), Some(1), "{output:?}");
    let value = report(&output)?;
    assert!(rules(&value)?.contains(&"editorial.report-too-short"));
    assert!(
        value["sources"][0]["authored_body_characters"]
            .as_u64()
            .is_some_and(|count| count < 100)
    );
    Ok(())
}

#[test]
fn detailed_reports_accept_sufficient_distinct_body_paragraphs() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let mut body = String::new();
    for index in 0..400 {
        writeln!(
            body,
            "The decoder accepts payload {index} and preserves its validated fields.\n"
        )?;
    }
    fs::write(
        directory.path().join("report.md"),
        format!("# Report\n\n{body}"),
    )?;
    let output = run(
        directory.path(),
        &[
            "--profile",
            "detailed-report",
            "--format",
            "json",
            "report.md",
        ],
    )?;
    assert!(output.status.success(), "{output:?}");
    let value = report(&output)?;
    assert_eq!(rules(&value)?, [] as [&str; 0]);
    assert_eq!(
        value["sources"][0]["authored_body_characters"],
        body.chars()
            .filter(|character| !character.is_whitespace())
            .count()
    );
    Ok(())
}

#[test]
fn detailed_report_metrics_count_unicode_characters_in_body_prose() -> Result<()> {
    let directory = tempfile::tempdir()?;
    fs::write(
        directory.path().join("report.md"),
        "# 検証\n\n入力を検証します。\n結果を返します。\n",
    )?;
    let output = run(
        directory.path(),
        &[
            "--profile",
            "detailed-report",
            "--format",
            "json",
            "report.md",
        ],
    )?;
    assert_eq!(output.status.code(), Some(1));
    assert_eq!(
        report(&output)?["sources"][0]["authored_body_characters"],
        17
    );
    Ok(())
}

#[test]
fn shared_house_policy_applies_to_both_profiles_and_human_metadata() -> Result<()> {
    let directory = tempfile::tempdir()?;
    fs::write(
        directory.path().join("house.md"),
        "---\ntitle: そのinterface\n---\n\n# Method\n\nこのメソッドは値を返します。\n",
    )?;
    for profile in ["general", "expository"] {
        let output = run(
            directory.path(),
            &["--profile", profile, "--format", "json", "house.md"],
        )?;
        assert_eq!(output.status.code(), Some(1));
        let value = report(&output)?;
        let found = rules(&value)?;
        assert_eq!(
            found
                .iter()
                .filter(|rule| **rule == "House.JapaneseDemonstrative")
                .count(),
            2
        );
        assert!(found.contains(&"House.TechnicalTerm"));
    }
    Ok(())
}

#[test]
fn both_editorial_profiles_reject_authored_markdown_and_html_layouts() -> Result<()> {
    let directory = tempfile::tempdir()?;
    fs::write(
        directory.path().join("layout.md"),
        "# Report\n\nThe parser returns the value.\n\n- The parser reads the input.\n\n<table><tr><td>Value</td></tr></table>\n",
    )?;
    for profile in ["general", "expository"] {
        let output = run(
            directory.path(),
            &["--profile", profile, "--format", "json", "layout.md"],
        )?;
        assert_eq!(output.status.code(), Some(1));
        let value = report(&output)?;
        let found = rules(&value)?;
        assert!(found.contains(&"House.List"), "{value}");
        assert!(found.contains(&"House.Table"), "{value}");
    }
    Ok(())
}

#[test]
fn rejects_editorial_shortcuts_with_actionable_diagnostics() -> Result<()> {
    let directory = tempfile::tempdir()?;
    fs::write(
        directory.path().join("article.md"),
        "# Release\n\n筆者には設計の進歩に見える。\n理解しやすい。\n詳しくは[公式資料](https://example.com/design)を参照してほしい。\n混同しないようにしたい。\n事実関係は公式資料に基づく。\n",
    )?;
    let output = run(directory.path(), &["--format", "json", "article.md"])?;
    assert_eq!(output.status.code(), Some(1));
    let value = report(&output)?;
    let found = rules(&value)?;
    for rule in [
        "editorial.personal-reaction",
        "editorial.vague-benefit",
        "editorial.external-handoff",
        "editorial.reader-admonition",
        "editorial.process-commentary",
    ] {
        assert!(found.contains(&rule), "missing {rule}: {value}");
    }
    assert_eq!(value["sources"][0]["path"], "article.md");
    assert_eq!(value["sources"][0]["findings"][0]["line"], 3);
    assert!(
        value["sources"][0]["findings"][0]["message"]
            .as_str()
            .is_some_and(|message| !message.is_empty())
    );
    Ok(())
}

#[test]
fn preserves_code_metadata_urls_and_attributed_quotations() -> Result<()> {
    let directory = tempfile::tempdir()?;
    fs::write(
        directory.path().join("article.md"),
        "---\ntitle: 'I think this is useful'\n---\n# Result\n\nThe parser rejects an unmatched delimiter.\n\n```text\n筆者には進歩に見える。\nTODO\n```\n\n> 筆者には進歩に見える。\n\nSource: [Recorded statement][record].\n\n[record]: https://example.com/quoted-record\n",
    )?;
    let output = run(directory.path(), &["--format", "json", "article.md"])?;
    assert!(output.status.success(), "{output:?}");
    assert_eq!(rules(&report(&output)?)?, [] as [&str; 0]);
    Ok(())
}

#[test]
fn requires_a_source_for_expository_block_quotes() -> Result<()> {
    let directory = tempfile::tempdir()?;
    fs::write(
        directory.path().join("quote.md"),
        "# Policy\n\n> The old behavior is preserved.\n",
    )?;
    let output = run(directory.path(), &["--format", "json", "quote.md"])?;
    assert_eq!(output.status.code(), Some(1));
    assert!(rules(&report(&output)?)?.contains(&"editorial.unattributed-quote"));
    let output = run(
        directory.path(),
        &["--profile", "general", "--format", "json", "quote.md"],
    )?;
    assert!(output.status.success(), "{output:?}");
    Ok(())
}

#[test]
fn rejects_unfinished_content_empty_sections_and_broken_references() -> Result<()> {
    let directory = tempfile::tempdir()?;
    fs::write(
        directory.path().join("draft.md"),
        "# Draft\n\nTODO: supply the measured result.\n\n## Missing result\n\n## Evidence\n\nThe parser preserves the input [record][missing-record].\n",
    )?;
    let output = run(directory.path(), &["--format", "json", "draft.md"])?;
    assert_eq!(output.status.code(), Some(1));
    let value = report(&output)?;
    for rule in [
        "editorial.unfinished-text",
        "editorial.empty-section",
        "editorial.broken-reference",
    ] {
        assert!(rules(&value)?.contains(&rule), "{value}");
    }
    fs::write(
        directory.path().join("empty.md"),
        "---\ntitle: Empty\n---\n",
    )?;
    let output = run(directory.path(), &["--format", "json", "empty.md"])?;
    assert_eq!(output.status.code(), Some(1));
    assert!(rules(&report(&output)?)?.contains(&"editorial.empty-document"));
    Ok(())
}

#[test]
fn compares_authored_paragraphs_without_erasing_structure() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let sentence = "The decoder rejects duplicate object names before assigning the value.";
    fs::write(
        directory.path().join("repeat.md"),
        format!("# Result\n\n{sentence}\n\n## Details\n\n{sentence}\n"),
    )?;
    let output = run(directory.path(), &["--format", "json", "repeat.md"])?;
    assert_eq!(output.status.code(), Some(1));
    assert!(rules(&report(&output)?)?.contains(&"editorial.duplicate-paragraph"));
    Ok(())
}

#[test]
fn binds_reports_to_each_input_and_preserves_existing_reports() -> Result<()> {
    let directory = tempfile::tempdir()?;
    fs::write(
        directory.path().join("clean.md"),
        "# Result\n\nThe decoder rejects duplicate names.\n",
    )?;
    let arguments = ["--format", "json", "--report", "review.json", "clean.md"];
    let output = run(directory.path(), &arguments)?;
    assert!(output.status.success(), "{output:?}");
    let value = report(&output)?;
    let retained = fs::read(directory.path().join("review.json"))?;
    assert_eq!(serde_json::from_slice::<Value>(&retained)?, value);
    assert_eq!(value["version"], 1);
    assert_eq!(value["profile"], "expository");
    assert_eq!(
        value["sources"][0]["sha256"].as_str().map(str::len),
        Some(64)
    );
    assert!(
        value["sources"][0]["blocks"]
            .as_u64()
            .is_some_and(|n| n > 0)
    );
    fs::write(
        directory.path().join("clean.md"),
        "# Result\n\nThe decoder accepts duplicate names.\n",
    )?;
    let changed = run(directory.path(), &["--format", "json", "clean.md"])?;
    assert_ne!(
        report(&changed)?["sources"][0]["sha256"],
        value["sources"][0]["sha256"]
    );
    let refused = run(directory.path(), &arguments)?;
    assert_eq!(refused.status.code(), Some(2));
    assert_eq!(fs::read(directory.path().join("review.json"))?, retained);
    Ok(())
}

#[test]
fn reports_input_failures_separately_from_editorial_findings() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let output = run(directory.path(), &["--format", "json", "absent.md"])?;
    assert_eq!(output.status.code(), Some(2));
    assert_eq!(output.stdout, [] as [u8; 0]);
    assert!(String::from_utf8(output.stderr)?.contains("absent.md"));
    Ok(())
}

#[test]
fn preserves_intentional_voice_without_skipping_unfinished_work() -> Result<()> {
    let directory = tempfile::tempdir()?;
    fs::write(
        directory.path().join("essay.md"),
        "# Essay\n\nIn my opinion, the ending needs a different rhythm.\n",
    )?;
    let general = run(
        directory.path(),
        &["--profile", "general", "--format", "json", "essay.md"],
    )?;
    assert!(general.status.success(), "{general:?}");
    let expository = run(directory.path(), &["--format", "json", "essay.md"])?;
    assert_eq!(expository.status.code(), Some(1));
    fs::write(
        directory.path().join("essay.md"),
        "# Essay\n\nTODO: finish the ending.\n",
    )?;
    let unfinished = run(
        directory.path(),
        &["--profile", "general", "--format", "json", "essay.md"],
    )?;
    assert_eq!(unfinished.status.code(), Some(1));
    assert!(rules(&report(&unfinished)?)?.contains(&"editorial.unfinished-text"));
    Ok(())
}

#[test]
fn retains_linked_source_footnotes_and_sections_with_executable_content() -> Result<()> {
    let directory = tempfile::tempdir()?;
    fs::write(
        directory.path().join("sources.md"),
        "# Design\n\n> The decoder rejects duplicate names.[^spec]\n\n## Example\n\n```go\nfunc main() {}\n```\n\n[^spec]: [Specification](https://example.com/specification)\n",
    )?;
    let output = run(directory.path(), &["--format", "json", "sources.md"])?;
    assert!(output.status.success(), "{output:?}");
    Ok(())
}

#[test]
fn preserves_unicode_locations_and_reports_each_document() -> Result<()> {
    let directory = tempfile::tempdir()?;
    fs::write(
        directory.path().join("en.md"),
        "# Result\n\nİ😀 I THINK THAT the change is useful.\n",
    )?;
    fs::write(
        directory.path().join("zh.md"),
        "# 结果\n\n在我看来，这个设计很好。\n",
    )?;
    let output = run(directory.path(), &["--format", "json", "en.md", "zh.md"])?;
    assert_eq!(output.status.code(), Some(1));
    let value = report(&output)?;
    assert_eq!(value["sources"].as_array().map(Vec::len), Some(2));
    assert_eq!(value["sources"][0]["findings"][0]["line"], 3);
    assert_eq!(value["sources"][0]["findings"][0]["column"], 4);
    assert_eq!(
        value["sources"][0]["findings"][0]["snippet"],
        "I THINK THAT"
    );
    Ok(())
}

#[test]
fn a_source_caption_cannot_cross_a_section_or_an_executable_block() -> Result<()> {
    let directory = tempfile::tempdir()?;
    for barrier in ["## Another topic\n\n", "```text\nunrelated\n```\n\n"] {
        fs::write(
            directory.path().join("quote.md"),
            format!(
                "# Design\n\n> A statement requiring attribution.\n\n{barrier}Source: [Other record](https://example.com/other).\n"
            ),
        )?;
        let output = run(directory.path(), &["--format", "json", "quote.md"])?;
        assert_eq!(output.status.code(), Some(1));
        assert!(rules(&report(&output)?)?.contains(&"editorial.unattributed-quote"));
    }
    Ok(())
}
