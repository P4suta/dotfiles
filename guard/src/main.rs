//! dotguard — the local policy gate.
//!
//! One binary behind three entry points, all of them installed globally rather than per-repository:
//!
//! ~/.local/bin/git            -> `dotguard git …`         (argv policy, then exec; on Windows a copy named git.exe is the wrapper itself) ~/.config/git/hooks/pre-push   -> `dotguard pre-push`   (force-push gate) ~/.config/git/hooks/commit-msg -> `dotguard commit-msg` (attribution, language) ~/.config/git/hooks/pre-commit -> `dotguard pre-commit` (language, staged diff)
//!
//! `core.hooksPath` in ~/.gitconfig points every repository on the machine at that hooks directory, so none of this is something a repository has to opt into — or can forget to.
//! lefthook still runs per-repo gates underneath; these are the rules that hold everywhere, including in a repository cloned five minutes ago.

use dotguard::{
    attribution, bypass, doctor, gitargv, lang, lint, postcommit, prepush, realgit, refusal,
    renovate, staged,
};

use bypass::Category;
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
        "lint" => code(lint::dispatch(&args[1..])),
        "doctor" => code(doctor::run(&args[1..])),
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

/// Windows cannot `exec` from a shell launcher, so a copy of this binary named `git.exe` stands in front of Git for Windows and applies the policy itself.
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

/// `~/.local/bin/git` delegates its whole job here and this never returns on the happy path: it `exec`s the real git in place, so there is no extra process sitting in the tree holding a pipe open.
fn git_wrapper(argv: &[String]) -> ExitCode {
    if let Some(denial) = gitargv::inspect(argv) {
        let full: Vec<String> = std::iter::once("git".to_owned())
            .chain(argv.iter().cloned())
            .collect();

        let waiver = denial.category.env();
        if let Some(env) = waiver.filter(|_| bypass::waived(denial.category)) {
            bypass::record("BYPASS", denial.category, &denial.reason, &full);
            eprintln!("::warning:: {env}=1 — allowing {}", denial.reason);
        } else {
            bypass::record("REJECT", denial.category, &denial.reason, &full);
            gitargv::refusal(&denial, argv).emit();
            return ExitCode::FAILURE;
        }
    }

    let Some(real) = realgit::find() else {
        eprintln!(
            "::error:: no git found at {}",
            realgit::candidates()
                .iter()
                .map(|p| p.display().to_string())
                .collect::<Vec<_>>()
                .join(", ")
        );
        return ExitCode::from(127);
    };
    let invoked: Vec<&str> = std::iter::once("git")
        .chain(argv.iter().map(String::as_str))
        .collect();
    let mut git = std::process::Command::new(&real);
    git.args(argv).env(INVOKED, refusal::command(&invoked));
    delegate(&real, git)
}

/// Carries the command the wrapper ran to the hooks of that command, so a hook's waiver can repeat it.
const INVOKED: &str = "DOTGUARD_COMMAND";

/// `ALLOW_FOREIGN=1` before the Git command whose hook refused, when that command passed through the wrapper.
fn foreign_waiver() -> Option<String> {
    std::env::var(INVOKED)
        .ok()
        .filter(|line| !line.trim().is_empty())
        .map(|line| format!("ALLOW_FOREIGN=1 {line}"))
}

/// Replaces this process with the real git, so no extra process holds a pipe open.
#[cfg(unix)]
fn delegate(real: &Path, mut git: std::process::Command) -> ExitCode {
    use std::os::unix::process::CommandExt;
    // exec() only returns on failure.
    let err = git.exec();
    eprintln!("::error:: could not exec {}: {err}", real.display());
    ExitCode::from(126)
}

/// Runs the real git and exits with its status, because Windows has no `exec`.
#[cfg(windows)]
fn delegate(real: &Path, mut git: std::process::Command) -> ExitCode {
    ignore_console_interrupts();
    match git.status() {
        Ok(status) => std::process::exit(status.code().unwrap_or(1)),
        Err(err) => {
            eprintln!("::error:: could not run {}: {err}", real.display());
            ExitCode::from(126)
        }
    }
}

/// The console sends Ctrl+C to every attached process; git handles it itself, and the wrapper waits so the shell returns only after git has exited.
/// A handler is registered instead of the null handler, because the ignore flag the null handler sets is inherited by git.
#[cfg(windows)]
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
    // SAFETY: `swallow` is a valid handler for the life of the process, touches no state, and only reports each control event as handled.
    unsafe {
        SetConsoleCtrlHandler(Some(swallow), 1);
    }
}

/// The language policy for the repository we are standing in.
///
/// `guard.lang` is per repository, and both the value and the off-switch are spelled out in every refusal.
/// English is the default because that is what this machine's history actually is; `japanese` exists because a repository whose *subject* is Japanese typesetting writes Japanese commit messages and should not have to argue about it; `off` exists because a repository full of i18n fixtures is a legitimate thing to have, and a gate that cannot be removed there is a gate that gets removed everywhere.
///
///     git config guard.lang japanese
///     git config guard.lang off
fn lang_mode() -> Option<lang::Mode> {
    lang::Mode::parse(realgit::capture(&["config", "--get", "guard.lang"]).as_deref())
}

fn comment_char() -> char {
    realgit::capture(&["config", "--get", "core.commentChar"])
        .and_then(|v| v.trim().chars().next())
        .filter(|c| c.is_ascii() && *c != 'a') // "auto" resolves per message; '#' is what it starts from
        .unwrap_or('#')
}

fn commit_msg(path: &str) -> i32 {
    if path.is_empty() {
        return 0;
    }
    let Ok(raw) = std::fs::read_to_string(path) else {
        return 0;
    };

    // Attribution first, so a Conventional Commits or line-length gate downstream sees the message that will actually be committed.
    let (cleaned, removed) = attribution::strip(&raw);
    if removed > 0 {
        if let Ok(mut f) = std::fs::File::create(path) {
            let _ = f.write_all(cleaned.as_bytes());
        }
        eprintln!(
            "::notice:: removed {removed} agent attribution line(s) from the commit message."
        );
    }

    let Some(mode) = lang_mode() else { return 0 };
    let body = lang::commit_message_body(&cleaned, comment_char());
    let hits = lang::scan(&body, mode);
    if hits.is_empty() {
        return 0;
    }

    let expected = match mode {
        lang::Mode::Japanese => "English or Japanese",
        _ => "English",
    };
    lang::report("commit message", &body, &hits);
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
        hits.iter()
            .map(|hit| lang::evidence("commit message", hit))
            .collect(),
        retry.ok().map(|words| refusal::command(&words)),
        foreign_waiver(),
    ))
}

/// The author and date of `revision`, as the refused commit took them from it.
fn authorship(revision: &str) -> Option<gitargv::Authorship> {
    let peeled = format!("{revision}^{{commit}}");
    let read = realgit::capture(&[
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

/// Whether a cherry-pick or rebase pick is in progress, as `git commit` decides it, or `None` when Git could not say.
fn picking() -> Option<bool> {
    // A pending merge makes the commit a merge even when a pick was also left behind.
    Some(!exists("MERGE_HEAD")? && exists("CHERRY_PICK_HEAD")?)
}

/// Whether the special reference `name` is present.
/// Git keeps `MERGE_HEAD` and `CHERRY_PICK_HEAD` as files in the worktree's Git directory under every reference backend, and `git commit` tests those files.
/// `git show-ref --exists` does not decide this on every supported Git: Git 2.43 reports a missing `MERGE_HEAD` as a lookup error.
fn exists(name: &str) -> Option<bool> {
    let path = realgit::capture(&["rev-parse", "--git-path", name])?;
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
        .filter(|f| !staged::is_exempt(&f.path))
        .collect();
    if files.is_empty() {
        return 0;
    }

    let hits: Vec<String> = files
        .iter()
        .flat_map(|f| {
            f.hits.iter().map(|(line, hit)| {
                lang::evidence(
                    &f.path,
                    &lang::Hit {
                        line: *line,
                        ..*hit
                    },
                )
            })
        })
        .collect();
    for hit in &hits {
        eprintln!("  {hit}");
    }
    let mut inspect = vec!["git", "diff", "--cached", "--"];
    inspect.extend(files.iter().map(|f| f.path.as_str()));
    refuse_foreign(&lang::refusal(
        "commit.language",
        "staged changes add text in a script this machine does not write; remove it, stage the result, and commit again",
        hits,
        Some(refusal::command(&inspect)),
        foreign_waiver(),
    ))
}

fn scan_paths(paths: &[String]) -> i32 {
    let mut hits = Vec::new();
    for p in paths {
        let Ok(text) = std::fs::read_to_string(p) else {
            continue;
        };
        if staged::exempts_itself(&text) {
            continue;
        }
        let found = lang::scan(&text, lang::Mode::Content);
        lang::report(p, &text, &found);
        hits.extend(found.iter().map(|hit| lang::evidence(p, hit)));
    }
    if hits.is_empty() {
        return 0;
    }
    let mut scan = vec!["dotguard", "scan"];
    scan.extend(paths.iter().map(String::as_str));
    refuse_foreign(&lang::refusal(
        "scan.language",
        &format!(
            "{} contaminating character(s); remove them and scan again",
            hits.len()
        ),
        hits,
        Some(refusal::command(&scan)),
        Some(format!("ALLOW_FOREIGN=1 {}", refusal::command(&scan))),
    ))
}

/// Shared tail for every language refusal: the recorded override, or the structured refusal.
fn refuse_foreign(refused: &refusal::Refusal) -> i32 {
    let argv: Vec<String> = std::env::args().collect();
    if bypass::waived(Category::Foreign) {
        bypass::record("BYPASS", Category::Foreign, "foreign script", &argv);
        eprintln!("\n::warning:: ALLOW_FOREIGN=1 — allowing it.");
        return 0;
    }
    bypass::record("REJECT", Category::Foreign, "foreign script", &argv);
    refused.emit();
    1
}
