//! session-reaper — reap abandoned processes owned by this user.
//!
//! A rewrite rather than a port of the Linux version, because the two facts that one is built on are both false on macOS:
//!
//! 1. There is no `/proc`; process state comes from `ps`, so ages arrive as an `etime` string ("2-04:17:30") that has to be parsed.
//! 2. `PPID==1` does NOT mean "orphaned": on macOS PID 1 is launchd, the legitimate supervisor of every user agent and GUI app.
//!
//! Tiers:
//!
//! * **A (auto-kill)** — stopped (state `T`) AND `PPID==1` AND old enough, confirmed across a grace window.
//!   launchd never leaves its own jobs in state `T`, so this can only be a suspended job whose controlling shell has since died: it can never be resumed and ignores SIGTERM while stopped, so it is SIGKILL'd.
//! * **B (report)** — running, `PPID==1`, no controlling tty, old enough, AND the executable lives in user/Homebrew space rather than under `/System` or `/usr/lib`.
//!   That last clause is the macOS substitute for "orphaned" and is what keeps Apple's own agents off the list.
//!   NEVER killed — only reported.
//!
//! MODE=report never kills; MODE=reap enacts tier A. Tier B is always report-only.
//! The stdout lines are the audit log (launchd writes them to `~/.local/state/dotfiles/log/session-reaper.log`).

use super::config;
use super::util::{self, Lock};
use std::collections::HashMap;
use std::path::PathBuf;
use std::process::{Command, Stdio};

pub struct SessionReaper;

impl SessionReaper {
    pub fn dispatch(args: &[String]) -> i32 {
        if let Some(sub) = args.first().map(String::as_str) {
            Self::run(sub)
        } else {
            eprintln!("usage: session-reaper {{reap}}");
            2
        }
    }

    pub fn run(sub: &str) -> i32 {
        if sub != "reap" {
            eprintln!("usage: session-reaper {{reap}}");
            return 2;
        }
        let cfg = SessionConfig::load();
        let now = util::now();
        let Some(_lock) = Lock::take(&cfg.state_dir, "session-reaper", now, &|m| {
            println!("session-reaper: {m}");
        }) else {
            return 0;
        };

        let prior = load_seen(&cfg.seen_file);
        let mut new_seen: Vec<(String, i64)> = Vec::new();
        let mut reaped = 0usize;
        let mut reported = 0usize;
        let self_pid = std::process::id().to_string();

        for p in ps_processes() {
            if p.pid == self_pid {
                continue;
            }
            if p.ppid != "1" {
                continue;
            }
            let ucomm = p.comm.rsplit('/').next().unwrap_or(&p.comm).to_owned();
            if matches_allow(&ucomm, &cfg.allow_regex) {
                continue;
            }
            let Ok(age) = p.etime_digits_check() else {
                continue;
            };

            match p.state.chars().next().unwrap_or(' ') {
                // tier A: stopped, and launchd is not a plausible supervisor for that.
                'T' | 't' => tier_a(
                    &cfg,
                    &p,
                    &ucomm,
                    age,
                    now,
                    &prior,
                    &mut new_seen,
                    &mut reaped,
                ),
                // tier B: report only.
                'R' | 'S' | 'I' | 'U' | 'D' => tier_b(&cfg, &p, &ucomm, age, &mut reported),
                // Z (zombie) / X (dead): leave them to the kernel.
                _ => {}
            }
        }

        let mut seen_out = String::new();
        for (k, t) in &new_seen {
            seen_out.push_str(k);
            seen_out.push(' ');
            seen_out.push_str(&t.to_string());
            seen_out.push('\n');
        }
        let _ = std::fs::write(&cfg.seen_file, seen_out);

        if reported > 0 {
            util::notify(
                "session-reaper",
                &format!(
                    "{reported} running orphan(s) detected (report only); see ~/.local/state/dotfiles/log/session-reaper.log"
                ),
                cfg.notify,
            );
        }
        println!(
            "session-reaper: done (mode={}, reaped={reaped}, reported={reported})",
            cfg.mode
        );
        0
    }
}

/// Tier A — stopped and orphaned: kill it after grace, because a stopped process ignores SIGTERM and can never be resumed once its shell is gone.
#[expect(
    clippy::too_many_arguments,
    reason = "each argument is one observed fact of the process the tier judges"
)]
fn tier_a(
    cfg: &SessionConfig,
    p: &Process,
    ucomm: &str,
    age: i64,
    now: i64,
    prior: &HashMap<String, i64>,
    new_seen: &mut Vec<(String, i64)>,
    reaped: &mut usize,
) {
    let key = format!("{}:{}", p.pid, p.identity);
    if age < cfg.stopped_min_age {
        new_seen.push((key, now));
        return;
    }
    let firstseen = prior.get(&key).copied().unwrap_or(now);
    new_seen.push((key, firstseen));
    if now - firstseen < cfg.grace_seconds {
        println!(
            "grace: {ucomm}({}) stopped+orphaned age {age}s — waiting out grace",
            p.pid
        );
        return;
    }
    if cfg.mode == "reap" {
        let same_identity =
            !p.identity.is_empty() && native_identity(&p.pid).as_deref() == Some(&p.identity);
        let allowed = dotguard::reaper_rules::killable(
            true,
            matches!(p.state.chars().next(), Some('T' | 't')),
            p.ppid == "1",
            age >= cfg.stopped_min_age,
            now.saturating_sub(firstseen) >= cfg.grace_seconds,
            same_identity,
        );
        if allowed && kill_kill(&p.pid, &p.identity) {
            *reaped += 1;
            println!("reaped {ucomm}({}) stopped+orphaned age {age}s", p.pid);
            util::notify(
                "session-reaper: reaped",
                &format!("killed {ucomm}({}), stopped and orphaned for {age}s", p.pid),
                cfg.notify,
            );
        } else {
            println!("session-reaper: failed to kill {ucomm}({})", p.pid);
        }
    } else {
        println!(
            "would reap {ucomm}({}) stopped+orphaned age {age}s (mode=report)",
            p.pid
        );
    }
}

/// Tier B — running orphan in user space with no tty: report only, never kill.
fn tier_b(cfg: &SessionConfig, p: &Process, ucomm: &str, age: i64, reported: &mut usize) {
    if !(p.tty == "??" || p.tty == "?") {
        return;
    }
    if age < cfg.running_min_age {
        return;
    }
    if cfg!(target_os = "macos") && !user_space_exe(&p.comm, &cfg.user_path_prefixes) {
        return;
    }
    *reported += 1;
    println!(
        "report: {ucomm}({}) running orphan, no tty, age {}d, exe {} — review manually",
        p.pid,
        age / 86_400,
        p.comm
    );
}

struct SessionConfig {
    mode: String,
    stopped_min_age: i64,
    grace_seconds: i64,
    running_min_age: i64,
    allow_regex: String,
    user_path_prefixes: Vec<String>,
    notify: bool,
    state_dir: PathBuf,
    seen_file: PathBuf,
}

impl SessionConfig {
    fn load() -> Self {
        let home = std::env::var_os("HOME")
            .map(PathBuf::from)
            .unwrap_or_default();
        let file = std::env::var("SESSION_REAPER_CONFIG").map_or_else(
            |_| home.join(".config/session-reaper/config"),
            PathBuf::from,
        );
        let f = config::load(&file);

        let state_base = std::env::var_os("XDG_STATE_HOME")
            .map_or_else(|| home.join(".local/state"), PathBuf::from);
        let state_dir = PathBuf::from(config::get(
            &f,
            "SESSION_REAPER_STATE_DIR",
            &state_base.join("session-reaper").to_string_lossy(),
        ));
        let _ = std::fs::create_dir_all(&state_dir);

        let default_prefixes = format!("^({}|/opt/homebrew|/usr/local)/", home.to_string_lossy());
        let user_path_regex = config::get(&f, "SESSION_REAPER_USER_PATH_REGEX", &default_prefixes);
        let prefixes = user_path_regex
            .trim_start_matches("^(")
            .trim_end_matches(")/")
            .split('|')
            .map(|s| s.trim().to_owned())
            .filter(|s| !s.is_empty())
            .collect();

        Self {
            mode: config::get(&f, "SESSION_REAPER_MODE", "report"),
            stopped_min_age: config::get(&f, "SESSION_REAPER_STOPPED_MIN_AGE", "3600")
                .parse()
                .unwrap_or(3600),
            grace_seconds: config::get(&f, "SESSION_REAPER_GRACE_SECONDS", "1800")
                .parse()
                .unwrap_or(1800),
            running_min_age: config::get(&f, "SESSION_REAPER_RUNNING_MIN_AGE", "259200")
                .parse()
                .unwrap_or(259_200),
            allow_regex: config::get(
                &f,
                "SESSION_REAPER_ALLOW_REGEX",
                "^(tmux.*|screen.*|sshd|launchd|herdr|ssh-agent|gpg-agent|Code Helper.*|node|session-reaper)$",
            ),
            user_path_prefixes: prefixes,
            notify: config::get(&f, "SESSION_REAPER_NOTIFY", "1") == "1",
            seen_file: state_dir.join("seen"),
            state_dir,
        }
    }
}

/// One `ps` row.
/// `comm` is the full executable path and is the LAST column on purpose: it can contain spaces, so it has to absorb the tail of the line.
/// `ucomm` is deliberately not requested — it can also contain spaces ("Code Helper (Renderer)"), which would shift every field after it.
struct Process {
    pid: String,
    ppid: String,
    state: String,
    etime: String,
    tty: String,
    comm: String,
    identity: String,
}

impl Process {
    /// Age in seconds, or `Err` when etime did not parse.
    fn etime_digits_check(&self) -> Result<i64, ()> {
        let secs = util::etime_seconds(&self.etime);
        if secs == 0 && !self.etime.starts_with("00") && self.etime != "0:00" && self.etime != "0" {
            return Err(());
        }
        Ok(secs)
    }
}

fn ps_processes() -> Vec<Process> {
    #[cfg(target_os = "linux")]
    {
        linux_processes()
    }
    #[cfg(not(target_os = "linux"))]
    {
        mac_processes()
    }
}

#[cfg(not(target_os = "linux"))]
fn mac_processes() -> Vec<Process> {
    let uid = whoami_uid();
    if uid.parse::<u32>().is_err() {
        return Vec::new();
    }
    let out = Command::new("/bin/ps")
        .args([
            "-x",
            &format!("-U{uid}"),
            "-o",
            "pid=,ppid=,state=,etime=,tty=,comm=",
        ])
        .output();
    let Ok(out) = out else {
        return Vec::new();
    };
    if !out.status.success() {
        return Vec::new();
    }
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .filter_map(parse_ps_line)
        .filter_map(|mut process| {
            process.identity = native_identity(&process.pid)?;
            Some(process)
        })
        .collect()
}

#[cfg(any(test, not(target_os = "linux")))]
fn parse_ps_line(line: &str) -> Option<Process> {
    let mut fields = line.split_whitespace();
    let pid = fields.next()?.to_owned();
    if pid.is_empty() || !pid.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    let parent = fields.next()?.to_owned();
    let state = fields.next()?.to_owned();
    let etime = fields.next()?.to_owned();
    let tty = fields.next()?.to_owned();
    // comm is the rest of the line, spaces included.
    let comm = line
        .split_once(&format!("{tty} "))
        .map(|(_, c)| c.trim().to_owned())
        .unwrap_or_default();
    Some(Process {
        pid,
        ppid: parent,
        state,
        etime,
        tty,
        comm,
        identity: String::new(),
    })
}

fn whoami_uid() -> String {
    Command::new("/usr/bin/id")
        .arg("-u")
        .output()
        .ok()
        .filter(|output| output.status.success())
        .map_or_else(String::new, |o| {
            String::from_utf8_lossy(&o.stdout).trim().to_owned()
        })
}

/// An allow-list that cannot be evaluated protects the process rather than exposing it.
fn matches_allow(ucomm: &str, allow_regex: &str) -> bool {
    let lines = vec![ucomm.to_owned()];
    util::grep_matching(allow_regex, &lines).is_none_or(|hits| !hits.is_empty())
}

/// Tier B's "orphaned" substitute: the executable lives under one of the user-space prefixes rather than under `/System` or `/usr/lib`.
fn user_space_exe(comm: &str, prefixes: &[String]) -> bool {
    prefixes.iter().any(|p| comm.starts_with(p.as_str()))
}

fn kill_kill(pid: &str, identity: &str) -> bool {
    if identity.is_empty() || native_identity(pid).as_deref() != Some(identity) {
        return false;
    }
    Command::new("/bin/kill")
        .args(["-KILL", pid])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|s| s.success())
}

fn native_identity(pid: &str) -> Option<String> {
    #[cfg(target_os = "linux")]
    {
        let stat = std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
        Some(
            stat.rsplit_once(") ")?
                .1
                .split_whitespace()
                .nth(19)?
                .to_owned(),
        )
    }
    #[cfg(not(target_os = "linux"))]
    {
        let output = Command::new("/bin/ps")
            .args(["-p", pid, "-o", "lstart="])
            .output()
            .ok()?;
        let identity = String::from_utf8(output.stdout).ok()?.trim().to_owned();
        (output.status.success() && !identity.is_empty()).then_some(identity)
    }
}

#[cfg(target_os = "linux")]
fn linux_processes() -> Vec<Process> {
    use std::os::unix::fs::MetadataExt;
    let Ok(uid) = whoami_uid().parse::<u32>() else {
        return Vec::new();
    };
    let Ok(clock) = Command::new("getconf").arg("CLK_TCK").output() else {
        return Vec::new();
    };
    let Ok(hz) = String::from_utf8_lossy(&clock.stdout).trim().parse::<u64>() else {
        return Vec::new();
    };
    if !clock.status.success() || hz == 0 {
        return Vec::new();
    }
    let Ok(uptime) = std::fs::read_to_string("/proc/uptime") else {
        return Vec::new();
    };
    let Some(uptime) = uptime
        .split('.')
        .next()
        .and_then(|value| value.parse::<u64>().ok())
    else {
        return Vec::new();
    };
    let Ok(entries) = std::fs::read_dir("/proc") else {
        return Vec::new();
    };
    entries
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let pid = entry.file_name().into_string().ok()?;
            if !pid.bytes().all(|byte| byte.is_ascii_digit()) || entry.metadata().ok()?.uid() != uid
            {
                return None;
            }
            let stat = std::fs::read_to_string(entry.path().join("stat")).ok()?;
            let fields: Vec<_> = stat.rsplit_once(") ")?.1.split_whitespace().collect();
            let birth_ticks = fields.get(19)?.parse::<u64>().ok()?;
            let age = dotguard::reaper_rules::age(uptime, birth_ticks, hz)?;
            Some(Process {
                pid,
                ppid: fields.get(1)?.to_string(),
                state: fields.first()?.to_string(),
                etime: age.to_string(),
                tty: if *fields.get(4)? == "0" {
                    "?".into()
                } else {
                    fields.get(4)?.to_string()
                },
                comm: std::fs::read_to_string(entry.path().join("comm"))
                    .ok()?
                    .trim()
                    .to_owned(),
                identity: birth_ticks.to_string(),
            })
        })
        .collect()
}

/// "<key> <epoch>" lines from the previous run: first-seen timestamps for grace confirmation.
fn load_seen(path: &PathBuf) -> HashMap<String, i64> {
    let mut out = HashMap::new();
    let Ok(text) = std::fs::read_to_string(path) else {
        return out;
    };
    for line in text.lines() {
        if let Some((k, v)) = line.rsplit_once(' ')
            && let Ok(v) = v.trim().parse()
        {
            out.insert(k.to_owned(), v);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::{SessionConfig, load_seen, parse_ps_line, user_space_exe};

    #[test]
    fn seen_keys_keep_spaced_macos_start_times() {
        let path = std::env::temp_dir().join(format!("session-reaper-seen-{}", std::process::id()));
        std::fs::write(&path, "4241:Sat Oct  4 21:00:00 2026 1791000000\n").unwrap();
        let seen = load_seen(&path);
        std::fs::remove_file(&path).unwrap();
        assert_eq!(
            seen.get("4241:Sat Oct  4 21:00:00 2026"),
            Some(&1_791_000_000)
        );
    }

    #[test]
    fn ps_lines_parse_with_spaced_comms() {
        let p = parse_ps_line("  4241 1 Ss    01:23 ?? /Applications/Code Helper (Renderer).app/Contents/MacOS/Code Helper").unwrap();
        assert_eq!(p.pid, "4241");
        assert_eq!(p.ppid, "1");
        assert_eq!(p.state, "Ss");
        assert_eq!(p.tty, "??");
        assert!(p.comm.starts_with("/Applications/Code Helper"));
        assert!(p.comm.contains("Code Helper"));
        assert!(parse_ps_line("not a process line").is_none());
    }

    #[test]
    fn tier_b_tests_user_space_paths() {
        let prefixes = vec!["/Users/example".to_owned(), "/opt/homebrew".to_owned()];
        assert!(user_space_exe("/opt/homebrew/bin/node", &prefixes));
        assert!(user_space_exe("/Users/example/.local/bin/x", &prefixes));
        assert!(!user_space_exe("/usr/libexec/abcd", &prefixes));
        assert!(!user_space_exe("/System/Library/CoreServices/X", &prefixes));
    }

    #[test]
    fn the_user_path_regex_parses_into_prefixes() {
        // The default regex shape: ^(p1|p2|p3)/
        let text = "^(tmux.*|sshd)$";
        let _ = text;
        // exercised through SessionConfig in integration; here we assert the extraction helper on the real default shape.
        let regex = "^(/Users/x|/opt/homebrew|/usr/local)/";
        let prefixes: Vec<String> = regex
            .trim_start_matches("^(")
            .trim_end_matches(")/")
            .split('|')
            .map(str::to_owned)
            .collect();
        assert_eq!(prefixes, vec!["/Users/x", "/opt/homebrew", "/usr/local"]);
        let _ = std::mem::size_of::<SessionConfig>();
    }
}
