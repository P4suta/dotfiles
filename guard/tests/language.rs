//! The language gate run as installed: a refused scan ends with one complete record.

use dotguard::refusal::{Refusal, words};
use std::process::Command;

#[test]
fn a_contaminated_file_is_refused_with_a_rescan_and_a_recorded_waiver() {
    let scope = std::env::temp_dir().join(format!("dotguard-language-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&scope);
    std::fs::create_dir_all(&scope).unwrap();
    let cyrillic = char::from_u32(0x0441).unwrap();
    std::fs::write(scope.join("a.txt"), format!("plain\nmixed {cyrillic}\n")).unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_dotguard"))
        .current_dir(&scope)
        .env("HOME", &scope)
        .env("USERPROFILE", &scope)
        .env_remove("ALLOW_FOREIGN")
        .args(["scan", "a.txt"])
        .output()
        .unwrap();
    let _ = std::fs::remove_dir_all(&scope);
    assert_eq!(output.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&output.stderr);
    let refusal = Refusal::parse(stderr.lines().last().unwrap()).expect("a structured refusal");
    assert!(refusal.is_complete(), "{refusal:?}");
    assert_eq!(refusal.rule, "scan.language");
    assert_eq!(refusal.next, "dotguard scan a.txt");
    assert_eq!(
        refusal.waiver.as_deref(),
        Some("ALLOW_FOREIGN=1 dotguard scan a.txt")
    );
    assert_eq!(
        refusal.evidence,
        ["a.txt:2:7  '\u{441}'  U+0441  [Cyrillic]"]
    );
}

/// A repository whose `git` is a copy of the wrapper and whose commit hooks run the language gates as installed.
struct Checkout {
    scope: std::path::PathBuf,
    git: std::path::PathBuf,
}

impl Drop for Checkout {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.scope);
    }
}

impl Checkout {
    fn new(name: &str) -> Self {
        let scope =
            std::env::temp_dir().join(format!("dotguard-language-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&scope);
        std::fs::create_dir_all(scope.join("repository")).unwrap();
        std::fs::write(
            scope.join("gitconfig"),
            "[user]\n\tname = Fixture\n\temail = fixture@example.invalid\n",
        )
        .unwrap();
        let git = scope.join(format!("git{}", std::env::consts::EXE_SUFFIX));
        std::fs::copy(env!("CARGO_BIN_EXE_dotguard"), &git).unwrap();
        let checkout = Self { scope, git };
        assert!(checkout.run(&["init", "-q"], &[]).status.success());
        let hooks = checkout.path().join(".git/hooks");
        std::fs::create_dir_all(&hooks).unwrap();
        let dotguard = env!("CARGO_BIN_EXE_dotguard").replace('\\', "/");
        for (hook, arguments) in [("pre-commit", ""), ("commit-msg", " \"$1\"")] {
            let script = hooks.join(hook);
            std::fs::write(
                &script,
                format!("#!/bin/sh\nexec '{dotguard}' {hook}{arguments}\n"),
            )
            .unwrap();
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
            }
        }
        checkout
    }

    fn path(&self) -> std::path::PathBuf {
        self.scope.join("repository")
    }

    fn isolate<'a>(&self, command: &'a mut Command) -> &'a mut Command {
        for (name, _) in std::env::vars_os() {
            let text = name.to_string_lossy();
            if text.starts_with("ALLOW_")
                || text.starts_with("GIT_")
                || text.starts_with("DOTGUARD_")
            {
                command.env_remove(name);
            }
        }
        command
            .current_dir(self.path())
            .env("HOME", self.scope.join("home"))
            .env("USERPROFILE", self.scope.join("home"))
            .env("GIT_CONFIG_GLOBAL", self.scope.join("gitconfig"))
            .env("GIT_CONFIG_NOSYSTEM", "1")
    }

    /// Runs the wrapper copy with each named waiver set.
    fn run(&self, arguments: &[&str], waivers: &[&str]) -> std::process::Output {
        let mut command = Command::new(&self.git);
        self.isolate(&mut command).args(arguments);
        for waiver in waivers {
            command.env(waiver, "1");
        }
        command.output().unwrap()
    }

    fn succeeds(&self, arguments: &[&str], waivers: &[&str]) -> String {
        let output = self.run(arguments, waivers);
        assert!(
            output.status.success(),
            "git {arguments:?}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8_lossy(&output.stdout).trim().to_owned()
    }
}

/// The record a refused command printed as the last line of its standard error.
fn last_record(output: &std::process::Output) -> Refusal {
    assert_eq!(output.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&output.stderr);
    let refusal = Refusal::parse(stderr.trim_end().lines().last().unwrap())
        .unwrap_or_else(|| panic!("the record must be the last line: {stderr}"));
    assert!(refusal.is_complete(), "{refusal:?}");
    refusal
}

#[test]
fn a_foreign_staged_line_is_refused_with_a_waiver_that_repeats_the_commit() {
    let checkout = Checkout::new("staged");
    let cyrillic = char::from_u32(0x0441).unwrap();
    std::fs::write(checkout.path().join("a.txt"), format!("mixed {cyrillic}\n")).unwrap();
    checkout.succeeds(&["add", "a.txt"], &[]);
    let refused = last_record(&checkout.run(&["commit", "-q", "-m", "add a"], &[]));
    assert_eq!(refused.rule, "commit.language");
    assert_eq!(refused.next, "git diff --cached -- a.txt");
    assert_eq!(
        refused.waiver.as_deref(),
        Some("ALLOW_FOREIGN=1 git commit -q -m 'add a'")
    );
    checkout.succeeds(&["commit", "-q", "-m", "add a"], &["ALLOW_FOREIGN"]);
    assert_eq!(checkout.succeeds(&["log", "--format=%s"], &[]), "add a");
}

/// The next step keeps every option of the refused commit, so an amend stays an amend.
#[test]
fn a_foreign_message_is_refused_with_a_retry_that_keeps_the_amend() {
    let checkout = Checkout::new("message");
    std::fs::write(checkout.path().join("a.txt"), "plain\n").unwrap();
    checkout.succeeds(&["add", "a.txt"], &[]);
    checkout.succeeds(&["commit", "-q", "-m", "add a"], &[]);
    let message = format!("fix {}", char::from_u32(0x0441).unwrap());
    let refused = last_record(&checkout.run(&["commit", "-q", "--amend", "-m", &message], &[]));
    assert_eq!(refused.rule, "commit.language");
    let next = words(&refused.next).expect("a command line");
    let (file, retry) = next.split_last().unwrap();
    assert_eq!(
        retry,
        ["git", "commit", "-q", "--amend", "--edit", "--file"]
    );
    assert!(std::path::Path::new(file).is_absolute(), "{file}");
    assert_eq!(
        std::fs::read_to_string(file).unwrap().trim_end(),
        message,
        "the saved message of the refused commit"
    );
    let mut command = Command::new(&checkout.git);
    let corrected = checkout
        .isolate(&mut command)
        .args(&next[1..])
        .env("GIT_EDITOR", "printf 'fix c\\n' >")
        .output()
        .unwrap();
    assert!(
        corrected.status.success(),
        "{}",
        String::from_utf8_lossy(&corrected.stderr)
    );
    assert_eq!(
        checkout.succeeds(&["log", "--format=%B"], &[]),
        "fix c",
        "the next step amends the one commit with the corrected message"
    );
}

/// The waiver repeats the refused amend exactly.
#[test]
fn a_foreign_message_is_refused_with_a_waiver_that_repeats_the_commit() {
    let checkout = Checkout::new("waived");
    std::fs::write(checkout.path().join("a.txt"), "plain\n").unwrap();
    checkout.succeeds(&["add", "a.txt"], &[]);
    checkout.succeeds(&["commit", "-q", "-m", "add a"], &[]);
    let message = format!("fix {}", char::from_u32(0x0441).unwrap());
    let refused = last_record(&checkout.run(&["commit", "-q", "--amend", "-m", &message], &[]));
    assert_eq!(
        refused.waiver.as_deref(),
        Some(format!("ALLOW_FOREIGN=1 git commit -q --amend -m '{message}'").as_str())
    );
    checkout.succeeds(
        &["commit", "-q", "--amend", "-m", &message],
        &["ALLOW_FOREIGN"],
    );
    assert_eq!(
        checkout.succeeds(&["log", "--format=%B"], &[]),
        message,
        "one amended commit with exactly the refused message"
    );
}

/// A hook run by a Git that did not pass through the wrapper cannot know the refused command, so it names the override in its cause instead of guessing a waiver.
#[test]
fn a_language_refusal_outside_the_wrapper_names_the_override_without_a_waiver() {
    let checkout = Checkout::new("unwrapped");
    let message = checkout.path().join("MESSAGE");
    std::fs::write(
        &message,
        format!("fix {}\n", char::from_u32(0x0441).unwrap()),
    )
    .unwrap();
    let mut command = Command::new(env!("CARGO_BIN_EXE_dotguard"));
    let output = checkout
        .isolate(&mut command)
        .arg("commit-msg")
        .arg(&message)
        .output()
        .unwrap();
    let refused = last_record(&output);
    assert_eq!(refused.rule, "commit.language");
    assert_eq!(refused.waiver, None);
    assert!(
        refused.cause.contains("ALLOW_FOREIGN=1"),
        "{}",
        refused.cause
    );
    assert_eq!(
        words(&refused.next).unwrap(),
        [
            "git",
            "commit",
            "<options>",
            "--edit",
            "--file",
            &message.display().to_string()
        ],
        "the options of an unknown commit are left for its author"
    );
}
