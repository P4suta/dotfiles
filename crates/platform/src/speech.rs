//! The bounded desktop queue shared by Git and supported agent notifications.

use rules::notification_rules::{Decision, Desktop, Event, Frequency, Origin, Receipt, Request};
use sha2::{Digest, Sha256};
use std::fmt::Write as _;
use std::fs::{File, OpenOptions};
use std::io;
use std::path::Path;
use std::process::Stdio;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

pub const CONFIG: &str = ".config/dotfiles/notifications";
pub const STATE: &str = ".local/state/dotfiles/notifications";
pub const MODE: &str = "mode";
pub const MATERIALS: &str = "materials.tsv";
const DEFAULT_MODE: &str = "frequency";
const QUEUE: &str = "queue";
const LEDGER: &str = "ledger";
const CANCELLED: &str = "cancelled";
const QUEUE_LOCK: &str = "queue.lock";
const WORKER_LOCK: &str = "worker.lock";
const WAITING: &str = "waiting";
pub const INTERNAL: &str = "DOTFILES_NOTIFICATION_INTERNAL";
const MAX_LEDGER: usize = 64;
const MAX_STATE_BYTES: u64 = 16_384;
const HEX_RADIX: u8 = 16;
const HEX: &[u8] = b"0123456789abcdef";
const IDENTITY_HEX_BYTES: usize = 64;
const FAILED_OPERATION: &str = "notification operation failed";
const QUEUE_LOCK_WAIT: Duration = Duration::from_millis(50);
const QUEUE_LOCK_POLL: Duration = Duration::from_millis(2);
const PRODUCER_TIMEOUT: Duration = Duration::from_secs(2);
#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;
static SEQUENCE: AtomicU64 = AtomicU64::new(0);

#[derive(Clone, Copy, Debug)]
pub enum Operation {
    Queue,
    Cancel,
    Callback,
    Worker,
    Forward,
    Start,
    Stop,
    Reap,
}

impl Operation {
    fn action(self) -> &'static str {
        match self {
            Self::Queue => "queue the spoken notification",
            Self::Cancel => "cancel the spoken notification",
            Self::Callback => "process the spoken notification callback",
            Self::Worker => "run the spoken notification worker",
            Self::Forward => "forward the existing notification callback",
            Self::Start => "start the spoken notification worker",
            Self::Stop => "stop the owned notification child after its deadline",
            Self::Reap => "reap the owned notification child",
        }
    }
}

/// Reports an optional operation's failure using only fixed text, without formatting its error.
pub fn report<T, E>(result: Result<T, E>, operation: Operation) -> Option<T> {
    crate::cleanup::report(result.map_err(|_| FAILED_OPERATION), operation.action())
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Packet {
    pub event: Event,
    pub id: String,
    pub issued: u64,
}

fn error(message: &'static str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message)
}

/// Reads the notification clock as Unix milliseconds.
///
/// # Errors
///
/// Returns an error when the clock precedes the epoch or milliseconds exceed the representation.
pub fn now() -> io::Result<u64> {
    u64::try_from(
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(io::Error::other)?
            .as_millis(),
    )
    .map_err(io::Error::other)
}

#[must_use]
pub fn identity(namespace: &str, opaque: &str) -> String {
    let mut hash = Sha256::new();
    hash.update(namespace.as_bytes());
    hash.update(b"\0");
    hash.update(opaque.as_bytes());
    let mut result = String::with_capacity(IDENTITY_HEX_BYTES);
    for byte in hash.finalize() {
        result.push(char::from(HEX[usize::from(byte / HEX_RADIX)]));
        result.push(char::from(HEX[usize::from(byte % HEX_RADIX)]));
    }
    result
}

#[must_use]
pub fn operation_id() -> String {
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| {
            u64::try_from(elapsed.as_millis()).unwrap_or_default()
        });
    identity(
        "operation",
        &format!(
            "{}:{}:{}",
            std::process::id(),
            millis,
            SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ),
    )
}

#[must_use]
pub fn frequency_text(value: Frequency) -> &'static str {
    match value {
        Frequency::Off => "off\n",
        Frequency::Attention => "attention\n",
        Frequency::Results => "results\n",
        Frequency::All => "all\n",
    }
}

/// Reads the owner's override before the managed notification frequency.
///
/// # Errors
///
/// Returns an error for missing, unreadable, linked, oversize, or invalid configuration.
pub fn frequency(home: &Path) -> io::Result<Frequency> {
    let override_path = home.join(STATE).join(MODE);
    let path = match std::fs::symlink_metadata(&override_path) {
        Ok(_) => override_path,
        Err(cause) if cause.kind() == io::ErrorKind::NotFound => {
            home.join(CONFIG).join(DEFAULT_MODE)
        }
        Err(cause) => return Err(cause),
    };
    match read_small(&path)?.as_str() {
        "off\n" => Ok(Frequency::Off),
        "attention\n" => Ok(Frequency::Attention),
        "results\n" => Ok(Frequency::Results),
        "all\n" => Ok(Frequency::All),
        _ => Err(error(
            "invalid notification frequency; no speech is enabled",
        )),
    }
}

#[must_use]
pub fn desktop() -> Desktop {
    if [
        "SSH_CONNECTION",
        "SSH_TTY",
        "CI",
        "DOTFILES_HOST_CACHE",
        "WSL_DISTRO_NAME",
        INTERNAL,
    ]
    .iter()
    .any(|name| std::env::var_os(name).is_some())
    {
        return Desktop::Headless;
    }
    if cfg!(target_os = "linux")
        && std::env::var_os("DISPLAY").is_none()
        && std::env::var_os("WAYLAND_DISPLAY").is_none()
    {
        return Desktop::Headless;
    }
    if cfg!(windows)
        && std::env::var("SESSIONNAME")
            .is_ok_and(|session| session.eq_ignore_ascii_case("services"))
    {
        return Desktop::Headless;
    }
    Desktop::Present
}

fn read_small(path: &Path) -> io::Result<String> {
    let metadata = std::fs::symlink_metadata(path)?;
    if !metadata.is_file() || metadata.len() > MAX_STATE_BYTES {
        return Err(error("notification state is not a bounded regular file"));
    }
    std::fs::read_to_string(path)
}

fn optional(path: &Path) -> io::Result<String> {
    match read_small(path) {
        Ok(text) => Ok(text),
        Err(cause) if cause.kind() == io::ErrorKind::NotFound => Ok(String::new()),
        Err(cause) => Err(cause),
    }
}

fn lock(state: &Path, name: &str) -> io::Result<File> {
    std::fs::create_dir_all(state)?;
    let path = state.join(name);
    if std::fs::symlink_metadata(&path).is_ok_and(|metadata| !metadata.is_file()) {
        return Err(error("notification lock is not a regular file"));
    }
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(path)?;
    let began = Instant::now();
    loop {
        match file.try_lock() {
            Ok(()) => return Ok(file),
            Err(cause) if name == WORKER_LOCK || began.elapsed() >= QUEUE_LOCK_WAIT => {
                return Err(io::Error::other(cause));
            }
            Err(_) => std::thread::sleep(QUEUE_LOCK_POLL),
        }
    }
}

fn parse_packet(line: &str) -> io::Result<Packet> {
    let mut fields = line.split('\t');
    let code = fields
        .next()
        .and_then(|field| field.parse::<u8>().ok())
        .and_then(rules::notification_rules::event)
        .ok_or_else(|| error("unknown queued event"))?;
    let id = fields
        .next()
        .ok_or_else(|| error("missing notification identity"))?;
    if id.len() != IDENTITY_HEX_BYTES
        || !id
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
    {
        return Err(error("invalid notification identity"));
    }
    let issued = fields
        .next()
        .and_then(|field| field.parse::<u64>().ok())
        .ok_or_else(|| error("missing notification time"))?;
    if fields.next().is_some() {
        return Err(error("unexpected queued notification field"));
    }
    Ok(Packet {
        event: code,
        id: id.to_owned(),
        issued,
    })
}

fn packets(state: &Path) -> io::Result<Vec<Packet>> {
    let found = optional(&state.join(QUEUE))?
        .lines()
        .map(parse_packet)
        .collect::<io::Result<Vec<_>>>()?;
    if found.len() > usize::from(rules::notification_rules::MAX_PENDING) {
        return Err(error("notification queue exceeds its bound"));
    }
    Ok(found)
}

fn write_packets(state: &Path, found: &[Packet]) -> io::Result<()> {
    let mut text = String::new();
    for packet in found {
        writeln!(
            text,
            "{}\t{}\t{}",
            packet.event as u8, packet.id, packet.issued
        )
        .map_err(io::Error::other)?;
    }
    std::fs::write(state.join(QUEUE), text)
}

fn remember(state: &Path, file: &str, id: &str) -> io::Result<()> {
    let text = optional(&state.join(file))?;
    let mut retained = text.lines().map(str::to_owned).collect::<Vec<_>>();
    if retained.len() > MAX_LEDGER {
        return Err(error("notification ledger exceeds its bound"));
    }
    if retained.len() == MAX_LEDGER {
        retained.remove(0);
    }
    retained.push(id.to_owned());
    std::fs::write(state.join(file), format!("{}\n", retained.join("\n")))
}

/// Queues an eligible fixed event under the bounded queue lock.
///
/// # Errors
///
/// Returns an error when configuration, clock, locking, or bounded queue and ledger I/O fails.
pub fn enqueue(
    home: &Path,
    event: Event,
    id: &str,
    issued: u64,
    origin: Origin,
) -> io::Result<Decision> {
    enqueue_observed(
        home,
        id,
        issued,
        Request {
            event: Some(event),
            frequency: frequency(home)?,
            desktop: desktop(),
            origin,
            receipt: Receipt::New,
            pending: 0,
            issued_millis: issued,
            now_millis: now()?,
        },
    )
}

fn enqueue_observed(
    home: &Path,
    id: &str,
    issued: u64,
    mut request: Request,
) -> io::Result<Decision> {
    if !matches!(
        rules::notification_rules::decide(request),
        Decision::Queue(_)
    ) {
        return Ok(rules::notification_rules::decide(request));
    }
    let state = home.join(STATE);
    let _lock = lock(&state, QUEUE_LOCK)?;
    let current = request.now_millis;
    let mut found = packets(&state)?;
    found.retain(|packet| {
        current
            .checked_sub(packet.issued)
            .is_some_and(|age| age <= rules::notification_rules::MAX_AGE_MILLIS)
    });
    let receipt = if optional(&state.join(CANCELLED))?
        .lines()
        .any(|prior| prior == id)
    {
        Receipt::Cancelled
    } else if optional(&state.join(LEDGER))?
        .lines()
        .any(|prior| prior == id)
    {
        Receipt::Duplicate
    } else {
        Receipt::New
    };
    request.receipt = receipt;
    request.pending = u8::try_from(found.len()).map_err(io::Error::other)?;
    let decision = rules::notification_rules::decide(request);
    if let Decision::Queue(accepted) = decision {
        found.push(Packet {
            event: accepted,
            id: id.to_owned(),
            issued,
        });
        write_packets(&state, &found)?;
        remember(&state, LEDGER, id)?;
    }
    Ok(decision)
}

/// Records a cancellation without playing or starting a worker.
///
/// # Errors
///
/// Returns an error when locking or bounded cancellation ledger I/O fails.
pub fn cancel(home: &Path, id: &str) -> io::Result<()> {
    let state = home.join(STATE);
    let _lock = lock(&state, QUEUE_LOCK)?;
    remember(&state, CANCELLED, id)
}

/// Associates a waiting identity with its client session scope.
///
/// # Errors
///
/// Returns an error when locking or bounded waiting ledger I/O fails.
pub fn remember_waiting(home: &Path, scope: &str, id: &str) -> io::Result<()> {
    let state = home.join(STATE);
    let _lock = lock(&state, QUEUE_LOCK)?;
    remember(&state, WAITING, &format!("{scope}\t{id}"))
}

/// Cancels the observed waiting identities belonging to one client session scope.
///
/// # Errors
///
/// Returns an error when locking or bounded waiting and cancellation ledger I/O fails.
pub fn resolve_waiting(home: &Path, scope: &str) -> io::Result<()> {
    let state = home.join(STATE);
    if !state.is_dir() {
        return Ok(());
    }
    let _lock = lock(&state, QUEUE_LOCK)?;
    for line in optional(&state.join(WAITING))?.lines() {
        if let Some((prior, id)) = line.split_once('\t')
            && prior == scope
        {
            remember(&state, CANCELLED, id)?;
        }
    }
    Ok(())
}

/// Acquires the single worker lock without waiting for another worker.
///
/// # Errors
///
/// Returns an error if state validation fails, access fails, or the lock already exists.
pub fn worker_lock(home: &Path) -> io::Result<File> {
    lock(&home.join(STATE), WORKER_LOCK)
}

/// A producer keeps its wake-up when this code releases the worker lock while holding the empty queue lock.
///
/// # Errors
///
/// Returns an error when locking, parsing bounded state, writing the remaining queue, or releasing the worker lock fails.
pub fn next(home: &Path, worker: &mut Option<File>) -> io::Result<Option<Packet>> {
    let state = home.join(STATE);
    let _lock = lock(&state, QUEUE_LOCK)?;
    let mut found = packets(&state)?;
    if found.is_empty() {
        if let Some(lock) = worker.as_ref() {
            lock.unlock()?;
        }
        worker.take();
        return Ok(None);
    }
    let packet = found.remove(0);
    write_packets(&state, &found)?;
    Ok(Some(packet))
}

/// Rechecks frequency, cancellation, desktop availability, and age before delivery.
///
/// # Errors
///
/// Returns an error if bounded cancellation state, configuration, or clock access fails.
pub fn pending_decision(home: &Path, packet: &Packet) -> io::Result<Decision> {
    let state = home.join(STATE);
    let receipt = if optional(&state.join(CANCELLED))?
        .lines()
        .any(|id| id == packet.id)
    {
        Receipt::Cancelled
    } else {
        Receipt::New
    };
    Ok(rules::notification_rules::decide(Request {
        event: Some(packet.event),
        frequency: frequency(home)?,
        desktop: desktop(),
        origin: Origin::Agent,
        receipt,
        pending: 0,
        issued_millis: packet.issued,
        now_millis: now()?,
    }))
}

fn runtime_command(
    home: &Path,
    environment: impl IntoIterator<Item = (std::ffi::OsString, std::ffi::OsString)>,
) -> io::Result<std::process::Command> {
    let executable = home.join(if cfg!(windows) {
        ".local/bin/dotfiles.exe"
    } else {
        ".local/bin/dotfiles"
    });
    if !executable.is_file() {
        return Err(error("notification runtime is unavailable"));
    }
    let environment = environment.into_iter().filter(|(name, _)| {
        !crate::process::GIT_LOCAL_ENVIRONMENT.iter().any(|local| {
            if cfg!(windows) {
                name.as_encoded_bytes()
                    .eq_ignore_ascii_case(local.as_bytes())
            } else {
                name == local
            }
        })
    });
    let command = crate::process::isolated_command(executable, environment);
    #[cfg(windows)]
    let command = {
        use std::os::windows::process::CommandExt;
        let mut command = command;
        command.creation_flags(CREATE_NO_WINDOW);
        command
    };
    Ok(command)
}

/// Starts the installed worker with the producer's embedded source identity and quiet streams.
///
/// # Errors
///
/// Returns an error if installation lacks a runtime or its process can't start.
pub fn launch_worker(home: &Path) -> io::Result<()> {
    let mut command = runtime_command(home, std::env::vars_os())?;
    command
        .args(["notifications", "worker", "--producer", crate::BUILD_HASH])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    let mut child = command.spawn()?;
    std::thread::spawn(move || {
        report(child.wait(), Operation::Reap);
    });
    Ok(())
}

fn dispatch_git(home: &Path, event: Event, operation: &str) -> io::Result<()> {
    let mut child = runtime_command(home, std::env::vars_os())?
        .args([
            "notifications",
            "git",
            "--producer",
            crate::BUILD_HASH,
            "--event",
        ])
        .arg((event as u8).to_string())
        .args(["--operation", operation])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::inherit())
        .spawn()?;
    let began = Instant::now();
    let failed = loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                return if status.success() {
                    Ok(())
                } else {
                    Err(error(FAILED_OPERATION))
                };
            }
            Ok(None) if began.elapsed() < PRODUCER_TIMEOUT => {
                std::thread::sleep(QUEUE_LOCK_POLL);
            }
            Ok(None) => break error("notification producer exceeded its deadline"),
            Err(cause) => break cause,
        }
    };
    report(child.kill(), Operation::Stop);
    report(child.wait(), Operation::Reap);
    Err(failed)
}

/// An optional notification preserves the calling operation's result and standard output.
/// Producer failures produce fixed nonfatal warnings on standard error.
pub fn announce(event: Event, operation: &str) {
    let Some(home) = crate::home::home() else {
        return;
    };
    if desktop() != Desktop::Present
        || !home.join(CONFIG).join(MATERIALS).is_file()
        || frequency(&home).unwrap_or(Frequency::Off) == Frequency::Off
    {
        return;
    }
    report(dispatch_git(&home, event, operation), Operation::Queue);
}

/// Queues the matching Git producer's event with its existing owner identity.
pub fn receive_git(event: Event, operation: &str) {
    let Some(home) = crate::home::home() else {
        return;
    };
    if desktop() != Desktop::Present
        || !home.join(CONFIG).join(MATERIALS).is_file()
        || frequency(&home).unwrap_or(Frequency::Off) == Frequency::Off
    {
        return;
    }
    let id = identity(&format!("git-{}", event as u8), operation);
    let queued = now().and_then(|issued| enqueue(&home, event, &id, issued, Origin::Owner));
    if matches!(report(queued, Operation::Queue), Some(Decision::Queue(_))) {
        report(launch_worker(&home), Operation::Start);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;
    use std::fmt;

    #[test]
    fn identity_keeps_the_original_preimage_and_every_digest_byte() {
        const BYTE_VALUES: usize = 256;
        const LONG_INPUT_REPETITIONS: usize = 32_768;

        let mut seen = [false; BYTE_VALUES];
        let mut check = |namespace: &str, opaque: &str| {
            let digest = Sha256::digest(format!("{namespace}\0{opaque}").as_bytes());
            let mut expected = String::new();
            for byte in &digest {
                write!(expected, "{byte:02x}").unwrap();
            }
            assert_eq!(identity(namespace, opaque), expected);
            for byte in digest {
                seen[usize::from(byte)] = true;
            }
        };
        let long = "\u{2603}\0".repeat(LONG_INPUT_REPETITIONS);
        for (namespace, opaque) in [
            ("", ""),
            ("namespace", ""),
            ("", "opaque"),
            ("n\u{2603}", "v\u{1f680}"),
            ("namespace\0suffix", "opaque\0suffix"),
            ("operation", long.as_str()),
        ] {
            check(namespace, opaque);
        }
        for value in u8::MIN..=u8::MAX {
            check("operation", &value.to_string());
        }
        assert!(seen.into_iter().all(std::convert::identity));
    }

    #[cfg(feature = "tools")]
    #[test]
    fn runtime_environment_preserves_parent_values_except_git_repository_context() {
        use std::ffi::OsString;

        let home = tempfile::tempdir().unwrap();
        let bin = home.path().join(".local/bin");
        std::fs::create_dir_all(&bin).unwrap();
        std::fs::write(
            bin.join(format!("dotfiles{}", std::env::consts::EXE_SUFFIX)),
            "owned runtime fixture",
        )
        .unwrap();
        let kept: Vec<(OsString, OsString)> = [
            ("HOME", "owned-home"),
            ("PATH", "owned-path"),
            ("DOPPLER_TOKEN", "owned-token-fixture"),
            (INTERNAL, "1"),
            ("UNICODE_VALUE", "\u{2603}"),
            ("GIT_CONFIG_GLOBAL", "owned-git-config"),
        ]
        .map(|(name, value)| (name.into(), value.into()))
        .into();
        let mut environment = kept.clone();
        for local in crate::process::GIT_LOCAL_ENVIRONMENT {
            environment.push((local.into(), "owned-repository".into()));
            if cfg!(windows) {
                environment.push((local.to_ascii_lowercase().into(), "owned-repository".into()));
            }
        }
        let command = runtime_command(home.path(), environment).unwrap();
        let actual: std::collections::BTreeMap<_, _> = command
            .get_envs()
            .map(|(name, value)| (name.to_owned(), value.map(std::ffi::OsStr::to_owned)))
            .collect();
        assert_eq!(actual.len(), kept.len());
        for (name, value) in kept {
            assert_eq!(actual.get(&name), Some(&Some(value)));
        }
    }

    struct SensitiveError<'a>(&'a Cell<bool>);

    impl fmt::Display for SensitiveError<'_> {
        fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
            self.0.set(true);
            formatter.write_str("private input and private path")
        }
    }

    #[test]
    fn optional_failure_reporting_never_formats_private_errors() {
        let displayed = Cell::new(false);
        assert_eq!(
            report::<bool, _>(Err(SensitiveError(&displayed)), Operation::Queue),
            None
        );
        assert!(!displayed.get());
        assert_eq!(
            report::<bool, SensitiveError<'_>>(Ok(true), Operation::Queue),
            Some(true)
        );
    }

    #[test]
    fn queued_packets_refuse_unknown_events_and_unbounded_or_nonopaque_identity() {
        let id = identity("test", "private raw argument");
        let line = format!("0\t{id}\t42");
        assert_eq!(parse_packet(&line).unwrap().event, Event::CommitStarted);
        for malformed in [
            "8\tid\t1",
            "0\tprivate raw argument\t1",
            "0\t\t1",
            "0\tid\tbad",
        ] {
            assert!(parse_packet(malformed).is_err());
        }
        assert!(parse_packet(&format!("{line}\textra")).is_err());
        assert_ne!(identity("first", "same"), identity("second", "same"));
    }
    #[cfg(feature = "tools")]
    #[test]
    fn a_bounded_queue_refuses_duplicates_capacity_and_cancelled_delivery() {
        let directory = tempfile::tempdir().unwrap();
        let home = directory.path();
        let request = Request {
            event: Some(Event::TurnCompleted),
            frequency: Frequency::All,
            desktop: Desktop::Present,
            origin: Origin::Agent,
            receipt: Receipt::New,
            pending: 0,
            issued_millis: 1,
            now_millis: 1,
        };
        let id = identity("test", "first");
        assert_eq!(
            enqueue_observed(home, &id, 1, request).unwrap(),
            Decision::Queue(Event::TurnCompleted)
        );
        assert_eq!(
            enqueue_observed(home, &id, 1, request).unwrap(),
            Decision::Quiet(rules::notification_rules::Reason::Duplicate)
        );
        for index in 1..rules::notification_rules::MAX_PENDING {
            let id = identity("test", &index.to_string());
            assert!(matches!(
                enqueue_observed(home, &id, 1, request).unwrap(),
                Decision::Queue(_)
            ));
        }
        let extra = identity("test", "extra");
        assert_eq!(
            enqueue_observed(home, &extra, 1, request).unwrap(),
            Decision::Quiet(rules::notification_rules::Reason::Full)
        );
        cancel(home, &extra).unwrap();
        assert_eq!(
            enqueue_observed(home, &extra, 1, request).unwrap(),
            Decision::Quiet(rules::notification_rules::Reason::Cancelled)
        );
        let state = home.join(STATE);
        assert_eq!(
            packets(&state).unwrap().len(),
            usize::from(rules::notification_rules::MAX_PENDING)
        );
        let mut worker = Some(worker_lock(home).unwrap());
        let duplicate = worker.as_ref().unwrap().try_clone().unwrap();
        assert!(worker_lock(home).is_err());
        for _ in 0..rules::notification_rules::MAX_PENDING {
            assert!(next(home, &mut worker).unwrap().is_some());
        }
        assert!(next(home, &mut worker).unwrap().is_none());
        assert!(worker.is_none());
        let successor = worker_lock(home).unwrap();
        drop(duplicate);
        assert!(worker_lock(home).is_err());
        successor.unlock().unwrap();
        worker_lock(home).unwrap();
    }
    #[cfg(feature = "tools")]
    #[test]
    fn owner_frequency_is_fail_closed_and_independent_of_the_managed_default() {
        let directory = tempfile::tempdir().unwrap();
        let home = directory.path();
        assert!(frequency(home).is_err());
        std::fs::create_dir_all(home.join(CONFIG)).unwrap();
        std::fs::write(home.join(CONFIG).join(DEFAULT_MODE), "all\n").unwrap();
        assert_eq!(frequency(home).unwrap(), Frequency::All);
        std::fs::create_dir_all(home.join(STATE)).unwrap();
        let owner = home.join(STATE).join(MODE);
        std::fs::write(&owner, "off\n").unwrap();
        std::fs::write(home.join(CONFIG).join(DEFAULT_MODE), "results\n").unwrap();
        assert_eq!(frequency(home).unwrap(), Frequency::Off);
        for invalid in ["all", "true\n", " all\n", "all\r\n"] {
            std::fs::write(&owner, invalid).unwrap();
            assert!(frequency(home).is_err());
        }
    }
}
