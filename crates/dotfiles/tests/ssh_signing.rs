//! Real OpenSSH agents keep signing independent of the session agent.
#![cfg(unix)]

use anyhow::{Context, Result, ensure};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

#[path = "../../dotguard/tests/support/fixture_git.rs"]
mod fixture_git;

const SSH_AGENT: &str = "/usr/bin/ssh-agent";
const SSH_ADD: &str = "/usr/bin/ssh-add";
const SSH_KEYGEN: &str = "/usr/bin/ssh-keygen";
const SIGNER: &str = env!("CARGO_BIN_EXE_dotfiles-ssh-keygen");
const CONFIG: &str = ".config/dotfiles/signing.toml";
const IDENTITY: &str = "fixture@example.invalid";
const PAYLOAD: &[u8] = b"A signature must use the configured password manager agent.\n";

struct Agent {
    socket: PathBuf,
    pid: u32,
}

impl Agent {
    fn start(socket: PathBuf) -> Result<Self> {
        let output = checked(
            platform::process::command(SSH_AGENT)
                .args(["-s", "-a"])
                .arg(&socket),
        )?;
        let environment = String::from_utf8(output.stdout)?;
        let pid = environment
            .lines()
            .find_map(|line| line.strip_prefix("SSH_AGENT_PID=")?.split(';').next())
            .context("ssh-agent returned no process identity")?
            .parse()?;
        Ok(Self { socket, pid })
    }
}

impl Drop for Agent {
    fn drop(&mut self) {
        platform::cleanup::report(
            checked(
                platform::process::command(SSH_AGENT)
                    .arg("-k")
                    .env("SSH_AGENT_PID", self.pid.to_string())
                    .env("SSH_AUTH_SOCK", &self.socket),
            ),
            "stop the fixture SSH agent",
        );
    }
}

fn checked(command: &mut Command) -> Result<Output> {
    let output = command.output()?;
    ensure!(
        output.status.success(),
        "{command:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    Ok(output)
}

fn input(command: &mut Command, bytes: &[u8]) -> Result<Output> {
    let mut child = command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    child
        .stdin
        .take()
        .context("fixture stdin is missing")?
        .write_all(bytes)?;
    Ok(child.wait_with_output()?)
}

struct Fixture {
    password_manager: Agent,
    session: Agent,
    scope: tempfile::TempDir,
    public_key: PathBuf,
}

impl Fixture {
    fn new() -> Result<Self> {
        let scope = tempfile::Builder::new().prefix("sign-").tempdir()?;
        let password_manager = Agent::start(scope.path().join("op.sock"))?;
        let session = Agent::start(scope.path().join("session.sock"))?;
        let private_key = scope.path().join("signing key");
        checked(
            platform::process::command(SSH_KEYGEN)
                .args(["-q", "-t", "ed25519", "-N", "", "-f"])
                .arg(&private_key),
        )?;
        checked(
            platform::process::command(SSH_ADD)
                .arg(&private_key)
                .env("SSH_AUTH_SOCK", &password_manager.socket),
        )?;
        fs::remove_file(&private_key)?;
        fs::create_dir_all(scope.path().join(".config/dotfiles"))?;
        fs::write(
            scope.path().join("gitconfig"),
            "[user]\nname = Fixture\nemail = fixture@example.invalid\n",
        )?;
        let fixture = Self {
            public_key: private_key.with_extension("pub"),
            scope,
            password_manager,
            session,
        };
        fixture.configure(&fixture.password_manager.socket)?;
        Ok(fixture)
    }

    fn configure(&self, socket: &Path) -> Result<()> {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let config = self.scope.path().join("chezmoi.toml");
        fs::write(&config, "")?;
        let overrides = serde_json::json!({
            "profile": if cfg!(target_os = "macos") { "mac" } else { "linux" },
            "ssh_signing": { "socket": socket },
        });
        let output = checked(
            dotfiles::profiles::chezmoi(&root, self.scope.path(), &config, self.scope.path())
                .arg("--override-data")
                .arg(serde_json::to_string(&overrides)?)
                .args(["execute-template", "--file"])
                .arg(root.join("home/dot_config/dotfiles/signing.toml.tmpl")),
        )?;
        fs::write(self.scope.path().join(CONFIG), output.stdout)?;
        Ok(())
    }

    fn command(&self, program: &str) -> Command {
        let mut command = fixture_git::command(program, &self.scope.path().join("gitconfig"));
        command
            .current_dir(self.scope.path())
            .env("HOME", self.scope.path())
            .env("SSH_AUTH_SOCK", &self.session.socket);
        command
    }

    fn signing(&self, program: &str) -> Command {
        let mut command = self.command(program);
        command
            .args(["-Y", "sign", "-n", "git", "-f"])
            .arg(&self.public_key)
            .arg("-U");
        command
    }

    fn signers(&self) -> Result<PathBuf> {
        let path = self.scope.path().join("allowed signers");
        fs::write(
            &path,
            format!("{IDENTITY} {}", fs::read_to_string(&self.public_key)?),
        )?;
        Ok(path)
    }
}

#[test]
fn signs_with_the_overridden_agent_and_preserves_openssh_output() -> Result<()> {
    let fixture = Fixture::new()?;
    let empty = fixture.command(SSH_ADD).arg("-l").output()?;
    assert_eq!(empty.status.code(), Some(1));
    let failed = input(&mut fixture.signing(SSH_KEYGEN), PAYLOAD)?;
    assert!(
        !failed.status.success(),
        "the session agent must lack the signing key"
    );

    let signed = input(&mut fixture.signing(SIGNER), PAYLOAD)?;
    let direct = input(
        fixture
            .signing(SSH_KEYGEN)
            .env("SSH_AUTH_SOCK", &fixture.password_manager.socket),
        PAYLOAD,
    )?;
    assert!(
        signed.status.success(),
        "{}",
        String::from_utf8_lossy(&signed.stderr)
    );
    assert_eq!(signed.status, direct.status);
    assert_eq!(signed.stdout, direct.stdout);
    assert_eq!(signed.stderr, direct.stderr);
    let signature = fixture.scope.path().join("signature with spaces");
    fs::write(&signature, signed.stdout)?;
    fixture.configure(&fixture.scope.path().join("absent.sock"))?;
    for program in [SSH_KEYGEN, SIGNER] {
        let output = input(
            fixture
                .command(program)
                .args(["-Y", "verify", "-n", "git", "-I", IDENTITY, "-f"])
                .arg(fixture.signers()?)
                .arg("-s")
                .arg(&signature),
            PAYLOAD,
        )?;
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    assert_eq!(
        fixture.command(SSH_ADD).arg("-l").output()?.status.code(),
        Some(1)
    );
    Ok(())
}

#[test]
fn git_signs_and_verifies_with_allowed_signers_while_the_session_agent_is_empty() -> Result<()> {
    let fixture = Fixture::new()?;
    checked(fixture.command("git").args(["init", "-q", "-b", "main"]))?;
    for (key, value) in [
        (
            "core.hooksPath",
            fixture.scope.path().join("no-hooks").into_os_string(),
        ),
        ("gpg.format", "ssh".into()),
        ("gpg.ssh.program", SIGNER.into()),
        (
            "gpg.ssh.allowedSignersFile",
            fixture.signers()?.into_os_string(),
        ),
        (
            "user.signingkey",
            format!("key::{}", fs::read_to_string(&fixture.public_key)?.trim()).into(),
        ),
    ] {
        checked(fixture.command("git").args(["config", key]).arg(value))?;
    }
    let tree = checked(fixture.command("git").arg("write-tree"))?;
    let tree = String::from_utf8(tree.stdout)?;
    let commit =
        checked(
            fixture
                .command("git")
                .args(["commit-tree", "-S", tree.trim(), "-m", "fixture"]),
        )?;
    let commit = String::from_utf8(commit.stdout)?;
    fixture.configure(&fixture.scope.path().join("absent.sock"))?;
    checked(
        fixture
            .command("git")
            .args(["verify-commit", commit.trim()]),
    )?;
    Ok(())
}

#[test]
fn missing_or_invalid_password_manager_socket_refuses_without_session_fallback() -> Result<()> {
    let fixture = Fixture::new()?;
    let missing = fixture.scope.path().join("missing.sock");
    let invalid = fixture.scope.path().join("regular file");
    fs::write(&invalid, "not an agent")?;
    for (socket, cause) in [(&missing, "is missing"), (&invalid, "is not a socket")] {
        fixture.configure(socket)?;
        let output = fixture
            .signing(SIGNER)
            .env("SSH_AUTH_SOCK", &fixture.password_manager.socket)
            .output()?;
        assert_eq!(output.status.code(), Some(1));
        assert_eq!(output.stdout, [] as [u8; 0]);
        let stderr = String::from_utf8(output.stderr)?;
        let refusal = dotfiles::refusal::Refusal::parse(stderr.lines().last().unwrap_or_default())
            .context("missing structured refusal")?;
        assert!(refusal.is_complete());
        assert!(refusal.cause.contains(cause), "{stderr}");
        assert!(stderr.contains(socket.to_str().unwrap()));
        assert_eq!(refusal.next.as_deref(), Some("start and unlock 1Password"));
    }
    let arguments = ["-Y", "invalid-operation"];
    let direct = fixture.command(SSH_KEYGEN).args(arguments).output()?;
    let forwarded = fixture.command(SIGNER).args(arguments).output()?;
    assert!(!direct.status.success());
    assert_eq!(forwarded.status, direct.status);
    assert_eq!(forwarded.stdout, direct.stdout);
    assert_eq!(forwarded.stderr, direct.stderr);
    Ok(())
}
