#![no_main]

use libfuzzer_sys::fuzz_target;
use rules::lint_hook_rules::{Outcome, ShellSyntax, shell_syntax};

fn reference_location(diagnostic: &str) -> bool {
    let Some((position, reason)) = diagnostic.split_once(": ") else {
        return false;
    };
    let Some((line, column)) = position.split_once(':') else {
        return false;
    };
    let positive_position = |value: &str| {
        value == "?"
            || !value.is_empty()
                && value.bytes().all(|byte| byte.is_ascii_digit())
                && value.bytes().any(|byte| byte != b'0')
    };
    positive_position(line) && positive_position(column) && !reason.trim().is_empty()
}

fuzz_target!(|data: &[u8]| {
    let Ok(text) = std::str::from_utf8(data) else {
        return;
    };
    for outcome in [
        Outcome::Clean,
        Outcome::Excluded,
        Outcome::Findings,
        Outcome::Missing,
        Outcome::Timeout,
        Outcome::Failed,
    ] {
        let result = shell_syntax(outcome, text);
        assert_eq!(
            result,
            match outcome {
                Outcome::Clean => ShellSyntax::Parsed,
                Outcome::Findings if reference_location(text) => ShellSyntax::Rejected,
                _ => ShellSyntax::Unavailable,
            }
        );
        assert_eq!(result == ShellSyntax::Parsed, outcome == Outcome::Clean);
        if !matches!(outcome, Outcome::Clean | Outcome::Findings) {
            assert_eq!(result, ShellSyntax::Unavailable);
        }
    }
    let (source, tree) = text.split_once("\n---\n").unwrap_or(("jq a", text));
    let _ = dotfiles::legacy::check_shell_spelling_tree(source, tree);
    let _ = dotfiles::legacy::check_powershell_spelling_tree(source, tree);
});
