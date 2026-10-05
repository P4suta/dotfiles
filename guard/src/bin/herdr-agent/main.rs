//! herdr-agent — Herdr's own ssh-agent: one 1Password approval when Herdr starts, none for each commit or push after that.
//!
//! The 1Password agent asks again for every new process tree, which is every agent an unattended session spawns.
//! This agent is loaded once: `op read` hands each private key to this process, which writes it straight into `ssh-add -`, so the key goes from 1Password to the agent's memory and never touches disk.
//! There is no lifetime: the keys stay until `herdr-agent lock`, logout, or the agent process dies.
//!
//! Which keys: the public keys in `~/.config/herdr-agent/keys` (rendered from `.chezmoidata.toml`), matched against every 1Password "SSH Key" item.
//! Who uses it: `~/.config/shell/ssh-agent.sh` and Nushell's `env.nu` ask `herdr-agent ready` inside a Herdr pane, and fall back to the 1Password agent whenever it answers no.

mod json;

use dotguard::locate;
use json::Value;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{ExitCode, Stdio};

const USAGE: &str = "usage: herdr-agent ensure|ready|status|lock\n\n  \
    ensure  start the agent and load the keys from 1Password if it holds none (ghostty-herdr runs this)\n  \
    ready   print the socket and exit 0 if the agent holds keys, exit 1 otherwise\n  \
    status  list the loaded keys\n  \
    lock    remove every key from the agent";

fn main() -> ExitCode {
    // The agent is a Unix-socket ssh-agent; Windows provisions Herdr's agent through dotctl instead.
    if cfg!(not(unix)) {
        eprintln!("herdr-agent manages a Unix-socket SSH agent and does not run on Windows");
        return ExitCode::from(2);
    }
    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_default();
    let sock = home.join(".local/state/herdr/agent.sock");
    let code = match std::env::args().nth(1).as_deref() {
        Some("ensure") => ensure(&sock, &home.join(".config/herdr-agent/keys")),
        Some("ready") => ready(&sock),
        Some("status") => ssh_add(&sock, "-l"),
        Some("lock") => ssh_add(&sock, "-D"),
        _ => {
            eprintln!("{USAGE}");
            2
        }
    };
    ExitCode::from(code)
}

/// What `ssh-add -l` says about the socket.
#[derive(Debug, PartialEq)]
enum State {
    Loaded,
    Empty,
    Absent,
}

fn state(sock: &Path) -> State {
    if !sock.exists() {
        return State::Absent;
    }
    let status = locate::child("ssh-add")
        .arg("-l")
        .env("SSH_AUTH_SOCK", sock)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
    match status.ok().and_then(|s| s.code()) {
        Some(0) => State::Loaded,
        Some(1) => State::Empty,
        _ => State::Absent,
    }
}

fn ready(sock: &Path) -> u8 {
    if state(sock) == State::Loaded {
        println!("{}", sock.display());
        0
    } else {
        1
    }
}

fn ssh_add(sock: &Path, flag: &str) -> u8 {
    let status = locate::child("ssh-add")
        .arg(flag)
        .env("SSH_AUTH_SOCK", sock)
        .status();
    status
        .ok()
        .and_then(|s| s.code())
        .map_or(1, |c| u8::try_from(c).unwrap_or(1))
}

fn ensure(sock: &Path, keys_file: &Path) -> u8 {
    let result = match state(sock) {
        State::Loaded => Ok(()),
        State::Empty => load(sock, keys_file),
        State::Absent => start(sock).and_then(|()| load(sock, keys_file)),
    };
    match result {
        Ok(()) => {
            println!("{}", sock.display());
            0
        }
        Err(e) => {
            eprintln!("herdr-agent: {e}; Herdr panes keep using the 1Password agent");
            1
        }
    }
}

fn start(sock: &Path) -> Result<(), String> {
    let dir = sock.parent().ok_or("socket has no parent directory")?;
    std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700))
            .map_err(|e| format!("{}: {e}", dir.display()))?;
    }
    // A socket left behind by a dead agent would make ssh-agent refuse to bind.
    let _ = std::fs::remove_file(sock);
    let ok = locate::child("ssh-agent")
        .arg("-a")
        .arg(sock)
        .stdout(Stdio::null())
        .status()
        .is_ok_and(|s| s.success());
    if ok {
        Ok(())
    } else {
        Err("ssh-agent did not start".into())
    }
}

fn load(sock: &Path, keys_file: &Path) -> Result<(), String> {
    let text =
        std::fs::read_to_string(keys_file).map_err(|e| format!("{}: {e}", keys_file.display()))?;
    let wanted = wanted_keys(&text);
    if wanted.is_empty() {
        return Err(format!("no public keys in {}", keys_file.display()));
    }

    let list = op(
        &[
            "item",
            "list",
            "--categories",
            "SSH Key",
            "--format",
            "json",
        ],
        None,
    )?;
    let items = op(&["item", "get", "-", "--format", "json"], Some(&list))?;
    let items = String::from_utf8(items).map_err(|_| "op printed invalid utf-8")?;
    let refs = private_key_refs(&json::parse_stream(&items)?, &wanted);

    let mut loaded = 0;
    for r in &refs {
        let mut key = op(&["read", r], None)?;
        let added = add_key(sock, &key);
        // The key is only ever in this buffer and the pipe; do not leave it in freed memory either.
        key.fill(0);
        added?;
        loaded += 1;
    }
    if loaded < wanted.len() {
        eprintln!(
            "herdr-agent: loaded {loaded} of {} keys; the rest have no matching 1Password item",
            wanted.len()
        );
    }
    if loaded == 0 {
        Err("no keys loaded".into())
    } else {
        Ok(())
    }
}

/// Run `op`, with stderr left on the terminal so its own errors and prompts reach the user.
fn op(args: &[&str], input: Option<&[u8]>) -> Result<Vec<u8>, String> {
    let mut child = locate::child("op")
        .args(args)
        .stdin(if input.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stdout(Stdio::piped())
        .spawn()
        .map_err(|e| format!("op: {e}"))?;
    if let (Some(data), Some(mut stdin)) = (input, child.stdin.take()) {
        stdin.write_all(data).map_err(|e| format!("op: {e}"))?;
    }
    let out = child.wait_with_output().map_err(|e| format!("op: {e}"))?;
    if out.status.success() {
        Ok(out.stdout)
    } else {
        Err(format!("`op {}` failed", args.join(" ")))
    }
}

fn add_key(sock: &Path, key: &[u8]) -> Result<(), String> {
    let mut child = locate::child("ssh-add")
        .args(["-q", "-"])
        .env("SSH_AUTH_SOCK", sock)
        .stdin(Stdio::piped())
        .spawn()
        .map_err(|e| format!("ssh-add: {e}"))?;
    if let Some(mut stdin) = child.stdin.take() {
        stdin.write_all(key).map_err(|e| format!("ssh-add: {e}"))?;
    }
    let ok = child.wait().is_ok_and(|s| s.success());
    if ok {
        Ok(())
    } else {
        Err("ssh-add refused a key".into())
    }
}

/// A public key reduced to `type base64`, so a trailing comment on either side cannot break a match.
fn normalize(key: &str) -> String {
    key.split_whitespace().take(2).collect::<Vec<_>>().join(" ")
}

/// One key per line; blank lines and `#` comments skipped.
fn wanted_keys(text: &str) -> Vec<String> {
    text.lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .map(normalize)
        .collect()
}

/// `op read` references to the private key of every item whose public key is wanted, in OpenSSH format because that is what `ssh-add -` reads.
/// The field is named by its id, `private_key`, because its label follows the display language of 1Password and `op read` resolves labels as written.
fn private_key_refs(items: &[Value], wanted: &[String]) -> Vec<String> {
    items
        .iter()
        .flat_map(|v| match v {
            Value::Arr(a) => a.iter().collect::<Vec<_>>(),
            other => vec![other],
        })
        .filter_map(|item| {
            let public = item.get("fields")?.as_arr().iter().find_map(|f| {
                let is_public = f.get("id").and_then(Value::as_str) == Some("public_key")
                    || f.get("label").and_then(Value::as_str) == Some("public key");
                if is_public {
                    f.get("value").and_then(Value::as_str)
                } else {
                    None
                }
            })?;
            if !wanted.contains(&normalize(public)) {
                return None;
            }
            let id = item.get("id")?.as_str()?;
            let vault = item.get("vault")?.get("id")?.as_str()?;
            Some(format!("op://{vault}/{id}/private_key?ssh-format=openssh"))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{json, private_key_refs, wanted_keys};

    const ITEMS: &str = r#"
{
  "id": "item1",
  "vault": {"id": "vault1", "name": "Private"},
  "fields": [
    {"id": "public_key", "label": "public key", "value": "ssh-ed25519 AAAAsign"},
    {"id": "fingerprint", "value": "SHA256:x"}
  ]
}
{
  "id": "item2",
  "vault": {"id": "vault1"},
  "fields": [{"id": "public_key", "value": "ssh-ed25519 AAAAother"}]
}
{"id": "item3", "vault": {"id": "vault2"}}
"#;

    #[test]
    fn only_wanted_items_are_referenced() {
        let items = json::parse_stream(ITEMS).unwrap();
        let wanted = wanted_keys("# signing\nssh-ed25519 AAAAsign comment\n\n");
        assert_eq!(
            private_key_refs(&items, &wanted),
            ["op://vault1/item1/private_key?ssh-format=openssh"]
        );
    }

    #[test]
    fn a_single_array_document_works_too() {
        let items = json::parse_stream(&format!("[{}]", ITEMS.replace("}\n{", "},\n{"))).unwrap();
        let wanted = wanted_keys("ssh-ed25519 AAAAother");
        assert_eq!(
            private_key_refs(&items, &wanted),
            ["op://vault1/item2/private_key?ssh-format=openssh"]
        );
    }
}
