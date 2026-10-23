//! A supervised SSH agent with on-demand loading and an SSH configuration selector.
//!
//! `op read` hands each private key to this process, which pipes it into `ssh-add -`, so the key never touches disk.
//! Each key stays loaded for at most `KEY_LIFETIME`, and `herdr-agent lock`, logout, or the death of the agent removes it sooner.
//!
//! It loads the 1Password SSH keys whose public keys appear in `~/.config/herdr-agent/keys`, which chezmoi renders from the `herdr_agent` data.
//! That file names the commit signing key only when the machine-local data opts in through `herdr_agent.load_signing_key`.

#[path = "json.rs"]
mod json;

#[cfg(unix)]
use dotguard::herdr::{KEY_LIFETIME, KEYS, SERVICE_LABEL, SERVICE_UNIT, SOCKET, agent as state};
use dotguard::locate;
use json::Value;
use rules::herdr_rules::{Agent as State, Context, Ensure, Route};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{ExitCode, Stdio};

const USAGE: &str = "usage: herdr-agent ensure|serve|ready|status|lock|route herdr|inherited|explicit|password-manager\n\n  \
    ensure  start the supervised agent and load the configured keys from 1Password\n  \
    serve   run ssh-agent in the foreground under launchd or systemd\n  \
    route   test the SSH route without loading keys\n  \
    ready   print the socket and exit 0 if the agent holds keys, exit 1 otherwise\n  \
    status  list the loaded keys\n  \
    lock    remove every key from the agent";

/// The exit status of a command the agent doesn't serve.
const USAGE_ERROR: u8 = 2;

/// `rwx------`: the socket directory admits only its owner.
#[cfg(unix)]
const OWNER_ONLY: u32 = 0o700;

/// An `authorized_keys` key's material: its type and its base64 body.
const KEY_MATERIAL_FIELDS: usize = 2;
const SERVICE_CHECKS: u8 = 50;
const SERVICE_POLL: std::time::Duration = std::time::Duration::from_millis(100);

pub fn main() -> ExitCode {
    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_default();
    if !home.is_absolute() {
        eprintln!("herdr-agent: HOME must name an absolute directory");
        return ExitCode::from(USAGE_ERROR);
    }
    let sock = home.join(SOCKET);
    let code = match std::env::args().nth(1).as_deref() {
        Some("ensure") => ensure(&home, &sock, &home.join(KEYS)),
        Some("serve") => serve(&home, &sock).map_or_else(
            |error| {
                eprintln!("herdr-agent: {error}");
                1
            },
            |()| 0,
        ),
        Some("ready") => ready(&home, &sock),
        Some("route") => select_route(&home),
        Some("status") => ssh_add(&sock, "-l"),
        Some("lock") => ssh_add(&sock, "-D"),
        _ => {
            eprintln!("{USAGE}");
            USAGE_ERROR
        }
    };
    ExitCode::from(code)
}

#[cfg(unix)]
fn ready(home: &Path, sock: &Path) -> u8 {
    if state(home) == State::Loaded {
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

#[cfg(unix)]
fn ensure(home: &Path, sock: &Path, keys_file: &Path) -> u8 {
    let result = (|| {
        prepare(sock)?;
        let lock = std::fs::File::create(sock.with_extension("load.lock"))
            .map_err(|error| error.to_string())?;
        lock.lock().map_err(|error| error.to_string())?;
        match rules::herdr_rules::ensure(state(home)) {
            Ensure::Ready => Ok(()),
            Ensure::Load => load(sock, keys_file),
            Ensure::StartService => start_service(home).and_then(|()| load(sock, keys_file)),
        }
    })();
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

#[cfg(unix)]
fn prepare(sock: &Path) -> Result<(), String> {
    let dir = sock.parent().ok_or("socket has no parent directory")?;
    std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(dir, std::fs::Permissions::from_mode(OWNER_ONLY))
            .map_err(|e| format!("{}: {e}", dir.display()))?;
    }
    Ok(())
}

#[cfg(unix)]
fn serve(home: &Path, sock: &Path) -> Result<(), String> {
    use std::os::unix::fs::FileTypeExt;
    prepare(sock)?;
    let lock = std::fs::File::create(sock.with_extension("service.lock"))
        .map_err(|error| error.to_string())?;
    lock.try_lock()
        .map_err(|error| format!("another agent supervisor holds the lock: {error}"))?;
    if matches!(state(home), State::Loaded | State::Empty) {
        return Err(
            "an agent already serves the socket; adopt the service after retiring the old agent"
                .into(),
        );
    }
    if std::os::unix::net::UnixStream::connect(sock).is_ok() {
        return Err(
            "an unresponsive agent still accepts connections; refusing to replace it".into(),
        );
    }
    if let Ok(metadata) = std::fs::symlink_metadata(sock) {
        if !metadata.file_type().is_socket() {
            return Err("refusing to replace a non-socket agent path".into());
        }
        std::fs::remove_file(sock).map_err(|error| error.to_string())?;
    }
    let ok = locate::child("ssh-agent")
        .args(["-D", "-t", KEY_LIFETIME, "-a"])
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

#[cfg(unix)]
fn start_service(home: &Path) -> Result<(), String> {
    let status = if cfg!(target_os = "macos") {
        let uid = locate::child("id")
            .arg("-u")
            .output()
            .map_err(|error| error.to_string())?;
        if !uid.status.success() {
            return Err("cannot resolve the launchd user".into());
        }
        locate::child("launchctl")
            .args([
                "kickstart",
                &format!(
                    "gui/{}/{SERVICE_LABEL}",
                    String::from_utf8_lossy(&uid.stdout).trim()
                ),
            ])
            .status()
    } else {
        locate::child("systemctl")
            .args(["--user", "start", SERVICE_UNIT])
            .status()
    }
    .map_err(|error| error.to_string())?;
    if !status.success() {
        return Err("enable the rendered herdr-agent service before running ensure".into());
    }
    for _ in 0..SERVICE_CHECKS {
        if matches!(state(home), State::Loaded | State::Empty) {
            return Ok(());
        }
        std::thread::sleep(SERVICE_POLL);
    }
    Err("the supervised agent did not become ready".into())
}

#[cfg(unix)]
fn select_route(home: &Path) -> u8 {
    use std::os::unix::fs::FileTypeExt;
    let requested = match std::env::args().nth(2).as_deref() {
        Some("herdr") => Route::Herdr,
        Some("explicit") => Route::Explicit,
        Some("inherited") => Route::Inherited,
        Some("password-manager") => Route::PasswordManager,
        _ => return USAGE_ERROR,
    };
    let explicit = std::env::var_os("DOTFILES_SSH_AUTH_SOCK").is_some_and(|path| {
        std::fs::metadata(path).is_ok_and(|metadata| metadata.file_type().is_socket())
    });
    let remote = std::env::var_os("SSH_CONNECTION").is_some_and(|value| !value.is_empty());
    let context = if explicit {
        Context::Explicit
    } else if remote {
        Context::Inherited
    } else {
        Context::Local
    };
    let agent = if context == Context::Local
        && matches!(requested, Route::Herdr | Route::PasswordManager)
    {
        state(home)
    } else {
        State::Missing
    };
    let selected = rules::herdr_rules::route(context, agent);
    u8::from(selected != requested)
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

/// `ssh-add` arguments that read one key from standard input and hold it for `KEY_LIFETIME`.
const fn add_arguments() -> [&'static str; 4] {
    ["-q", "-t", KEY_LIFETIME, "-"]
}

fn add_key(sock: &Path, key: &[u8]) -> Result<(), String> {
    let mut child = locate::child("ssh-add")
        .args(add_arguments())
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

fn normalize(key: &str) -> String {
    key.split_whitespace()
        .take(KEY_MATERIAL_FIELDS)
        .collect::<Vec<_>>()
        .join(" ")
}

fn wanted_keys(text: &str) -> Vec<String> {
    text.lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .map(normalize)
        .collect()
}

/// `op read` references to the private key of each wanted item, in the OpenSSH format that `ssh-add -` reads.
/// The id `private_key` names the field: its label follows the display language of 1Password, and `op read` resolves each label exactly as it appears.
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
    use super::{KEY_LIFETIME, add_arguments, json, private_key_refs, wanted_keys};

    #[test]
    fn every_loaded_key_has_a_lifetime() {
        let arguments = add_arguments();
        let flag = arguments.iter().position(|argument| *argument == "-t");
        assert_eq!(flag.map(|index| arguments[index + 1]), Some(KEY_LIFETIME));
        assert_eq!(arguments.last(), Some(&"-"));
    }

    const ITEMS: &str = r#"
{
  "id": "item1",
  "vault": {"id": "vault1", "name": "Private"},
  "fields": [
    {"id": "public_key", "label": "public key", "value": "ssh-ed25519 AAAA"},
    {"id": "fingerprint", "value": "SHA256:x"}
  ]
}
{
  "id": "item2",
  "vault": {"id": "vault1"},
  "fields": [{"id": "public_key", "value": "ssh-ed25519 BBBB"}]
}
{"id": "item3", "vault": {"id": "vault2"}}
"#;

    #[test]
    fn only_wanted_items_are_referenced() {
        let items = json::parse_stream(ITEMS).unwrap();
        let wanted = wanted_keys("# signing\nssh-ed25519 AAAA comment\n\n");
        assert_eq!(
            private_key_refs(&items, &wanted),
            ["op://vault1/item1/private_key?ssh-format=openssh"]
        );
    }

    #[test]
    fn a_single_array_document_works_too() {
        let items = json::parse_stream(&format!("[{}]", ITEMS.replace("}\n{", "},\n{"))).unwrap();
        let wanted = wanted_keys("ssh-ed25519 BBBB");
        assert_eq!(
            private_key_refs(&items, &wanted),
            ["op://vault1/item2/private_key?ssh-format=openssh"]
        );
    }
}
