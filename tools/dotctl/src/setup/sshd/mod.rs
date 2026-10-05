//! Turns this box into an SSH target, so the mac and Linux machines can drive it.
//!
//! Windows ships OpenSSH.Server as an uninstalled capability, so none of this exists until something asks for it: the capability, the service, the host keys, `sshd_config`, the firewall rule and the login shell.
//!
//! Privilege model.
//! Every step here needs an admin token, while everything else chezmoi does writes into `$HOME` as you, so the elevation is scoped to this command rather than to the whole apply: `dotctl setup sshd` re-runs itself elevated through `assets/elevate.ps1` and prints what the elevated run logged.
//! Declining the prompt is a warning, never a failed apply: profile application restores managed files on a non-zero exit, and an unanswered UAC dialog must not roll back unrelated files.

mod config;
mod keys;

use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use serde_json::{Value, json};

use crate::{env, proc};

const HOST_HELPER: &str = include_str!("../../../assets/sshd-host.ps1");
const ELEVATE_HELPER: &str = include_str!("../../../assets/elevate.ps1");

const FIREWALL_RULE: &str = "OpenSSH-Server-In-TCP";

#[derive(Debug)]
pub struct Options {
    pub port: u16,
    pub default_shell: String,
    pub password_auth: bool,
    pub firewall_profile: String,
    pub authorized_key: String,
    /// Set on the elevated re-run, so the child does the work instead of asking for elevation again.
    pub elevated: bool,
}

pub fn run(options: &Options) -> Result<i32> {
    if !options.elevated {
        return elevate(options);
    }
    println!(">>> OpenSSH server");
    install(options)
}

/// Re-runs this command elevated and replays its log.
///
/// No attempt is made to detect an existing admin token: `Start-Process -Verb RunAs` from an already-elevated process raises no prompt, so the check would only buy a process hop, and every way of asking needs either unsafe code or a localized parse.
fn elevate(options: &Options) -> Result<i32> {
    let exe = std::env::current_exe().context("locating dotctl")?;
    let log = std::env::temp_dir().join("dotctl-sshd.log");
    let manifest = proc::write_temp(
        &json!({
            "exe": exe.to_string_lossy(),
            "log": log.to_string_lossy(),
            "arguments": arguments(options),
        })
        .to_string(),
        ".json",
    )?;
    let helper = proc::write_temp(ELEVATE_HELPER, ".ps1")?;
    // Start from an empty log so the tail below cannot replay a previous run.
    let _ = std::fs::remove_file(&log);

    println!(">>> sshd setup needs elevation - answer the UAC prompt");
    let mut cmd = env::command("pwsh");
    cmd.args(["-NoLogo", "-NoProfile", "-File"])
        .arg(helper.as_os_str())
        .arg("-Manifest")
        .arg(manifest.as_os_str());
    let mut child = cmd
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .context("starting the elevation helper")?;

    // The elevated window is hidden, so this is the only place its output appears.
    // Tailing it as it is written means a step that hangs or throws shows up here while it happens, rather than after the run is over.
    let mut tail = LogTail::new(&log);
    let status = loop {
        tail.drain();
        if let Some(status) = child.try_wait()? {
            break status;
        }
        std::thread::sleep(std::time::Duration::from_millis(150));
    };
    tail.drain();
    tail.flush();

    if !status.success() {
        let code = status.code().unwrap_or(1);
        eprintln!("sshd setup did not complete (exit {code})");
        if let Some(mut stderr) = child.stderr.take() {
            use std::io::Read;
            let mut text = String::new();
            let _ = stderr.read_to_string(&mut text);
            if !text.trim().is_empty() {
                eprintln!("{}", text.trim_end());
            }
        }
        eprintln!(
            "Re-run it with `dotctl setup sshd` once the cause is fixed; log: {}",
            log.display()
        );
    }
    // Never fail the apply over this: see the module comment.
    Ok(0)
}

/// Prints new complete lines of a file as they are appended.
#[derive(Debug)]
struct LogTail {
    path: PathBuf,
    offset: u64,
    partial: String,
}

impl LogTail {
    fn new(path: &Path) -> Self {
        Self {
            path: path.to_path_buf(),
            offset: 0,
            partial: String::new(),
        }
    }

    fn drain(&mut self) {
        use std::io::{Read, Seek, SeekFrom};

        let Ok(mut file) = std::fs::File::open(&self.path) else {
            return;
        };
        if file.seek(SeekFrom::Start(self.offset)).is_err() {
            return;
        }
        let mut chunk = Vec::new();
        if file.read_to_end(&mut chunk).is_err() || chunk.is_empty() {
            return;
        }
        self.offset += chunk.len() as u64;
        self.partial.push_str(&String::from_utf8_lossy(&chunk));

        // Hold back an unterminated line: the child may still be writing it.
        while let Some(end) = self.partial.find('\n') {
            let line: String = self.partial.drain(..=end).collect();
            println!("    {}", line.trim_end());
        }
    }

    fn flush(&mut self) {
        if !self.partial.trim().is_empty() {
            println!("    {}", self.partial.trim_end());
        }
        self.partial.clear();
    }
}

/// The command line the elevated child receives.
fn arguments(options: &Options) -> Vec<String> {
    vec![
        "setup".to_owned(),
        "sshd".to_owned(),
        "--port".to_owned(),
        options.port.to_string(),
        "--default-shell".to_owned(),
        options.default_shell.clone(),
        "--password-auth".to_owned(),
        options.password_auth.to_string(),
        "--firewall-profile".to_owned(),
        options.firewall_profile.clone(),
        "--authorized-key".to_owned(),
        options.authorized_key.clone(),
        "--elevated".to_owned(),
    ]
}

fn install(options: &Options) -> Result<i32> {
    let state = host(&["-Query"])?;

    let capability = state
        .pointer("/capability/name")
        .and_then(Value::as_str)
        .context("this Windows image offers no OpenSSH.Server capability")?
        .to_owned();
    let installed = state.pointer("/capability/state").and_then(Value::as_str) == Some("Installed");
    if installed {
        println!("    {capability} already installed");
    } else {
        println!("    installing {capability}");
    }

    // The first start is what generates the host keys and the default config, so it has to happen before anything edits that file.
    let applied = host_apply(&json!({
        "installCapability": if installed { Value::Null } else { Value::String(capability) },
        "serviceAutomatic": true,
        "startService": true,
    }))?;
    report_done(&applied);
    if applied.get("restartNeeded").and_then(Value::as_bool) == Some(true) {
        eprintln!("Windows wants a reboot to finish installing the capability");
    }

    let ssh_dir = program_data().join("ssh");
    let config_path = wait_for_config(&ssh_dir)?;

    // Pristine copy, taken once, before this tool has ever edited the file.
    let pristine = config_path.with_extension("orig");
    if !pristine.exists() {
        std::fs::copy(&config_path, &pristine)?;
    }

    let original = std::fs::read_to_string(&config_path)?;
    let updated = config::apply(
        &original,
        &[
            ("Port", options.port.to_string()),
            ("PubkeyAuthentication", "yes".to_owned()),
            (
                "PasswordAuthentication",
                if options.password_auth { "yes" } else { "no" }.to_owned(),
            ),
        ],
    );
    let mut config_changed = updated != original;
    if config_changed {
        // No BOM: sshd would read one as part of the first directive.
        std::fs::write(&config_path, updated.as_bytes())?;
        println!(
            "    sshd_config: port {}, pubkey auth on, password auth {}",
            options.port,
            if options.password_auth { "on" } else { "off" }
        );
    } else {
        println!("    sshd_config already says what it should");
    }

    // A run that edited the config and then failed before restarting leaves sshd serving what it read at startup - which is how a "password auth off" config ended up advertising password auth.
    // Trust the timestamps,
    // not just what this run did.
    if !config_changed && config_predates_service(&config_path, &state) {
        println!("    sshd_config is newer than the running sshd; restarting it");
        config_changed = true;
    }

    if !options.authorized_key.is_empty() {
        keys::authorize(&ssh_dir, &options.authorized_key, &original)?;
    }

    set_default_shell(&options.default_shell)?;

    let applied = host_apply(&json!({
        "firewall": {
            "name": FIREWALL_RULE,
            "displayName": "OpenSSH Server (sshd)",
            "port": options.port.to_string(),
            "profile": options.firewall_profile,
        },
        "restartService": config_changed,
    }))?;
    report_done(&applied);
    warn_about_network_profiles(&state, &options.firewall_profile);

    summarize(&state, &ssh_dir, options.port);
    Ok(0)
}

/// Runs the PowerShell effector and parses its JSON.
///
/// Windows PowerShell 5.1, deliberately: DISM is a 5.1 module, and Get-WindowsCapability under an elevated pwsh 7 fails with "Class not registered" instead of answering.
/// See the helper's own header.
fn host(args: &[&str]) -> Result<Value> {
    let helper = proc::write_temp(HOST_HELPER, ".ps1")?;
    let mut cmd = env::command("powershell.exe");
    cmd.args([
        "-NoLogo",
        "-NoProfile",
        "-ExecutionPolicy",
        "Bypass",
        "-File",
    ])
    .arg(helper.as_os_str())
    .args(args)
    // stdout is the JSON and nothing else; stderr carries the helper's progress and its errors, and goes straight through to ours - which under elevation is the log being tailed live.
    // A capability install pulls hundreds of megabytes from Windows Update, and saying so while it happens is the difference between waiting and wondering.
    .stderr(std::process::Stdio::inherit());
    let text = proc::capture_ok(&mut cmd).context("querying the Windows OpenSSH state")?;
    serde_json::from_str(text.trim())
        .with_context(|| format!("the host helper returned no JSON:\n{text}"))
}

fn host_apply(request: &Value) -> Result<Value> {
    let path = proc::write_temp(&request.to_string(), ".json")?;
    host(&["-Apply", &path.to_string_lossy()])
}

fn report_done(applied: &Value) {
    let Some(done) = applied.get("done").and_then(Value::as_array) else {
        return;
    };
    for entry in done.iter().filter_map(Value::as_str) {
        println!("    {entry}");
    }
}

/// sshd_config appears only once the service has started for the first time.
fn wait_for_config(ssh_dir: &Path) -> Result<PathBuf> {
    let path = ssh_dir.join("sshd_config");
    for _ in 0..40 {
        if path.is_file() {
            return Ok(path);
        }
        std::thread::sleep(std::time::Duration::from_millis(250));
    }
    bail!("sshd is running but {} never appeared", path.display())
}

/// `HKLM\SOFTWARE\OpenSSH\DefaultShell` decides what an inbound session lands in.
/// Without it that is cmd.exe, which makes the whole thing useless for agent work.
fn set_default_shell(choice: &str) -> Result<()> {
    const KEY: &str = r"HKLM\SOFTWARE\OpenSSH";

    if choice != "pwsh" {
        clear_default_shell(KEY);
        println!("    login shell: Windows default (cmd.exe)");
        return Ok(());
    }

    let Some(shell) = which_pwsh() else {
        eprintln!("pwsh is available only as the Store build, which sshd cannot exec.");
        eprintln!("Falling back to Windows' default shell. To get pwsh sessions:");
        eprintln!("  winget install Microsoft.PowerShell   # the MSI build, not the Store one");
        eprintln!("  just sshd");
        // Clear whatever an earlier run wrote: a DefaultShell that cannot be exec'd is worse than none, because every login fails with "exec request failed on channel 0" and nothing says why.
        clear_default_shell(KEY);
        println!("    login shell: Windows default (cmd.exe)");
        return Ok(());
    };
    let shell = shell.to_string_lossy().into_owned();
    reg_set(KEY, "DefaultShell", &shell)?;
    // sshd passes this when a client runs `ssh host <command>`: cmd.exe takes /c, pwsh takes -c.
    reg_set(KEY, "DefaultShellCommandOption", "-c")?;
    println!("    login shell: {shell}");
    Ok(())
}

/// Absent already is the desired state, so a failure here is not one.
fn clear_default_shell(key: &str) {
    for value in ["DefaultShell", "DefaultShellCommandOption"] {
        let mut cmd = env::command("reg.exe");
        cmd.args(["delete", key, "/v", value, "/f"]);
        let _ = proc::capture(&mut cmd);
    }
}

fn reg_set(key: &str, value: &str, data: &str) -> Result<()> {
    let mut cmd = env::command("reg.exe");
    cmd.args(["add", key, "/v", value, "/t", "REG_SZ", "/d", data, "/f"]);
    proc::capture_ok(&mut cmd).with_context(|| format!("setting {key}\\{value}"))?;
    Ok(())
}

/// Where DefaultShell should point, or None when nothing usable exists.
///
/// Not "the first pwsh.exe on PATH": that is a mise shim here, and a shim as the login shell means every inbound session runs mise's resolution first.
/// Observed once: an ssh login spent its first seconds reinstalling a Rust toolchain and then failed with access denied.
///
/// Not the Store build either, by either of its paths.
/// sshd cannot exec an MSIX-packaged binary - neither the App Execution Alias, which is a reparse point needing app-model resolution, nor the versioned path under WindowsApps.
/// The client sees only "exec request failed on channel 0",
/// with nothing in the server log to explain it.
/// The MSI build is what this repo's own prerequisites list, and it is the one that works.
fn which_pwsh() -> Option<PathBuf> {
    if let Some(msi) = std::env::var_os("ProgramFiles")
        .map(|dir| PathBuf::from(dir).join("PowerShell/7/pwsh.exe"))
        .filter(|path| path.is_file())
    {
        return Some(msi);
    }

    std::env::split_paths(&env::augmented_path())
        .filter(|dir| !is_shim_dir(dir) && !is_packaged_dir(dir))
        .map(|dir| dir.join("pwsh.exe"))
        .find(|path| path.is_file())
}

/// MSIX-packaged binaries and their aliases, which sshd cannot exec.
fn is_packaged_dir(dir: &Path) -> bool {
    dir.components()
        .any(|part| part.as_os_str().eq_ignore_ascii_case("WindowsApps"))
}

/// mise's shim directory, whose entries are launchers rather than binaries.
fn is_shim_dir(dir: &Path) -> bool {
    dir.components().any(|part| part.as_os_str() == "shims")
}

/// A rule scoped to Private does nothing on a network Windows calls Public,
/// and that is invisible until a connection times out.
fn warn_about_network_profiles(state: &Value, profile: &str) {
    if profile.eq_ignore_ascii_case("Any") {
        return;
    }
    let Some(categories) = state.get("networks").and_then(Value::as_array) else {
        return;
    };
    let active: Vec<&str> = categories.iter().filter_map(Value::as_str).collect();
    if active.is_empty() {
        return;
    }
    let allowed: Vec<&str> = profile.split(',').map(str::trim).collect();
    if active.iter().any(|category| allowed.contains(category)) {
        return;
    }
    eprintln!(
        "active networks are {}, which the {profile} rule does not cover.",
        active.join(", ")
    );
    eprintln!("Set the LAN to Private, or widen [sshd].firewall_profile.");
}

/// The fingerprint is printed so the first connection from the other side can be checked against something seen here, rather than trusted blind.
fn summarize(state: &Value, ssh_dir: &Path, port: u16) {
    let keygen = std::path::Path::new(&std::env::var_os("SystemRoot").unwrap_or_default())
        .join("System32")
        .join("OpenSSH")
        .join("ssh-keygen.exe");
    let host_key = ssh_dir.join("ssh_host_ed25519_key.pub");
    if keygen.is_file() && host_key.is_file() {
        let mut cmd = std::process::Command::new(&keygen);
        cmd.arg("-l").arg("-f").arg(&host_key);
        if let Ok(out) = proc::capture(&mut cmd) {
            println!("    host key: {}", out.stdout_text().trim());
        }
    }

    let user = std::env::var("USERNAME").unwrap_or_default();
    let host = std::env::var("COMPUTERNAME").unwrap_or_default();
    println!("    reachable as: ssh {user}@{host} -p {port}");
    if let Some(addresses) = state.get("addresses").and_then(Value::as_array) {
        for address in addresses.iter().filter_map(Value::as_str) {
            println!("                  ssh {user}@{address} -p {port}");
        }
    }
}

/// True when the config on disk was written after sshd started, so the running process cannot have read it.
fn config_predates_service(config_path: &Path, state: &Value) -> bool {
    let Some(started) = state
        .pointer("/service/startedAt")
        .and_then(Value::as_str)
        .and_then(|text| text.parse::<jiff::Timestamp>().ok())
    else {
        return false;
    };
    let Ok(modified) = std::fs::metadata(config_path).and_then(|meta| meta.modified()) else {
        return false;
    };
    jiff::Timestamp::try_from(modified).is_ok_and(|written| written > started)
}

fn program_data() -> PathBuf {
    std::env::var_os("ProgramData")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(r"C:\ProgramData"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn options() -> Options {
        Options {
            port: 2222,
            default_shell: "pwsh".to_owned(),
            password_auth: true,
            firewall_profile: "Domain,Private".to_owned(),
            authorized_key: "ssh-ed25519 AAAA a comment".to_owned(),
            elevated: false,
        }
    }

    /// The elevated child re-parses this, so a value that does not round trip costs a UAC prompt to discover.
    #[test]
    fn the_elevated_child_gets_every_knob_and_the_flag() {
        let args = arguments(&options());
        assert_eq!(&args[..2], ["setup", "sshd"]);
        for (flag, value) in [
            ("--port", "2222"),
            ("--default-shell", "pwsh"),
            ("--password-auth", "true"),
            ("--firewall-profile", "Domain,Private"),
            ("--authorized-key", "ssh-ed25519 AAAA a comment"),
        ] {
            let at = args.iter().position(|arg| arg == flag).expect(flag);
            assert_eq!(args[at + 1], value, "value after {flag}");
        }
        assert!(args.contains(&"--elevated".to_owned()));
    }

    /// Without this the child would elevate again, forever.
    #[test]
    fn the_elevated_flag_is_the_last_word() {
        assert_eq!(arguments(&options()).last().unwrap(), "--elevated");
    }
}
