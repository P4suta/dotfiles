use anyhow::Result;
use platform::contribution::Notice;

const REGISTRATION: &str = r#"{
    "schema": 1,
    "repository": "dotnet/runtime",
    "notice": "AI generated this contribution.",
    "source": "https://github.com/dotnet/runtime/blob/main/.github/copilot-instructions.md",
    "reason": "The destination requires a visible disclosure.",
    "remove_when": "The destination removes its disclosure requirement."
}"#;

#[test]
fn an_exact_required_notice_leaves_all_other_prose_visible() -> Result<()> {
    let policy = Notice::parse(REGISTRATION.as_bytes(), "dotnet/runtime")?;
    assert_eq!(
        policy
            .project("The iterator keeps its final value.\n\nAI generated this contribution.\n")?,
        "The iterator keeps its final value.\n\n"
    );
    let remaining =
        policy.project("AI-assisted unrelated wording.\n\nAI generated this contribution.\n")?;
    assert!(rules::provenance_rules::find(&remaining).is_some());
    Ok(())
}

#[test]
fn missing_duplicate_embedded_and_nonfinal_notices_fail() -> Result<()> {
    let policy = Notice::parse(REGISTRATION.as_bytes(), "dotnet/runtime")?;
    for body in [
        "The iterator keeps its final value.",
        "AI generated this contribution.\nAI generated this contribution.",
        "A prefix. AI generated this contribution.",
        "AI generated this contribution.\nMore text.",
        "`AI generated this contribution.`",
        "<!-- AI generated this contribution. -->",
    ] {
        assert!(policy.project(body).is_err(), "{body}");
    }
    Ok(())
}

#[test]
fn registrations_cannot_expand_to_another_destination_or_hidden_text() {
    assert!(Notice::parse(REGISTRATION.as_bytes(), "JoshClose/CsvHelper").is_err());
    for (from, to) in [
        ("\"schema\": 1", "\"schema\": 2"),
        ("https://github.com/", "http://github.com/"),
        (
            "AI generated this contribution.",
            "`AI generated this contribution.`",
        ),
        (
            "AI generated this contribution.",
            "AI generated\\nthis contribution.",
        ),
        (
            "\"reason\": \"The destination requires a visible disclosure.\"",
            "\"reason\": \"\"",
        ),
        (
            "\"remove_when\": \"The destination removes its disclosure requirement.\"",
            "\"remove_when\": \"\"",
        ),
    ] {
        let invalid = REGISTRATION.replace(from, to);
        assert!(
            Notice::parse(invalid.as_bytes(), "dotnet/runtime").is_err(),
            "{invalid}"
        );
    }
}

#[test]
fn owner_files_never_apply_to_an_unregistered_destination() -> Result<()> {
    let home = tempfile::tempdir()?;
    let directory = home.path().join(".config/dotfiles/contribution-notices");
    std::fs::create_dir_all(&directory)?;
    std::fs::write(directory.join("dotnet__runtime.json"), REGISTRATION)?;
    let body = "The iterator keeps its final value.\n\nAI generated this contribution.\n";
    assert_eq!(
        platform::contribution::project_in(home.path(), "dotnet/runtime", body)?,
        "The iterator keeps its final value.\n\n"
    );
    assert_eq!(
        platform::contribution::project_in(home.path(), "JoshClose/CsvHelper", body)?,
        body
    );
    std::fs::write(directory.join("dotnet__runtime.json"), "{}")?;
    assert!(platform::contribution::project_in(home.path(), "dotnet/runtime", body).is_err());
    Ok(())
}
