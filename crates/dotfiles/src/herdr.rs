//! Session recovery observes processes and agents before applying the pure decisions.

use crate::herdr_rules::{self as rules, Agent, Instances, Launch};
use anyhow::{Context, Result, ensure};
use std::io::{IsTerminal, Write};
use std::os::unix::fs::FileTypeExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

const PS: &str = "/bin/ps";
const KILL: &str = "/bin/kill";
const HEALTH_LIMIT: Duration = Duration::from_secs(2);
const INIT_PID: u32 = 1;
const START_FIELDS: usize = 5;
const AUTH_SOCKET: &str = "SSH_AUTH_SOCK";
const SSH_COMMAND: &str = "GIT_SSH_COMMAND";
const IDENTITY_AGENT: &str = "IdentityAgent=";
const LAUNCH_LOCK: &str = ".local/state/herdr/launch.lock";
const SSH_CONFIG: &str = ".config/dotfiles/ssh-agent.conf";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Role {
    Server,
    Client,
    Codex,
    Domyjob,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Process {
    pub pid: u32,
    pub parent: u32,
    pub started: String,
    pub command: String,
    pub role: Role,
}

trait Observations {
    fn agent(&mut self) -> Agent;
    fn processes(&mut self) -> Result<Vec<Process>>;
    fn responding(&mut self) -> Result<bool>;
    fn dead_socket(&mut self, daemon: &Process) -> Result<bool>;
}

#[derive(Debug)]
struct Inspection {
    agent: Agent,
    launch: Launch,
    processes: Vec<Process>,
    stale_daemons: Vec<Process>,
}

fn inspect(observations: &mut impl Observations) -> Result<Inspection> {
    let agent = observations.agent();
    let processes = observations.processes()?;
    let servers = processes
        .iter()
        .filter(|process| process.role == Role::Server)
        .count();
    let abandoned = processes
        .iter()
        .filter(|process| process.role == Role::Client && process.parent == INIT_PID)
        .count();
    let launch = rules::launch(Instances {
        servers: u16::try_from(servers).unwrap_or(u16::MAX),
        abandoned: u16::try_from(abandoned).unwrap_or(u16::MAX),
        responding: servers > 0 && observations.responding()?,
    });
    let mut stale_daemons = Vec::new();
    for process in &processes {
        if matches!(process.role, Role::Codex | Role::Domyjob)
            && observations.dead_socket(process)?
        {
            stale_daemons.push(process.clone());
        }
    }
    Ok(Inspection {
        agent,
        launch,
        processes,
        stale_daemons,
    })
}

struct Native<'a> {
    home: &'a Path,
    deadline: Instant,
}

impl<'a> Native<'a> {
    fn new(home: &'a Path) -> Self {
        Self {
            home,
            deadline: Instant::now() + HEALTH_LIMIT,
        }
    }

    fn read(&self, command: Command) -> Result<String> {
        let remaining = self.deadline.saturating_duration_since(Instant::now());
        ensure!(!remaining.is_zero(), "Herdr health observation timed out");
        let (status, bytes) = dotguard::herdr::capture(command, remaining)?;
        ensure!(status.success(), "Herdr diagnostic command failed");
        Ok(String::from_utf8(bytes)?)
    }
}

fn field(text: &str) -> Option<(&str, &str)> {
    let text = text.trim_start();
    let split = text.find(char::is_whitespace)?;
    Some(text.split_at(split))
}

fn process(line: &str) -> Option<Process> {
    let (pid, rest) = field(line)?;
    let (parent, mut rest) = field(rest)?;
    let mut started = Vec::new();
    for _ in 0..START_FIELDS {
        let (word, remaining) = field(rest)?;
        started.push(word);
        rest = remaining;
    }
    let command = rest.trim();
    let mut words = command.split_whitespace();
    let executable = Path::new(words.next()?).file_name()?.to_str()?;
    let arguments: Vec<_> = words.collect();
    let role = match (executable, arguments.first().copied()) {
        ("herdr", Some("server")) => Role::Server,
        ("herdr", None | Some("attach")) => Role::Client,
        ("herdr", Some(argument)) if argument.starts_with('-') => Role::Client,
        ("codex", Some("app-server")) => Role::Codex,
        ("domyjob", Some("chat")) if arguments.contains(&"serve") => Role::Domyjob,
        _ => return None,
    };
    Some(Process {
        pid: pid.parse().ok()?,
        parent: parent.parse().ok()?,
        started: started.join(" "),
        command: command.into(),
        role,
    })
}

fn environment<'a>(text: &'a str, name: &str) -> Option<&'a str> {
    let marker = format!(" {name}=");
    let value = text
        .strip_prefix(&format!("{name}="))
        .or_else(|| text.split_once(&marker).map(|(_, value)| value))?;
    let end = value
        .char_indices()
        .filter(|(_, c)| *c == ' ')
        .find_map(|(index, _)| {
            let rest = value.get(index + 1..)?;
            let (candidate, _) = rest.split_once('=')?;
            (!candidate.is_empty()
                && candidate
                    .bytes()
                    .all(|byte| byte.is_ascii_uppercase() || byte.is_ascii_digit() || byte == b'_'))
            .then_some(index)
        })
        .unwrap_or(value.len());
    value.get(..end).map(str::trim)
}

fn socket_alive(home: &Path, value: &str, remaining: Duration) -> bool {
    let value = value.trim_matches(['\'', '"']);
    let socket = value
        .strip_prefix("~/")
        .map_or_else(|| PathBuf::from(value), |path| home.join(path));
    socket.is_absolute()
        && std::fs::metadata(&socket).is_ok_and(|metadata| metadata.file_type().is_socket())
        && dotguard::herdr::socket_responding(&socket, remaining)
}

impl Observations for Native<'_> {
    fn agent(&mut self) -> Agent {
        dotguard::herdr::agent(self.home)
    }

    fn processes(&mut self) -> Result<Vec<Process>> {
        let mut id = crate::tool::Tool::Id.command();
        id.arg("-u");
        let uid = self.read(id)?;
        let mut ps = platform::process::command(PS);
        ps.args(["-ww", "-U", uid.trim(), "-o", "pid=,ppid=,lstart=,command="])
            .env("LC_ALL", "C");
        Ok(self.read(ps)?.lines().filter_map(process).collect())
    }

    fn responding(&mut self) -> Result<bool> {
        let mut command = dotguard::locate::child("herdr");
        command.args(["status", "server"]);
        Ok(self
            .read(command)
            .is_ok_and(|text| text.lines().any(|line| line.trim() == "status: running")))
    }

    fn dead_socket(&mut self, daemon: &Process) -> Result<bool> {
        let mut ps = platform::process::command(PS);
        ps.args(["eww", "-p", &daemon.pid.to_string(), "-o", "command="]);
        let text = self.read(ps)?;
        let agent = environment(&text, AUTH_SOCKET).filter(|value| !value.is_empty());
        let pinned = environment(&text, SSH_COMMAND)
            .and_then(|command| command.split_once(IDENTITY_AGENT))
            .map(|(_, path)| path.trim());
        Ok(agent.into_iter().chain(pinned).any(|socket| {
            socket != "none"
                && !socket_alive(
                    self.home,
                    socket,
                    self.deadline.saturating_duration_since(Instant::now()),
                )
        }))
    }
}

impl Inspection {
    fn messages(&self) -> Vec<String> {
        let findings = rules::health(rules::Health {
            agent: self.agent,
            instances: self.launch,
            dead_daemon_socket: !self.stale_daemons.is_empty(),
        });
        let mut messages = Vec::new();
        if let Some(agent) = findings.agent {
            messages.push(format!("Herdr agent: {agent:?}; SSH falls back to 1Password; run `herdr-agent ensure` to reload keys"));
        }
        if let Some(instance) = findings.instances {
            messages.push(format!("Herdr processes: {instance:?}; inspect `dotfiles herdr health` and use `dotfiles herdr cleanup --pid PID`"));
            for process in self
                .processes
                .iter()
                .filter(|process| matches!(process.role, Role::Client | Role::Server))
            {
                messages.push(format!(
                    "Herdr PID {} (parent {}, started {}): {}",
                    process.pid, process.parent, process.started, process.command
                ));
            }
        }
        if findings.dead_daemon_socket {
            for daemon in &self.stale_daemons {
                messages.push(format!("{:?} daemon PID {} has a dead SSH agent in its environment; adopt the managed SSH configuration and reopen the daemon when its work is saved", daemon.role, daemon.pid));
            }
        }
        messages
    }
}

/// Report session health without changing any process or service.
///
/// # Errors
/// Returns errors from bounded observations.
pub fn health(home: &Path) -> Result<bool> {
    let inspection = inspect(&mut Native::new(home))?;
    let messages = inspection.messages();
    for message in &messages {
        println!("{message}");
    }
    if messages.is_empty() {
        println!("Herdr agent, sessions, and daemon environments are healthy");
    }
    Ok(messages.is_empty())
}

/// Add advisory findings to the existing session start hook.
pub fn session_start(home: &Path) {
    if !home.join(SSH_CONFIG).is_file() {
        return;
    }
    if let Err(error) = health(home) {
        println!("Herdr health unavailable: {error:#}; run `dotfiles herdr health` when ready");
    }
}

/// Check the server through a bounded diagnostic child.
///
/// # Errors
/// Returns an observation error.
pub fn server_running(home: &Path, limit: Duration) -> Result<bool> {
    Native {
        home,
        deadline: Instant::now() + limit,
    }
    .responding()
}

/// An exclusive launch lease that unlocks when its owner leaves scope.
#[derive(Debug)]
#[must_use]
pub struct LaunchLease {
    file: std::fs::File,
}

impl Drop for LaunchLease {
    fn drop(&mut self) {
        platform::cleanup::report(self.file.unlock(), "release the Herdr launch lease");
    }
}

/// Hold the launch lock until the server has become responsive.
///
/// # Errors
/// Returns errors when another launcher owns the lock or an observation fails.
pub fn launch_guard(home: &Path) -> Result<(LaunchLease, Launch)> {
    let lock = launch_lock(home)?;
    let mut native = Native::new(home);
    let processes = native.processes()?;
    let servers = processes.iter().filter(|p| p.role == Role::Server).count();
    let abandoned = processes
        .iter()
        .filter(|p| p.role == Role::Client && p.parent == INIT_PID)
        .count();
    let decision = rules::launch(Instances {
        servers: u16::try_from(servers).unwrap_or(u16::MAX),
        abandoned: u16::try_from(abandoned).unwrap_or(u16::MAX),
        responding: servers > 0 && native.responding()?,
    });
    ensure!(
        !matches!(decision, Launch::RefuseDuplicate | Launch::RefuseStale),
        "Herdr has a stale or duplicate instance; run `dotfiles herdr health`, then `dotfiles herdr cleanup --pid PID`; no process was stopped"
    );
    Ok((lock, decision))
}

fn launch_lock(home: &Path) -> Result<LaunchLease> {
    let path = home.join(LAUNCH_LOCK);
    std::fs::create_dir_all(path.parent().context("launch lock has no directory")?)?;
    let lock = std::fs::File::create(path)?;
    lock.try_lock()
        .context("another Herdr launcher is active; try again after it finishes")?;
    Ok(LaunchLease { file: lock })
}

/// Stop one owned Herdr instance after exact interactive confirmation and identity revalidation.
///
/// # Errors
/// Refuses noninteractive use, changed identities, non-Herdr PIDs, and failed signals.
pub fn cleanup(home: &Path, pid: u32) -> Result<()> {
    ensure!(
        std::io::stdin().is_terminal(),
        "Herdr cleanup requires the owner's interactive confirmation"
    );
    let before = Native::new(home)
        .processes()?
        .into_iter()
        .find(|p| p.pid == pid && matches!(p.role, Role::Client | Role::Server))
        .context("the PID is not an owned Herdr instance")?;
    println!(
        "PID {} (parent {}, started {}): {}",
        before.pid, before.parent, before.started, before.command
    );
    println!("Stopping this process can interrupt live sessions. Save their work first.");
    print!("Type `terminate {pid}` to send SIGTERM: ");
    std::io::stdout().flush()?;
    let mut answer = String::new();
    std::io::stdin().read_line(&mut answer)?;
    let current = Native::new(home)
        .processes()?
        .into_iter()
        .find(|p| p.pid == pid);
    let decision = rules::cleanup(
        answer.trim() == format!("terminate {pid}"),
        current.as_ref() == Some(&before),
        true,
    );
    ensure!(
        decision == rules::Cleanup::Terminate,
        "cleanup cancelled or process identity changed; no signal was sent"
    );
    let status = platform::process::command(KILL)
        .args(["-TERM", "--", &pid.to_string()])
        .status()?;
    ensure!(status.success(), "SIGTERM failed");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const LEASE_HOME: &str = "DOTFILES_LAUNCH_LEASE_TEST_HOME";
    const LEASE_AVAILABLE: &str = "DOTFILES_LAUNCH_LEASE_TEST_AVAILABLE";
    const LEASE_PROBE: &str = "herdr::tests::child_process_checks_the_fixture_lease";
    const LEASE_PROBE_RESULT: &str = "test result: ok. 1 passed; 0 failed;";

    fn child_lease(home: &Path, available: bool) -> Result<()> {
        let output = platform::process::command(std::env::current_exe()?)
            .args(["--exact", LEASE_PROBE, "--ignored", "--nocapture"])
            .env(LEASE_HOME, home)
            .env(LEASE_AVAILABLE, available.to_string())
            .output()?;
        let stdout = String::from_utf8(output.stdout)?;
        ensure!(
            output.status.success() && stdout.contains(LEASE_PROBE_RESULT),
            "the child lease probe did not pass: {stdout}\n{}",
            String::from_utf8_lossy(&output.stderr)
        );
        Ok(())
    }

    #[test]
    #[ignore = "Runs inside the retained-handle test's child process"]
    fn child_process_checks_the_fixture_lease() -> Result<()> {
        let home = std::env::var_os(LEASE_HOME).context("fixture home missing")?;
        let expected: bool = std::env::var(LEASE_AVAILABLE)?.parse()?;
        let available = match launch_lock(Path::new(&home)) {
            Ok(_lease) => true,
            Err(error)
                if matches!(
                    error.downcast_ref::<std::fs::TryLockError>(),
                    Some(std::fs::TryLockError::WouldBlock)
                ) =>
            {
                false
            }
            Err(error) => return Err(error),
        };
        ensure!(available == expected, "unexpected child lease availability");
        Ok(())
    }

    #[test]
    fn a_launch_lock_serializes_launchers_and_releases_after_failure() -> Result<()> {
        let home = tempfile::tempdir()?;
        let lock = launch_lock(home.path())?;
        assert!(launch_lock(home.path()).is_err());
        drop(lock);
        let _lock = launch_lock(home.path())?;
        Ok(())
    }

    #[test]
    fn a_launch_lease_releases_with_a_duplicate_handle_still_open() -> Result<()> {
        let home = tempfile::tempdir()?;
        let lock = launch_lock(home.path())?;
        let duplicate = lock.file.try_clone()?;
        assert!(launch_lock(home.path()).is_err());
        assert!(launch_lock(home.path()).is_err());
        child_lease(home.path(), false)?;
        drop(lock);
        child_lease(home.path(), true)?;
        let _next = launch_lock(home.path())
            .context("the owning lease must release before its duplicate closes")?;
        drop(duplicate);
        Ok(())
    }

    struct Fake {
        agent: Agent,
        processes: Vec<Process>,
        responding: bool,
        dead: Vec<u32>,
    }

    impl Observations for Fake {
        fn agent(&mut self) -> Agent {
            self.agent
        }
        fn processes(&mut self) -> Result<Vec<Process>> {
            Ok(self.processes.clone())
        }
        fn responding(&mut self) -> Result<bool> {
            Ok(self.responding)
        }
        fn dead_socket(&mut self, daemon: &Process) -> Result<bool> {
            Ok(self.dead.contains(&daemon.pid))
        }
    }

    fn process(pid: u32, role: Role) -> Process {
        Process {
            pid,
            parent: 1,
            started: "Fri Oct 23 12:00:00 2026".into(),
            command: "fixture".into(),
            role,
        }
    }

    #[test]
    fn fake_agent_and_process_list_expose_all_health_findings() -> Result<()> {
        let mut fake = Fake {
            agent: Agent::Empty,
            processes: vec![
                process(10, Role::Server),
                process(11, Role::Server),
                process(12, Role::Codex),
                process(13, Role::Domyjob),
            ],
            responding: true,
            dead: vec![12, 13],
        };
        let report = inspect(&mut fake)?;
        assert_eq!(report.agent, Agent::Empty);
        assert_eq!(report.launch, Launch::RefuseDuplicate);
        assert_eq!(
            report
                .stale_daemons
                .iter()
                .map(|p| p.pid)
                .collect::<Vec<_>>(),
            [12, 13]
        );
        fake.agent = Agent::Stale;
        assert_eq!(inspect(&mut fake)?.agent, Agent::Stale);
        Ok(())
    }

    #[test]
    fn a_frozen_server_or_abandoned_client_refuses_a_second_server() -> Result<()> {
        let mut fake = Fake {
            agent: Agent::Loaded,
            processes: vec![process(10, Role::Server)],
            responding: false,
            dead: vec![],
        };
        assert_eq!(inspect(&mut fake)?.launch, Launch::RefuseStale);
        fake.processes = vec![process(11, Role::Client)];
        assert_eq!(inspect(&mut fake)?.launch, Launch::RefuseStale);
        fake.processes.clear();
        assert_eq!(inspect(&mut fake)?.launch, Launch::Start);
        Ok(())
    }

    #[test]
    fn herdr_process_parser_includes_clients_with_arguments() {
        for command in [
            "herdr",
            "herdr --session fixture",
            "/opt/bin/herdr attach fixture",
        ] {
            let line = format!("22 1 Fri Oct 23 12:00:00 2026 {command}");
            assert_eq!(super::process(&line).map(|p| p.role), Some(Role::Client));
        }
        for command in [
            "herdr status server",
            "herdr-agent serve",
            "dotfiles herdr health",
        ] {
            let line = format!("22 1 Fri Oct 23 12:00:00 2026 {command}");
            assert!(super::process(&line).is_none());
        }
    }

    #[test]
    fn daemon_environment_parser_reads_the_first_variable_and_values_with_spaces() {
        let text = "SSH_AUTH_SOCK=/dead/agent.sock GIT_SSH_COMMAND=ssh -o IdentityAgent=/home/fixture with spaces/agent.sock NEXT=value";
        assert_eq!(environment(text, AUTH_SOCKET), Some("/dead/agent.sock"));
        assert_eq!(
            environment(text, SSH_COMMAND),
            Some("ssh -o IdentityAgent=/home/fixture with spaces/agent.sock")
        );
    }

    #[test]
    fn missing_agents_are_reported_and_an_attached_client_is_not_a_duplicate() -> Result<()> {
        let mut client = process(11, Role::Client);
        client.parent = 99;
        let report = inspect(&mut Fake {
            agent: Agent::Missing,
            processes: vec![process(10, Role::Server), client],
            responding: true,
            dead: vec![],
        })?;
        assert_eq!(report.launch, Launch::Attach);
        assert_eq!(report.messages().len(), 1);
        assert!(report.messages()[0].contains("Missing"));
        Ok(())
    }
}
