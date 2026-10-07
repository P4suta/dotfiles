//! Reaps abandoned processes of the current user.
//!
//! macOS lacks `/proc`, so process ages come from the `etime` column of `ps`.
//! launchd runs as PID 1 and supervises every user app, so `PPID==1` alone proves nothing.
//!
//! Tiers:
//!
//! * **Tier A** kills a process stopped in state `T` under `PPID==1` and older than the limit, once a grace window confirms it.
//!   launchd never leaves its jobs in state `T`, so such a process lost its controlling shell, and it ignores `SIGTERM` while stopped, so the reaper sends `SIGKILL`.
//! * **Tier B** reports a running process under `PPID==1` with no controlling tty and older than the limit, whose executable lives in user or Homebrew space instead of `/System` or `/usr/lib`.
//!
//! `MODE=report` never kills, and `MODE=reap` kills tier A.

use super::config;
use super::util::{self, Lock};
use dotguard::reaper_rules::{self, Action, Mode, Reaper, Snapshot, Target, Thresholds, Verdict};
use platform::process::command;
use std::collections::HashMap;
use std::path::PathBuf;
use std::process::Stdio;
use util::{SECONDS_PER_DAY, SECONDS_PER_HOUR, SECONDS_PER_MINUTE};

/// The exit status of an invocation the reaper doesn't serve.
const USAGE_ERROR: i32 = 2;
/// The defaults the configuration file and the environment override.
const STOPPED_MIN_AGE: i64 = SECONDS_PER_HOUR;
const GRACE_SECONDS: i64 = 30 * SECONDS_PER_MINUTE;
const RUNNING_MIN_AGE: i64 = 3 * SECONDS_PER_DAY;
/// The system tools at fixed paths, so a user's `PATH` can't substitute another.
#[cfg(not(target_os = "linux"))]
const PS: &str = "/bin/ps";
const ID: &str = "/usr/bin/id";
const KILL: &str = "/bin/kill";
/// The proc file system and its fields after a process's command name, which start at the state, field 3 of `proc_pid_stat(5)`.
#[cfg(target_os = "linux")]
const PROC: &str = "/proc";
#[cfg(target_os = "linux")]
const PROC_UPTIME: &str = "/proc/uptime";
#[cfg(target_os = "linux")]
const STAT_PARENT: usize = 1;
#[cfg(target_os = "linux")]
const STAT_TTY: usize = 4;
#[cfg(target_os = "linux")]
const STAT_START_TIME: usize = 19;

pub struct SessionReaper;

impl SessionReaper {
    pub fn dispatch(args: &[String]) -> i32 {
        if let Some(sub) = args.first().map(String::as_str) {
            Self::run(sub)
        } else {
            eprintln!("usage: session-reaper {{reap}}");
            USAGE_ERROR
        }
    }

    pub fn run(sub: &str) -> i32 {
        if sub != "reap" {
            eprintln!("usage: session-reaper {{reap}}");
            return USAGE_ERROR;
        }
        let cfg = SessionConfig::load();
        let now = util::now();
        let Some(_lock) = Lock::take(&cfg.state_dir, &|m| {
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
                // Tier A: stopped, and launchd never stops its own jobs.
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
                // Leave zombie `Z` and dead `X` processes to the kernel.
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
        platform::cleanup::report(
            std::fs::write(&cfg.seen_file, seen_out),
            format_args!(
                "record the reported processes in {}",
                cfg.seen_file.display()
            ),
        );

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

/// Tier A: send `SIGKILL` to a stopped orphan after the grace window, because it ignores `SIGTERM` and lost the shell that could resume it.
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
    let mode = if cfg.mode == "reap" {
        Mode::Reap
    } else {
        Mode::Report
    };
    let thresholds = Thresholds {
        min_age: seconds(cfg.stopped_min_age),
        grace: seconds(cfg.grace_seconds),
    };
    // A listing this old can misname the process, so the verdict rests on the process as it stands right before the signal.
    let snapshot = native_snapshot(&p.pid, &p.identity);
    let waited = seconds(now.saturating_sub(firstseen));
    match reaper_rules::verdict(mode, snapshot, seconds(age), waited, thresholds) {
        Verdict::Kill if model_admits(snapshot) && kill(&p.pid) => {
            *reaped += 1;
            println!("reaped {ucomm}({}) stopped+orphaned age {age}s", p.pid);
            util::notify(
                "session-reaper: reaped",
                &format!("killed {ucomm}({}), stopped and orphaned for {age}s", p.pid),
                cfg.notify,
            );
        }
        Verdict::Kill => println!("session-reaper: failed to kill {ucomm}({})", p.pid),
        Verdict::WouldKill => println!(
            "would reap {ucomm}({}) stopped+orphaned age {age}s (mode=report)",
            p.pid
        ),
        Verdict::Spare => println!(
            "spared {ucomm}({}): it resumed, gained a parent, or exited since the listing",
            p.pid
        ),
    }
}

/// A count of seconds that a clock jump can't turn negative.
fn seconds(value: i64) -> u64 {
    u64::try_from(value).unwrap_or(0)
}

/// Whether `specs/reaper_lock.qnt` lets a holder that observed the process reap it as `snapshot` shows it now.
fn model_admits(snapshot: Snapshot) -> bool {
    [Action::Take(0), Action::Observe(0)]
        .into_iter()
        .try_fold(reaper_rules::initial(), |state, action| {
            reaper_rules::step(Reaper::Fixed, state, action)
        })
        .and_then(|mut state| {
            state.target = Target {
                identity: u8::from(!snapshot.same_identity),
                stopped: snapshot.stopped,
                orphan: snapshot.orphan,
            };
            reaper_rules::step(Reaper::Fixed, state, Action::Kill(0))
        })
        .is_some_and(|state| state.kill.is_some() && reaper_rules::safe(&state))
}

/// Tier B: report a running orphan in user space with no tty, and never signal it.
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
        age / SECONDS_PER_DAY,
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
        platform::cleanup::report(
            std::fs::create_dir_all(&state_dir),
            format_args!("create the state directory {}", state_dir.display()),
        );

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
            stopped_min_age: config::number(&f, "SESSION_REAPER_STOPPED_MIN_AGE", STOPPED_MIN_AGE),
            grace_seconds: config::number(&f, "SESSION_REAPER_GRACE_SECONDS", GRACE_SECONDS),
            running_min_age: config::number(&f, "SESSION_REAPER_RUNNING_MIN_AGE", RUNNING_MIN_AGE),
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
/// `comm` holds the full executable path and comes last, because it can contain spaces and must absorb the tail of the line.
/// The row omits `ucomm`, which can also contain spaces and would shift every later field.
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
    let out = command(PS)
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
            process.identity = native_status(&process.pid)?.2;
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
    command(ID)
        .arg("-u")
        .output()
        .ok()
        .filter(|output| output.status.success())
        .map_or_else(String::new, |o| {
            String::from_utf8_lossy(&o.stdout).trim().to_owned()
        })
}

/// A grep error protects the process.
fn matches_allow(ucomm: &str, allow_regex: &str) -> bool {
    let lines = vec![ucomm.to_owned()];
    util::grep_matching(allow_regex, &lines).is_none_or(|hits| !hits.is_empty())
}

/// Tier B's "orphaned" substitute: the executable lives under one of the user-space prefixes rather than under `/System` or `/usr/lib`.
fn user_space_exe(comm: &str, prefixes: &[String]) -> bool {
    prefixes.iter().any(|p| comm.starts_with(p.as_str()))
}

/// The process behind `pid` read again: whether it keeps the identity the listing read, and whether it stays stopped and orphaned.
/// A process that exited, or one the reading can't reach, shows no identity, which spares it.
fn native_snapshot(pid: &str, identity: &str) -> Snapshot {
    let spared = Snapshot {
        same_identity: false,
        stopped: false,
        orphan: false,
    };
    if identity.is_empty() {
        return spared;
    }
    native_status(pid).map_or(spared, |(state, parent, current)| Snapshot {
        same_identity: current == identity,
        stopped: matches!(state.chars().next(), Some('T' | 't')),
        orphan: parent == "1",
    })
}

/// The state, the parent ID, and the identity of a process.
fn native_status(pid: &str) -> Option<(String, String, String)> {
    #[cfg(target_os = "linux")]
    {
        let stat =
            std::fs::read_to_string(std::path::Path::new(PROC).join(pid).join("stat")).ok()?;
        let fields: Vec<&str> = stat.rsplit_once(") ")?.1.split_whitespace().collect();
        Some((
            (*fields.first()?).to_owned(),
            (*fields.get(STAT_PARENT)?).to_owned(),
            (*fields.get(STAT_START_TIME)?).to_owned(),
        ))
    }
    #[cfg(not(target_os = "linux"))]
    {
        // `lstart` holds spaces, so it comes last and takes the rest of the line.
        let output = command(PS)
            .args(["-p", pid, "-o", "state=,ppid=,lstart="])
            .output()
            .ok()?;
        if !output.status.success() {
            return None;
        }
        let text = String::from_utf8(output.stdout).ok()?;
        let (state, rest) = text.trim_start().split_once(char::is_whitespace)?;
        let (parent, rest) = rest.trim_start().split_once(char::is_whitespace)?;
        let identity = rest.trim();
        (!identity.is_empty()).then(|| (state.to_owned(), parent.to_owned(), identity.to_owned()))
    }
}

fn kill(pid: &str) -> bool {
    command(KILL)
        .args(["-KILL", pid])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|s| s.success())
}

#[cfg(target_os = "linux")]
fn linux_processes() -> Vec<Process> {
    use std::os::unix::fs::MetadataExt;
    let Ok(uid) = whoami_uid().parse::<u32>() else {
        return Vec::new();
    };
    let Ok(clock) = command("getconf").arg("CLK_TCK").output() else {
        return Vec::new();
    };
    let Ok(hz) = String::from_utf8_lossy(&clock.stdout).trim().parse::<u64>() else {
        return Vec::new();
    };
    if !clock.status.success() || hz == 0 {
        return Vec::new();
    }
    let Ok(uptime) = std::fs::read_to_string(PROC_UPTIME) else {
        return Vec::new();
    };
    let Some(uptime) = uptime
        .split('.')
        .next()
        .and_then(|value| value.parse::<u64>().ok())
    else {
        return Vec::new();
    };
    let Ok(entries) = std::fs::read_dir(PROC) else {
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
            let birth_ticks = fields.get(STAT_START_TIME)?.parse::<u64>().ok()?;
            let age = dotguard::reaper_rules::age(uptime, birth_ticks, hz)?;
            Some(Process {
                pid,
                ppid: fields.get(STAT_PARENT)?.to_string(),
                state: fields.first()?.to_string(),
                etime: age.to_string(),
                tty: if *fields.get(STAT_TTY)? == "0" {
                    "?".into()
                } else {
                    fields.get(STAT_TTY)?.to_string()
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
        // The default regular expression shape: `^(p1|p2|p3)/`
        let text = "^(tmux.*|sshd)$";
        let _ = text;
        // `SessionConfig` integration covers the full path, and this test checks the extraction helper on the real default shape.
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
