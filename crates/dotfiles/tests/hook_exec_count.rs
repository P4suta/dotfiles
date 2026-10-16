//! The processes each hook spawns, counted through gate stand-ins and the actual signing worker's self-reentry.
//! Each hook spawns exactly the invocations below in this order, and `policy/baselines.toml` records how many as its `exec-count`.

use anyhow::{Context, Result, ensure};
use clap::ValueEnum;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Output;

#[path = "../../dotguard/tests/support/fixture_git.rs"]
mod fixture_git;

#[path = "support/catalog.rs"]
mod catalog;

/// An isolated home whose dotguard, lefthook, Git, and storage-scout log each start, and a repository with one staged file.
struct Host {
    scope: tempfile::TempDir,
    git: PathBuf,
}

/// The real Git on the test's `PATH`, which the stand-in forwards to.
fn real_git() -> Result<PathBuf> {
    let name = format!("git{}", std::env::consts::EXE_SUFFIX);
    std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default())
        .map(|directory| directory.join(&name))
        .find(|path| path.is_file())
        .context("Git is on PATH")
}

impl Host {
    fn new() -> Result<Self> {
        let scope = tempfile::tempdir()?;
        let home = scope.path().join("home");
        let bin = home.join(".local/bin");
        let cargo = home.join(".cargo/bin");
        fs::create_dir_all(&bin)?;
        fs::create_dir_all(&cargo)?;
        fs::create_dir_all(scope.path().join("repository"))?;
        fs::write(
            scope.path().join("gitconfig"),
            "[user]\n\tname = Fixture\n\temail = fixture@example.invalid\n",
        )?;
        let suffix = std::env::consts::EXE_SUFFIX;
        let counter = scope.path().join(format!("counter{suffix}"));
        let status = platform::process::command("rustc")
            .args(["--edition", "2024"])
            .arg(Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/counter.rs"))
            .arg("-o")
            .arg(&counter)
            .status()?;
        ensure!(status.success(), "fixture counter compilation failed");
        for name in ["dotguard", "lefthook", "git"] {
            fs::copy(&counter, bin.join(format!("{name}{suffix}")))?;
        }
        // The hook looks for storage-scout without an extension, and Windows starts the `.exe` beside it.
        fs::copy(&counter, cargo.join("storage-scout"))?;
        fs::copy(&counter, cargo.join(format!("storage-scout{suffix}")))?;
        catalog::install(&home)?;
        let host = Self {
            scope,
            git: real_git()?,
        };
        host.run("git", &["init", "-q"])?;
        fs::write(host.repository().join("a.txt"), "a\n")?;
        host.run("git", &["add", "a.txt"])?;
        Ok(host)
    }

    fn home(&self) -> PathBuf {
        self.scope.path().join("home")
    }

    fn repository(&self) -> PathBuf {
        self.scope.path().join("repository")
    }

    fn log(&self) -> PathBuf {
        self.scope.path().join("counter.log")
    }

    /// Configures the global and repository Lefthook files and the storage policy.
    fn configure(&self) -> Result<()> {
        let home = self.home();
        fs::create_dir_all(home.join(".config/lefthook"))?;
        fs::write(home.join(".config/lefthook/global.yml"), "pre-commit: {}\n")?;
        fs::create_dir_all(home.join(".config/storage-scout"))?;
        fs::write(home.join(".config/storage-scout/auto.toml"), "")?;
        fs::write(self.repository().join("lefthook.yml"), "pre-commit: {}\n")?;
        Ok(())
    }

    fn command(&self, program: &str) -> std::process::Command {
        let mut command = fixture_git::command(program, &self.scope.path().join("gitconfig"));
        command
            .current_dir(self.repository())
            .env("HOME", self.home())
            .env("USERPROFILE", self.home())
            .env("COUNTER_LOG", self.log())
            .env("COUNTER_GIT", &self.git);
        command
    }

    fn run(&self, program: &str, arguments: &[&str]) -> Result<Output> {
        let output = self.command(program).args(arguments).output()?;
        ensure!(
            output.status.success(),
            "{program} {arguments:?}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        Ok(output)
    }

    /// Runs one hook and returns each spawned gate's program and first argument.
    /// Git counts only for `write-tree`, since the other Git reads only discover facts.
    fn spawned(&self, arguments: &[&str]) -> Result<Vec<String>> {
        let _ = fs::remove_file(self.log());
        let output = self
            .command(
                std::env::current_exe()?
                    .to_str()
                    .context("a UTF-8 test executable")?,
            )
            .arg("fixture-hook")
            .args(arguments)
            .output()?;
        ensure!(
            output.status.success(),
            "hook {arguments:?}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let log = fs::read_to_string(self.log()).unwrap_or_default();
        Ok(log
            .lines()
            .map(|line| line.split(' ').take(2).collect::<Vec<_>>().join(" "))
            .filter(|line| !line.starts_with("git ") || line == "git write-tree")
            .collect())
    }
}

/// The recorded `exec-count` of `hook` in `policy/baselines.toml`.
fn baseline(hook: &str) -> Result<usize> {
    let text = fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../policy/baselines.toml"),
    )?;
    let document: toml::Table = toml::from_str(&text)?;
    let metrics = document["metric"].as_array().context("a metric list")?;
    let exact = metrics
        .iter()
        .find(|metric| {
            metric["metric"].as_str() == Some("exec-count") && metric["hook"].as_str() == Some(hook)
        })
        .and_then(|metric| metric["exact"].as_integer())
        .with_context(|| format!("policy/baselines.toml records no exec-count for {hook}"))?;
    Ok(usize::try_from(exact)?)
}

fn each_hook_spawns_exactly_its_gates_in_order() -> Result<()> {
    let host = Host::new()?;
    host.configure()?;
    let message = host.scope.path().join("message");
    fs::write(&message, "fixture\n")?;
    let message = message.to_str().context("a UTF-8 path")?;
    let url = "https://example.invalid/r";
    let expected: [(&str, Vec<&str>, Vec<&str>); 7] = [
        (
            "pre-commit",
            vec!["pre-commit"],
            vec![
                "git write-tree",
                "dotguard pre-commit",
                "lefthook dump",
                "lefthook run",
                "lefthook dump",
                "lefthook run",
                "git write-tree",
            ],
        ),
        (
            "commit-msg",
            vec!["commit-msg", "--", message],
            vec!["dotguard commit-msg", "lefthook dump", "lefthook run"],
        ),
        (
            "pre-push",
            vec!["pre-push", "--", "origin", url],
            vec![
                "dotguard pre-push",
                "lefthook dump",
                "lefthook run",
                "dotguard renovate-gate",
            ],
        ),
        (
            "post-commit",
            vec!["post-commit"],
            vec!["dotfiles post-commit-worker"],
        ),
        (
            "post-checkout",
            vec!["post-checkout", "--", "0", "0", "1"],
            vec!["lefthook dump", "lefthook run", "storage-scout auto"],
        ),
        (
            "post-merge",
            vec!["post-merge", "--", "0"],
            vec!["lefthook dump", "lefthook run", "storage-scout auto"],
        ),
        (
            "reference-transaction",
            vec!["reference-transaction", "--", "committed"],
            vec!["storage-scout auto"],
        ),
    ];
    for (hook, arguments, gates) in expected {
        assert_eq!(host.spawned(&arguments)?, gates, "{hook}");
        assert_eq!(baseline(hook)?, gates.len(), "{hook}");
    }
    Ok(())
}

fn an_unconfigured_hook_spawns_no_gate() -> Result<()> {
    let host = Host::new()?;
    for arguments in [
        vec!["post-checkout", "--", "0", "0", "1"],
        vec!["post-merge", "--", "0"],
        vec!["reference-transaction", "--", "prepared"],
    ] {
        assert_eq!(
            host.spawned(&arguments)?,
            Vec::<String>::new(),
            "{arguments:?}"
        );
    }
    Ok(())
}

fn main() -> Result<()> {
    let mut arguments = std::env::args().skip(1);
    match arguments.next().as_deref() {
        Some("fixture-hook") => {
            let hook = dotfiles::hooks::Hook::from_str(
                &arguments.next().context("a fixture hook name")?,
                false,
            )
            .map_err(anyhow::Error::msg)?;
            let mut arguments = arguments.map(std::ffi::OsString::from).collect::<Vec<_>>();
            if arguments.first().is_some_and(|argument| argument == "--") {
                arguments.remove(0);
            }
            dotfiles::hooks::run(hook, &arguments)
        }
        Some("post-commit-worker") => {
            let producer = arguments.next().context("the worker's producer")?;
            ensure!(arguments.next().is_none(), "unexpected worker argument");
            let mut log = fs::OpenOptions::new()
                .append(true)
                .create(true)
                .open(std::env::var_os("COUNTER_LOG").context("the worker's invocation log")?)?;
            writeln!(log, "dotfiles post-commit-worker")?;
            std::process::exit(dotfiles::hooks::post_commit_worker(&producer));
        }
        None | Some("--nocapture") => {
            ensure!(
                arguments.next().is_none(),
                "unsupported count test argument"
            );
            each_hook_spawns_exactly_its_gates_in_order()?;
            an_unconfigured_hook_spawns_no_gate()?;
            println!("test result: ok. 2 passed; 0 failed");
            Ok(())
        }
        Some(argument) => anyhow::bail!("unsupported count test argument: {argument}"),
    }
}
