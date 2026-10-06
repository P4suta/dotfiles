//! Stops idle Docker dev containers and cleans up abandoned ones.
//!
//! A dev container carries the label `com.docker.compose.service=dev`, and the label `dev-reaper.keep=true` opts any container out.
//!
//! Any of three signals vetoes a stop: recent `exec` or `attach` activity, a running build or server process, or an instantaneous CPU reading over the threshold.
//! A per-container state file accumulates idle time across runs, so a container stops only after `DEV_REAPER_IDLE_STOP_SECONDS` of continuous quiet.

use super::config;
use super::util::{self, Lock};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

pub struct DevReaper;

impl DevReaper {
    pub fn dispatch(args: &[String]) -> i32 {
        if let Some(sub) = args.first().map(String::as_str) {
            Self::run(sub)
        } else {
            eprintln!("usage: dev-reaper {{reap|cleanup}}");
            2
        }
    }

    pub fn run(sub: &str) -> i32 {
        let cfg = DevConfig::load();
        match sub {
            "reap" => Self::reap(&cfg),
            "cleanup" => Self::cleanup(&cfg),
            _ => {
                eprintln!("usage: dev-reaper {{reap|cleanup}}");
                2
            }
        }
    }

    fn reap(cfg: &DevConfig) -> i32 {
        let Some(docker) = resolve_docker() else {
            println!("dev-reaper: no docker binary found, skipping");
            return 0;
        };
        if !docker_info(&docker) {
            println!("dev-reaper: docker not reachable, skipping");
            return 0;
        }
        let now = util::now();
        let Some(_lock) = Lock::take(&cfg.state_dir, "dev-reaper", now, &|m| {
            println!("dev-reaper: {m}");
        }) else {
            return 0;
        };

        let activity = load_activity(cfg, &docker, now);
        gc_state(cfg, &docker);

        for fid in running_dev_containers(&docker, &cfg.select_label) {
            reap_one(cfg, &docker, &activity, &fid, now);
        }
        0
    }

    fn cleanup(cfg: &DevConfig) -> i32 {
        let Some(docker) = resolve_docker() else {
            println!("dev-reaper: no docker binary found, skipping");
            return 0;
        };
        if !docker_info(&docker) {
            println!("dev-reaper: docker not reachable, skipping");
            return 0;
        }
        let now = util::now();
        let Some(_lock) = Lock::take(&cfg.state_dir, "dev-reaper", now, &|m| {
            println!("dev-reaper: {m}");
        }) else {
            return 0;
        };

        for fid in exited_dev_containers(&docker, &cfg.select_label) {
            cleanup_one(cfg, &docker, &fid, now);
        }

        // Safe prune only in full mode: dangling images + bounded build cache.
        if cfg.mode == "full" {
            let img = docker_capture(&docker, &["image", "prune", "-f"]);
            let bld = docker_capture(
                &docker,
                &[
                    "builder",
                    "prune",
                    "-f",
                    "--reserved-space",
                    &cfg.builder_reserved,
                ],
            );
            println!(
                "prune: images -> {}; build cache -> {}",
                img.unwrap_or_else(|| "n/a".into()),
                bld.unwrap_or_else(|| "n/a".into())
            );
        }
        0
    }
}

struct DevConfig {
    mode: String,
    select_label: String,
    keep_key: String,
    keep_val: String,
    exclude: Vec<String>,
    idle_stop_seconds: i64,
    exited_remove_seconds: i64,
    threshold_pct: f64,
    min_uptime_seconds: i64,
    exec_quiet_seconds: i64,
    stop_timeout: String,
    builder_reserved: String,
    heavy_regex: String,
    notify: bool,
    state_dir: PathBuf,
}

impl DevConfig {
    fn load() -> Self {
        let home = std::env::var_os("HOME")
            .map(PathBuf::from)
            .unwrap_or_default();
        let file = std::env::var("DEV_REAPER_CONFIG")
            .map_or_else(|_| home.join(".config/dev-reaper/config"), PathBuf::from);
        let f = config::load(&file);

        let state_base = std::env::var_os("XDG_STATE_HOME")
            .map_or_else(|| home.join(".local/state"), PathBuf::from);
        let state_dir = PathBuf::from(config::get(
            &f,
            "DEV_REAPER_STATE_DIR",
            &state_base.join("dev-reaper").to_string_lossy(),
        ));
        let _ = std::fs::create_dir_all(&state_dir);

        let keep_label = config::get(&f, "DEV_REAPER_KEEP_LABEL", "dev-reaper.keep=true");
        let (keep_key, keep_val) = keep_label
            .split_once('=')
            .unwrap_or(("dev-reaper.keep", "true"));

        Self {
            mode: config::get(&f, "DEV_REAPER_MODE", "log"),
            select_label: config::get(
                &f,
                "DEV_REAPER_SELECT_LABEL",
                "com.docker.compose.service=dev",
            ),
            keep_key: keep_key.to_owned(),
            keep_val: keep_val.to_owned(),
            exclude: config::get(&f, "DEV_REAPER_EXCLUDE", "")
                .split_whitespace()
                .map(str::to_owned)
                .collect(),
            idle_stop_seconds: config::get(&f, "DEV_REAPER_IDLE_STOP_SECONDS", "7200")
                .parse()
                .unwrap_or(7200),
            exited_remove_seconds: config::get(&f, "DEV_REAPER_EXITED_REMOVE_SECONDS", "604800")
                .parse()
                .unwrap_or(604_800),
            threshold_pct: config::get(&f, "DEV_REAPER_THRESHOLD_PCT", "3")
                .parse()
                .unwrap_or(3.0),
            min_uptime_seconds: config::get(&f, "DEV_REAPER_MIN_UPTIME_SECONDS", "1800")
                .parse()
                .unwrap_or(1800),
            exec_quiet_seconds: config::get(&f, "DEV_REAPER_EXEC_QUIET_SECONDS", "7200")
                .parse()
                .unwrap_or(7200),
            stop_timeout: config::get(&f, "DEV_REAPER_STOP_TIMEOUT", "30"),
            builder_reserved: config::get(&f, "DEV_REAPER_BUILDER_RESERVED", "10GB"),
            heavy_regex: config::get(
                &f,
                "DEV_REAPER_HEAVY_REGEX",
                "java|gradle|kotlin|node|vite|webpack|spotless|ktlint|javac|jlink|mvn|sbt",
            ),
            notify: config::get(&f, "DEV_REAPER_NOTIFY", "1") == "1",
            state_dir,
        }
    }

    fn is_excluded(&self, project: &str) -> bool {
        self.exclude.iter().any(|e| e == project)
    }
}

/// Resolves docker from `OrbStack` first, then Homebrew and system locations, then `PATH`.
fn resolve_docker() -> Option<PathBuf> {
    if let Some(d) = std::env::var_os("DOCKER") {
        let p = PathBuf::from(d);
        if p.is_file() {
            return Some(p);
        }
    }
    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_default();
    let candidates = [
        home.join(".orbstack/bin/docker"),
        PathBuf::from("/usr/local/bin/docker"),
        PathBuf::from("/opt/homebrew/bin/docker"),
        PathBuf::from("/usr/bin/docker"),
    ];
    candidates
        .iter()
        .find(|p| p.is_file())
        .cloned()
        .or_else(|| {
            std::env::var_os("PATH").and_then(|p| {
                std::env::split_paths(&p)
                    .map(|d| d.join("docker"))
                    .find(|p| p.is_file())
            })
        })
}

fn docker_info(docker: &Path) -> bool {
    Command::new(docker)
        .arg("info")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|s| s.success())
}

fn docker_capture(docker: &Path, args: &[&str]) -> Option<String> {
    let out = Command::new(docker).args(args).output().ok()?;
    out.status
        .success()
        .then(|| String::from_utf8_lossy(&out.stdout).trim().to_owned())
}

fn running_dev_containers(docker: &Path, label: &str) -> Vec<String> {
    docker_capture(
        docker,
        &[
            "ps",
            "--no-trunc",
            "-q",
            &format!("--filter=label={label}"),
            "--filter=label=com.docker.compose.oneoff=False",
        ],
    )
    .map(|s| s.lines().map(str::to_owned).collect())
    .unwrap_or_default()
}

fn exited_dev_containers(docker: &Path, label: &str) -> Vec<String> {
    docker_capture(
        docker,
        &[
            "ps",
            "-aq",
            "--no-trunc",
            "--filter=status=exited",
            &format!("--filter=label={label}"),
            "--filter=label=com.docker.compose.oneoff=False",
        ],
    )
    .map(|s| s.lines().map(str::to_owned).collect())
    .unwrap_or_default()
}

/// The inspect line both reap and cleanup consume: `project|restart|started|keep|name`.
fn inspect(docker: &Path, keep_key: &str, fid: &str, extra: bool) -> Option<InspectLine> {
    let format = if extra {
        format!(
            "{{{{index .Config.Labels \"com.docker.compose.project\"}}}}|{{{{.HostConfig.RestartPolicy.Name}}}}|{{{{.State.StartedAt}}}}|{{{{index .Config.Labels \"{keep_key}\"}}}}|{{{{.Name}}}}|{{{{.State.FinishedAt}}}}"
        )
    } else {
        format!(
            "{{{{index .Config.Labels \"com.docker.compose.project\"}}}}|{{{{.HostConfig.RestartPolicy.Name}}}}|{{{{.State.StartedAt}}}}|{{{{index .Config.Labels \"{keep_key}\"}}}}|{{{{.Name}}}}"
        )
    };
    let out = docker_capture(docker, &["inspect", "-f", &format, fid])?;
    let f: Vec<&str> = out.split('|').collect();
    if f.len() < 5 {
        return None;
    }
    Some(InspectLine {
        project: f[0].to_owned(),
        restart: f[1].to_owned(),
        started: f[2].to_owned(),
        keep: f[3].to_owned(),
        name: f[4].trim_start_matches('/').to_owned(),
        finished: f.get(5).map(|s| (*s).to_owned()),
    })
}

struct InspectLine {
    project: String,
    restart: String,
    started: String,
    keep: String,
    name: String,
    finished: Option<String>,
}

/// One `key=value` per line, and `name` keeps spaces because the reader splits on the first `=` only.
/// The reader decodes `%q` escapes from bash-written files.
#[derive(Default, Clone)]
struct ContainerState {
    idle_since: i64,
    last_activity: i64,
    name: String,
}

fn state_path(cfg: &DevConfig, fid: &str) -> PathBuf {
    let sid: String = fid.chars().take(12).collect();
    cfg.state_dir.join(format!("{sid}.state"))
}

fn read_state(p: &Path) -> ContainerState {
    let mut s = ContainerState::default();
    let Ok(text) = std::fs::read_to_string(p) else {
        return s;
    };
    for line in text.lines() {
        let Some((k, v)) = line.split_once('=') else {
            continue;
        };
        let v = unquote(v);
        match k {
            "idle_since" => s.idle_since = v.parse().unwrap_or(0),
            "last_activity" => s.last_activity = v.parse().unwrap_or(0),
            "name" => s.name = v,
            _ => {}
        }
    }
    s
}

fn write_state(p: &Path, s: &ContainerState) {
    let _ = std::fs::write(
        p,
        format!(
            "idle_since={}\nlast_activity={}\nname={}\n",
            s.idle_since, s.last_activity, s.name
        ),
    );
}

fn unquote(v: &str) -> String {
    let mut out = String::with_capacity(v.len());
    let mut chars = v.chars();
    while let Some(c) = chars.next() {
        if c == '\\' {
            out.push(chars.next().unwrap_or('\\'));
        } else {
            out.push(c);
        }
    }
    out
}

/// "<container-id> <epoch>" events since the last cursor.
fn load_activity(cfg: &DevConfig, docker: &Path, now: i64) -> HashMap<String, i64> {
    let cursor_file = cfg.state_dir.join(".events-cursor");
    let mut cursor: i64 = std::fs::read_to_string(&cursor_file)
        .ok()
        .and_then(|s| s.trim().parse().ok())
        .unwrap_or(now - cfg.exec_quiet_seconds - 60);
    // Clamp absurd cursors from a clock jump or daemon restart to a sane lookback.
    if cursor > now || now - cursor > 86_400 {
        cursor = now - cfg.exec_quiet_seconds - 60;
    }

    let mut map: HashMap<String, i64> = HashMap::new();
    if let Some(out) = docker_capture(
        docker,
        &[
            "events",
            &format!("--since={cursor}"),
            &format!("--until={now}"),
            "--filter=type=container",
            "--filter=event=exec_start",
            "--filter=event=exec_create",
            "--filter=event=exec_die",
            "--filter=event=start",
            "--filter=event=restart",
            "--filter=event=attach",
            "--filter=event=resize",
            "--format={{.Actor.ID}} {{.Time}}",
        ],
    ) {
        for line in out.lines() {
            let Some((id, t)) = line.split_once(' ') else {
                continue;
            };
            if let Ok(t) = t.parse::<i64>() {
                let e = map.entry(id.to_owned()).or_insert(0);
                if t > *e {
                    *e = t;
                }
            }
        }
    }
    let _ = std::fs::write(&cursor_file, format!("{now}\n"));
    map
}

fn gc_state(cfg: &DevConfig, docker: &Path) {
    let live: Vec<String> = docker_capture(docker, &["ps", "-aq", "--no-trunc"])
        .map(|s| s.lines().map(|l| l.chars().take(12).collect()).collect())
        .unwrap_or_default();
    let Ok(entries) = std::fs::read_dir(&cfg.state_dir) else {
        return;
    };
    for e in entries.flatten() {
        let name = e.file_name().to_string_lossy().into_owned();
        if let Some(sid) = name.strip_suffix(".state")
            && !live.iter().any(|l| l == sid)
        {
            let _ = std::fs::remove_file(e.path());
        }
    }
}

fn reap_one(cfg: &DevConfig, docker: &Path, activity: &HashMap<String, i64>, fid: &str, now: i64) {
    let path = state_path(cfg, fid);
    let mut st = read_state(&path);

    let Some(info) = inspect(docker, &cfg.keep_key, fid, false) else {
        return;
    };
    st.name.clone_from(&info.name);

    // Explicit opt-out label, such as `dev-reaper.keep=true`.
    if info.keep == cfg.keep_val {
        st.idle_since = 0;
        write_state(&path, &st);
        return;
    }
    // Skip resident containers that restart on their own.
    if !info.restart.is_empty() && info.restart != "no" {
        return;
    }
    if cfg.is_excluded(&info.project) {
        st.idle_since = 0;
        write_state(&path, &st);
        return;
    }

    let started_epoch = util::epoch_of(&info.started);
    if started_epoch == 0 || now - started_epoch < cfg.min_uptime_seconds {
        st.idle_since = 0;
        write_state(&path, &st);
        return;
    }

    if let Some(ev) = activity.get(fid) {
        st.last_activity = st.last_activity.max(*ev);
    }

    // signal 1: recent exec/attach => actively working
    let active = if st.last_activity > 0 && now - st.last_activity < cfg.exec_quiet_seconds {
        true
    } else if heavy_process_running(docker, fid, &cfg.heavy_regex) {
        // Signal 2 vetoes: a heavy build or server process runs.
        true
    } else {
        match cpu_percent(docker, fid) {
            // Sampling miss: keep the counter and wait.
            None => {
                write_state(&path, &st);
                return;
            }
            // Signal 3: the instantaneous CPU reading.
            Some(cpu) => cpu >= cfg.threshold_pct,
        }
    };

    if active {
        st.idle_since = 0;
    } else {
        if st.idle_since == 0 {
            st.idle_since = now;
        }
        let idle = now - st.idle_since;
        if idle >= cfg.idle_stop_seconds {
            if cfg.mode == "log" {
                println!("would stop {} (idle {idle}s, mode=log)", st.name);
            } else {
                let stopped = Command::new(docker)
                    .args(["stop", "--timeout", &cfg.stop_timeout, fid])
                    .stdout(Stdio::null())
                    .stderr(Stdio::null())
                    .status()
                    .is_ok_and(|s| s.success());
                if stopped {
                    println!(
                        "dev-reaper: stopped | stopped {} after {idle}s idle",
                        st.name
                    );
                    util::notify(
                        "dev-reaper: stopped",
                        &format!("stopped {} after {idle}s idle", st.name),
                        cfg.notify,
                    );
                } else {
                    println!("dev-reaper: failed to stop {}", st.name);
                }
            }
            st.idle_since = 0;
        }
    }

    write_state(&path, &st);
}

fn heavy_process_running(docker: &Path, fid: &str, regex: &str) -> bool {
    let Some(out) = docker_capture(docker, &["top", fid, "-eo", "comm"]) else {
        return false;
    };
    let lines: Vec<String> = out.lines().skip(1).map(str::to_owned).collect();
    // An unanswerable pattern keeps the container rather than treating it as idle.
    util::grep_matching(regex, &lines).is_none_or(|hits| !hits.is_empty())
}

/// `docker stats --no-stream --format '{{.CPUPerc}}'` → the number without the `%`.
/// `None` when the CPU reading fails.
fn cpu_percent(docker: &Path, fid: &str) -> Option<f64> {
    docker_capture(
        docker,
        &["stats", "--no-stream", "--format", "{{.CPUPerc}}", fid],
    )?
    .trim_end_matches('%')
    .parse()
    .ok()
}

fn cleanup_one(cfg: &DevConfig, docker: &Path, fid: &str, now: i64) {
    let Some(info) = inspect(docker, &cfg.keep_key, fid, true) else {
        return;
    };
    if info.keep == cfg.keep_val {
        return;
    }
    if !info.restart.is_empty() && info.restart != "no" {
        return;
    }
    if cfg.is_excluded(&info.project) {
        return;
    }
    let Some(finished) = info.finished.as_deref().map(util::epoch_of) else {
        return;
    };
    if finished == 0 {
        return;
    }
    let age = now - finished;
    if age < cfg.exited_remove_seconds {
        return;
    }

    if cfg.mode == "full" {
        let removed = Command::new(docker)
            .args(["rm", fid])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .is_ok_and(|s| s.success());
        if removed {
            let _ = std::fs::remove_file(state_path(cfg, fid));
            println!(
                "dev-reaper: removed | removed {} after {} days abandoned",
                info.name,
                age / 86_400
            );
            util::notify(
                "dev-reaper: removed",
                &format!(
                    "removed {} after {} days abandoned",
                    info.name,
                    age / 86_400
                ),
                cfg.notify,
            );
        } else {
            println!("dev-reaper: failed to rm {}", info.name);
        }
    } else {
        println!(
            "would remove {} (exited {}d, mode={})",
            info.name,
            age / 86_400,
            cfg.mode
        );
    }
}

#[cfg(test)]
mod tests {
    use super::{ContainerState, read_state, unquote, write_state};

    #[test]
    fn state_round_trips_and_decodes_bash_percent_q() {
        let dir = std::env::temp_dir().join("reaper-test-state");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join("abc123.state");

        let mut s = ContainerState {
            idle_since: 42,
            last_activity: 7,
            name: "dev box".into(),
        };
        write_state(&p, &s);
        assert_eq!(read_state(&p).name, "dev box");
        assert_eq!(read_state(&p).idle_since, 42);

        // A bash-written file with `%q` escapes reads back clean.
        std::fs::write(&p, "idle_since=0\nlast_activity=0\nname=my\\-dev\\ box\n").unwrap();
        s = read_state(&p);
        assert_eq!(unquote("my\\-dev"), "my-dev");
        assert_eq!(s.name, "my-dev box");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
