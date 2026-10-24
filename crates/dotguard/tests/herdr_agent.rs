#![cfg(unix)]

use anyhow::{Context, Result, ensure};
use std::fs;
use std::io::{Read, Write};
use std::os::unix::net::UnixListener;
use std::path::Path;
use std::process::Command;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::JoinHandle;
use std::time::Duration;

const SELECTOR: &str = env!("CARGO_BIN_EXE_herdr-agent");
const SSH: &str = "/usr/bin/ssh";
const POLL: Duration = Duration::from_millis(5);
const REQUEST_LIMIT: Duration = Duration::from_secs(1);
const REQUEST_IDENTITIES: u8 = 11;
const ANSWER_IDENTITIES: u8 = 12;
const REMOVE_ALL_IDENTITIES: u8 = 19;
const REMOVE_ALL_LEGACY_IDENTITIES: u8 = 9;
const AGENT_SUCCESS: u8 = 6;
const AGENT_FAILURE: u8 = 5;
const AGENT_EXTENSION: u8 = 27;
const MAX_REQUEST: usize = 1024;
const KEY: &str =
    "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA";

struct Agent {
    loaded: Arc<AtomicBool>,
    stopped: Arc<AtomicBool>,
    worker: Option<JoinHandle<Result<()>>>,
}

impl Agent {
    fn start(socket: &Path) -> Result<Self> {
        fs::create_dir_all(socket.parent().context("socket directory missing")?)?;
        let listener = UnixListener::bind(socket)?;
        listener.set_nonblocking(true)?;
        let loaded = Arc::new(AtomicBool::new(true));
        let stopped = Arc::new(AtomicBool::new(false));
        let state = loaded.clone();
        let stop = stopped.clone();
        let worker = std::thread::spawn(move || {
            while !stop.load(Ordering::Relaxed) {
                match listener.accept() {
                    Ok((mut stream, _)) => {
                        stream.set_nonblocking(false)?;
                        stream.set_read_timeout(Some(REQUEST_LIMIT))?;
                        stream.set_write_timeout(Some(REQUEST_LIMIT))?;
                        loop {
                            let mut length = [0; 4];
                            match stream.read_exact(&mut length) {
                                Ok(()) => {}
                                Err(error)
                                    if matches!(
                                        error.kind(),
                                        std::io::ErrorKind::UnexpectedEof
                                            | std::io::ErrorKind::ConnectionReset
                                    ) =>
                                {
                                    break;
                                }
                                Err(error) => return Err(error.into()),
                            }
                            let length = usize::try_from(u32::from_be_bytes(length))?;
                            ensure!(
                                (1..=MAX_REQUEST).contains(&length),
                                "unexpected agent request length: {length}"
                            );
                            let mut request = vec![0; length];
                            stream.read_exact(&mut request)?;
                            let response = reply(request[0], &state)?;
                            stream.write_all(&u32::try_from(response.len())?.to_be_bytes())?;
                            stream.write_all(&response)?;
                        }
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        std::thread::sleep(POLL);
                    }
                    Err(error) => return Err(error.into()),
                }
            }
            Ok(())
        });
        Ok(Self {
            loaded,
            stopped,
            worker: Some(worker),
        })
    }
}

fn reply(request: u8, loaded: &AtomicBool) -> Result<Vec<u8>> {
    if request == AGENT_EXTENSION {
        return Ok(vec![AGENT_FAILURE]);
    }
    if matches!(
        request,
        REMOVE_ALL_IDENTITIES | REMOVE_ALL_LEGACY_IDENTITIES
    ) {
        loaded.store(false, Ordering::Relaxed);
        return Ok(vec![AGENT_SUCCESS]);
    }
    ensure!(
        request == REQUEST_IDENTITIES,
        "unexpected agent request: {request}"
    );
    let mut response = vec![ANSWER_IDENTITIES];
    if loaded.load(Ordering::Relaxed) {
        response.extend(1_u32.to_be_bytes());
        let mut key = b"\0\0\0\x0bssh-ed25519\0\0\0\x20".to_vec();
        key.extend([0; 32]);
        response.extend(u32::try_from(key.len())?.to_be_bytes());
        response.extend(key);
        response.extend(0_u32.to_be_bytes());
    } else {
        response.extend(0_u32.to_be_bytes());
    }
    Ok(response)
}

impl Drop for Agent {
    fn drop(&mut self) {
        self.stopped.store(true, Ordering::Relaxed);
        if let Some(worker) = self.worker.take() {
            platform::cleanup::report(
                worker
                    .join()
                    .map_err(|_| anyhow::anyhow!("fake agent panicked"))
                    .and_then(std::convert::identity),
                "join the fake SSH agent",
            );
        }
    }
}

fn selector(home: &Path, route: &str) -> Command {
    let mut command = platform::process::command(SELECTOR);
    command
        .args(["route", route])
        .env("HOME", home)
        .env_remove("SSH_CONNECTION")
        .env_remove("DOTFILES_SSH_AUTH_SOCK");
    command
}

#[test]
fn routing_rechecks_a_fake_agent_after_lock_and_socket_removal() -> Result<()> {
    let scope = tempfile::tempdir()?;
    let home = scope.path();
    let socket = home.join(dotguard::herdr::SOCKET);
    fs::create_dir_all(home.join(".config/herdr-agent"))?;
    fs::write(home.join(dotguard::herdr::KEYS), KEY)?;
    let agent = Agent::start(&socket)?;
    assert!(selector(home, "herdr").status()?.success());
    assert!(
        platform::process::command(SELECTOR)
            .arg("lock")
            .env("HOME", home)
            .status()?
            .success()
    );
    assert!(!agent.loaded.load(Ordering::Relaxed));
    assert!(!selector(home, "herdr").status()?.success());
    assert!(selector(home, "password-manager").status()?.success());
    drop(agent);
    assert!(selector(home, "password-manager").status()?.success());
    fs::remove_file(&socket)?;
    assert!(selector(home, "password-manager").status()?.success());
    Ok(())
}

#[test]
fn openssh_resolves_an_old_daemon_environment_and_explicit_socket_at_each_use() -> Result<()> {
    let scope = tempfile::tempdir()?;
    let home = scope.path();
    let bin = home.join(".local/bin");
    fs::create_dir_all(&bin)?;
    fs::copy(SELECTOR, bin.join("herdr-agent"))?;
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let config = home.join("chezmoi.toml");
    fs::write(&config, "")?;
    let output = platform::tool::Tool::Chezmoi
        .command()
        .args(["--config"])
        .arg(&config)
        .arg("--source")
        .arg(root.join("home"))
        .arg("--override-data")
        .arg(serde_json::to_string(&serde_json::json!({
            "profile": "linux", "chezmoi": {"homeDir": home},
            "ssh_signing": {"socket": "/fixture/onepassword.sock"}
        }))?)
        .args(["execute-template", "--file"])
        .arg(root.join("home/dot_config/dotfiles/ssh-agent.conf.tmpl"))
        .output()?;
    ensure!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let ssh_config = home.join("ssh_config");
    fs::write(&ssh_config, output.stdout)?;
    let effective = |explicit: Option<&Path>| -> Result<String> {
        let mut command = platform::process::command(SSH);
        command
            .args(["-G", "-F"])
            .arg(&ssh_config)
            .arg("fixture.invalid")
            .env("HOME", home)
            .env("SSH_AUTH_SOCK", "/dead/agent.sock")
            .env("GIT_SSH_COMMAND", "ssh -o IdentityAgent=/dead/agent.sock")
            .env_remove("SSH_CONNECTION")
            .env_remove("DOTFILES_SSH_AUTH_SOCK");
        if let Some(path) = explicit {
            command.env("DOTFILES_SSH_AUTH_SOCK", path);
        }
        let output = command.output()?;
        ensure!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8(output.stdout)?
            .lines()
            .find_map(|line| line.strip_prefix("identityagent ").map(str::to_owned))
            .context("OpenSSH reported no agent")
    };
    assert_eq!(effective(None)?, "/fixture/onepassword.sock");
    fs::create_dir_all(home.join(".config/herdr-agent"))?;
    fs::write(home.join(dotguard::herdr::KEYS), KEY)?;
    let agent = Agent::start(&home.join(dotguard::herdr::SOCKET))?;
    assert!(effective(None)?.ends_with(".local/state/herdr/agent.sock"));
    agent.loaded.store(false, Ordering::Relaxed);
    assert_eq!(effective(None)?, "/fixture/onepassword.sock");
    drop(agent);
    assert_eq!(effective(None)?, "/fixture/onepassword.sock");
    let explicit = home.join("explicit.sock");
    let _agent = Agent::start(&explicit)?;
    assert_eq!(effective(Some(&explicit))?, "$DOTFILES_SSH_AUTH_SOCK");
    assert!(
        selector(home, "explicit")
            .env("DOTFILES_SSH_AUTH_SOCK", &explicit)
            .status()?
            .success()
    );
    Ok(())
}

#[test]
fn supervisor_refuses_an_existing_agent_and_a_non_socket_path() -> Result<()> {
    let scope = tempfile::tempdir()?;
    let socket = scope.path().join(dotguard::herdr::SOCKET);
    let agent = Agent::start(&socket)?;
    let serve = || {
        platform::process::command(SELECTOR)
            .arg("serve")
            .env("HOME", scope.path())
            .output()
    };
    let output = serve()?;
    assert!(!output.status.success());
    assert!(String::from_utf8(output.stderr)?.contains("already serves"));
    drop(agent);
    fs::remove_file(&socket)?;
    fs::write(&socket, "preserve this file")?;
    let output = serve()?;
    assert!(!output.status.success());
    assert!(String::from_utf8(output.stderr)?.contains("non-socket"));
    assert_eq!(fs::read_to_string(socket)?, "preserve this file");
    Ok(())
}
