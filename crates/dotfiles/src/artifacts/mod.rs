//! Verified acquisition material, separate from native installation evidence.

mod cache;
mod cargo_tool_consume;
mod cli;
mod consume;
mod mise_consume;
mod native;
mod plan;
mod rustup_consume;
mod transport;
mod workload;

pub use cache::{Cache, Fetch, FetchFailure, VerifiedArtifact, VerifiedPlan};
pub use cargo_tool_consume::{NativeMaterialFailure, NativeMaterialOperation};
pub use cli::{Action, run};
pub use consume::{CargoBudget, PreparedCargo, prepare_cargo_workload, prepare_source_cargo};
pub use mise_consume::{
    InstalledMiseArchives, MiseArchiveWorkload, MiseBootstrapIndex, MiseDownloadPin, MiseOperation,
    PreparedMiseArchives,
};
pub use native::{NativeConfiguration, NativeInputs, NativePlan, NativeWorkload};
pub use plan::{Artifact, Diagnostic, Digest, Inputs, Plan, Reason, Request, Target};
pub use rustup_consume::{
    CheckedRustKaniStores, InstalledRustKani, KaniBundleMetadata, NativeRustCatalog,
    PreparedRustKani, RustDistributionMetadata, RustKaniWorkload, prepare_source_rust_kani,
    replay_stdlib_archive,
};
pub use transport::CurlRelease;
pub use workload::CargoWorkload;
