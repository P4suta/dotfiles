//! Git policy and native process delegation shared by the source-matched executables.

use crate::{bypass, gate_rules, gitargv, real_git, refusal, stack};
use gate_rules::Admission;
use std::path::Path;
use std::process::ExitCode;

/// Carries the command the wrapper ran to the hooks of that command, so a hook's bypass can repeat it.
pub const INVOKED: &str = "DOTGUARD_COMMAND";

/// The shell's statuses for a command it couldn't find and for one it found but couldn't run.
const COMMAND_NOT_FOUND: u8 = 127;
const COMMAND_NOT_EXECUTABLE: u8 = 126;

/// `~/.local/bin/git` delegates here, and on success this function never returns because it replaces the process with the real git.
pub fn run<const SPEECH: bool>(argv: &[String]) -> ExitCode {
    let globals = gitargv::global_options(argv);
    let subcommand = argv
        .get(globals.len())
        .map(String::as_str)
        .unwrap_or_default();
    let spoken = SpokenGit::<SPEECH>::new(subcommand);
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
                spoken.finish(false);
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
        spoken.finish(false);
        return ExitCode::from(COMMAND_NOT_FOUND);
    };
    let invoked: Vec<&str> = std::iter::once("git")
        .chain(argv.iter().map(String::as_str))
        .collect();
    let mut git = real_git::at(&real);
    git.args(argv).env(INVOKED, refusal::command(&invoked));
    if spoken.operation.is_some() {
        git.env(platform::speech::INTERNAL, "1");
    }
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
    delegate(&real, git, push, rewrite, published, &spoken)
}

#[derive(Debug)]
struct SpokenGit<const SPEECH: bool> {
    operation: Option<(bool, String)>,
}

impl<const SPEECH: bool> SpokenGit<SPEECH> {
    fn new(subcommand: &str) -> Self {
        use rules::notification_rules::Event;
        let operation = if SPEECH {
            match subcommand {
                "commit" | "push" if std::env::var_os(platform::speech::INTERNAL).is_none() => {
                    let push = subcommand == "push";
                    let id = platform::speech::operation_id();
                    platform::speech::announce(
                        if push {
                            Event::PushStarted
                        } else {
                            Event::CommitStarted
                        },
                        &id,
                    );
                    Some((push, id))
                }
                _ => None,
            }
        } else {
            None
        };
        Self { operation }
    }
    fn finish(&self, success: bool) {
        use rules::notification_rules::Event;
        if SPEECH && let Some((push, id)) = &self.operation {
            let event = match (*push, success) {
                (false, false) => Event::CommitFailed,
                (false, true) => Event::CommitCompleted,
                (true, false) => Event::PushFailed,
                (true, true) => Event::PushCompleted,
            };
            platform::speech::announce(event, id);
        }
    }
}

#[cfg(unix)]
fn delegate<const SPEECH: bool>(
    real: &Path,
    mut git: std::process::Command,
    push: bool,
    rewrite: bool,
    published: bool,
    spoken: &SpokenGit<SPEECH>,
) -> ExitCode {
    use std::os::unix::process::CommandExt;
    if push || rewrite && published || spoken.operation.is_some() {
        return observed_delegate(real, git, push, rewrite, published, spoken);
    }
    // `exec` returns only on failure.
    let err = git.exec();
    eprintln!("::error:: could not exec {}: {err}", real.display());
    ExitCode::from(COMMAND_NOT_EXECUTABLE)
}

#[cfg(windows)]
fn delegate<const SPEECH: bool>(
    real: &Path,
    git: std::process::Command,
    push: bool,
    rewrite: bool,
    published: bool,
    spoken: &SpokenGit<SPEECH>,
) -> ExitCode {
    ignore_console_interrupts();
    observed_delegate(real, git, push, rewrite, published, spoken)
}

fn observed_delegate<const SPEECH: bool>(
    real: &Path,
    mut git: std::process::Command,
    push: bool,
    rewrite: bool,
    published: bool,
    spoken: &SpokenGit<SPEECH>,
) -> ExitCode {
    let invoked = git
        .get_envs()
        .find(|(name, _)| *name == INVOKED)
        .and_then(|(_, value)| value)
        .map_or_else(String::new, |value| value.to_string_lossy().into_owned());
    match git.status() {
        Ok(status) => {
            spoken.finish(status.success());
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
            std::process::exit(shell_status(status))
        }
        Err(err) => {
            spoken.finish(false);
            eprintln!("::error:: could not run {}: {err}", real.display());
            ExitCode::from(COMMAND_NOT_EXECUTABLE)
        }
    }
}

fn shell_status(status: std::process::ExitStatus) -> i32 {
    #[cfg(unix)]
    {
        use std::os::unix::process::ExitStatusExt;
        const SIGNAL_STATUS_OFFSET: i32 = 128;
        status.code().unwrap_or_else(|| {
            status
                .signal()
                .map_or(1, |signal| SIGNAL_STATUS_OFFSET + signal)
        })
    }
    #[cfg(not(unix))]
    status.code().unwrap_or(1)
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
pub fn ignore_console_interrupts() {
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
