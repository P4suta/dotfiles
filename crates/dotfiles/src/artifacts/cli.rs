use super::transport::Curl;
use super::{
    Cache, CargoWorkload, Digest, Inputs, NativeInputs, NativeWorkload, Plan, Request, Target,
    VerifiedArtifact,
};
use anyhow::{Context, Result, ensure};
use serde::Serialize;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

const PROJECT_CONFIG: &str = "mise.toml";
const MISE_LOCK: &str = "mise.lock";
const CARGO_LOCK: &str = "Cargo.lock";
const RELEASE_PINS: &str = "home/.chezmoidata/versions.toml";
const ACQUISITION_SCOPE: &str = "acquisition-only";
const CARGO_SCOPE: &str = "cargo-materials-only";

#[derive(Debug, clap::Subcommand)]
pub enum Action {
    /// Report project and actual rendered native materials, without installation credit.
    NativePlan {
        #[arg(long, value_enum)]
        target: Target,
        /// Native `ChezMoi` answers, outside the public source.
        #[arg(long)]
        configuration: PathBuf,
        /// The exact lock of the rendered native mise configuration.
        #[arg(long)]
        lock: PathBuf,
        #[arg(long)]
        destination: PathBuf,
        #[arg(long)]
        output: Option<PathBuf>,
    },
    /// Report materials from every Cargo consumer lock in this source.
    CargoPlan {
        #[arg(long)]
        output: Option<PathBuf>,
    },
    /// Collect the complete Cargo package union without granting native stage credit.
    CargoAcquire {
        #[arg(long, value_parser = cache_path)]
        cache: PathBuf,
    },
    /// Compare the complete Cargo package union with current source inputs.
    CargoVerify {
        #[arg(long, value_parser = cache_path)]
        cache: PathBuf,
    },
    /// Report the pinned acquisition materials and unresolved dependencies.
    Plan {
        #[arg(long = "target", value_enum)]
        targets: Vec<Target>,
        #[arg(long = "tool")]
        tools: Vec<String>,
        /// Write the report to a new file.
        #[arg(long)]
        output: Option<PathBuf>,
    },
    /// Fetch pinned materials into an explicit cache.
    Acquire {
        #[arg(long = "target", value_enum)]
        targets: Vec<Target>,
        #[arg(long = "tool")]
        tools: Vec<String>,
        #[arg(long, value_parser = cache_path)]
        cache: PathBuf,
    },
    /// Match every cached material to its current acquisition plan.
    Verify {
        #[arg(long = "target", value_enum)]
        targets: Vec<Target>,
        #[arg(long = "tool")]
        tools: Vec<String>,
        #[arg(long, value_parser = cache_path)]
        cache: PathBuf,
    },
}

fn cache_path(value: &str) -> std::result::Result<PathBuf, String> {
    let path = PathBuf::from(value);
    if path.is_absolute() {
        Ok(path)
    } else {
        Err("artifact cache requires an absolute path".into())
    }
}

fn plan(root: &Path, request: Request) -> Result<Plan> {
    let project = fs::read_to_string(root.join(PROJECT_CONFIG))
        .context("read the project mise configuration")?;
    let mise_lock =
        fs::read_to_string(root.join(MISE_LOCK)).context("read the mise acquisition lock")?;
    let cargo_lock =
        fs::read_to_string(root.join(CARGO_LOCK)).context("read the workspace Cargo lock")?;
    let versions =
        fs::read_to_string(root.join(RELEASE_PINS)).context("read the bootstrap release pins")?;
    Plan::parse(
        Inputs {
            project: &project,
            mise_lock: &mise_lock,
            cargo_lock: &cargo_lock,
            versions: &versions,
        },
        request,
    )
}

fn report(value: &impl Serialize, output: Option<&Path>) -> Result<()> {
    let mut bytes = serde_json::to_vec_pretty(value).context("encode the acquisition report")?;
    bytes.push(b'\n');
    if let Some(path) = output {
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(path)
            .context("create a new acquisition report")?;
        let result = file.write_all(&bytes).and_then(|()| file.sync_all());
        if let Err(error) = result {
            drop(file);
            platform::cleanup::report(
                fs::remove_file(path),
                "remove the incomplete acquisition report",
            );
            return Err(error).context("write the acquisition report");
        }
        Ok(())
    } else {
        std::io::stdout()
            .lock()
            .write_all(&bytes)
            .context("write the acquisition report")
    }
}

#[derive(Debug, Serialize)]
struct Summary {
    scope: &'static str,
    plan: Digest,
    materials: usize,
}

fn scoped_summary(
    plan: &Plan,
    mut artifacts: impl Iterator<Item = Result<VerifiedArtifact>>,
    scope: &'static str,
) -> Result<()> {
    let materials = artifacts.try_fold(0, |count, artifact| {
        drop(artifact?);
        Ok::<_, anyhow::Error>(count + 1)
    })?;
    ensure!(
        rules::artifact_rules::admission(
            plan.artifacts().len(),
            materials,
            plan.diagnostics().len(),
        ) == rules::artifact_rules::Admission::Ready,
        "acquired artifact set is incomplete"
    );
    report(
        &Summary {
            scope,
            plan: plan.digest(),
            materials,
        },
        None,
    )
}

/// Run acquisition planning or verification for the requested source scope.
///
/// # Errors
///
/// Returns malformed input, unresolved material, transport, cache, and report errors.
pub fn run(root: &Path, action: Action) -> Result<()> {
    match action {
        Action::NativePlan {
            target,
            configuration,
            lock,
            destination,
            output,
        } => {
            let renderer = super::native::renderer(root)?;
            let workload = NativeWorkload::capture(
                root,
                target,
                NativeInputs {
                    configuration: &configuration,
                    lock: &lock,
                    renderer: &renderer,
                    destination: &destination,
                },
            )?;
            let plan = workload.plan()?;
            report(&plan, output.as_deref())?;
            plan.complete_acquisition()
        }
        Action::CargoPlan { output } => {
            let plan = CargoWorkload::capture(root)?.plan()?;
            report(&plan, output.as_deref())?;
            plan.complete()
        }
        Action::CargoAcquire { cache } => {
            let plan = CargoWorkload::capture(root)?.plan()?;
            let cache = Cache::new(cache);
            scoped_summary(&plan, cache.acquire_stream(&plan, &Curl)?, CARGO_SCOPE)
        }
        Action::CargoVerify { cache } => {
            let plan = CargoWorkload::capture(root)?.plan()?;
            let cache = Cache::new(cache);
            scoped_summary(&plan, cache.verify_stream(&plan)?, CARGO_SCOPE)
        }
        Action::Plan {
            targets,
            tools,
            output,
        } => {
            let plan = plan(root, Request { targets, tools })?;
            report(&plan, output.as_deref())?;
            plan.complete()
        }
        Action::Acquire {
            targets,
            tools,
            cache,
        } => {
            let plan = plan(root, Request { targets, tools })?;
            let cache = Cache::new(cache);
            scoped_summary(
                &plan,
                cache.acquire_stream(&plan, &Curl)?,
                ACQUISITION_SCOPE,
            )
        }
        Action::Verify {
            targets,
            tools,
            cache,
        } => {
            let plan = plan(root, Request { targets, tools })?;
            let cache = Cache::new(cache);
            scoped_summary(&plan, cache.verify_stream(&plan)?, ACQUISITION_SCOPE)
        }
    }
}
