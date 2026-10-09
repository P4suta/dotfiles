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

use dotguard::{
    attribution, bypass, gate_rules, gitargv, lang, postcommit, prepush, real_git, refusal,
    renovate, stack, staged,
};

use bypass::Rule;
use gate_rules::Admission;
use std::io::Write;
use std::path::Path;
use std::process::ExitCode;

fn main() -> ExitCode {
    let mut process_args = std::env::args();
    let invoked = process_args.next().unwrap_or_default();
    let args: Vec<String> = process_args.collect();
    if invoked_as_git(&invoked) {
        return git_wrapper(&args);
    }
    let Some(cmd) = args.first().map(String::as_str) else {
        usage();
        return ExitCode::FAILURE;
    };

    match cmd {
        "git" => git_wrapper(&args[1..]),
        "pre-push" => code(prepush::run(args.get(1).map_or("origin", String::as_str))),
        "commit-msg" => code(commit_msg(args.get(1).map_or("", String::as_str))),
        "pre-commit" => code(pre_commit()),
        "post-commit" => code(postcommit::run()),
        "renovate-gate" => code(renovate::run(args.get(1).map_or("origin", String::as_str))),
        "renovate" => code(renovate::dispatch(&args[1..])),
        "scan" => code(scan_paths(&args[1..])),
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

/// `~/.local/bin/git` delegates here, and on success this function never returns because it replaces the process with the real git.
fn git_wrapper(argv: &[String]) -> ExitCode {
    if let Some(denial) = gitargv::inspect_in(argv, &stack::Checkout) {
        let full: Vec<String> = std::iter::once("git".to_owned())
            .chain(argv.iter().cloned())
            .collect();
        let tooling = denial.stack && stack::leased(gitargv::global_options(argv));
        match gate_rules::admission(denial.stack, tooling, bypass::bypassed(denial.rule)) {
            Admission::Tooling => {
                bypass::record("STACK", denial.rule, &denial.reason, &full);
            }
            Admission::Waived => {
                bypass::record("BYPASS", denial.rule, &denial.reason, &full);
                eprintln!(
                    "::warning:: {}=1 — allowing {}",
                    denial.rule.env().unwrap_or_default(),
                    denial.reason
                );
            }
            Admission::Refused => {
                bypass::record("REJECT", denial.rule, &denial.reason, &full);
                gitargv::refusal(&denial, argv).emit();
                return ExitCode::FAILURE;
            }
        }
    }

    let Some(real) = real_git::find() else {
        eprintln!(
            "::error:: no git found at {}",
            real_git::updates()
                .iter()
                .map(|p| p.display().to_string())
                .collect::<Vec<_>>()
                .join(", ")
        );
        return ExitCode::from(COMMAND_NOT_FOUND);
    };
    let invoked: Vec<&str> = std::iter::once("git")
        .chain(argv.iter().map(String::as_str))
        .collect();
    let mut git = real_git::at(&real);
    git.args(argv).env(INVOKED, refusal::command(&invoked));
    let globals = gitargv::global_options(argv);
    let subcommand = argv
        .get(globals.len())
        .map(String::as_str)
        .unwrap_or_default();
    let push = subcommand == "push";
    let rewrite = subcommand == "revert"
        || subcommand == "commit" && argv.iter().any(|arg| arg == "--amend")
        || push
            && argv
                .iter()
                .any(|arg| arg.starts_with("--force") || arg == "-f");
    let published = if push {
        rewrite
    } else if rewrite {
        let mut query: Vec<&str> = globals.iter().map(String::as_str).collect();
        query.extend([
            "for-each-ref",
            "--contains=HEAD",
            "--format=%(refname)",
            "refs/remotes/",
        ]);
        real_git::capture(&query).is_some_and(|refs| !refs.trim().is_empty())
    } else {
        false
    };
    delegate(&real, git, push, rewrite, published)
}

/// Carries the command the wrapper ran to the hooks of that command, so a hook's bypass can repeat it.
const INVOKED: &str = "DOTGUARD_COMMAND";

/// The shell's statuses for a command it couldn't find and for one it found but couldn't run.
const COMMAND_NOT_FOUND: u8 = 127;
const COMMAND_NOT_EXECUTABLE: u8 = 126;

/// `ALLOW_FOREIGN=1` before the Git command whose hook refused, when that command passed through the wrapper.
fn foreign_waiver() -> Option<String> {
    std::env::var(INVOKED)
        .ok()
        .filter(|line| !line.trim().is_empty())
        .map(|line| format!("ALLOW_FOREIGN=1 {line}"))
}

#[cfg(unix)]
fn delegate(
    real: &Path,
    mut git: std::process::Command,
    push: bool,
    rewrite: bool,
    published: bool,
) -> ExitCode {
    use std::os::unix::process::CommandExt;
    if push || rewrite && published {
        return observed_delegate(real, git, push, rewrite, published);
    }
    // `exec` returns only on failure.
    let err = git.exec();
    eprintln!("::error:: could not exec {}: {err}", real.display());
    ExitCode::from(COMMAND_NOT_EXECUTABLE)
}

#[cfg(windows)]
fn delegate(
    real: &Path,
    git: std::process::Command,
    push: bool,
    rewrite: bool,
    published: bool,
) -> ExitCode {
    ignore_console_interrupts();
    observed_delegate(real, git, push, rewrite, published)
}

fn observed_delegate(
    real: &Path,
    mut git: std::process::Command,
    push: bool,
    rewrite: bool,
    published: bool,
) -> ExitCode {
    let invoked = git
        .get_envs()
        .find(|(name, _)| *name == INVOKED)
        .and_then(|(_, value)| value)
        .map_or_else(String::new, |value| value.to_string_lossy().into_owned());
    match git.status() {
        Ok(status) => {
            if let Some(source) = rules::incident_rules::signal(
                rules::incident_rules::Operation {
                    push,
                    rewrite,
                    published,
                },
                status.success(),
            ) {
                let rule = if source == rules::incident_rules::Source::FailedDelivery {
                    "push.failed"
                } else {
                    "git.published-rewrite"
                };
                let observation = refusal::Refusal::new(
                    rule,
                    "the Git operation produced a failure observation",
                    "skill-ops discharge --help",
                )
                .evidence(invoked)
                .evidence(status.to_string());
                platform::incidents::record(&observation, source);
            }
            std::process::exit(status.code().unwrap_or(1))
        }
        Err(err) => {
            eprintln!("::error:: could not run {}: {err}", real.display());
            ExitCode::from(COMMAND_NOT_EXECUTABLE)
        }
    }
}

/// The console sends Ctrl+C to every attached process, git handles it, and the wrapper waits so the shell returns only after git exits.
/// The wrapper registers its own handler, because git inherits the ignore flag that the null handler sets.
/// Exempt as semantic-boundary: the Windows console decides when the registered handler runs.
/// Evidence: `crates/dotguard/tests/git_wrapper.rs::a_copy_named_git_passes_arguments_streams_and_status_through`.
/// Reconsider when the standard library or a dependency offers a safe console control handler.
#[cfg(windows)]
#[expect(
    unsafe_code,
    reason = "SetConsoleCtrlHandler is the only interface to the console control handler"
)]
fn ignore_console_interrupts() {
    unsafe extern "system" fn swallow(_event: u32) -> i32 {
        1
    }
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn SetConsoleCtrlHandler(
            handler: Option<unsafe extern "system" fn(u32) -> i32>,
            add: i32,
        ) -> i32;
    }
    // SAFETY: `swallow` stays valid for the life of the process, touches no state, and only reports each control event as handled.
    unsafe {
        SetConsoleCtrlHandler(Some(swallow), 1);
    }
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
