//! One source-bound image preparation and its captured `BuildKit` lifetime.

use anyhow::{Context, Result, ensure};
use clap::Args;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs;
use std::io::{Read as _, Write as _};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use rules::proof_image_prepare_rules::{
    self as lifecycle, Capacity, Ownership, Release, Running, Work,
};
use rules::resource_rules::{self, Allocation, COMPONENTS, Inodes};

const SCHEMA: u32 = 1;
const GIB: u64 = 1 << 30;
const KIB_BYTES: u64 = 1024;
const BUILDER_CPUS: u64 = 2;
const LOAD_WINDOW_COUNT: usize = 3;
const MINIMUM_LOAD: f64 = 0.0;
#[cfg(unix)]
const SHARED_WRITE_MASK: u32 = 0o022;
const FLOOR_BYTES: u64 = 40 * GIB;
const BUILD_BYTES: u64 = 20 * GIB;
const TEMPORARY_BYTES: u64 = GIB;
const MARGIN_BYTES: u64 = GIB;
const COMPETING_BYTES: u64 = 8 * GIB;
const DECLARED_INODES: [u64; COMPONENTS] = [100_000, 2_000_000, 0, 100_000, 100_000, 1_000_000];
const MEMORY_BYTES: u64 = 8 * GIB;
const CPU_PERIOD: u64 = 100_000;
const CPU_QUOTA: i64 = 200_000;
const POLL: Duration = Duration::from_millis(250);
const METADATA_LIMIT: usize = 64 * 1024;
const SHA256_LENGTH: usize = 64;
const BUILDER_PREFIX: &str = "dotfiles-proof-156-";
const LINUX_AMD64: &str = "linux/amd64";
const BUILDKIT_IMAGE: &str = "docker.io/moby/buildkit@sha256:3ad6bb9bc8c78c0069d03247adb9a59b3b43d68e55353e876e558b888c6c1768";
const BUILDKIT_CONFIG: &str =
    "sha256:f27f9c00a3aca2c219642d1500610eade3ddcb6b873ea8847852ab156663d32f";
const RESTART_POLICY: &str = "no";
const BUILD_INODES: u64 = 2_000_000;
const FLOOR_INODES: u64 = 100_000;
const INPUTS: [&str; 5] = [
    crate::skill_proofs::PROOF_IMAGE,
    "mise.toml",
    "mise.lock",
    "home/.chezmoidata/versions.toml",
    "crates/platform/src/script_analyzer.rs",
];

#[derive(Args, Debug)]
#[group(required = true, multiple = false)]
pub struct Request {
    /// Print the source-bound allocation and commands without Docker effects.
    #[arg(long)]
    pub plan: bool,
    /// Keep one execution's original output in a new private directory.
    #[arg(long)]
    pub output: Option<PathBuf>,
}

#[derive(Debug, Serialize)]
struct Input {
    path: &'static str,
    sha256: String,
}

#[derive(Debug, Serialize)]
pub(crate) struct Plan {
    schema: u32,
    inputs: Vec<Input>,
    producing_source: String,
    image: String,
    arguments: [String; 6],
    admission_bytes: u64,
    protected_floor_bytes: u64,
    declared_growth_bytes: u64,
    wall_seconds: u64,
    work_seconds: u64,
}

impl Plan {
    fn capture(root: &Path) -> Result<Self> {
        let inputs = INPUTS
            .iter()
            .map(|path| {
                let location = root.join(path);
                ensure!(
                    fs::symlink_metadata(&location)?.file_type().is_file(),
                    "recipe input is not a regular file"
                );
                Ok(Input {
                    path,
                    sha256: crate::build_inputs::hex(&Sha256::digest(fs::read(location)?)),
                })
            })
            .collect::<Result<Vec<_>>>()?;
        Ok(Self {
            schema: SCHEMA,
            inputs,
            producing_source: crate::build_inputs::content_hash(root)?,
            image: crate::skill_proofs::proof_image(root)?,
            arguments: crate::skill_proofs::proof_image_arguments(root)?,
            admission_bytes: resource_rules::required(allocation())
                .context("image allocation overflow")?
                .bytes,
            protected_floor_bytes: FLOOR_BYTES,
            declared_growth_bytes: BUILD_BYTES,
            wall_seconds: lifecycle::WALL_SECONDS,
            work_seconds: lifecycle::WORK_SECONDS,
        })
    }

    fn require_unchanged(&self, root: &Path) -> Result<()> {
        ensure!(
            serde_json::to_vec(self)? == serde_json::to_vec(&Self::capture(root)?)?,
            "image recipe or producing source changed"
        );
        Ok(())
    }
}

fn allocation() -> [Allocation; COMPONENTS] {
    let [floor, build, proof, temporary, margin, competing] = DECLARED_INODES;
    [
        Allocation {
            bytes: FLOOR_BYTES,
            inodes: floor,
        },
        Allocation {
            bytes: BUILD_BYTES,
            inodes: build,
        },
        Allocation {
            bytes: 0,
            inodes: proof,
        },
        Allocation {
            bytes: TEMPORARY_BYTES,
            inodes: temporary,
        },
        Allocation {
            bytes: MARGIN_BYTES,
            inodes: margin,
        },
        Allocation {
            bytes: COMPETING_BYTES,
            inodes: competing,
        },
    ]
}

#[derive(Debug)]
struct MeasuredCapacity {
    host_bytes: u64,
    guest_bytes: u64,
    host_inodes: Inodes,
    guest_inodes: Inodes,
}

impl MeasuredCapacity {
    fn require_admission(&self) -> Result<()> {
        for (bytes, inodes) in [
            (self.host_bytes, self.host_inodes),
            (self.guest_bytes, self.guest_inodes),
        ] {
            resource_rules::admission(allocation(), Some(bytes), inodes).map_err(|reason| {
                anyhow::anyhow!("image preparation admission refused: {reason:?}")
            })?;
        }
        Ok(())
    }
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Server {
    id: String,
    name: String,
    created: String,
    image: String,
    state_volume: String,
    running: bool,
    cpu_period: u64,
    cpu_quota: i64,
    memory: u64,
    memory_swap: i64,
    restart_policy: String,
}

/// Decode server observations without importing an ownership capability.
///
/// # Errors
/// Rejects oversize, malformed, duplicate, unknown, or incomplete fields.
pub fn decode(bytes: &[u8]) -> Result<Server> {
    ensure!(
        bytes.len() <= METADATA_LIMIT,
        "BuildKit readback exceeds its metadata bound"
    );
    let server: Server = serde_json::from_slice(bytes).context("invalid BuildKit readback")?;
    ensure!(canonical_id(&server.id), "invalid complete server ID");
    ensure!(
        !server.name.is_empty()
            && !server.created.is_empty()
            && !server.image.is_empty()
            && !server.state_volume.is_empty(),
        "incomplete server identity"
    );
    Ok(server)
}

fn canonical_id(id: &str) -> bool {
    id.len() == SHA256_LENGTH
        && id
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

#[derive(Debug)]
struct PendingBuilder {
    name: String,
    container_name: String,
    state_volume: String,
}

impl PendingBuilder {
    fn new(nonce: &str) -> Result<Self> {
        ensure!(canonical_id(nonce), "invalid private invocation nonce");
        let name = format!("{BUILDER_PREFIX}{nonce}");
        let container_name = format!("/buildx_buildkit_{name}0");
        let state_volume = format!("buildx_buildkit_{name}0_state");
        Ok(Self {
            name,
            container_name,
            state_volume,
        })
    }

    #[cfg(test)]
    fn create_command(&self) -> Command {
        let mut command = crate::tool::Tool::Docker.command();
        command.args([
            "buildx",
            "create",
            "--name",
            &self.name,
            "--driver",
            "docker-container",
        ]);
        for option in [
            format!("image={BUILDKIT_IMAGE}"),
            format!("cpu-period={CPU_PERIOD}"),
            format!("cpu-quota={CPU_QUOTA}"),
            format!("memory={MEMORY_BYTES}"),
            format!("memory-swap={MEMORY_BYTES}"),
            format!("restart-policy={RESTART_POLICY}"),
        ] {
            command.arg("--driver-opt").arg(option);
        }
        command
    }

    fn capture_identity(self, server: Server) -> Result<CapturedBuildKit> {
        ensure!(
            canonical_id(&server.id) && !server.created.is_empty(),
            "BuildKit has no complete identity"
        );
        ensure!(
            server.name == self.container_name && server.state_volume == self.state_volume,
            "bootstrap did not create this invocation's server"
        );
        Ok(CapturedBuildKit {
            builder: self.name,
            server,
        })
    }
}

/// Constructed only from this invocation's validated fresh-name server readback.
/// This doesn't import receipts or clone success claims.
#[derive(Debug)]
struct CapturedBuildKit {
    builder: String,
    server: Server,
}

impl CapturedBuildKit {
    fn require_limits(&self) -> Result<()> {
        let server = &self.server;
        ensure!(
            server.image == BUILDKIT_CONFIG && server.running,
            "BuildKit image or initial state is unexpected"
        );
        ensure!(
            server.cpu_period == CPU_PERIOD
                && server.cpu_quota == CPU_QUOTA
                && server.memory == MEMORY_BYTES
                && server.memory_swap == i64::try_from(MEMORY_BYTES)?
                && server.restart_policy == RESTART_POLICY,
            "BuildKit resource limits or restart policy differ"
        );
        Ok(())
    }

    fn ownership(&self, current: &Server) -> Ownership {
        if current.id == self.server.id
            && current.name == self.server.name
            && current.created == self.server.created
            && current.image == self.server.image
            && current.state_volume == self.server.state_volume
        {
            Ownership::Captured
        } else {
            Ownership::Changed
        }
    }

    #[cfg(test)]
    fn stop_command(&self) -> Command {
        let mut command = crate::tool::Tool::Docker.command();
        command.args(["container", "stop", "--timeout", "0", &self.server.id]);
        command
    }

    fn acknowledged(&self, elapsed: u64, current: &Server) -> bool {
        lifecycle::release(
            elapsed,
            self.ownership(current),
            if current.running {
                Running::Yes
            } else {
                Running::No
            },
        ) == Release::Acknowledged
    }
}

#[derive(Debug)]
struct Controller {
    started: Instant,
}

impl Controller {
    fn start_before_effects() -> Self {
        Self {
            started: Instant::now(),
        }
    }

    fn elapsed(&self) -> u64 {
        self.started.elapsed().as_secs()
    }

    fn require_work(
        &self,
        server: &CapturedBuildKit,
        current: &Server,
        capacity: Capacity,
    ) -> Result<()> {
        ensure!(
            lifecycle::work(self.elapsed(), server.ownership(current), capacity) == Work::Continue,
            "owned image preparation must stop new work"
        );
        Ok(())
    }
}

/// Print the actual checked recipe without preparing an environment.
///
/// # Errors
/// Refuses missing or changed recipe inputs.
pub(crate) fn print_plan(root: &Path) -> Result<()> {
    println!("{}", serde_json::to_string_pretty(&Plan::capture(root)?)?);
    Ok(())
}
#[derive(Debug)]
struct LoadedImage {
    id: String,
    platform: String,
}

#[derive(Debug)]
struct Observation {
    capacity: MeasuredCapacity,
    host_memory_available: u64,
    guest_memory_available: u64,
    supported_native_route: bool,
    contention: Contention,
}

#[derive(Debug)]
enum Contention {
    Admitted,
    Saturated,
}

impl Observation {
    fn admit(&self) -> Result<()> {
        ensure!(
            self.supported_native_route,
            "bounded preparation requires the validated Darwin-arm64/Linux-arm64 route"
        );
        self.capacity.require_admission()?;
        ensure!(
            self.host_memory_available >= MEMORY_BYTES
                && self.guest_memory_available >= MEMORY_BYTES,
            "BuildKit memory allocation does not fit current available memory"
        );
        ensure!(
            matches!(self.contention, Contention::Admitted),
            "current workload contention is unknown or saturated"
        );
        Ok(())
    }

    fn continuing(&self, initial: &Self) -> Result<Capacity> {
        for (current_bytes, initial_bytes, current_inodes, initial_inodes) in [
            (
                self.capacity.host_bytes,
                initial.capacity.host_bytes,
                self.capacity.host_inodes,
                initial.capacity.host_inodes,
            ),
            (
                self.capacity.guest_bytes,
                initial.capacity.guest_bytes,
                self.capacity.guest_inodes,
                initial.capacity.guest_inodes,
            ),
        ] {
            ensure!(
                lifecycle::observed_storage(
                    Some(initial_bytes),
                    Some(current_bytes),
                    FLOOR_BYTES,
                    BUILD_BYTES + TEMPORARY_BYTES + MARGIN_BYTES
                ) == lifecycle::ObservedStorage::Admitted,
                "observed storage crossed the protected floor or declared allocation"
            );
            match (current_inodes, initial_inodes) {
                (Inodes::Available(current), Inodes::Available(initial)) => {
                    ensure!(
                        lifecycle::observed_storage(
                            Some(initial),
                            Some(current),
                            FLOOR_INODES,
                            BUILD_INODES
                        ) == lifecycle::ObservedStorage::Admitted,
                        "observed inodes crossed the protected floor or declared allocation"
                    );
                }
                (Inodes::NotApplicable, Inodes::NotApplicable) if cfg!(windows) => {}
                _ => anyhow::bail!("current inode observation is unknown"),
            }
        }
        ensure!(
            self.host_memory_available >= MEMORY_BYTES
                && self.guest_memory_available >= MEMORY_BYTES,
            "current available memory crossed the declared builder allocation"
        );
        ensure!(
            matches!(self.contention, Contention::Admitted),
            "current load is unknown or saturated"
        );
        Ok(Capacity::Admitted)
    }
}

#[derive(Debug)]
struct CommandReceipt {
    status: Option<i32>,
    originals: Originals,
}

#[derive(Debug)]
enum Originals {
    Files {
        stdout: PathBuf,
        stderr: PathBuf,
    },
    ReleaseMemory {
        stdout: Vec<u8>,
        stderr: Vec<u8>,
        recording: anyhow::Error,
    },
}

struct OperationFiles {
    stdout: PathBuf,
    stderr: PathBuf,
    output: fs::File,
    error: fs::File,
}

/// A private port specific to this preparation, not a general job framework.
/// Production obtains every observation from bounded real commands and owned files.
/// The failing fixture scripts these operations and checks their exact order and IDs.
trait ImagePreparation {
    fn begin_release(&mut self);
    fn initial_observation(&mut self, root: &Path) -> Result<Observation>;
    fn observe(&mut self, server: Option<&CapturedBuildKit>) -> Result<Observation>;
    fn require_namespace_absent(&mut self, pending: &PendingBuilder) -> Result<()>;
    fn create(&mut self, pending: &PendingBuilder) -> Result<CommandReceipt>;
    fn bootstrap(
        &mut self,
        pending: &PendingBuilder,
        initial: &Observation,
    ) -> Result<CommandReceipt>;
    fn created_server(&mut self, pending: &PendingBuilder) -> Result<Server>;
    fn current_server(&mut self, server: &CapturedBuildKit) -> Result<Server>;
    fn effective_limits(&mut self, server: &CapturedBuildKit) -> Result<()>;
    fn build(
        &mut self,
        root: &Path,
        plan: &Plan,
        server: &CapturedBuildKit,
        initial: &Observation,
    ) -> Result<CommandReceipt>;
    fn stop(&mut self, server: &CapturedBuildKit) -> Result<CommandReceipt>;
    fn kill(&mut self, server: &CapturedBuildKit) -> Result<CommandReceipt>;
    fn loaded_image(&mut self, image: &str) -> Result<LoadedImage>;
    fn retain(&mut self, operation: &'static str, result: &CommandReceipt) -> Result<()>;
    fn retain_server(&mut self, operation: &'static str, server: &Server) -> Result<()>;
    fn retain_failure(&mut self, failure: &anyhow::Error) -> Result<()>;
}

fn natural_success(receipt: &CommandReceipt) -> Result<()> {
    ensure!(
        receipt.status == Some(0),
        "image operation failed or did not finish naturally"
    );
    Ok(())
}

fn remember_release(result: Result<()>, failure: &mut Option<anyhow::Error>) {
    if let Err(error) = result {
        match failure {
            None => *failure = Some(error),
            Some(_) => {
                platform::cleanup::report::<(), _>(
                    Err(error),
                    "retain owned image release evidence",
                );
            }
        }
    }
}

fn settle(
    driver: &mut impl ImagePreparation,
    controller: &Controller,
    owned: &CapturedBuildKit,
) -> Result<()> {
    driver.begin_release();
    let current = driver.current_server(owned)?;
    let mut recording = None;
    remember_release(
        driver.retain_server("release-before", &current),
        &mut recording,
    );
    match lifecycle::release(
        controller.elapsed(),
        owned.ownership(&current),
        if current.running {
            Running::Yes
        } else {
            Running::No
        },
    ) {
        Release::Acknowledged => return recording.map_or(Ok(()), Err),
        Release::Unsettled => anyhow::bail!("owned BuildKit stop is unacknowledged"),
        Release::Stop => {}
    }
    let stop_failed = match driver.stop(owned) {
        Ok(receipt) => {
            remember_release(driver.retain("stop", &receipt), &mut recording);
            receipt.status != Some(0)
        }
        Err(error) => {
            remember_release(driver.retain_failure(&error), &mut recording);
            true
        }
    };
    let current = driver.current_server(owned)?;
    remember_release(
        driver.retain_server("stop-readback", &current),
        &mut recording,
    );
    ensure!(
        owned.ownership(&current) == Ownership::Captured,
        "owned server identity changed after stop"
    );
    if stop_failed || current.running {
        remember_release(
            driver.retain_server("kill-before", &current),
            &mut recording,
        );
        ensure!(
            controller.elapsed() < lifecycle::WALL_SECONDS,
            "owned server release deadline expired"
        );
        if current.running {
            let killed = driver.kill(owned)?;
            remember_release(driver.retain("kill", &killed), &mut recording);
            natural_success(&killed)?;
        }
    }
    let current = driver.current_server(owned)?;
    remember_release(
        driver.retain_server("release-after", &current),
        &mut recording,
    );
    ensure!(
        owned.acknowledged(controller.elapsed(), &current),
        "owned BuildKit server stop is unacknowledged"
    );
    recording.map_or(Ok(()), Err)
}

fn execute_owned(
    root: &Path,
    plan: &Plan,
    pending: PendingBuilder,
    controller: &Controller,
    driver: &mut impl ImagePreparation,
) -> Result<LoadedImage> {
    plan.require_unchanged(root)?;
    let initial = driver.initial_observation(root)?;
    initial.admit()?;
    driver.require_namespace_absent(&pending)?;
    ensure!(
        controller.elapsed() < lifecycle::WORK_SECONDS,
        "image preparation cannot start after its work deadline"
    );
    let created = driver.create(&pending)?;
    driver.retain("create", &created)?;
    natural_success(&created)?;
    let bootstrapped = driver.bootstrap(&pending, &initial);
    let retained = match &bootstrapped {
        Ok(receipt) => driver.retain("bootstrap", receipt),
        Err(error) => {
            driver.begin_release();
            driver.retain_failure(error)
        }
    };
    // A partial bootstrap can already have created the server.
    // Read its exact fresh-name identity before interpreting status or limits.
    let server = driver.created_server(&pending).map_err(|error| {
        error.context("bootstrap server identity could not be recovered; ownership is unsettled")
    })?;
    let owned = pending.capture_identity(server)?;
    let captured = driver.retain_server("captured", &owned.server);
    let outcome = (|| -> Result<LoadedImage> {
        retained?;
        captured?;
        natural_success(&bootstrapped?)?;
        owned.require_limits()?;
        driver.effective_limits(&owned)?;
        plan.require_unchanged(root)?;
        let current = driver.current_server(&owned)?;
        let observation = driver.observe(Some(&owned))?;
        controller.require_work(&owned, &current, observation.continuing(&initial)?)?;
        let built = driver.build(root, plan, &owned, &initial)?;
        driver.retain("build", &built)?;
        natural_success(&built)?;
        plan.require_unchanged(root)?;
        let loaded = driver.loaded_image(&plan.image)?;
        ensure!(
            loaded.id.strip_prefix("sha256:").is_some_and(canonical_id)
                && loaded.platform == LINUX_AMD64,
            "loaded proof image is missing or has an unsupported identity/platform"
        );
        Ok(loaded)
    })();
    let released = settle(driver, controller, &owned);
    match (outcome, released) {
        (Ok(image), Ok(())) => Ok(image),
        (Err(primary), Ok(())) => {
            driver.retain_failure(&primary)?;
            Err(primary)
        }
        (Ok(_), Err(release)) => {
            driver.retain_failure(&release)?;
            Err(release.context("image prepared but owned server release is unsettled"))
        }
        (Err(primary), Err(release)) => {
            driver.retain_failure(&primary)?;
            driver.retain_failure(&release)?;
            Err(primary
                .context("owned BuildKit release also failed; original diagnostics retained"))
        }
    }
}
#[derive(Clone, Copy)]
enum Monitor<'a> {
    Bootstrap(&'a Observation),
    Build(&'a CapturedBuildKit, &'a Observation),
}

const DF: &str = "/bin/df";
const SYSCTL: &str = "/usr/sbin/sysctl";
const VM_STAT: &str = "/usr/bin/vm_stat";
const GUEST_DF: &str = "/bin/df";
const GUEST_CAT: &str = "/bin/cat";
const GUEST_TIMEOUT: &str = "/usr/bin/timeout";
const GUEST_MEMORY: &str = "/proc/meminfo";
const GUEST_LOAD: &str = "/proc/loadavg";
const BUILDKIT_STATE: &str = "/var/lib/buildkit";
#[cfg(unix)]
const PRIVATE_MODE: u32 = 0o700;
const PROGRESS_LIMIT: u64 = 64 * 1024 * 1024;
const CAPACITY_INTERVAL: Duration = Duration::from_secs(30);
const METADATA_SECONDS: u64 = 10;
const RELEASE_CALL_SECONDS: u64 = 5;
const PROBE_MEMORY: &str = "128m";
const PROBE_SECONDS: &str = "5";
const NATIVE_ARM64: &str = "linux/arm64";
const SERVER_TEMPLATE: &str = r#"{"id":{{json .Id}},"name":{{json .Name}},"created":{{json .Created}},"image":{{json .Image}},"state_volume":{{range .Mounts}}{{if eq .Destination "/var/lib/buildkit"}}{{json .Name}}{{end}}{{end}},"running":{{json .State.Running}},"cpu_period":{{json .HostConfig.CpuPeriod}},"cpu_quota":{{json .HostConfig.CpuQuota}},"memory":{{json .HostConfig.Memory}},"memory_swap":{{json .HostConfig.MemorySwap}},"restart_policy":{{json .HostConfig.RestartPolicy.Name}}}"#;
const IMAGE_TEMPLATE: &str = r"{{.Id}} {{.Os}}/{{.Architecture}}";
const IMAGE_ID_FILE: &str = "built-image.id";
const DAEMON_TEMPLATE: &str = r"{{.OperatingSystem}}|{{.NCPU}}|{{.MemTotal}}";
const CLIENT_TEMPLATE: &str = r"{{.Client.Os}}/{{.Client.Arch}} {{.Server.Os}}/{{.Server.Arch}}";
const CONTEXT_TEMPLATE: &str = r#"{{(index .Endpoints "docker").Host}}"#;
const CPU_MAX: &str = "/sys/fs/cgroup/cpu.max";
const MEMORY_MAX: &str = "/sys/fs/cgroup/memory.max";
const SWAP_MAX: &str = "/sys/fs/cgroup/memory.swap.max";

#[derive(Debug)]
struct Docker {
    controller: Controller,
    directory: PathBuf,
    context: String,
    endpoint: String,
    probe: String,
    operations: u64,
    releasing: bool,
    recording_failed: bool,
}

#[cfg(unix)]
fn no_links(path: &Path) -> Result<()> {
    for ancestor in path.ancestors() {
        let metadata = fs::symlink_metadata(ancestor)?;
        ensure!(
            !metadata.file_type().is_symlink(),
            "private output has a symlink ancestor"
        );
        ensure!(
            metadata.is_dir(),
            "private output ancestor is not a directory"
        );
    }
    Ok(())
}

#[cfg(unix)]
fn private_output(root: &Path, requested: &Path) -> Result<PathBuf> {
    ensure!(
        requested.is_absolute() && !requested.exists(),
        "output must be a new absolute private path"
    );
    let parent = requested.parent().context("private output parent")?;
    no_links(parent)?;
    let parent = crate::canonical(parent)?;
    let source = crate::canonical(root)?;
    let working = crate::canonical(&std::env::current_dir()?)?;
    ensure!(
        !parent.starts_with(&source) && !parent.starts_with(&working),
        "mutable output must be outside source and the watched working directory"
    );
    let home = platform::home::home().context("owner home")?;
    ensure!(
        parent.starts_with(crate::canonical(&home)?),
        "output must remain in the owner's home"
    );
    #[cfg(unix)]
    {
        use std::os::unix::fs::{DirBuilderExt as _, MetadataExt as _, PermissionsExt as _};
        ensure!(
            fs::metadata(&parent)?.uid() == rustix::process::geteuid().as_raw(),
            "private parent belongs to a different owner"
        );
        ensure!(
            fs::metadata(&parent)?.permissions().mode() & SHARED_WRITE_MASK == 0,
            "private parent can be renamed by a different owner"
        );
        fs::DirBuilder::new().mode(PRIVATE_MODE).create(requested)?;
    }
    Ok(crate::canonical(requested)?)
}

#[cfg(not(unix))]
fn private_output(_root: &Path, _requested: &Path) -> Result<PathBuf> {
    anyhow::bail!("owned ARM64 BuildKit preparation requires its native Mac route")
}

impl Docker {
    fn new(root: &Path, directory: PathBuf, controller: &Controller) -> Result<Self> {
        ensure!(
            cfg!(all(target_os = "macos", target_arch = "aarch64")),
            "the reviewed BuildKit pin only supports native Mac ARM64 preparation"
        );
        for path in [DF, SYSCTL, VM_STAT] {
            ensure!(
                fs::metadata(path)?.is_file(),
                "required fixed native capacity tool is missing"
            );
        }
        let mut driver = Self {
            controller: Controller {
                started: controller.started,
            },
            directory,
            context: String::new(),
            endpoint: String::new(),
            probe: String::new(),
            operations: 0,
            releasing: false,
            recording_failed: false,
        };
        fs::create_dir(driver.directory.join("buildx"))?;
        let mut command = crate::tool::Tool::Docker.command();
        command.args(["context", "show"]);
        let context = driver.call("context", command, None)?;
        Docker::text(&context)?
            .trim()
            .clone_into(&mut driver.context);
        ensure!(
            !driver.context.is_empty()
                && !driver.context.starts_with('-')
                && !driver.context.chars().any(char::is_control),
            "invalid current Docker context"
        );
        let endpoint = driver.call(
            "endpoint",
            driver.docker(&[
                "context",
                "inspect",
                "--format",
                CONTEXT_TEMPLATE,
                &driver.context,
            ]),
            None,
        )?;
        let endpoint = Docker::text(&endpoint)?;
        let socket = endpoint
            .trim()
            .strip_prefix("unix://")
            .context("preparation requires a local Unix Docker endpoint")?;
        ensure!(
            Path::new(socket).is_absolute(),
            "Docker endpoint is not absolute"
        );
        #[cfg(unix)]
        {
            use std::os::unix::fs::FileTypeExt as _;
            ensure!(
                fs::metadata(socket)?.file_type().is_socket(),
                "Docker endpoint is not an existing local socket"
            );
        }
        endpoint.trim().clone_into(&mut driver.endpoint);
        let client = driver.call(
            "client",
            driver.docker(&["version", "--format", CLIENT_TEMPLATE]),
            None,
        )?;
        ensure!(
            Docker::text(&client)?.trim() == "darwin/arm64 linux/arm64",
            "prepared builder platform differs from the actual client and daemon"
        );
        let version = driver.call(
            "buildx-version",
            driver.docker(&["buildx", "version"]),
            None,
        )?;
        ensure!(
            Self::text(&version)?
                .trim()
                .starts_with("github.com/docker/buildx "),
            "the current Buildx program has no supported version identity"
        );
        driver.capture_capacity_probe(root)?;
        Ok(driver)
    }

    fn capture_capacity_probe(&mut self, root: &Path) -> Result<()> {
        let probe = crate::alloy::image(root)?;
        let identity = self.call(
            "probe-image",
            self.docker(&["image", "inspect", "--format", IMAGE_TEMPLATE, &probe]),
            None,
        )?;
        let text = Docker::text(&identity)?;
        let mut words = text.split_whitespace();
        let id = words.next().context("probe image ID")?;
        ensure!(
            id.strip_prefix("sha256:").is_some_and(canonical_id)
                && words.next() == Some(NATIVE_ARM64)
                && words.next().is_none(),
            "existing managed capacity probe image is missing or unsupported"
        );
        id.clone_into(&mut self.probe);
        Ok(())
    }

    fn docker(&self, arguments: &[&str]) -> Command {
        let mut command = crate::tool::Tool::Docker.command();
        command.env("BUILDX_CONFIG", self.directory.join("buildx"));
        if self.endpoint.is_empty() {
            command.args(["--context", &self.context]);
        } else {
            command.args(["--host", &self.endpoint]);
        }
        command.args(arguments);
        command
    }

    fn text(receipt: &CommandReceipt) -> Result<String> {
        natural_success(receipt)?;
        match &receipt.originals {
            Originals::Files { stdout, .. } => read_metadata(stdout),
            Originals::ReleaseMemory { stdout, .. } => {
                ensure!(
                    stdout.len() <= METADATA_LIMIT,
                    "release metadata exceeds its bound"
                );
                Ok(String::from_utf8(stdout.clone())?)
            }
        }
    }

    fn operation_files(&self, prefix: &str, command: &Command) -> Result<OperationFiles> {
        let stdout = self.directory.join(format!("{prefix}.stdout"));
        let stderr = self.directory.join(format!("{prefix}.stderr"));
        let output = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&stdout)?;
        let error = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&stderr)?;
        let arguments: Vec<_> = command
            .get_args()
            .map(std::ffi::OsStr::as_encoded_bytes)
            .collect();
        fs::write(
            self.directory.join(format!("{prefix}.request.json")),
            serde_json::to_vec(&(command.get_program().as_encoded_bytes(), arguments))?,
        )?;
        Ok(OperationFiles {
            stdout,
            stderr,
            output,
            error,
        })
    }

    fn stop_client(&self, child: &mut std::process::Child) {
        platform::cleanup::report(child.kill(), "stop this image operation's Docker client");
        while self.controller.elapsed() < lifecycle::WALL_SECONDS {
            match child.try_wait() {
                Ok(Some(_)) => break,
                Ok(None) => std::thread::sleep(POLL),
                Err(error) => {
                    platform::cleanup::report::<(), _>(
                        Err(error),
                        "read this image operation's client termination",
                    );
                    break;
                }
            }
        }
    }

    fn await_file_command(
        &mut self,
        child: &mut std::process::Child,
        files: &OperationFiles,
        started: Instant,
        monitor: Option<Monitor<'_>>,
    ) -> Result<Option<i32>> {
        let mut observed = Instant::now();
        let limit = if self.releasing {
            lifecycle::WALL_SECONDS
        } else {
            lifecycle::WORK_SECONDS
        };
        let call_limit = if self.releasing {
            RELEASE_CALL_SECONDS
        } else {
            METADATA_SECONDS
        };
        loop {
            ensure!(
                self.controller.elapsed() < limit,
                "owned image lifetime reached its stop boundary"
            );
            if monitor.is_none() {
                ensure!(
                    started.elapsed() < Duration::from_secs(call_limit),
                    "owned metadata or release command exceeded its bounded duration"
                );
            }
            ensure!(
                files.output.metadata()?.len() <= PROGRESS_LIMIT
                    && files.error.metadata()?.len() <= PROGRESS_LIMIT,
                "operation output exceeded its declared bound"
            );
            if let Some(status) = child.try_wait()? {
                return Ok(status.code());
            }
            if let Some(monitor) = monitor
                && observed.elapsed() >= CAPACITY_INTERVAL
            {
                match monitor {
                    Monitor::Bootstrap(initial) => {
                        self.observe(None)?.continuing(initial)?;
                    }
                    Monitor::Build(owned, initial) => {
                        let current = self.current_server(owned)?;
                        let capacity = self.observe(Some(owned))?.continuing(initial)?;
                        self.controller.require_work(owned, &current, capacity)?;
                    }
                }
                observed = Instant::now();
            }
            std::thread::sleep(POLL);
        }
    }

    fn finish_file_command(
        &self,
        prefix: &str,
        files: &OperationFiles,
        terminal: &Result<Option<i32>>,
        started: Instant,
    ) -> Result<()> {
        files.output.sync_all()?;
        files.error.sync_all()?;
        let record = match terminal {
            Ok(status) => {
                serde_json::json!({"status":status,"elapsed_ms":started.elapsed().as_millis()})
            }
            Err(error) => {
                serde_json::json!({"status":null,"failure":error.to_string(),"elapsed_ms":started.elapsed().as_millis()})
            }
        };
        fs::write(
            self.directory.join(format!("{prefix}.terminal.json")),
            serde_json::to_vec(&record)?,
        )?;
        Ok(())
    }

    fn call(
        &mut self,
        operation: &str,
        mut command: Command,
        monitor: Option<Monitor<'_>>,
    ) -> Result<CommandReceipt> {
        self.operations = self
            .operations
            .checked_add(1)
            .context("operation count overflow")?;
        let prefix = format!("{}-{operation}", self.operations);
        let files = match self.operation_files(&prefix, &command) {
            Ok(files) => files,
            Err(recording) if self.releasing => {
                self.recording_failed = true;
                return self.release_memory(command, recording);
            }
            Err(error) => return Err(error),
        };
        let mut child = command
            .stdin(Stdio::null())
            .stdout(Stdio::from(files.output.try_clone()?))
            .stderr(Stdio::from(files.error.try_clone()?))
            .spawn()?;
        let started = Instant::now();
        let terminal = self.await_file_command(&mut child, &files, started, monitor);
        if terminal.is_err() {
            self.stop_client(&mut child);
        }
        if let Err(error) = self.finish_file_command(&prefix, &files, &terminal, started) {
            self.recording_failed = true;
            return Err(error);
        }
        Ok(CommandReceipt {
            status: terminal?,
            originals: Originals::Files {
                stdout: files.stdout,
                stderr: files.stderr,
            },
        })
    }

    fn release_memory(
        &self,
        mut command: Command,
        recording: anyhow::Error,
    ) -> Result<CommandReceipt> {
        ensure!(
            self.releasing,
            "memory output is restricted to owned release"
        );
        let mut child = command
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;
        let started = Instant::now();
        let outcome = (|| -> Result<CommandReceipt> {
            let stdout = release_reader(child.stdout.take().context("owned release stdout")?)?;
            let stderr = release_reader(child.stderr.take().context("owned release stderr")?)?;
            let mut output = None;
            let mut error = None;
            let mut status = None;
            loop {
                ensure!(
                    self.controller.elapsed() < lifecycle::WALL_SECONDS
                        && started.elapsed() < Duration::from_secs(RELEASE_CALL_SECONDS),
                    "bounded in-memory release readback expired"
                );
                if output.is_none() {
                    output = release_bytes(&stdout)?;
                }
                if error.is_none() {
                    error = release_bytes(&stderr)?;
                }
                if status.is_none() {
                    status = child.try_wait()?;
                }
                if let (Some(output), Some(error), Some(status)) = (&output, &error, &status) {
                    ensure!(
                        output.len() <= METADATA_LIMIT && error.len() <= METADATA_LIMIT,
                        "release output exceeded its memory bound"
                    );
                    return Ok(CommandReceipt {
                        status: status.code(),
                        originals: Originals::ReleaseMemory {
                            stdout: output.clone(),
                            stderr: error.clone(),
                            recording,
                        },
                    });
                }
                std::thread::sleep(POLL);
            }
        })();
        if outcome.is_err() {
            self.stop_client(&mut child);
        }
        outcome
    }

    fn guest(
        &mut self,
        operation: &str,
        owned: Option<&CapturedBuildKit>,
        program: &str,
        arguments: &[&str],
    ) -> Result<String> {
        let mut command = match owned {
            Some(server) => self.docker(&["exec", &server.server.id, program]),
            None => self.docker(&[
                "run",
                "--rm",
                "--pull",
                "never",
                "--platform",
                NATIVE_ARM64,
                "--network",
                "none",
                "--read-only",
                "--cpus",
                "1",
                "--memory",
                PROBE_MEMORY,
                "--memory-swap",
                PROBE_MEMORY,
                "--pids-limit",
                "32",
                "--entrypoint",
                GUEST_TIMEOUT,
                &self.probe,
                PROBE_SECONDS,
                program,
            ]),
        };
        command.args(arguments);
        let receipt = self.call(operation, command, None)?;
        Self::text(&receipt)
    }

    fn sample(&mut self, owned: Option<&CapturedBuildKit>) -> Result<Observation> {
        let mut command = platform::process::command(DF);
        command.arg("-Pk").arg(&self.directory);
        let disk = self.call("host-bytes", command, None)?;
        let host_bytes = df_available(&Self::text(&disk)?, "Available")?
            .checked_mul(KIB_BYTES)
            .context("host capacity overflow")?;
        let mut command = platform::process::command(DF);
        command.arg("-i").arg(&self.directory);
        let inode = self.call("host-inodes", command, None)?;
        let host_inodes = Inodes::Available(df_available(&Self::text(&inode)?, "ifree")?);
        let state = if owned.is_some() { BUILDKIT_STATE } else { "/" };
        let guest_bytes = df_available(
            &self.guest("guest-bytes", owned, GUEST_DF, &["-Pk", state])?,
            "Available",
        )?
        .checked_mul(KIB_BYTES)
        .context("guest capacity overflow")?;
        let guest_inodes = Inodes::Available(df_available(
            &self.guest("guest-inodes", owned, GUEST_DF, &["-Pi", state])?,
            "IFree",
        )?);
        let host_memory = self.call("host-memory", platform::process::command(VM_STAT), None)?;
        let host_memory_available = mac_free_memory(&Self::text(&host_memory)?)?;
        let guest_memory_available =
            memory_available(&self.guest("guest-memory", owned, GUEST_CAT, &[GUEST_MEMORY])?)?;
        let mut command = platform::process::command(SYSCTL);
        command.args(["-n", "hw.ncpu", "vm.loadavg"]);
        let host_load = self.call("host-load", command, None)?;
        let guest_load = self.guest("guest-load", owned, GUEST_CAT, &[GUEST_LOAD])?;
        let daemon = self.call(
            "daemon",
            self.docker(&["info", "--format", DAEMON_TEMPLATE]),
            None,
        )?;
        let daemon = Self::text(&daemon)?;
        let mut fields = daemon.trim().split('|');
        let provider = fields.next().context("daemon provider")?;
        let cpus = fields.next().context("daemon CPU count")?.parse::<u64>()?;
        let memory = fields.next().context("daemon memory")?.parse::<u64>()?;
        ensure!(
            matches!(provider, "OrbStack" | "Docker Desktop")
                && fields.next().is_none()
                && cpus > BUILDER_CPUS
                && memory >= MEMORY_BYTES,
            "daemon allocation or provider is unsupported"
        );
        let contention = if load_admitted(&Self::text(&host_load)?, &guest_load, cpus)? {
            Contention::Admitted
        } else {
            Contention::Saturated
        };
        Ok(Observation {
            capacity: MeasuredCapacity {
                host_bytes,
                guest_bytes,
                host_inodes,
                guest_inodes,
            },
            host_memory_available,
            guest_memory_available,
            supported_native_route: true,
            contention,
        })
    }
}

impl ImagePreparation for Docker {
    fn begin_release(&mut self) {
        self.releasing = true;
    }
    fn initial_observation(&mut self, _root: &Path) -> Result<Observation> {
        self.sample(None)
    }
    fn observe(&mut self, server: Option<&CapturedBuildKit>) -> Result<Observation> {
        self.sample(server)
    }
    fn require_namespace_absent(&mut self, pending: &PendingBuilder) -> Result<()> {
        for (operation, command) in [
            (
                "server-collision",
                self.docker(&[
                    "container",
                    "ls",
                    "--all",
                    "--filter",
                    &format!("name=^{}$", pending.container_name),
                    "--format",
                    "{{.ID}}",
                ]),
            ),
            (
                "volume-collision",
                self.docker(&[
                    "volume",
                    "ls",
                    "--filter",
                    &format!("name=^{}$", pending.state_volume),
                    "--format",
                    "{{.Name}}",
                ]),
            ),
        ] {
            let receipt = self.call(operation, command, None)?;
            ensure!(
                Self::text(&receipt)?.trim().is_empty(),
                "fresh builder namespace already exists"
            );
        }
        // A generic Docker failure doesn't prove that buildx inspect found nothing.
        // The private Buildx directory must also contain the exact fresh configuration name.
        ensure!(
            !self
                .directory
                .join("buildx/instances")
                .join(&pending.name)
                .exists(),
            "fresh Buildx configuration already exists"
        );
        Ok(())
    }
    fn create(&mut self, pending: &PendingBuilder) -> Result<CommandReceipt> {
        let mut command = self.docker(&[
            "buildx",
            "create",
            "--name",
            &pending.name,
            "--driver",
            "docker-container",
        ]);
        for option in [
            format!("image={BUILDKIT_IMAGE}"),
            format!("cpu-period={CPU_PERIOD}"),
            format!("cpu-quota={CPU_QUOTA}"),
            format!("memory={MEMORY_BYTES}"),
            format!("memory-swap={MEMORY_BYTES}"),
            format!("restart-policy={RESTART_POLICY}"),
        ] {
            command.arg("--driver-opt").arg(option);
        }
        self.call("create", command, None)
    }
    fn bootstrap(
        &mut self,
        pending: &PendingBuilder,
        initial: &Observation,
    ) -> Result<CommandReceipt> {
        let command = self.docker(&["buildx", "inspect", "--bootstrap", &pending.name]);
        // Bootstrap uses the total work boundary, not the short metadata bound.
        self.call("bootstrap", command, Some(Monitor::Bootstrap(initial)))
    }
    fn created_server(&mut self, pending: &PendingBuilder) -> Result<Server> {
        let command = self.docker(&[
            "container",
            "inspect",
            "--format",
            SERVER_TEMPLATE,
            &pending.container_name,
        ]);
        let receipt = self.call("capture", command, None)?;
        decode(Self::text(&receipt)?.as_bytes())
    }
    fn current_server(&mut self, server: &CapturedBuildKit) -> Result<Server> {
        let command = self.docker(&[
            "container",
            "inspect",
            "--format",
            SERVER_TEMPLATE,
            &server.server.id,
        ]);
        let receipt = self.call("server-readback", command, None)?;
        decode(Self::text(&receipt)?.as_bytes())
    }
    fn effective_limits(&mut self, server: &CapturedBuildKit) -> Result<()> {
        let values = self.guest(
            "effective-limits",
            Some(server),
            GUEST_CAT,
            &[CPU_MAX, MEMORY_MAX, SWAP_MAX],
        )?;
        ensure!(
            values.lines().eq(["200000 100000", "8589934592", "0"]),
            "effective BuildKit cgroup limits differ"
        );
        Ok(())
    }
    fn build(
        &mut self,
        root: &Path,
        plan: &Plan,
        server: &CapturedBuildKit,
        initial: &Observation,
    ) -> Result<CommandReceipt> {
        let context = self.directory.join("context");
        fs::create_dir(&context)?;
        let definition = fs::read(root.join(crate::skill_proofs::PROOF_IMAGE))?;
        let expected = plan
            .inputs
            .iter()
            .find(|input| input.path == crate::skill_proofs::PROOF_IMAGE)
            .context("captured image definition")?;
        ensure!(
            crate::build_inputs::hex(&Sha256::digest(&definition)) == expected.sha256,
            "image definition changed before snapshot"
        );
        fs::write(context.join("Dockerfile"), definition)?;
        let mut command = self.docker(&[
            "buildx",
            "build",
            "--builder",
            &server.builder,
            "--platform",
            LINUX_AMD64,
            "--progress",
            "plain",
            "--tag",
            &plan.image,
            "--file",
        ]);
        command.arg(context.join("Dockerfile"));
        for pin in &plan.arguments {
            command.arg("--build-arg").arg(pin);
        }
        command
            .arg("--iidfile")
            .arg(self.directory.join(IMAGE_ID_FILE));
        command.arg("--load").arg(context);
        self.call("build", command, Some(Monitor::Build(server, initial)))
    }
    fn stop(&mut self, server: &CapturedBuildKit) -> Result<CommandReceipt> {
        self.call(
            "stop",
            self.docker(&["container", "stop", "--timeout", "0", &server.server.id]),
            None,
        )
    }
    fn kill(&mut self, server: &CapturedBuildKit) -> Result<CommandReceipt> {
        self.call(
            "kill",
            self.docker(&["container", "kill", "--signal", "KILL", &server.server.id]),
            None,
        )
    }
    fn loaded_image(&mut self, image: &str) -> Result<LoadedImage> {
        let produced = self.directory.join(IMAGE_ID_FILE);
        ensure!(
            fs::symlink_metadata(&produced)?.is_file()
                && fs::metadata(&produced)?.len() <= METADATA_LIMIT as u64,
            "the owned build did not preserve its bounded image ID"
        );
        let produced = read_metadata(&produced)?;
        let produced = produced.trim();
        ensure!(
            produced.strip_prefix("sha256:").is_some_and(canonical_id),
            "the owned build image ID is malformed"
        );
        let command = self.docker(&["image", "inspect", "--format", IMAGE_TEMPLATE, image]);
        let receipt = self.call("loaded-image", command, None)?;
        let text = Self::text(&receipt)?;
        let mut fields = text.split_whitespace();
        let id = fields.next().context("loaded image ID")?.to_owned();
        let platform = fields.next().context("loaded image platform")?.to_owned();
        ensure!(
            fields.next().is_none() && id == produced,
            "the loaded tag differs from this owned build's image ID"
        );
        Ok(LoadedImage { id, platform })
    }
    fn retain(&mut self, operation: &'static str, result: &CommandReceipt) -> Result<()> {
        let (stdout, stderr) = match &result.originals {
            Originals::Files { stdout, stderr } => (stdout, stderr),
            Originals::ReleaseMemory {
                stdout,
                stderr,
                recording,
            } => {
                for (kind, bytes) in [("stdout", stdout), ("stderr", stderr)] {
                    let name = format!("{}-{operation}-{kind}.release-memory", self.operations);
                    platform::cleanup::report(
                        fs::write(self.directory.join(name), bytes),
                        "retain bounded owned release output",
                    );
                }
                return Err(anyhow::anyhow!(
                    "owned release required an unretained original: {recording:#}"
                ));
            }
        };
        for (kind, path) in [("stdout", stdout), ("stderr", stderr)] {
            ensure!(
                fs::symlink_metadata(path)?.is_file()
                    && path.parent() == Some(self.directory.as_path()),
                "receipt refers outside owned original output"
            );
            let file = fs::File::open(path)?;
            ensure!(
                file.metadata()?.len() <= PROGRESS_LIMIT,
                "retained raw output exceeds its bound"
            );
            let mut bytes = Vec::new();
            file.take(PROGRESS_LIMIT + 1).read_to_end(&mut bytes)?;
            ensure!(
                bytes.len() as u64 <= PROGRESS_LIMIT,
                "retained original output grew beyond its bound"
            );
            let hash = crate::build_inputs::hex(&Sha256::digest(&bytes));
            let name = path
                .file_name()
                .and_then(std::ffi::OsStr::to_str)
                .context("original output filename")?;
            let row = serde_json::json!({"operation":operation,"kind":kind,"file":name,"bytes":bytes.len(),"sha256":hash,"status":result.status});
            let mut manifest = fs::OpenOptions::new()
                .append(true)
                .create(true)
                .open(self.directory.join("artifacts.jsonl"))?;
            serde_json::to_writer(&mut manifest, &row)?;
            manifest.write_all(b"\n")?;
            manifest.sync_all()?;
        }
        Ok(())
    }
    fn retain_server(&mut self, operation: &'static str, server: &Server) -> Result<()> {
        if operation == "captured" {
            fs::write(
                self.directory.join("custody.json"),
                serde_json::to_vec(&serde_json::json!({
                    "schema":SCHEMA,"endpoint":self.endpoint,"server":server,
                    "wall_seconds":lifecycle::WALL_SECONDS
                }))?,
            )?;
        }
        fs::write(
            self.directory.join(format!("{operation}.server.json")),
            serde_json::to_vec(server)?,
        )?;
        ensure!(
            !self.recording_failed,
            "owned release evidence retention was incomplete"
        );
        Ok(())
    }
    fn retain_failure(&mut self, failure: &anyhow::Error) -> Result<()> {
        let bytes = format!("{failure:#}").into_bytes();
        let hash = crate::build_inputs::hex(&Sha256::digest(&bytes));
        fs::write(self.directory.join(format!("{hash}.failure")), bytes).map_err(Into::into)
    }
}

fn release_reader(
    stream: impl std::io::Read + Send + 'static,
) -> Result<std::sync::mpsc::Receiver<std::io::Result<Vec<u8>>>> {
    let (sender, receiver) = std::sync::mpsc::sync_channel(1);
    let _reader = std::thread::Builder::new().spawn(move || {
        let mut bytes = Vec::new();
        let result = stream
            .take((METADATA_LIMIT + 1) as u64)
            .read_to_end(&mut bytes)
            .map(|_| bytes);
        platform::cleanup::report(sender.send(result), "retain owned release pipe readback");
    })?;
    Ok(receiver)
}

fn release_bytes(
    receiver: &std::sync::mpsc::Receiver<std::io::Result<Vec<u8>>>,
) -> Result<Option<Vec<u8>>> {
    match receiver.try_recv() {
        Ok(result) => {
            let bytes = result?;
            ensure!(
                bytes.len() <= METADATA_LIMIT,
                "owned release pipe exceeded its bound"
            );
            Ok(Some(bytes))
        }
        Err(std::sync::mpsc::TryRecvError::Empty) => Ok(None),
        Err(std::sync::mpsc::TryRecvError::Disconnected) => {
            anyhow::bail!("owned release pipe ended without readback")
        }
    }
}

fn read_metadata(path: &Path) -> Result<String> {
    ensure!(
        fs::symlink_metadata(path)?.is_file(),
        "metadata is not a regular file"
    );
    let file = fs::File::open(path)?;
    ensure!(
        file.metadata()?.len() <= METADATA_LIMIT as u64,
        "metadata exceeds its read bound"
    );
    let mut bytes = Vec::new();
    file.take((METADATA_LIMIT + 1) as u64)
        .read_to_end(&mut bytes)?;
    ensure!(
        bytes.len() <= METADATA_LIMIT,
        "metadata grew during readback"
    );
    Ok(String::from_utf8(bytes)?)
}

fn df_available(text: &str, heading: &str) -> Result<u64> {
    let mut lines = text.lines().filter(|line| !line.trim().is_empty());
    let header = lines.next().context("capacity output has no header")?;
    let column = header
        .split_whitespace()
        .position(|word| word.eq_ignore_ascii_case(heading))
        .context("capacity output has no declared column")?;
    let mut minimum = None;
    for line in lines {
        let value = line
            .split_whitespace()
            .nth(column)
            .context("capacity row is incomplete")?
            .parse::<u64>()?;
        minimum = Some(minimum.map_or(value, |previous: u64| previous.min(value)));
    }
    minimum.context("capacity output has no measured row")
}

fn memory_available(text: &str) -> Result<u64> {
    let mut found = text
        .lines()
        .filter_map(|line| line.strip_prefix("MemAvailable:"));
    let field = found.next().context("guest memory is unmeasured")?;
    ensure!(found.next().is_none(), "duplicate guest memory observation");
    let mut words = field.split_whitespace();
    let count = words.next().context("guest memory count")?.parse::<u64>()?;
    ensure!(
        words.next() == Some("kB") && words.next().is_none(),
        "guest memory units are unsupported"
    );
    count
        .checked_mul(KIB_BYTES)
        .context("guest memory count overflows")
}

fn mac_free_memory(text: &str) -> Result<u64> {
    let first = text.lines().next().context("host memory is unmeasured")?;
    let size = first
        .split_once("page size of ")
        .context("host memory page size")?
        .1
        .split_whitespace()
        .next()
        .context("host memory page size count")?
        .parse::<u64>()?;
    ensure!(size > 0, "invalid host memory page size");
    let mut pages = 0u64;
    for name in ["Pages free:", "Pages inactive:", "Pages speculative:"] {
        let mut fields = text.lines().filter_map(|line| line.strip_prefix(name));
        let field = fields
            .next()
            .context("host free/reclaimable pages are incomplete")?;
        ensure!(fields.next().is_none(), "duplicate host memory field");
        let count = field.trim().trim_end_matches('.').parse::<u64>()?;
        pages = pages
            .checked_add(count)
            .context("host page count overflow")?;
    }
    pages
        .checked_mul(size)
        .context("host memory bytes overflow")
}

fn load_admitted(host: &str, guest: &str, guest_cpus: u64) -> Result<bool> {
    let mut lines = host.lines();
    let host_cpus = lines
        .next()
        .context("host CPU count")?
        .trim()
        .parse::<u64>()?;
    let host_load = lines.next().context("host load")?;
    ensure!(
        lines.next().is_none() && host_cpus > BUILDER_CPUS && guest_cpus > BUILDER_CPUS,
        "host CPU observation is unsupported"
    );
    let parse = |text: &str| -> Result<Vec<f64>> {
        text.split_whitespace()
            .filter(|word| !matches!(*word, "{" | "}"))
            .take(LOAD_WINDOW_COUNT)
            .map(|word| {
                let value = word.parse::<f64>()?;
                ensure!(
                    value.is_finite() && value >= MINIMUM_LOAD,
                    "invalid current load observation"
                );
                Ok(value)
            })
            .collect()
    };
    let host = parse(host_load)?;
    let guest = parse(guest)?;
    ensure!(
        host.len() == LOAD_WINDOW_COUNT && guest.len() == LOAD_WINDOW_COUNT,
        "current load observation is incomplete"
    );
    let host_limit = f64::from(u32::try_from(host_cpus - BUILDER_CPUS)?);
    let guest_limit = f64::from(u32::try_from(guest_cpus - BUILDER_CPUS)?);
    Ok(
        host.iter().all(|load| *load <= host_limit)
            && guest.iter().all(|load| *load <= guest_limit),
    )
}

/// Prepare one checked image and require its captured server to stop.
///
/// # Errors
/// Refuses stale source, unsupported routes, missing measurements, failed operations, and an unsettled owned server.
pub(crate) fn run(root: &Path, request: &Request) -> Result<()> {
    if request.plan && request.output.is_none() {
        return print_plan(root);
    }
    ensure!(!request.plan, "plan and execution are separate requests");
    ensure!(
        cfg!(all(target_os = "macos", target_arch = "aarch64")),
        "the reviewed builder pin only supports native Mac ARM64 preparation"
    );
    let requested = request
        .output
        .as_deref()
        .context("new private output is required")?;
    let plan = Plan::capture(root)?;
    ensure!(
        plan.producing_source == platform::BUILD_HASH,
        "the image preparation producer does not match the current source"
    );
    let controller = Controller::start_before_effects();
    let directory = private_output(root, requested)?;
    fs::write(directory.join("plan.json"), serde_json::to_vec(&plan)?)?;
    let nonce = crate::build_inputs::hex(&Sha256::digest(directory.as_os_str().as_encoded_bytes()));
    let pending = PendingBuilder::new(&nonce)?;
    fs::write(
        directory.join("pending.json"),
        serde_json::to_vec(&serde_json::json!({
            "schema":SCHEMA,"builder":pending.name,
            "container_name":pending.container_name,"state_volume":pending.state_volume,
            "wall_seconds":lifecycle::WALL_SECONDS
        }))?,
    )?;
    let result = (|| -> Result<LoadedImage> {
        let mut driver = Docker::new(root, directory.clone(), &controller)?;
        execute_owned(root, &plan, pending, &controller, &mut driver)
    })();
    let terminal = match &result {
        Ok(image) => serde_json::json!({
            "schema":SCHEMA,"completed":true,"image":plan.image,"id":image.id,
            "platform":image.platform,"source":plan.producing_source,
            "elapsed_ms":controller.started.elapsed().as_millis()
        }),
        Err(error) => {
            let bytes = format!("{error:#}").into_bytes();
            let digest = crate::build_inputs::hex(&Sha256::digest(&bytes));
            fs::write(directory.join(format!("{digest}.failure")), bytes)?;
            serde_json::json!({
                "schema":SCHEMA,"completed":false,"failure_sha256":digest,
                "elapsed_ms":controller.started.elapsed().as_millis()
            })
        }
    };
    fs::write(
        directory.join("terminal.json"),
        serde_json::to_vec(&terminal)?,
    )?;
    result?;
    println!("{}", serde_json::to_string(&terminal)?);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::OsString;

    const NONCE: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    const SERVER_ID: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
    const CHILD_STATE: &str = "DOTFILES_TEST_PROOF_IMAGE_STATE";
    const DRIVER: &str = if cfg!(windows) {
        "docker.exe"
    } else {
        "docker"
    };

    fn root() -> &'static Path {
        Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap()
    }

    fn server(pending: &PendingBuilder) -> Server {
        Server {
            id: SERVER_ID.to_owned(),
            name: pending.container_name.clone(),
            created: "2026-10-14T00:00:00Z".to_owned(),
            image: BUILDKIT_CONFIG.to_owned(),
            state_volume: pending.state_volume.clone(),
            running: true,
            cpu_period: CPU_PERIOD,
            cpu_quota: CPU_QUOTA,
            memory: MEMORY_BYTES,
            memory_swap: i64::try_from(MEMORY_BYTES).unwrap(),
            restart_policy: RESTART_POLICY.to_owned(),
        }
    }

    fn observation() -> Result<Observation> {
        let required = resource_rules::required(allocation()).context("allocation")?;
        Ok(Observation {
            capacity: MeasuredCapacity {
                host_bytes: required.bytes,
                guest_bytes: required.bytes,
                host_inodes: Inodes::Available(required.inodes),
                guest_inodes: Inodes::Available(required.inodes),
            },
            host_memory_available: MEMORY_BYTES,
            guest_memory_available: MEMORY_BYTES,
            supported_native_route: true,
            contention: Contention::Admitted,
        })
    }

    struct Fixture {
        boundary: Docker,
        executable: PathBuf,
        scenario: &'static str,
        name: String,
        volume: String,
    }

    impl Fixture {
        fn new(
            executable: &Path,
            directory: &Path,
            scenario: &'static str,
            controller: &Controller,
        ) -> Self {
            Self {
                boundary: Docker {
                    controller: Controller {
                        started: controller.started,
                    },
                    directory: directory.to_path_buf(),
                    context: "fixture".to_owned(),
                    endpoint: "fixture".to_owned(),
                    probe: String::new(),
                    operations: 0,
                    releasing: false,
                    recording_failed: false,
                },
                executable: executable.to_path_buf(),
                scenario,
                name: String::new(),
                volume: String::new(),
            }
        }

        fn command(&mut self, operation: &str) -> Result<CommandReceipt> {
            let mut command = platform::process::command(&self.executable);
            command.args([
                self.scenario,
                operation,
                SERVER_ID,
                &self.name,
                &self.volume,
            ]);
            command.env(CHILD_STATE, self.boundary.directory.join("server.state"));
            self.boundary.call(operation, command, None)
        }

        fn read_server(&mut self, operation: &str) -> Result<Server> {
            let receipt = self.command(operation)?;
            decode(Docker::text(&receipt)?.as_bytes())
        }
    }

    impl ImagePreparation for Fixture {
        fn begin_release(&mut self) {
            self.boundary.begin_release();
        }
        fn initial_observation(&mut self, _root: &Path) -> Result<Observation> {
            observation()
        }
        fn observe(&mut self, _server: Option<&CapturedBuildKit>) -> Result<Observation> {
            observation()
        }
        fn require_namespace_absent(&mut self, _pending: &PendingBuilder) -> Result<()> {
            let receipt = self.command("absent")?;
            natural_success(&receipt)?;
            ensure!(
                Docker::text(&receipt)?.trim().is_empty(),
                "fixture namespace collision"
            );
            Ok(())
        }
        fn create(&mut self, pending: &PendingBuilder) -> Result<CommandReceipt> {
            self.name.clone_from(&pending.container_name);
            self.volume.clone_from(&pending.state_volume);
            self.command("create")
        }
        fn bootstrap(
            &mut self,
            _pending: &PendingBuilder,
            _initial: &Observation,
        ) -> Result<CommandReceipt> {
            self.command("bootstrap")
        }
        fn created_server(&mut self, _pending: &PendingBuilder) -> Result<Server> {
            self.read_server("capture")
        }
        fn current_server(&mut self, _server: &CapturedBuildKit) -> Result<Server> {
            self.read_server("inspect")
        }
        fn effective_limits(&mut self, _server: &CapturedBuildKit) -> Result<()> {
            let receipt = self.command("limits")?;
            natural_success(&receipt)
        }
        fn build(
            &mut self,
            _root: &Path,
            _plan: &Plan,
            _server: &CapturedBuildKit,
            _initial: &Observation,
        ) -> Result<CommandReceipt> {
            self.command("build")
        }
        fn stop(&mut self, server: &CapturedBuildKit) -> Result<CommandReceipt> {
            ensure!(
                server.server.id == SERVER_ID,
                "fixture was asked to stop a foreign ID"
            );
            self.command("stop")
        }
        fn kill(&mut self, server: &CapturedBuildKit) -> Result<CommandReceipt> {
            ensure!(
                server.server.id == SERVER_ID,
                "fixture was asked to kill a foreign ID"
            );
            self.command("kill")
        }
        fn loaded_image(&mut self, _image: &str) -> Result<LoadedImage> {
            let receipt = self.command("loaded")?;
            let text = Docker::text(&receipt)?;
            let mut fields = text.split_whitespace();
            let id = fields.next().context("fixture image ID")?.to_owned();
            let platform = fields.next().context("fixture platform")?.to_owned();
            ensure!(fields.next().is_none(), "unexpected fixture image identity");
            Ok(LoadedImage { id, platform })
        }
        fn retain(&mut self, operation: &'static str, receipt: &CommandReceipt) -> Result<()> {
            self.boundary.retain(operation, receipt)
        }
        fn retain_server(&mut self, operation: &'static str, server: &Server) -> Result<()> {
            if self.scenario == "retention-failure" && operation == "release-before" {
                let prefix = self
                    .boundary
                    .operations
                    .checked_add(1)
                    .context("fixture counter")?;
                fs::write(
                    self.boundary
                        .directory
                        .join(format!("{prefix}-stop.stdout")),
                    b"fixture: existing output prevents initial evidence creation",
                )?;
                return Err(std::io::Error::other("fixture: evidence write failed").into());
            }
            self.boundary.retain_server(operation, server)
        }
        fn retain_failure(&mut self, error: &anyhow::Error) -> Result<()> {
            self.boundary.retain_failure(error)
        }
    }

    fn compile_fixture(directory: &Path) -> Result<PathBuf> {
        let executable = directory.join(DRIVER);
        let compiled = platform::process::command("rustc")
            .args(["--edition", "2024"])
            .arg(root().join("xtask/tests/fixtures/proof_image_docker.rs"))
            .arg("-o")
            .arg(&executable)
            .output()?;
        ensure!(
            compiled.status.success(),
            "compile maintained fixture: {}",
            String::from_utf8_lossy(&compiled.stderr)
        );
        Ok(executable)
    }

    fn exercise(executable: &Path, scenario: &'static str) -> Result<(bool, String, Vec<u8>)> {
        let output = tempfile::tempdir()?;
        let plan = Plan::capture(root())?;
        let controller = Controller::start_before_effects();
        let pending = PendingBuilder::new(NONCE)?;
        let mut fixture = Fixture::new(executable, output.path(), scenario, &controller);
        let result = execute_owned(root(), &plan, pending, &controller, &mut fixture);
        let calls = fs::read_to_string(output.path().join("server.calls"))?;
        let state = fs::read(output.path().join("server.state"))?;
        ensure!(
            output.path().join("artifacts.jsonl").is_file(),
            "original boundary outputs were not retained"
        );
        Ok((result.is_ok(), calls, state))
    }

    #[test]
    fn a_natural_image_build_without_an_owned_stop_acknowledgement_is_refused() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let executable = compile_fixture(directory.path())?;
        let (credited, calls, state) = exercise(&executable, "no-ack")?;
        assert!(
            !credited,
            "an exit-zero build was credited without an owned server stop acknowledgement"
        );
        assert_eq!(state, b"running");
        assert!(calls.lines().any(|line| line.starts_with("build ")));
        assert!(calls.lines().any(|line| line.starts_with("stop ")));
        assert!(calls.lines().any(|line| line.starts_with("kill ")));
        Ok(())
    }

    #[test]
    fn boundary_unlimited_swap_is_captured_and_stopped_before_quota_refusal() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let executable = compile_fixture(directory.path())?;
        let (credited, calls, state) = exercise(&executable, "negative-swap")?;
        assert!(
            !credited,
            "an unlimited swap server received preparation credit"
        );
        assert_eq!(
            state, b"stopped",
            "valid ownership was lost before quota refusal"
        );
        assert!(calls.lines().any(|line| line.starts_with("stop ")));
        assert!(!calls.lines().any(|line| line.starts_with("build ")));
        Ok(())
    }

    #[test]
    fn boundary_recording_failure_cannot_prevent_an_owned_stop() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let executable = compile_fixture(directory.path())?;
        let (credited, calls, state) = exercise(&executable, "retention-failure")?;
        assert!(
            !credited,
            "failed evidence retention received preparation credit"
        );
        assert_eq!(
            state, b"stopped",
            "failed evidence retention prevented owned release"
        );
        assert!(calls.lines().any(|line| line.starts_with("stop ")));
        assert!(calls.lines().any(|line| line.starts_with("build ")));
        Ok(())
    }

    #[test]
    fn original_build_and_release_outcomes_remain_independent_at_the_real_boundary() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let executable = compile_fixture(directory.path())?;
        for scenario in [
            "success",
            "stop-failure",
            "false-stop",
            "build-failure",
            "bad-limits",
            "foreign",
            "bad-image",
            "bootstrap-failure",
        ] {
            let (credited, calls, state) = exercise(&executable, scenario)?;
            assert_eq!(
                credited,
                matches!(scenario, "success" | "stop-failure"),
                "{scenario}"
            );
            if matches!(scenario, "foreign" | "false-stop") {
                assert_eq!(state, b"running");
            } else {
                assert_eq!(state, b"stopped", "{scenario}");
            }
            if scenario == "foreign" {
                assert!(!calls.lines().any(|line| line.starts_with("stop ")));
                assert!(!calls.lines().any(|line| line.starts_with("kill ")));
            }
            if scenario == "stop-failure" {
                assert!(calls.lines().any(|line| line.starts_with("kill ")));
            }
            if matches!(scenario, "bad-limits" | "bootstrap-failure") {
                assert!(!calls.lines().any(|line| line.starts_with("build ")));
            }
        }
        Ok(())
    }

    #[test]
    fn server_readback_is_strict_data_and_cannot_import_ownership() -> Result<()> {
        let pending = PendingBuilder::new(NONCE)?;
        let original = server(&pending);
        let bytes = serde_json::to_vec(&original)?;
        let decoded = decode(&bytes)?;
        assert_eq!(decoded.id, SERVER_ID);
        for invalid in [
            b"{}".as_slice(),
            b"null".as_slice(),
            br#"{"id":"a","id":"b"}"#.as_slice(),
        ] {
            assert!(decode(invalid).is_err());
        }
        let mut extra = serde_json::to_value(&original)?;
        extra
            .as_object_mut()
            .context("server object")?
            .insert("accepted".to_owned(), serde_json::json!(true));
        assert!(decode(&serde_json::to_vec(&extra)?).is_err());
        let mut foreign = decode(&bytes)?;
        foreign.name.push('x');
        assert!(pending.capture_identity(foreign).is_err());
        assert!(decode(&vec![b' '; METADATA_LIMIT + 1]).is_err());
        Ok(())
    }

    #[test]
    fn admission_and_progress_refuse_unknown_overflow_and_declared_resource_loss() -> Result<()> {
        let required = resource_rules::required(allocation()).context("allocation")?;
        assert_eq!(required.bytes, 70 * GIB);
        assert_eq!(required.inodes, 3_300_000);
        let mut current = observation()?;
        current.admit()?;
        current.capacity.guest_bytes -= 1;
        assert!(current.admit().is_err());
        current.capacity.guest_inodes = Inodes::Unmeasured;
        assert!(current.admit().is_err());
        let initial = observation()?;
        let mut current = observation()?;
        current.capacity.host_bytes -= BUILD_BYTES + TEMPORARY_BYTES + MARGIN_BYTES + 1;
        assert!(current.continuing(&initial).is_err());
        let mut current = observation()?;
        current.capacity.guest_inodes = Inodes::Available(FLOOR_INODES - 1);
        assert!(current.continuing(&initial).is_err());
        assert!(memory_available("MemAvailable: 18446744073709551615 kB").is_err());
        assert!(memory_available("MemAvailable: 1 kB\nMemAvailable: 2 kB").is_err());
        assert!(df_available("Filesystem Available\nx not-a-count", "Available").is_err());
        assert!(load_admitted("18\n{ NaN 0 0 }", "0 0 0 1/1 1", 8).is_err());
        assert!(!load_admitted("18\n{ 17 0 0 }", "0 0 0 1/1 1", 8)?);
        Ok(())
    }

    #[test]
    fn builder_creation_preserves_the_default_selection_and_exact_limits() -> Result<()> {
        let pending = PendingBuilder::new(NONCE)?;
        let command = pending.create_command();
        let arguments: Vec<_> = command.get_args().map(OsString::from).collect();
        assert!(!arguments.iter().any(|argument| argument == "--use"));
        for option in [
            format!("image={BUILDKIT_IMAGE}"),
            format!("cpu-period={CPU_PERIOD}"),
            format!("cpu-quota={CPU_QUOTA}"),
            format!("memory={MEMORY_BYTES}"),
            format!("memory-swap={MEMORY_BYTES}"),
            format!("restart-policy={RESTART_POLICY}"),
        ] {
            assert!(
                arguments
                    .windows(2)
                    .any(|pair| pair == [OsString::from("--driver-opt"), OsString::from(&option)])
            );
        }
        let original = server(&pending);
        let owned = pending.capture_identity(original)?;
        let mut changed = server(&PendingBuilder::new(NONCE)?);
        changed.memory = 0;
        let wrongly_limited = PendingBuilder::new(NONCE)?.capture_identity(changed)?;
        assert!(wrongly_limited.require_limits().is_err());
        assert_eq!(
            wrongly_limited.stop_command().get_args().last(),
            Some(std::ffi::OsStr::new(SERVER_ID))
        );
        let mut current = server(&PendingBuilder::new(NONCE)?);
        assert!(!owned.acknowledged(0, &current));
        current.running = false;
        assert!(owned.acknowledged(lifecycle::WALL_SECONDS, &current));
        current.created.push('x');
        assert!(!owned.acknowledged(0, &current));
        Ok(())
    }
}
