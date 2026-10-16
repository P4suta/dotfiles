//! The local policy gate.
//!
//! One binary serves four global entry points:
//!
//! - `~/.local/bin/git` runs `dotguard git …`, which applies the argument policy and then executes git, and on Windows a copy named `git.exe` acts as the wrapper.
//! - `~/.config/git/hooks/pre-push` runs `dotguard pre-push`, the force-push gate.
//! - `~/.config/git/hooks/commit-msg` runs `dotguard commit-msg`, which strips attribution and checks the language.
//! - `~/.config/git/hooks/pre-commit` runs `dotguard pre-commit`, the language and staged-diff gates.
//!
//! `core.hooksPath` in the global Git configuration points every repository at that hooks directory, so no repository needs to opt in.

use dotguard::{attribution, bypass, gitargv, lang, prepush, real_git, refusal, staged};

use bypass::Rule;
use dotguard::git_wrapper::INVOKED;
use std::io::Write;
use std::path::Path;
use std::process::{ExitCode, Stdio};

fn main() -> ExitCode {
    let mut process_args = std::env::args();
    let invoked = process_args.next().unwrap_or_default();
    if invoked_as_git(&invoked) {
        return git_wrapper(&process_args.collect::<Vec<_>>());
    }
    let Some(cmd) = process_args.next() else {
        usage();
        return ExitCode::FAILURE;
    };

    match cmd.as_str() {
        "git" => git_wrapper(&process_args.collect::<Vec<_>>()),
        "pre-push" => code(prepush::run(
            process_args.next().as_deref().unwrap_or("origin"),
        )),
        "commit-msg" => code(commit_msg(process_args.next().as_deref().unwrap_or(""))),
        "pre-commit" => code(pre_commit()),
        "post-commit" | "renovate-gate" | "renovate" => companion(&cmd, process_args),
        "scan" => code(scan_paths(&process_args.collect::<Vec<_>>())),
        "--version" | "-V" | "version" => {
            println!("dotguard {}", env!("CARGO_PKG_VERSION"));
            ExitCode::SUCCESS
        }
        _ => {
            usage();
            ExitCode::FAILURE
        }
    }
}

/// Windows lacks `exec` from a shell launcher, so a copy of this binary named `git.exe` stands in front of Git for Windows and applies the policy itself.
fn invoked_as_git(argv0: &str) -> bool {
    Path::new(argv0)
        .file_stem()
        .is_some_and(|stem| stem.eq_ignore_ascii_case("git"))
}

fn code(n: i32) -> ExitCode {
    ExitCode::from(u8::try_from(n).unwrap_or(1))
}

fn companion(mode: &str, arguments: impl Iterator<Item = String>) -> ExitCode {
    let program = match std::env::current_exe() {
        Ok(executable) => executable.with_file_name(format!(
            "{}{}",
            dotguard::RENOVATE_BINARY,
            std::env::consts::EXE_SUFFIX
        )),
        Err(error) => return companion_failure(&error),
    };
    let mut command = platform::process::repository_command(program);
    command
        .arg(platform::BUILD_HASH)
        .arg(mode)
        .args(arguments)
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit());
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        companion_failure(&command.exec())
    }
    #[cfg(windows)]
    {
        if mode == "git" {
            dotguard::git_wrapper::ignore_console_interrupts();
        }
        match command.status() {
            Ok(status) => std::process::exit(status.code().unwrap_or(1)),
            Err(error) => companion_failure(&error),
        }
    }
}

fn companion_failure(error: &std::io::Error) -> ExitCode {
    refusal::Refusal::new(
        "tool.failure",
        "the matching policy companion could not be started",
        "chezmoi apply",
    )
    .evidence(error.to_string())
    .emit();
    ExitCode::FAILURE
}

fn usage() {
    eprintln!(
        "dotguard {}\n\n\
         usage:\n  \
         dotguard git <args…>        apply policy, then exec the real git\n  \
         dotguard pre-push <remote>  refuse deletions and non-fast-forward pushes (reads stdin)\n  \
         dotguard commit-msg <file>  strip agent attribution, refuse foreign scripts\n  \
         dotguard pre-commit         refuse foreign scripts in the staged diff\n  \
         dotguard post-commit        re-sign an unsigned HEAD, or roll it back\n  \
         dotguard renovate-gate <remote>  refuse a push that leaves a touched dependency stale (reads stdin)\n  \
         dotguard renovate run         the read-only lookup container, by hand\n  \
         dotguard scan <path…>       the same language check, on files\n  \
",
        env!("CARGO_PKG_VERSION")
    );
}

fn git_wrapper(argv: &[String]) -> ExitCode {
    let globals = gitargv::global_options(argv);
    if matches!(
        argv.get(globals.len()).map(String::as_str),
        Some("commit" | "push")
    ) {
        return companion("git", argv.iter().cloned());
    }
    dotguard::git_wrapper::run::<false>(argv)
}

/// `ALLOW_FOREIGN=1` before the Git command whose hook refused, when that command passed through the wrapper.
fn foreign_waiver() -> Option<String> {
    std::env::var(INVOKED)
        .ok()
        .filter(|line| !line.trim().is_empty())
        .map(|line| format!("ALLOW_FOREIGN=1 {line}"))
}

/// The language policy for the current repository.
///
/// `guard.lang` defaults to English and accepts `japanese` and `off`, and every refusal names it.
///
///     git config guard.lang japanese
///     git config guard.lang off
fn lang_mode() -> Option<lang::Mode> {
    lang::Mode::parse(real_git::capture(&["config", "--get", "guard.lang"]).as_deref())
}

fn comment_char() -> char {
    real_git::capture(&["config", "--get", "core.commentChar"])
        .and_then(|v| v.trim().chars().next())
        .filter(|c| c.is_ascii() && *c != 'a') // `auto` resolves per message and starts from `#`.
        .unwrap_or('#')
}

fn commit_msg(path: &str) -> i32 {
    if path.is_empty() {
        return 0;
    }
    let Ok(raw) = std::fs::read_to_string(path) else {
        return 0;
    };

    let (cleaned, removed) = attribution::strip(&raw);
    if removed > 0 {
        platform::cleanup::report(
            std::fs::File::create(path).and_then(|mut f| f.write_all(cleaned.as_bytes())),
            format_args!("rewrite {path} without the attribution"),
        );
        eprintln!(
            "::notice:: removed {removed} agent attribution line(s) from the commit message."
        );
    }

    let Some(mode) = lang_mode() else { return 0 };
    let body = lang::commit_message_body(&cleaned, comment_char());
    let diagnostics = lang::scan(&body, mode);
    if diagnostics.is_empty() {
        return 0;
    }

    let expected = match mode {
        lang::Mode::Japanese => "English or Japanese",
        _ => "English",
    };
    lang::report("commit message", &body, &diagnostics);
    // Git runs the hook from the top of the work tree, and the retry may run from anywhere.
    let message = std::path::absolute(path)
        .map_or_else(|_| path.to_owned(), |file| file.display().to_string());
    let retry = std::env::var(INVOKED)
        .ok()
        .and_then(|line| refusal::words(&line))
        .and_then(|words| gitargv::commit_retry(&words, &message, authorship, picking))
        .unwrap_or_else(|| {
            Ok(["git", "commit", "<options>", "--edit", "--file", &message]
                .map(str::to_owned)
                .into())
        });
    let mut cause =
        format!("the commit message is not written in {expected}; rewrite it and commit again");
    if let Err(reason) = &retry {
        cause.push_str(".\nNo command is suggested: ");
        cause.push_str(reason.trim_end_matches('.'));
    }
    refuse_foreign(&lang::refusal(
        "commit.language",
        &cause,
        diagnostics
            .iter()
            .map(|hit| lang::evidence("commit message", hit))
            .collect(),
        retry.ok().map(|words| refusal::command(&words)),
        foreign_waiver(),
    ))
}

/// The authorship and date of `revision`, as the refused commit took them from it.
fn authorship(revision: &str) -> Option<gitargv::Authorship> {
    let peeled = format!("{revision}^{{commit}}");
    let read = real_git::capture(&[
        "log",
        "-1",
        "--no-show-signature",
        "--date=raw",
        "--format=%an <%ae>%x00%ad",
        "--end-of-options",
        &peeled,
        "--",
    ])?;
    let (author, date) = read.trim_end_matches('\n').split_once('\0')?;
    Some(gitargv::Authorship {
        author: author.to_owned(),
        date: date.to_owned(),
    })
}

/// Whether a cherry-pick or rebase pick runs, as `git commit` decides it, or `None` when Git couldn't say.
fn picking() -> Option<bool> {
    // A pending merge makes the commit a merge even when a pick also remains.
    Some(!exists("MERGE_HEAD")? && exists("CHERRY_PICK_HEAD")?)
}

/// Whether the special reference `name` exists.
/// Git keeps `MERGE_HEAD` and `CHERRY_PICK_HEAD` as files in the Git directory of the worktree under every reference storage format, and `git commit` checks those files.
/// `git show-ref --exists` doesn't decide this on every supported Git version.
/// Git 2.43 reports a missing `MERGE_HEAD` as a lookup error.
fn exists(name: &str) -> Option<bool> {
    let path = real_git::capture(&["rev-parse", "--git-path", name])?;
    std::fs::exists(path.trim_end_matches(['\r', '\n'])).ok()
}

fn pre_commit() -> i32 {
    if lang_mode().is_none() {
        return 0;
    }
    let Some(diff) = staged::staged_diff() else {
        return 0;
    };
    let files: Vec<_> = staged::scan_diff(&diff)
        .into_iter()
        .filter(|f| !staged::is_suppressed(&f.path))
        .collect();
    if files.is_empty() {
        return 0;
    }

    let diagnostics: Vec<String> = files
        .iter()
        .flat_map(|f| {
            f.diagnostics.iter().map(|(line, hit)| {
                lang::evidence(
                    &f.path,
                    &lang::Diagnostic {
                        line: *line,
                        ..*hit
                    },
                )
            })
        })
        .collect();
    for hit in &diagnostics {
        eprintln!("  {hit}");
    }
    let mut inspect = vec!["git", "diff", "--cached", "--"];
    inspect.extend(files.iter().map(|f| f.path.as_str()));
    refuse_foreign(&lang::refusal(
        "commit.language",
        "staged changes add text in a script this machine does not write; remove it, stage the result, and commit again",
        diagnostics,
        Some(refusal::command(&inspect)),
        foreign_waiver(),
    ))
}

fn scan_paths(paths: &[String]) -> i32 {
    let mut diagnostics = Vec::new();
    for p in paths {
        let Ok(text) = std::fs::read_to_string(p) else {
            continue;
        };
        if staged::suppressed_in_source(&text) {
            continue;
        }
        let found = lang::scan(&text, lang::Mode::Content);
        lang::report(p, &text, &found);
        diagnostics.extend(found.iter().map(|hit| lang::evidence(p, hit)));
    }
    if diagnostics.is_empty() {
        return 0;
    }
    let mut scan = vec!["dotguard", "scan"];
    scan.extend(paths.iter().map(String::as_str));
    refuse_foreign(&lang::refusal(
        "scan.language",
        &format!(
            "{} contaminating character(s); remove them and scan again",
            diagnostics.len()
        ),
        diagnostics,
        Some(refusal::command(&scan)),
        Some(format!("ALLOW_FOREIGN=1 {}", refusal::command(&scan))),
    ))
}

/// The shared tail of every language refusal: the recorded override, or the structured refusal.
fn refuse_foreign(refused: &refusal::Refusal) -> i32 {
    let argv: Vec<String> = std::env::args().collect();
    if bypass::bypassed(Rule::Foreign) {
        bypass::record("BYPASS", Rule::Foreign, "foreign script", &argv);
        eprintln!("\n::warning:: ALLOW_FOREIGN=1 — allowing it.");
        return 0;
    }
    bypass::record("REJECT", Rule::Foreign, "foreign script", &argv);
    refused.emit();
    1
}
