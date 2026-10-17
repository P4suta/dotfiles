use crate::phase_evidence::RawResult;
use crate::tool::Tool;
use anyhow::{Context, Result, ensure};
use rules::benchmark_rules::{ProfileEvidence, ProfileMode, ProfileVerdict, profile};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

const LOADER: &str = "/lib64/ld-linux-x86-64.so.2";
const LIBC: &str = "/lib/x86_64-linux-gnu/libc.so.6";
const BINFMT: &str = "/proc/sys/fs/binfmt_misc";
const VALGRIND_BINARY: &str = "/usr/bin/valgrind";
const GUNGRAUN_RUNNER_BINARY: &str = "/usr/local/cargo/bin/gungraun-runner";
const CPUID_SLOTS: usize = 9;
const REGISTERS: usize = 4;
const REGISTER_STAGES: [&str; 2] = ["cpuid", "active"];
const SHA256_HEX_DIGITS: usize = 64;
const HEXADECIMAL_RADIX: u32 = 16;
const MAX_CALIBRATION_EXECUTION_BYTES: usize = 256;
const MAX_METADATA_BYTES: usize = 1024 * 1024;
const MAX_METADATA_ITEMS: usize = 512;
const MAX_INTERPRETER_BYTES: u64 = 64 * 1024 * 1024;
const RUST_RELEASE: &str = "release: 1.99.0";
const GLIBC_VERSION: &str = "2.36";
const RUST_FEATURES: usize = 21;
const TOOL_BINARIES: usize = 5;
pub const PROFILE_ENV: &str = "DOTFILES_BENCHMARK_PROFILE";

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Ceiling {
    pub function: String,
    pub instructions: u64,
    pub heap_peak: u64,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Identity {
    pub image_id: String,
    pub processor_hash: String,
    pub rust_features_hash: String,
    pub runtime_hash: String,
    pub toolchain_hash: String,
    pub configuration_hash: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Recorded {
    pub schema: u32,
    pub identity: Identity,
    pub calibration_sources: [String; 2],
    pub calibration_executions: [String; 2],
    pub measurements: Vec<Ceiling>,
    pub binary_size: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Registration {
    name: String,
    original: Vec<u8>,
    interpreter_hash: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Metadata {
    pub(crate) loader: RawResult,
    pub(crate) instrumented_loader: RawResult,
    pub(crate) instrumented_features: RawResult,
    pub(crate) libc: RawResult,
    pub(crate) rustc: RawResult,
    pub(crate) sysroot: RawResult,
    pub(crate) kernel: RawResult,
    pub(crate) rust_features: BTreeMap<String, bool>,
    pub(crate) binaries: BTreeMap<String, String>,
    pub(crate) registrations: Vec<Registration>,
}

fn hash(bytes: &[u8]) -> String {
    use sha2::{Digest as _, Sha256};
    crate::build_inputs::hex(&Sha256::digest(bytes))
}

fn hexadecimal(value: &str) -> bool {
    value.len() == SHA256_HEX_DIGITS
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

/// Reads every processor identifier and active register emitted by the pinned glibc 2.36 loader.
///
/// # Errors
///
/// Refuses truncated, duplicate, malformed, or differently shaped processor diagnostics.
pub fn processor(text: &str) -> Result<BTreeMap<String, u64>> {
    ensure!(
        text.len() <= MAX_METADATA_BYTES,
        "processor diagnostics exceed their byte bound"
    );
    let mut values = BTreeMap::new();
    for line in text
        .lines()
        .filter(|line| line.starts_with("x86.cpu_features."))
    {
        ensure!(
            values.len() < MAX_METADATA_ITEMS,
            "processor diagnostics exceed their item bound"
        );
        let (key, value) = line
            .split_once('=')
            .context("processor diagnostic has no value")?;
        let value = value
            .strip_prefix("0x")
            .context("processor diagnostic is not hexadecimal")?;
        ensure!(
            !value.is_empty() && value.bytes().all(|byte| byte.is_ascii_hexdigit()),
            "malformed processor value"
        );
        let value = u64::from_str_radix(value, HEXADECIMAL_RADIX)?;
        ensure!(
            values.insert(key.to_owned(), value).is_none(),
            "duplicate processor diagnostic"
        );
    }
    for name in [
        "basic.kind",
        "basic.max_cpuid",
        "basic.family",
        "basic.model",
        "basic.stepping",
        "isa_1",
        "data_cache_size",
        "shared_cache_size",
        "rep_movsb_threshold",
        "rep_stosb_threshold",
    ] {
        ensure!(
            values.contains_key(&format!("x86.cpu_features.{name}")),
            "processor diagnostic lacks {name}"
        );
    }
    for slot in 0..CPUID_SLOTS {
        for kind in REGISTER_STAGES {
            for register in 0..REGISTERS {
                let key = format!("x86.cpu_features.features[0x{slot:x}].{kind}[0x{register:x}]");
                ensure!(
                    values
                        .get(&key)
                        .is_some_and(|value| u32::try_from(*value).is_ok()),
                    "processor diagnostic lacks {key}"
                );
            }
        }
    }
    ensure!(
        values
            .keys()
            .any(|key| key.starts_with("x86.cpu_features.preferred.")),
        "processor preferred dispatch is absent"
    );
    ensure!(
        values
            .keys()
            .filter(|key| key.starts_with("x86.cpu_features.features["))
            .count()
            == CPUID_SLOTS * REGISTERS * REGISTER_STAGES.len(),
        "unsupported processor register inventory"
    );
    Ok(values)
}

#[cfg(target_arch = "x86_64")]
fn rust_features() -> BTreeMap<String, bool> {
    macro_rules! features {
        ($($name:tt),+ $(,)?) => {
            [$(($name.to_owned(), std::is_x86_feature_detected!($name))),+].into()
        };
    }
    features!(
        "sse",
        "sse2",
        "sse3",
        "ssse3",
        "sse4.1",
        "sse4.2",
        "avx",
        "avx2",
        "aes",
        "sha",
        "pclmulqdq",
        "fma",
        "bmi1",
        "bmi2",
        "adx",
        "popcnt",
        "lzcnt",
        "xsave",
        "xsaveopt",
        "xsavec",
        "xsaves"
    )
}

/// Prints only safe runtime feature observations for the current AMD64 process.
///
/// # Errors
///
/// Refuses any other target or an output failure.
pub fn print_processor_features() -> Result<()> {
    use std::io::Write as _;
    ensure!(
        cfg!(all(target_os = "linux", target_arch = "x86_64")),
        "processor dispatch observations require the Linux AMD64 guest"
    );
    let mut stdout = std::io::stdout().lock();
    serde_json::to_writer(&mut stdout, &rust_features())?;
    stdout.write_all(b"\n")?;
    Ok(())
}

#[cfg(not(target_arch = "x86_64"))]
fn rust_features() -> BTreeMap<String, bool> {
    BTreeMap::new()
}

fn registrations() -> Result<Vec<Registration>> {
    let mut entries = Vec::new();
    let directory = Path::new(BINFMT);
    if !directory.is_dir() {
        return Ok(entries);
    }
    for entry in fs::read_dir(directory)? {
        ensure!(
            entries.len() < MAX_METADATA_ITEMS,
            "runtime registration inventory is too large"
        );
        let entry = entry?;
        let name = entry
            .file_name()
            .into_string()
            .map_err(|_| anyhow::anyhow!("invalid runtime registration name"))?;
        if matches!(name.as_str(), "register" | "status") {
            continue;
        }
        let original =
            crate::benchmarks::read_bounded(&entry.path(), u64::try_from(MAX_METADATA_BYTES)?)?;
        let text = std::str::from_utf8(&original)?;
        let interpreter_hash = text
            .lines()
            .find_map(|line| line.strip_prefix("interpreter "))
            .map(PathBuf::from)
            .filter(|path| path.is_absolute() && path.is_file())
            .map(|path| {
                ensure!(
                    fs::metadata(&path)?.len() <= MAX_INTERPRETER_BYTES,
                    "runtime interpreter exceeds its byte bound"
                );
                crate::phase_evidence::verifier_digest(&path)
            })
            .transpose()?;
        entries.push(Registration {
            name,
            original,
            interpreter_hash,
        });
    }
    entries.sort_by(|left, right| left.name.cmp(&right.name));
    Ok(entries)
}

pub(crate) fn collect() -> Result<Metadata> {
    ensure!(
        cfg!(all(target_os = "linux", target_arch = "x86_64")),
        "processor observations require the Linux AMD64 guest"
    );
    let loader = crate::phase_evidence::output(
        crate::tool::external(LOADER).arg("--list-diagnostics"),
        "benchmark guest processor diagnostics",
    )?
    .into();
    let instrumented_loader = crate::phase_evidence::output(
        crate::tool::external("valgrind").args([
            "--tool=none",
            "--quiet",
            LOADER,
            "--list-diagnostics",
        ]),
        "benchmark Valgrind processor diagnostics",
    )?
    .into();
    let instrumented_features = crate::phase_evidence::output(
        crate::tool::external("valgrind")
            .args(["--tool=none", "--quiet"])
            .arg(std::env::current_exe()?)
            .arg("benchmarks-cpu-features"),
        "benchmark Valgrind Rust dispatch diagnostics",
    )?
    .into();
    let libc = crate::phase_evidence::output(
        &mut crate::tool::external(LIBC),
        "benchmark guest libc version",
    )?
    .into();
    let rustc = crate::phase_evidence::output(
        Tool::Rustc.command().arg("-vV"),
        "benchmark guest compiler version",
    )?
    .into();
    let sysroot: RawResult = crate::phase_evidence::output(
        Tool::Rustc.command().args(["--print", "sysroot"]),
        "benchmark guest compiler distribution",
    )?
    .into();
    ensure!(
        sysroot.success(),
        "the actual compiler distribution is unavailable"
    );
    let compiler = Path::new(sysroot.stdout()?.trim()).join("bin/rustc");
    let kernel = crate::phase_evidence::output(
        crate::tool::external("uname").args(["-srvm"]),
        "benchmark guest kernel and runtime",
    )?
    .into();
    let binaries = tool_binaries(compiler)?;
    Ok(Metadata {
        loader,
        instrumented_loader,
        instrumented_features,
        libc,
        rustc,
        sysroot,
        kernel,
        rust_features: rust_features(),
        binaries,
        registrations: registrations()?,
    })
}

fn tool_binaries(compiler: PathBuf) -> Result<BTreeMap<String, String>> {
    [
        ("loader", PathBuf::from(LOADER)),
        ("libc", PathBuf::from(LIBC)),
        ("rustc", compiler),
        ("valgrind", PathBuf::from(VALGRIND_BINARY)),
        ("gungraun-runner", PathBuf::from(GUNGRAUN_RUNNER_BINARY)),
    ]
    .into_iter()
    .map(|(name, path)| {
        Ok((
            name.to_owned(),
            crate::phase_evidence::verifier_digest(&path)?,
        ))
    })
    .collect()
}

pub(crate) fn identity(
    metadata: &Metadata,
    image_id: &str,
    host_info: &RawResult,
    host_version: &RawResult,
    configuration: &str,
) -> Result<Identity> {
    validate_guest_tools(metadata)?;
    let runtime_hash = runtime_hash(metadata, host_info, host_version)?;
    let instrumented_features = instrumented_features(metadata)?;
    Ok(Identity {
        image_id: image_id.into(),
        processor_hash: processor_hash(metadata)?,
        rust_features_hash: hash(&serde_json::to_vec(&(
            &metadata.rust_features,
            instrumented_features,
        ))?),
        runtime_hash,
        toolchain_hash: hash(&serde_json::to_vec(&(
            &metadata.binaries,
            metadata.libc.stdout()?,
            metadata.rustc.stdout()?,
        ))?),
        configuration_hash: hash(configuration.as_bytes()),
    })
}

fn validate_guest_tools(metadata: &Metadata) -> Result<()> {
    ensure!(
        metadata.libc.success()
            && metadata.libc.stdout()?.contains(GLIBC_VERSION)
            && metadata.rustc.success()
            && metadata
                .rustc
                .stdout()?
                .lines()
                .any(|line| line == RUST_RELEASE)
            && metadata.sysroot.success()
            && metadata.kernel.success(),
        "unsupported guest libc, compiler or runtime"
    );
    Ok(())
}

fn runtime_hash(
    metadata: &Metadata,
    host_info: &RawResult,
    host_version: &RawResult,
) -> Result<String> {
    ensure!(
        host_info.success() && host_version.success(),
        "the original host runtime readback is unavailable"
    );
    let host: Host = serde_json::from_slice(&host_info.stdout)?;
    ensure!(
        !host.kernel.is_empty() && !host.engine.is_empty(),
        "unsupported container runtime"
    );
    let client: serde_json::Value = serde_json::from_slice(&host_version.stdout)?;
    ensure!(
        client["Os"] == "darwin"
            && client["Version"]
                .as_str()
                .is_some_and(|value| !value.is_empty()),
        "unsupported host client"
    );
    let translated = matches!(host.architecture.as_str(), "aarch64" | "arm64");
    ensure!(
        (translated && client["Arch"] == "arm64")
            || (host.architecture == "x86_64" && client["Arch"] == "amd64"),
        "unsupported processor translation runtime"
    );
    ensure!(
        host.operating_system != RuntimeProvider::OrbStack || translated,
        "OrbStack observations require the Darwin ARM64 and Linux ARM64 route"
    );
    if translated {
        ensure!(
            metadata
                .registrations
                .iter()
                .any(
                    |entry| std::str::from_utf8(&entry.original).is_ok_and(|text| text
                        .lines()
                        .next()
                        == Some("enabled")
                        && text
                            .lines()
                            .any(|line| line.starts_with("magic ") && line.contains("3e00")))
                        && entry.interpreter_hash.as_deref().is_some_and(hexadecimal)
                ),
            "translated AMD64 execution has no identified active interpreter"
        );
    }
    Ok(hash(&serde_json::to_vec(&(
        host,
        client,
        metadata.kernel.stdout()?,
        &metadata.registrations,
    ))?))
}

fn instrumented_features(metadata: &Metadata) -> Result<BTreeMap<String, bool>> {
    let instrumented_features: BTreeMap<String, bool> =
        serde_json::from_slice(&metadata.instrumented_features.stdout)?;
    ensure!(
        metadata.instrumented_features.success()
            && instrumented_features.len() == metadata.rust_features.len()
            && metadata
                .rust_features
                .keys()
                .all(|key| instrumented_features.contains_key(key))
            && serde_json::to_string(&instrumented_features)?
                == metadata.instrumented_features.stdout()?.trim(),
        "instrumented Rust dispatch observation is missing or malformed"
    );
    ensure!(
        metadata.rust_features.len() == RUST_FEATURES
            && metadata.rust_features.get("sse2") == Some(&true)
            && instrumented_features.get("sse2") == Some(&true)
            && metadata.binaries.len() == TOOL_BINARIES
            && metadata.binaries.values().all(|value| hexadecimal(value)),
        "guest dispatch or tool binary inventory is incomplete"
    );
    Ok(instrumented_features)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
enum RuntimeProvider {
    #[serde(rename = "Docker Desktop")]
    DockerDesktop,
    #[serde(rename = "OrbStack")]
    OrbStack,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Host {
    architecture: String,
    operating_system: RuntimeProvider,
    kernel: String,
    engine: String,
}

pub(crate) fn processor_hash(metadata: &Metadata) -> Result<String> {
    ensure!(
        metadata.loader.success() && metadata.instrumented_loader.success(),
        "processor diagnostics failed or were interrupted"
    );
    Ok(hash(&serde_json::to_vec(&(
        processor(metadata.loader.stdout()?)?,
        processor(metadata.instrumented_loader.stdout()?)?,
    ))?))
}

/// Reads a complete recorded profile without accepting it as a current measurement.
///
/// # Errors
///
/// Refuses unknown fields, malformed identity, incomplete ceilings, or missing calibration provenance.
pub fn read_recorded(path: &Path) -> Result<Recorded> {
    let bytes = crate::benchmarks::read_bounded(path, u64::try_from(MAX_METADATA_BYTES)?)?;
    parse_recorded(std::str::from_utf8(&bytes)?)
}

/// Decodes recorded ceilings without granting their past measurements current credit.
///
/// # Errors
///
/// Refuses unknown fields, invalid identity, incomplete inventory, or missing calibration provenance.
pub fn parse_recorded(text: &str) -> Result<Recorded> {
    ensure!(
        text.len() <= MAX_METADATA_BYTES,
        "recorded profile exceeds its byte bound"
    );
    let recorded: Recorded = toml::from_str(text)?;
    validate_recorded(&recorded)?;
    Ok(recorded)
}

fn validate_recorded(recorded: &Recorded) -> Result<()> {
    let identity = &recorded.identity;
    ensure!(
        recorded.schema == 1
            && identity
                .image_id
                .strip_prefix("sha256:")
                .is_some_and(hexadecimal)
            && [
                &identity.processor_hash,
                &identity.rust_features_hash,
                &identity.runtime_hash,
                &identity.toolchain_hash,
                &identity.configuration_hash
            ]
            .into_iter()
            .all(|value| hexadecimal(value)),
        "unsupported benchmark profile identity"
    );
    ensure!(
        recorded
            .calibration_sources
            .iter()
            .all(|value| hexadecimal(value))
            && recorded
                .calibration_executions
                .iter()
                .all(|value| !value.is_empty()
                    && value.len() <= MAX_CALIBRATION_EXECUTION_BYTES
                    && value
                        .bytes()
                        .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-'))
            && recorded.calibration_executions[0] != recorded.calibration_executions[1],
        "profile needs two original calibration executions"
    );
    let names: Vec<_> = recorded
        .measurements
        .iter()
        .map(|value| value.function.as_str())
        .collect();
    ensure!(
        names.len() == crate::benchmarks::FUNCTIONS.len()
            && crate::benchmarks::FUNCTIONS.iter().all(|name| names
                .iter()
                .filter(|found| **found == *name)
                .count()
                == 1),
        "profile ceilings do not cover the exact benchmark inventory"
    );
    ensure!(
        recorded.binary_size > 0
            && recorded
                .measurements
                .iter()
                .all(|value| value.instructions > 0),
        "profile contains an empty instruction or binary ceiling"
    );
    Ok(())
}

pub(crate) fn disposition(
    mode: ProfileMode,
    expected: &Identity,
    actual: &Identity,
    source: bool,
    inventory: bool,
    controls: bool,
) -> ProfileVerdict {
    profile(
        mode,
        ProfileEvidence {
            processor: expected.processor_hash == actual.processor_hash
                && expected.rust_features_hash == actual.rust_features_hash,
            runtime: expected.runtime_hash == actual.runtime_hash
                && expected.image_id == actual.image_id,
            tools: expected.toolchain_hash == actual.toolchain_hash
                && expected.configuration_hash == actual.configuration_hash,
            source,
            inventory,
            controls,
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    const OBSERVED_ENGINE: &str = "29.4.0";
    const OBSERVED_KERNEL: &str = "7.0.14-orbstack-00380-ga7e0a2dc9535";

    fn original(text: &str) -> RawResult {
        RawResult {
            status: Some(0),
            stdout: text.as_bytes().to_vec(),
            stderr: Vec::new(),
        }
    }

    fn host_fixture(
        provider: &str,
        architecture: &str,
        client_architecture: &str,
    ) -> (RawResult, RawResult) {
        (
            original(
                &serde_json::json!({
                    "architecture": architecture,
                    "operating_system": provider,
                    "kernel": OBSERVED_KERNEL,
                    "engine": OBSERVED_ENGINE
                })
                .to_string(),
            ),
            original(
                &serde_json::json!({
                    "Os": "darwin",
                    "Arch": client_architecture,
                    "Version": OBSERVED_ENGINE
                })
                .to_string(),
            ),
        )
    }

    fn runtime_fixture() -> Metadata {
        Metadata {
            loader: original(""),
            instrumented_loader: original(""),
            instrumented_features: original("{}"),
            libc: original(""),
            rustc: original(""),
            sysroot: original(""),
            kernel: original(&format!("Linux {OBSERVED_KERNEL} x86_64\n")),
            rust_features: BTreeMap::new(),
            binaries: BTreeMap::new(),
            registrations: vec![Registration {
                name: "synthetic-amd64-translator".into(),
                original: b"enabled\ninterpreter /synthetic/translator\nflags: F\nmagic 7f454c4602010100000000000000000002003e00\n".to_vec(),
                interpreter_hash: Some("a".repeat(64)),
            }],
        }
    }

    #[test]
    fn observed_orbstack_route_requires_translation_evidence_without_granting_profile_credit()
    -> Result<()> {
        let metadata = runtime_fixture();
        let (host, client) = host_fixture("OrbStack", "aarch64", "arm64");
        let current = runtime_hash(&metadata, &host, &client)?;
        assert!(hexadecimal(&current));
        assert!(identity(&metadata, "unmeasured", &host, &client, "unmeasured").is_err());
        for (provider, architecture, client_architecture) in [
            ("OrbStack", "arm64", "arm64"),
            ("Docker Desktop", "aarch64", "arm64"),
            ("Docker Desktop", "x86_64", "amd64"),
        ] {
            let (host, client) = host_fixture(provider, architecture, client_architecture);
            let other = runtime_hash(&metadata, &host, &client)?;
            assert_ne!(current, other);
        }
        for variation in 0..6 {
            let mut altered = metadata.clone();
            match variation {
                0 => altered.registrations.clear(),
                1 => altered.registrations[0].interpreter_hash = None,
                2 => altered.registrations[0].interpreter_hash = Some("unknown".into()),
                3 => altered.registrations[0].original = b"disabled\nmagic 3e00\n".to_vec(),
                4 => altered.registrations[0].original = b"enabled\nmagic b700\n".to_vec(),
                _ => altered.registrations[0].original = vec![u8::MAX],
            }
            assert!(runtime_hash(&altered, &host, &client).is_err());
        }
        Ok(())
    }

    #[test]
    fn runtime_identity_refuses_unknown_providers_routes_and_missing_original_readbacks()
    -> Result<()> {
        let metadata = runtime_fixture();
        for (provider, architecture, client_architecture) in [
            ("unknown", "aarch64", "arm64"),
            ("orbstack", "aarch64", "arm64"),
            ("OrbStack", "x86_64", "amd64"),
            ("OrbStack", "aarch64", "amd64"),
            ("OrbStack", "aarch64", ""),
            ("Docker Desktop", "aarch64", "amd64"),
            ("Docker Desktop", "unknown", "arm64"),
        ] {
            let (host, client) = host_fixture(provider, architecture, client_architecture);
            assert!(
                runtime_hash(&metadata, &host, &client).is_err(),
                "{provider}/{architecture}/{client_architecture}"
            );
        }
        let (host, client) = host_fixture("OrbStack", "aarch64", "arm64");
        for variation in 0..8 {
            let mut altered_host = host.clone();
            let mut altered_client = client.clone();
            match variation {
                0 => altered_host.status = None,
                1 => altered_client.status = Some(1),
                2 => altered_host.stdout = b"null".to_vec(),
                3 => altered_client.stdout = b"null".to_vec(),
                4 | 5 => {
                    let mut value: serde_json::Value = serde_json::from_slice(&host.stdout)?;
                    value[if variation == 4 { "engine" } else { "kernel" }] = "".into();
                    altered_host.stdout = serde_json::to_vec(&value)?;
                }
                6 => {
                    altered_client.stdout =
                        b"{\"Os\":\"darwin\",\"Arch\":\"arm64\",\"Version\":\"\"}".to_vec();
                }
                _ => {
                    altered_client.stdout =
                        b"{\"Os\":\"linux\",\"Arch\":\"arm64\",\"Version\":\"29.4.0\"}".to_vec();
                }
            }
            assert!(runtime_hash(&metadata, &altered_host, &altered_client).is_err());
        }
        Ok(())
    }

    #[test]
    fn runtime_identity_binds_provider_versions_kernel_and_actual_interpreter() -> Result<()> {
        let metadata = runtime_fixture();
        let (host, client) = host_fixture("OrbStack", "aarch64", "arm64");
        let expected = runtime_hash(&metadata, &host, &client)?;
        for variation in 0..5 {
            let mut altered_metadata = metadata.clone();
            let mut altered_host = host.clone();
            let mut altered_client = client.clone();
            match variation {
                0 | 1 => {
                    let mut value: serde_json::Value = serde_json::from_slice(&host.stdout)?;
                    value[if variation == 0 { "engine" } else { "kernel" }] = "changed".into();
                    altered_host.stdout = serde_json::to_vec(&value)?;
                }
                2 => {
                    let mut value: serde_json::Value = serde_json::from_slice(&client.stdout)?;
                    value["Version"] = "changed".into();
                    altered_client.stdout = serde_json::to_vec(&value)?;
                }
                3 => {
                    let mut value: serde_json::Value = serde_json::from_slice(&host.stdout)?;
                    value["operating_system"] = "Docker Desktop".into();
                    altered_host.stdout = serde_json::to_vec(&value)?;
                }
                _ => altered_metadata.registrations[0].interpreter_hash = Some("b".repeat(64)),
            }
            assert_ne!(
                expected,
                runtime_hash(&altered_metadata, &altered_host, &altered_client)?
            );
        }
        Ok(())
    }

    fn diagnostics() -> String {
        let mut values = Vec::new();
        for name in [
            "basic.kind",
            "basic.max_cpuid",
            "basic.family",
            "basic.model",
            "basic.stepping",
            "isa_1",
            "data_cache_size",
            "shared_cache_size",
            "rep_movsb_threshold",
            "rep_stosb_threshold",
            "preferred.Fast_Unaligned_Copy",
        ] {
            values.push(format!("x86.cpu_features.{name}=0x1"));
        }
        for slot in 0..CPUID_SLOTS {
            for kind in ["cpuid", "active"] {
                for register in 0..REGISTERS {
                    values.push(format!(
                        "x86.cpu_features.features[0x{slot:x}].{kind}[0x{register:x}]=0x0"
                    ));
                }
            }
        }
        values.join("\n")
    }

    #[test]
    fn processor_identity_requires_every_original_cpuid_and_active_register() -> Result<()> {
        let valid = diagnostics();
        let values = processor(&valid)?;
        assert_eq!(values.len(), 83);
        for line in valid.lines() {
            let missing = valid
                .lines()
                .filter(|value| *value != line)
                .collect::<Vec<_>>()
                .join("\n");
            assert!(processor(&missing).is_err(), "{line}");
        }
        assert!(
            processor(&format!(
                "{valid}\n{}",
                valid.lines().next().unwrap_or_default()
            ))
            .is_err()
        );
        assert!(
            processor(&format!(
                "{valid}\nx86.cpu_features.features[0x9].cpuid[0x0]=0x0"
            ))
            .is_err()
        );
        assert!(processor(&valid.replace("=0x1", "=null")).is_err());
        assert!(processor("uname.machine=\"x86_64\"").is_err());
        Ok(())
    }

    #[test]
    fn an_observed_identity_never_supplies_verification_credit() {
        let identity = Identity {
            image_id: format!("sha256:{}", "a".repeat(64)),
            processor_hash: "b".repeat(64),
            rust_features_hash: "c".repeat(64),
            runtime_hash: "d".repeat(64),
            toolchain_hash: "e".repeat(64),
            configuration_hash: "f".repeat(64),
        };
        assert_eq!(
            disposition(ProfileMode::Observe, &identity, &identity, true, true, true),
            ProfileVerdict::Observation
        );
        assert_eq!(
            disposition(ProfileMode::Verify, &identity, &identity, true, true, true),
            ProfileVerdict::Accepted
        );
        assert_eq!(
            disposition(ProfileMode::Verify, &identity, &identity, false, true, true),
            ProfileVerdict::Refused
        );
        let mut other = identity.clone();
        other.processor_hash = "0".repeat(64);
        assert_eq!(
            disposition(ProfileMode::Verify, &identity, &other, true, true, true),
            ProfileVerdict::Refused
        );
    }

    fn recorded() -> Recorded {
        Recorded {
            schema: 1,
            identity: Identity {
                image_id: format!("sha256:{}", "a".repeat(64)),
                processor_hash: "b".repeat(64),
                rust_features_hash: "c".repeat(64),
                runtime_hash: "d".repeat(64),
                toolchain_hash: "e".repeat(64),
                configuration_hash: "f".repeat(64),
            },
            calibration_sources: ["1".repeat(64), "1".repeat(64)],
            calibration_executions: [
                "synthetic-cold-fixture".into(),
                "synthetic-warm-fixture".into(),
            ],
            measurements: crate::benchmarks::FUNCTIONS
                .map(|name| Ceiling {
                    function: name.into(),
                    instructions: 100,
                    heap_peak: 0,
                })
                .into(),
            binary_size: 1000,
        }
    }

    #[test]
    fn recorded_profiles_require_full_inventory_and_distinct_original_executions() -> Result<()> {
        let valid = recorded();
        parse_recorded(&toml::to_string(&valid)?)?;
        let mut altered = valid.clone();
        altered.measurements.pop();
        assert!(parse_recorded(&toml::to_string(&altered)?).is_err());
        altered = valid.clone();
        altered.measurements.push(altered.measurements[0].clone());
        assert!(parse_recorded(&toml::to_string(&altered)?).is_err());
        altered = valid.clone();
        altered.measurements[0].function = "unknown".into();
        assert!(parse_recorded(&toml::to_string(&altered)?).is_err());
        altered = valid.clone();
        altered.identity.runtime_hash = "unknown".into();
        assert!(parse_recorded(&toml::to_string(&altered)?).is_err());
        altered = valid.clone();
        altered.calibration_executions[1] = altered.calibration_executions[0].clone();
        assert!(parse_recorded(&toml::to_string(&altered)?).is_err());
        assert!(
            parse_recorded(&format!("unverified = true\n{}", toml::to_string(&valid)?)).is_err()
        );
        assert!(parse_recorded("null").is_err());
        assert!(parse_recorded(&" ".repeat(MAX_METADATA_BYTES + 1)).is_err());
        Ok(())
    }
}
