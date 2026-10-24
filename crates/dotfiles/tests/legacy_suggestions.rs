#![cfg(unix)]

use anyhow::{Context, Result};
use std::path::Path;

const BASH: &str = "/bin/bash";
const ARGUMENT_FUNCTIONS: &str = "jq() { builtin printf '%s\\0' \"$@\"; }\njaq() { builtin printf '%s\\0' \"$@\"; }\ngrep() { builtin printf '%s\\0' \"$@\"; }\nrg() { builtin printf '%s\\0' \"$@\"; }\nfd() { builtin printf '%s\\0' \"$@\"; }\n";

fn arguments(command: &str, directory: &Path) -> Result<Vec<u8>> {
    let output = platform::process::command(BASH)
        .args([
            "--noprofile",
            "--norc",
            "-c",
            &format!("{ARGUMENT_FUNCTIONS}{command}"),
        ])
        .env_remove("BASH_ENV")
        .env_remove("ENV")
        .env_remove("SHELLOPTS")
        .env_remove("BASHOPTS")
        .env("DOTFILES_ARGUMENT_TEST", "value with spaces")
        .env("DOTFILES_ARGUMENT_SEQUENCE", "0")
        .current_dir(directory)
        .output()?;
    anyhow::ensure!(
        output.status.success(),
        "Bash argument observation failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    Ok(output.stdout)
}

#[test]
fn suggestions_deliver_the_expected_arguments_to_bash() -> Result<()> {
    let directory = tempfile::tempdir()?;
    for (source, expected) in [
        (
            "jq -c '.foo' ~/.codex/hooks.json 2>&1",
            "jaq -c '.foo' ~/.codex/hooks.json",
        ),
        (
            "jq '.foo' '~/path with spaces'",
            "jaq '.foo' '~/path with spaces'",
        ),
        (
            "jq '.foo' ~/path\\ with\\ spaces",
            "jaq '.foo' ~/path\\ with\\ spaces",
        ),
        (
            "jq '.foo' \"$HOME/file name\" ''",
            "jaq '.foo' \"$HOME/file name\" ''",
        ),
        (
            "jq '.foo' 'literal'\"suffix\"",
            "jaq '.foo' 'literal'\"suffix\"",
        ),
        (
            "jq '.foo' \"$DOTFILES_ARGUMENT_TEST\"",
            "jaq '.foo' \"$DOTFILES_ARGUMENT_TEST\"",
        ),
        (
            "grep -n 'a b' ~/path\\ with\\ spaces",
            "rg -n 'a b' ~/path\\ with\\ spaces",
        ),
        (
            "grep -e \"$DOTFILES_ARGUMENT_TEST\" \"$HOME/file name\"",
            "rg -e \"$DOTFILES_ARGUMENT_TEST\" \"$HOME/file name\"",
        ),
        (
            "grep --include \"$DOTFILES_ARGUMENT_TEST\" x",
            "rg -g \"$DOTFILES_ARGUMENT_TEST\" x",
        ),
        (
            "grep -e'filter with spaces' x",
            "rg -e 'filter with spaces' x",
        ),
        ("grep --include='*.rs' x", "rg -g '*.rs' x"),
        ("grep --include= x", "rg -g '' x"),
        ("grep -A3 x f", "rg -A 3 x f"),
        (
            "grep -e \"$((DOTFILES_ARGUMENT_SEQUENCE++))\" \"$((DOTFILES_ARGUMENT_SEQUENCE++))\"",
            "rg -e 0 1",
        ),
        (
            "grep x -e \"$DOTFILES_ARGUMENT_TEST\"",
            "rg -e \"$DOTFILES_ARGUMENT_TEST\" x",
        ),
        (
            "find -name \"$DOTFILES_ARGUMENT_TEST\"",
            "fd --glob \"$DOTFILES_ARGUMENT_TEST\"",
        ),
        (
            "find -name '~/path with spaces'",
            "fd --glob '~/path with spaces'",
        ),
    ] {
        let message = dotfiles::legacy::refusal(source, Some(directory.path()))?
            .context("legacy refusal missing")?;
        let suggested = message
            .lines()
            .find_map(|line| line.strip_prefix("Next action: "))
            .context("suggested command missing")?;
        assert_eq!(
            arguments(suggested, directory.path())?,
            arguments(expected, directory.path())?,
            "{source}: {suggested}"
        );
    }
    Ok(())
}

#[test]
fn uncertain_option_splitting_gives_guidance_instead_of_a_reconstructed_command() -> Result<()> {
    for source in [
        "grep -e\"$DOTFILES_ARGUMENT_TEST\" x",
        "grep --include=\"$DOTFILES_ARGUMENT_TEST\" x",
        "grep --exclude \"$DOTFILES_ARGUMENT_TEST\" x",
        "grep -e",
        "grep --include",
        "find -name",
        "find . -exec echo {}",
        "find . -exec ';'",
        "find . -name a -name b",
    ] {
        let message = dotfiles::legacy::refusal(source, None)?.context("legacy refusal missing")?;
        let next = message
            .lines()
            .find_map(|line| line.strip_prefix("Next action: "))
            .context("next action missing")?;
        assert!(next.starts_with("Use `"), "{source}: {next}");
    }
    Ok(())
}

#[test]
fn reordered_expansions_and_dynamic_option_names_get_guidance() -> Result<()> {
    let mut unsafe_suggestions = Vec::new();
    for source in [
        "grep \"$((DOTFILES_ARGUMENT_SEQUENCE++))\" -e \"$((DOTFILES_ARGUMENT_SEQUENCE++))\"",
        "find \"$((DOTFILES_ARGUMENT_SEQUENCE++))\" -name \"$((DOTFILES_ARGUMENT_SEQUENCE++))\"",
        "grep -\"$DOTFILES_ARGUMENT_TEST\" x",
        "grep -$DOTFILES_ARGUMENT_TEST x",
    ] {
        let message = dotfiles::legacy::refusal(source, None)?.context("legacy refusal missing")?;
        let next = message
            .lines()
            .find_map(|line| line.strip_prefix("Next action: "))
            .context("next action missing")?;
        if !next.starts_with("Use `") {
            unsafe_suggestions.push(format!("{source}: {next}"));
        }
    }
    assert!(
        unsafe_suggestions.is_empty(),
        "{}",
        unsafe_suggestions.join("\n")
    );
    Ok(())
}
