//! The language gate run as installed: a refused scan ends with one complete record.

use dotguard::refusal::{Refusal, words};
use std::process::Command;

#[path = "support/fixture_git.rs"]
mod fixture_git;
#[path = "support/wrapper.rs"]
mod wrapper;

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
    assert_eq!(
        refusal.next.as_deref().unwrap_or_default(),
        "dotguard scan a.txt"
    );
    assert_eq!(
        refusal.waiver.as_deref(),
        Some("ALLOW_FOREIGN=1 dotguard scan a.txt")
    );
    assert_eq!(
        refusal.evidence,
        ["a.txt:2:7  '\u{441}'  U+0441  [Cyrillic]"]
    );
}

/// A repository with a copy of the wrapper as `git`, whose commit hooks run the language gates as installed.
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
        wrapper::install(&git);
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

    fn command(&self, program: impl AsRef<std::ffi::OsStr>) -> Command {
        let mut command = fixture_git::command(program, &self.scope.join("gitconfig"));
        for (name, _) in std::env::vars_os() {
            let text = name.to_string_lossy();
            if text.starts_with("ALLOW_") || text.starts_with("DOTGUARD_") {
                command.env_remove(name);
            }
        }
        command
            .current_dir(self.path())
            .env("HOME", self.scope.join("home"))
            .env("USERPROFILE", self.scope.join("home"));
        command
    }

    /// Runs the wrapper copy with each named waiver set.
    fn run(&self, arguments: &[&str], waivers: &[&str]) -> std::process::Output {
        let mut command = self.command(&self.git);
        command.args(arguments);
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
    assert_eq!(
        refused.next.as_deref().unwrap_or_default(),
        "git diff --cached -- a.txt"
    );
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
    let next = words(refused.next.as_deref().unwrap_or_default()).expect("a command line");
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
    let corrected = checkout
        .command(&checkout.git)
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

/// A hook run by a Git that didn't pass through the wrapper can't know the refused command.
/// It names the override in its cause instead of guessing a waiver.
#[test]
fn a_language_refusal_outside_the_wrapper_names_the_override_without_a_waiver() {
    let checkout = Checkout::new("unwrapped");
    let message = checkout.path().join("MESSAGE");
    std::fs::write(
        &message,
        format!("fix {}\n", char::from_u32(0x0441).unwrap()),
    )
    .unwrap();
    let output = checkout
        .command(env!("CARGO_BIN_EXE_dotguard"))
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
        words(refused.next.as_deref().unwrap_or_default()).unwrap(),
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

/// Runs `arguments` through the wrapper with an editor that writes `message`.
fn edited(checkout: &Checkout, arguments: &[&str], message: &str) -> std::process::Output {
    checkout
        .command(&checkout.git)
        .args(arguments)
        .env("GIT_EDITOR", format!("printf '{message}' >"))
        .output()
        .unwrap()
}

/// The suggested retry of a refused commit, run with an editor that writes `message`.
fn retried(checkout: &Checkout, refused: &Refusal, message: &str) {
    let next = words(refused.next.as_deref().expect("a retry")).expect("a command line");
    let output = edited(
        checkout,
        &next[1..].iter().map(String::as_str).collect::<Vec<_>>(),
        message,
    );
    assert!(
        output.status.success(),
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

/// `--fixup=amend:` and `--fixup=reword:` commits may have no changes, and a reword leaves staged changes out, so the retry makes the same commit.
#[test]
fn a_refused_fixup_is_retried_as_the_same_kind_of_commit() {
    for kind in ["amend", "reword"] {
        let checkout = Checkout::new(&format!("fixup-{kind}"));
        std::fs::write(checkout.path().join("a.txt"), "plain\n").unwrap();
        checkout.succeeds(&["add", "a.txt"], &[]);
        checkout.succeeds(&["commit", "-q", "-m", "add a"], &[]);
        std::fs::write(checkout.path().join("b.txt"), "plain\n").unwrap();
        if kind == "reword" {
            checkout.succeeds(&["add", "b.txt"], &[]);
        }
        let refused = last_record(&edited(
            &checkout,
            &["commit", "-q", &format!("--fixup={kind}:HEAD")],
            r"amend! add a\n\nfix \321\201\n",
        ));
        assert_eq!(refused.rule, "commit.language");
        retried(&checkout, &refused, r"amend! add a\n\nfix c\n");
        assert_eq!(
            checkout.succeeds(&["log", "--format=%s"], &[]),
            "amend! add a\nadd a",
            "git {kind}: one new commit"
        );
        assert_eq!(
            checkout.succeeds(
                &["diff-tree", "--no-commit-id", "--name-only", "-r", "HEAD"],
                &[]
            ),
            "",
            "git {kind}: the commit is empty"
        );
        if kind == "reword" {
            assert_eq!(
                checkout.succeeds(&["diff", "--cached", "--name-only"], &[]),
                "b.txt",
                "a reword leaves the staged change staged"
            );
        }
    }
}

/// `-C` copies the authorship and date of its commit, so the retry records the same authorship.
#[test]
fn a_refused_reuse_is_retried_with_the_reused_authorship() {
    let checkout = Checkout::new("reuse");
    std::fs::write(checkout.path().join("a.txt"), "plain\n").unwrap();
    checkout.succeeds(&["add", "a.txt"], &[]);
    let message = format!("fix {}", char::from_u32(0x0441).unwrap());
    checkout.succeeds(
        &[
            "commit",
            "-q",
            "--author",
            "Other <other@example.invalid>",
            "--date",
            "1700000000 +0900",
            "-m",
            &message,
        ],
        &["ALLOW_FOREIGN"],
    );
    std::fs::write(checkout.path().join("b.txt"), "plain\n").unwrap();
    checkout.succeeds(&["add", "b.txt"], &[]);
    let refused = last_record(&checkout.run(&["commit", "-q", "-C", "HEAD"], &[]));
    assert_eq!(refused.rule, "commit.language");
    retried(&checkout, &refused, r"fix c\n");
    assert_eq!(
        checkout.succeeds(
            &["log", "-1", "--date=raw", "--format=%an <%ae> %ad|%s"],
            &[]
        ),
        "Other <other@example.invalid> 1700000000 +0900|fix c"
    );
}

/// `--reset-author` needs `-C`, `-c`, or `--amend`, so the retry drops it with the reuse and still records the committer's authorship.
#[test]
fn a_refused_reuse_with_a_reset_author_is_retried_as_the_committer() {
    for letter in ["-C", "-c"] {
        let checkout = Checkout::new(&format!("reset-author{letter}"));
        std::fs::write(checkout.path().join("a.txt"), "plain\n").unwrap();
        checkout.succeeds(&["add", "a.txt"], &[]);
        let message = format!("fix {}", char::from_u32(0x0441).unwrap());
        checkout.succeeds(
            &[
                "commit",
                "-q",
                "--author",
                "Other <other@example.invalid>",
                "-m",
                &message,
            ],
            &["ALLOW_FOREIGN"],
        );
        std::fs::write(checkout.path().join("b.txt"), "plain\n").unwrap();
        checkout.succeeds(&["add", "b.txt"], &[]);
        let refused = last_record(&edited(
            &checkout,
            &["commit", "-q", letter, "HEAD", "--reset-author"],
            &message,
        ));
        assert_eq!(refused.rule, "commit.language");
        retried(&checkout, &refused, r"fix c\n");
        assert_eq!(
            checkout.succeeds(&["log", "--format=%an <%ae>|%s"], &[]),
            "Fixture <fixture@example.invalid>|fix c\nOther <other@example.invalid>|fix \u{441}",
            "git commit {letter} HEAD --reset-author"
        );
    }
}

/// During a cherry-pick Git takes the authorship from the picked commit rather than from `-c`, so the retry does too.
#[test]
fn a_refused_reuse_during_a_pick_is_retried_with_the_picked_authorship() {
    let checkout = Checkout::new("pick");
    let commit = |content: &str, author: &str| {
        std::fs::write(checkout.path().join("a.txt"), content).unwrap();
        checkout.succeeds(&["add", "a.txt"], &[]);
        checkout.succeeds(&["commit", "-q", "--author", author, "-m", content], &[]);
    };
    commit("base\n", "Fixture <fixture@example.invalid>");
    checkout.succeeds(&["checkout", "-q", "-b", "side"], &[]);
    commit("side\n", "Picked <picked@example.invalid>");
    checkout.succeeds(&["checkout", "-q", "-"], &[]);
    commit("main\n", "Other <other@example.invalid>");
    assert!(!checkout.run(&["cherry-pick", "side"], &[]).status.success());
    std::fs::write(checkout.path().join("a.txt"), "both\n").unwrap();
    checkout.succeeds(&["add", "a.txt"], &[]);
    let refused = last_record(&edited(
        &checkout,
        &["commit", "-q", "-c", "HEAD"],
        &format!("both {}", char::from_u32(0x0441).unwrap()),
    ));
    assert_eq!(refused.rule, "commit.language");
    retried(&checkout, &refused, r"both\n");
    assert_eq!(
        checkout.succeeds(&["log", "-1", "--format=%an|%s"], &[]),
        "Picked|both"
    );
}
