//! The local policy gate.
//!
//! One binary serves four global entry points:
//!
//! - `~/.local/bin/git` runs `dotguard git …`, which applies the argument policy and then executes git, and on Windows a copy named `git.exe` acts as the wrapper.
//! - `~/.config/git/hooks/pre-push` runs `dotguard pre-push`, the force-push gate.
//! - `~/.config/git/hooks/commit-msg` runs `dotguard commit-msg`, which strips attribution and checks the language.
//! - `~/.config/git/hooks/pre-commit` runs `dotguard pre-commit`, the language and staged-diff gates.
//!
//! `core.hooksPath` in `~/.gitconfig` points every repository at that hooks directory, so no repository needs to opt in.

use dotguard::{
    attribution, bypass, doctor, gitargv, lang, lint, postcommit, prepush, realgit, renovate,
    staged,
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
            let hint = waiver.map_or_else(
                || "Signing and hook checks cannot be bypassed.".to_owned(),
                |env| format!(
                    "If this is deliberate, waive it for this one command:\n\n  {env}=1 git {}\n\nThe bypass is recorded in ~/.local/state/git-bypass.log.",
                    argv.join(" ")
                ),
            );
            eprintln!(
                "::error:: refusing `{}`.\n\n{}\n\n{hint}",
                denial.reason, denial.hint
            );
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
    delegate(&real, argv)
}

#[cfg(unix)]
fn delegate(real: &Path, argv: &[String]) -> ExitCode {
    use std::os::unix::process::CommandExt;
    let err = std::process::Command::new(real).args(argv).exec();
    eprintln!("::error:: could not exec {}: {err}", real.display());
    ExitCode::from(126)
}

#[cfg(windows)]
fn delegate(real: &Path, argv: &[String]) -> ExitCode {
    ignore_console_interrupts();
    match std::process::Command::new(real).args(argv).status() {
        Ok(status) => std::process::exit(status.code().unwrap_or(1)),
        Err(err) => {
            eprintln!("::error:: could not run {}: {err}", real.display());
            ExitCode::from(126)
        }
    }
}

/// The console sends Ctrl+C to every attached process, git handles it, and the wrapper waits so the shell returns only after git exits.
/// The wrapper registers its own handler, because git inherits the ignore flag that the null handler sets.
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
    lang::Mode::parse(realgit::capture(&["config", "--get", "guard.lang"]).as_deref())
}

fn comment_char() -> char {
    realgit::capture(&["config", "--get", "core.commentChar"])
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
    eprintln!("::error:: the commit message is not written in {expected}:\n");
    lang::report("commit message", &body, &hits);
    refuse_foreign("git commit")
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

    eprintln!("::error:: staged changes add text in a script this machine does not write:\n");
    for f in &files {
        for (lineno, hit) in &f.hits {
            eprintln!(
                "  {}:{}:{}  {:?}  U+{:04X}  [{}]",
                f.path, lineno, hit.col, hit.ch, hit.ch as u32, hit.kind
            );
        }
    }
    refuse_foreign("git commit")
}

fn scan_paths(paths: &[String]) -> i32 {
    let mut bad = 0;
    for p in paths {
        let Ok(text) = std::fs::read_to_string(p) else {
            continue;
        };
        if staged::exempts_itself(&text) {
            continue;
        }
        let hits = lang::scan(&text, lang::Mode::Content);
        if !hits.is_empty() {
            lang::report(p, &text, &hits);
            bad += hits.len();
        }
    }
    if bad == 0 {
        return 0;
    }
    eprintln!("\n::error:: {bad} contaminating character(s).");
    refuse_foreign("this check")
}

/// The shared tail of every language refusal: the bypass, the per-repository switch, and the reason a single stray character matters.
fn refuse_foreign(what: &str) -> i32 {
    let argv: Vec<String> = std::env::args().collect();
    if bypass::waived(Category::Foreign) {
        bypass::record("BYPASS", Category::Foreign, "foreign script", &argv);
        eprintln!("\n::warning:: ALLOW_FOREIGN=1 — allowing it.");
        return 0;
    }
    bypass::record("REJECT", Category::Foreign, "foreign script", &argv);
    eprintln!(
        "\n\
         Cyrillic '\u{0441}' and Latin 'c' are indistinguishable on screen, so this is not only\n\
         language rule: it is the same check that catches a homoglyph in an identifier and a\n\
         bidi override in a comment.\n\n\
         If the text is intentional:\n\n  \
         ALLOW_FOREIGN=1 {what}              once\n  \
         git config guard.lang japanese      this repository writes Japanese\n  \
         git config guard.lang off           this repository carries multilingual fixtures\n"
    );
    1
}
