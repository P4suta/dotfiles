use anyhow::{Context, Result};
use dotfiles::artifacts::{
    Cache, CargoBudget, CargoWorkload, Digest, Fetch, FetchFailure, PreparedCargo,
};
use flate2::{Compression, write::GzEncoder};
use std::cell::Cell;
use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

#[cfg(unix)]
#[path = "../../platform/tests/support/fixture_process.rs"]
mod fixture_process;

const NAME: &str = "example";
const VERSION: &str = "1.2.3";
const CRATES_IO: &str = "registry+https://github.com/rust-lang/crates.io-index";

struct Bytes<'a> {
    bytes: &'a [u8],
    calls: Cell<usize>,
}

impl Fetch for Bytes<'_> {
    fn fetch(&self, _source: &str, destination: &Path) -> Result<(), FetchFailure> {
        self.calls.set(self.calls.get() + 1);
        fs::write(destination, self.bytes).map_err(|error| FetchFailure::Refused(error.to_string()))
    }
}

fn archive(entries: &[(&str, &[u8], tar::EntryType)]) -> Result<Vec<u8>> {
    let gzip = GzEncoder::new(Vec::new(), Compression::default());
    let mut archive = tar::Builder::new(gzip);
    for (path, bytes, kind) in entries {
        let mut header = tar::Header::new_ustar();
        header.set_size(bytes.len() as u64);
        header.set_mode(0o644);
        header.set_entry_type(*kind);
        if path.len() >= 100 {
            archive.append_data(&mut header, path, *bytes)?;
            continue;
        }
        // Raw header bytes let the decoder see invalid paths that tar's writer refuses.
        let raw = header.as_mut_bytes();
        raw[..100].fill(0);
        raw[..path.len()].copy_from_slice(path.as_bytes());
        header.set_cksum();
        archive.append(&header, *bytes)?;
    }
    let gzip = archive.into_inner()?;
    Ok(gzip.finish()?)
}

fn release(extra: &[(&str, &[u8], tar::EntryType)]) -> Result<Vec<u8>> {
    let mut entries = vec![
        (
            "example-1.2.3/Cargo.toml",
            b"[package]\nname='example'\nversion='1.2.3'\nedition='2024'\n".as_slice(),
            tar::EntryType::Regular,
        ),
        (
            "example-1.2.3/src/lib.rs",
            b"pub fn value() -> u32 { 37 }\n".as_slice(),
            tar::EntryType::Regular,
        ),
    ];
    entries.extend_from_slice(extra);
    archive(&entries)
}

fn lock(digest: Digest) -> String {
    format!(
        "version=4\n[[package]]\nname='consumer'\nversion='0.1.0'\ndependencies=['example']\n[[package]]\nname='{NAME}'\nversion='{VERSION}'\nsource='{CRATES_IO}'\nchecksum='{}'\n",
        digest.hex()
    )
}

struct Fixture {
    scope: tempfile::TempDir,
    root: PathBuf,
    cache: PathBuf,
    parent: PathBuf,
}

impl Fixture {
    fn new(bytes: &[u8]) -> Result<Self> {
        let scope = tempfile::tempdir()?;
        let root = scope.path().join("source");
        let cache = scope.path().join("cache");
        let parent = scope.path().join("prepared");
        fs::create_dir_all(root.join("src"))?;
        fs::create_dir(&parent)?;
        fs::write(
            root.join("Cargo.toml"),
            "[package]\nname='consumer'\nversion='0.1.0'\nedition='2024'\n[workspace]\n[dependencies]\nexample='=1.2.3'\n",
        )?;
        fs::write(root.join("Cargo.lock"), lock(Digest::of(bytes)))?;
        fs::write(
            root.join("src/main.rs"),
            "fn main() { println!(\"{}\", example::value()); }\n",
        )?;
        Ok(Self {
            scope,
            root,
            cache,
            parent,
        })
    }

    fn prepare(&self, bytes: &[u8]) -> Result<(CargoWorkload, PreparedCargo)> {
        let workload = CargoWorkload::capture(&self.root)?;
        let fetch = Bytes {
            bytes,
            calls: Cell::new(0),
        };
        let prepared =
            Cache::new(self.cache.clone()).prepare_cargo(&workload, &self.parent, &fetch)?;
        assert_eq!(fetch.calls.get(), 1);
        Ok((workload, prepared))
    }
}

struct Releases {
    archives: BTreeMap<String, Vec<u8>>,
    calls: Cell<usize>,
    poison: Option<(String, PathBuf)>,
}

impl Fetch for Releases {
    fn fetch(&self, source: &str, destination: &Path) -> Result<(), FetchFailure> {
        let name = source.rsplit('/').next().unwrap_or_default();
        let bytes = self
            .archives
            .get(name)
            .ok_or_else(|| FetchFailure::Refused(format!("unplanned fixture source {source}")))?;
        self.calls.set(self.calls.get() + 1);
        fs::write(destination, bytes).map_err(|error| FetchFailure::Refused(error.to_string()))?;
        if let Some((trigger, path)) = &self.poison
            && trigger == name
        {
            fs::write(path, b"changed after earlier acquisition")
                .map_err(|error| FetchFailure::Refused(error.to_string()))?;
        }
        Ok(())
    }
}

fn material_set(count: usize, invalid_last: bool) -> Result<(Fixture, Releases)> {
    let fixture = Fixture::new(&release(&[])?)?;
    fs::write(fixture.root.join("Cargo.toml"), "[workspace]\n")?;
    let mut lock = String::from("version=4\n");
    let mut archives = BTreeMap::new();
    for index in 0..count {
        let name = format!("material{index:03}");
        let manifest = format!("[package]\nname='{name}'\nversion='{VERSION}'\nedition='2024'\n");
        let bytes = if invalid_last && index + 1 == count {
            b"authenticated but not a gzip archive".to_vec()
        } else {
            archive(&[
                (
                    &format!("{name}-{VERSION}/Cargo.toml"),
                    manifest.as_bytes(),
                    tar::EntryType::Regular,
                ),
                (
                    &format!("{name}-{VERSION}/src/lib.rs"),
                    b"pub fn value() -> u32 { 37 }\n",
                    tar::EntryType::Regular,
                ),
            ])?
        };
        writeln!(
            lock,
            "[[package]]\nname='{name}'\nversion='{VERSION}'\nsource='{CRATES_IO}'\nchecksum='{}'",
            Digest::of(&bytes).hex()
        )?;
        archives.insert(format!("{name}-{VERSION}.crate"), bytes);
    }
    fs::write(fixture.root.join("Cargo.lock"), lock)?;
    Ok((
        fixture,
        Releases {
            archives,
            calls: Cell::new(0),
            poison: None,
        },
    ))
}

#[cfg(unix)]
#[test]
fn cargo_consumption_and_cli_summaries_fit_below_the_material_count() -> Result<()> {
    const CHILD: &str = "DOTFILES_TEST_ARTIFACT_DESCRIPTOR_CHILD";
    const SHELL: &str = "/bin/sh";
    const DESCRIPTOR_LIMIT: usize = 256;
    const MATERIALS: usize = DESCRIPTOR_LIMIT + 1;
    const TOO_MANY_OPEN_FILES: i32 = 24;
    if std::env::var_os(CHILD).is_none() {
        let scope = tempfile::tempdir()?;
        let output = fixture_process::command(SHELL, scope.path())
            .args([
                "-c",
                &format!("ulimit -S -n {DESCRIPTOR_LIMIT} && exec \"$@\""),
                "artifact-descriptor-child",
            ])
            .arg(std::env::current_exe()?)
            .args([
                "--exact",
                "cargo_consumption_and_cli_summaries_fit_below_the_material_count",
                "--nocapture",
            ])
            .env(CHILD, "1")
            .output()?;
        assert!(
            output.status.success(),
            "{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        return Ok(());
    }
    let limit = platform::process::command(SHELL)
        .args(["-c", "ulimit -S -n"])
        .output()?;
    assert!(limit.status.success());
    assert_eq!(
        String::from_utf8(limit.stdout)?.trim().parse::<usize>()?,
        DESCRIPTOR_LIMIT
    );
    let (fixture, fetch) = material_set(MATERIALS, false)?;
    let workload = CargoWorkload::capture(&fixture.root)?;
    let plan = workload.plan()?;
    assert_eq!(plan.artifacts().len(), MATERIALS);
    let cache = Cache::new(fixture.cache.clone());
    let eager = cache
        .acquire(&plan, &fetch)
        .expect_err("the eager control must exceed the child's descriptor limit");
    assert!(eager.chain().any(|cause| {
        cause
            .downcast_ref::<std::io::Error>()
            .is_some_and(|error| error.raw_os_error() == Some(TOO_MANY_OPEN_FILES))
    }));
    let prepared = cache.prepare_cargo(&workload, &fixture.parent, &fetch)?;
    assert_eq!(fetch.calls.get(), MATERIALS);
    assert_eq!(prepared.digest(), plan.digest());
    assert_eq!(
        fs::read_dir(prepared.directory().join("vendor"))?.count(),
        MATERIALS
    );
    for index in 0..MATERIALS {
        assert_eq!(
            fs::read(
                prepared
                    .directory()
                    .join(format!("vendor/material{index:03}-{VERSION}/src/lib.rs"))
            )?,
            b"pub fn value() -> u32 { 37 }\n"
        );
    }
    prepared.configure(&workload, &mut platform::tool::Tool::Cargo.command())?;
    for action in ["cargo-acquire", "cargo-verify"] {
        let output = fixture_process::command(env!("CARGO_BIN_EXE_dotfiles"), fixture.scope.path())
            .arg("--root")
            .arg(&fixture.root)
            .args(["artifacts", action, "--cache"])
            .arg(&fixture.cache)
            .output()?;
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let report: serde_json::Value = serde_json::from_slice(&output.stdout)?;
        assert_eq!(report["materials"], MATERIALS);
        assert_eq!(report["plan"], plan.digest().hex());
    }
    Ok(())
}

#[test]
fn late_acquisition_changes_and_decoder_failures_remove_the_entire_candidate() -> Result<()> {
    const MATERIALS: usize = 3;
    for invalid_archive in [false, true] {
        let (fixture, mut fetch) = material_set(MATERIALS, invalid_archive)?;
        let workload = CargoWorkload::capture(&fixture.root)?;
        let plan = workload.plan()?;
        if !invalid_archive {
            let earlier = &plan.artifacts()[MATERIALS - 2];
            let trigger = fetch
                .archives
                .keys()
                .next_back()
                .context("last fixture acquisition")?
                .clone();
            assert_eq!(
                plan.artifacts()
                    .last()
                    .context("last planned material")?
                    .subject(),
                "crate:material002@1.2.3"
            );
            fetch.poison = Some((
                trigger,
                fixture
                    .cache
                    .join(format!("{}.blob", earlier.expected().hex())),
            ));
        }
        let error = Cache::new(fixture.cache.clone())
            .prepare_cargo(&workload, &fixture.parent, &fetch)
            .expect_err("late material refusal must not publish a partial consumer");
        assert_eq!(
            fetch.calls.get(),
            MATERIALS,
            "complete acquisition precedes consumption"
        );
        if !invalid_archive {
            assert!(format!("{error:#}").contains("poisoned artifact"));
        }
        assert_eq!(fs::read_dir(&fixture.parent)?.count(), 0);
    }
    let (fixture, fetch) = material_set(MATERIALS, false)?;
    let workload = CargoWorkload::capture(&fixture.root)?;
    let prepared =
        Cache::new(fixture.cache.clone()).prepare_cargo(&workload, &fixture.parent, &fetch)?;
    prepared.configure(&workload, &mut platform::tool::Tool::Cargo.command())?;
    Ok(())
}

#[test]
fn streamed_capabilities_keep_the_opened_file_and_recheck_later_receipts() -> Result<()> {
    use std::io::Read as _;
    const MATERIALS: usize = 2;
    let (fixture, fetch) = material_set(MATERIALS, false)?;
    let workload = CargoWorkload::capture(&fixture.root)?;
    let plan = workload.plan()?;
    let cache = Cache::new(fixture.cache.clone());
    let mut artifacts = cache.acquire_stream(&plan, &fetch)?;
    let first = artifacts.next().context("first capability")??;
    let digest = first.expected();
    let blob = fixture.cache.join(format!("{}.blob", digest.hex()));
    fs::rename(&blob, fixture.cache.join("retained-original"))?;
    fs::write(&blob, b"replacement is not the opened capability")?;
    let mut file = first.into_file();
    assert!(file.set_len(0).is_err());
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes)?;
    assert_eq!(Digest::of(&bytes), digest);
    drop(file);
    let last = plan
        .artifacts()
        .last()
        .context("last planned material")?
        .expected()
        .hex();
    let mut receipt = None;
    for entry in fs::read_dir(&fixture.cache)? {
        let path = entry?.path();
        if path
            .extension()
            .is_some_and(|extension| extension == "json")
        {
            let value: serde_json::Value = serde_json::from_slice(&fs::read(&path)?)?;
            if value["expected"] == last {
                assert!(receipt.replace(path).is_none());
            }
        }
    }
    fs::write(
        receipt.context("last authenticated receipt")?,
        b"poisoned provenance",
    )?;
    assert!(artifacts.next().context("last capability")?.is_err());
    assert!(artifacts.next().is_none());
    Ok(())
}

#[cfg(unix)]
#[test]
fn an_interleaved_cache_directory_link_refuses_the_next_capability() -> Result<()> {
    const MATERIALS: usize = 2;
    let (fixture, fetch) = material_set(MATERIALS, false)?;
    let workload = CargoWorkload::capture(&fixture.root)?;
    let plan = workload.plan()?;
    let cache = Cache::new(fixture.cache.clone());
    let mut artifacts = cache.acquire_stream(&plan, &fetch)?;
    drop(artifacts.next().context("first capability")??);
    let original = fixture.scope.path().join("original-cache");
    fs::rename(&fixture.cache, &original)?;
    std::os::unix::fs::symlink(&original, &fixture.cache)?;
    let error = artifacts
        .next()
        .context("last capability")?
        .expect_err("cache link must refuse");
    assert!(format!("{error:#}").contains("acquisition cache must be a real directory"));
    Ok(())
}

#[test]
fn all_source_locks_include_fuzz_materials_and_bind_changes() -> Result<()> {
    let bytes = release(&[])?;
    let fixture = Fixture::new(&bytes)?;
    fs::create_dir(fixture.root.join("fuzz"))?;
    fs::write(fixture.root.join("fuzz/Cargo.toml"), "[workspace]\n")?;
    fs::write(
        fixture.root.join("fuzz/Cargo.lock"),
        "version=4\n[[package]]\nname='unicase'\nversion='2.10.0'\nsource='registry+https://github.com/rust-lang/crates.io-index'\nchecksum='aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa'\n",
    )?;
    let workload = CargoWorkload::capture(&fixture.root)?;
    let plan = workload.plan()?;
    assert_eq!(plan.artifacts().len(), 2);
    assert!(
        plan.artifacts()
            .iter()
            .any(|artifact| artifact.subject() == "crate:unicase@2.10.0")
    );
    fs::write(fixture.root.join("fuzz/Cargo.lock"), "version=4\n")?;
    assert!(workload.require_current(&fixture.root).is_err());
    assert_ne!(
        CargoWorkload::capture(&fixture.root)?.plan()?.digest(),
        plan.digest()
    );
    Ok(())
}

#[test]
fn unresolved_and_conflicting_workspaces_cannot_mint_cargo_readiness() -> Result<()> {
    let bytes = release(&[])?;
    let fixture = Fixture::new(&bytes)?;
    fs::create_dir(fixture.root.join("fuzz"))?;
    fs::write(fixture.root.join("fuzz/Cargo.toml"), "[workspace]\n")?;
    assert!(CargoWorkload::capture(&fixture.root).is_err());
    fs::write(
        fixture.root.join("fuzz/Cargo.lock"),
        lock(Digest::of(b"different package")),
    )?;
    assert!(CargoWorkload::capture(&fixture.root)?.plan().is_err());
    fs::write(
        fixture.root.join("fuzz/Cargo.lock"),
        "version=4\n[[package]]\nname='foreign'\nversion='1.0.0'\nsource='git+https://example.com/foreign#aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa'\n",
    )?;
    assert!(
        CargoWorkload::capture(&fixture.root)?
            .plan()?
            .complete()
            .is_err()
    );
    Ok(())
}

#[test]
fn source_overrides_and_external_paths_are_not_cargo_material_authority() -> Result<()> {
    let bytes = release(&[])?;
    let fixture = Fixture::new(&bytes)?;
    fs::create_dir(fixture.root.join(".cargo"))?;
    fs::write(
        fixture.root.join(".cargo/config.toml"),
        "[source.crates-io]\nreplace-with='unverified'\n",
    )?;
    assert!(CargoWorkload::capture(&fixture.root).is_err());
    fs::write(
        fixture.root.join(".cargo/config.toml"),
        "[alias]\nverify='check --locked'\n",
    )?;
    CargoWorkload::capture(&fixture.root)?;
    fs::write(
        fixture.root.join("Cargo.toml"),
        "[workspace]\n[dependencies.foreign]\npath='../'\n",
    )?;
    assert!(CargoWorkload::capture(&fixture.root).is_err());
    Ok(())
}

#[test]
fn environment_only_consumption_refuses_unknown_ancestor_configuration() -> Result<()> {
    let bytes = release(&[])?;
    let fixture = Fixture::new(&bytes)?;
    let (workload, prepared) = fixture.prepare(&bytes)?;
    fs::create_dir(fixture.scope.path().join(".cargo"))?;
    fs::write(
        fixture.scope.path().join(".cargo/config.toml"),
        "[source.crates-io]\nreplace-with='foreign'\n",
    )?;
    let mut command = platform::tool::Tool::Cargo.command();
    assert!(
        prepared
            .configure_environment(&workload, &mut command)
            .is_err()
    );
    assert_eq!(command.get_args().count(), 0);
    assert!(!command.get_envs().any(|(key, _)| key == "CARGO_HOME"));
    prepared.configure(&workload, &mut command)?;
    assert!(command.get_args().any(|arg| arg == "--config"));
    Ok(())
}

#[test]
fn prepared_cargo_home_cannot_override_verified_sources_with_a_legacy_config() -> Result<()> {
    let bytes = release(&[])?;
    let fixture = Fixture::new(&bytes)?;
    let (workload, prepared) = fixture.prepare(&bytes)?;
    fs::write(
        prepared.directory().join("cargo-home/config"),
        "[source.crates-io]\nreplace-with='unverified'\n",
    )?;
    let mut command = platform::tool::Tool::Cargo.command();
    assert!(prepared.configure(&workload, &mut command).is_err());
    assert_eq!(command.get_args().count(), 0);
    Ok(())
}

#[test]
fn verified_archives_install_in_fresh_cargo_storage_without_a_registry_index() -> Result<()> {
    let bytes = release(&[])?;
    let fixture = Fixture::new(&bytes)?;
    let (workload, prepared) = fixture.prepare(&bytes)?;
    assert!(!prepared.directory().join("cargo-home/registry").exists());
    let install = fixture.scope.path().join("install");
    let mut cargo = platform::tool::Tool::Cargo.command();
    cargo
        .current_dir(&fixture.root)
        .args(["install", "--path", ".", "--root"])
        .arg(&install)
        .env("CARGO_TARGET_DIR", fixture.scope.path().join("target"))
        .env("CARGO_BUILD_JOBS", "2")
        .env("CARGO_PROFILE_RELEASE_DEBUG", "0");
    prepared.configure(&workload, &mut cargo)?;
    let output = cargo.output()?;
    assert!(
        output.status.success(),
        "actual fresh Cargo install failed:\n{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let binary = install.join(format!("bin/consumer{}", std::env::consts::EXE_SUFFIX));
    let actual = platform::process::command(binary).output()?;
    assert!(actual.status.success());
    assert_eq!(actual.stdout, b"37\n");
    assert!(
        !prepared
            .directory()
            .join("cargo-home/registry/index")
            .exists()
    );
    let mut next = platform::tool::Tool::Cargo.command();
    prepared.configure_environment(&workload, &mut next)?;
    assert_eq!(
        next.get_args().count(),
        0,
        "environment-only configuration cannot append Cargo flags"
    );
    Ok(())
}

#[test]
fn archive_paths_links_and_case_collisions_are_refused_before_consumption() -> Result<()> {
    for path in [
        "../escaped",
        "/absolute",
        "example-1.2.3/../escaped",
        "example-1.2.3/C:/escaped",
        "example-1.2.3/src\\escaped",
        "example-1.2.3/CON.txt",
        "example-1.2.3/file.",
        "other-1.2.3/src/lib.rs",
        "example-1.2.3/.cargo-checksum.json",
    ] {
        let bytes = release(&[(path, b"bad", tar::EntryType::Regular)])?;
        let fixture = Fixture::new(&bytes)?;
        assert!(fixture.prepare(&bytes).is_err(), "{path}");
        assert_eq!(
            fs::read_dir(&fixture.parent)?.count(),
            0,
            "refused archives leave no prepared capability"
        );
    }
    for kind in [
        tar::EntryType::Symlink,
        tar::EntryType::Link,
        tar::EntryType::Char,
        tar::EntryType::Block,
        tar::EntryType::Fifo,
        tar::EntryType::GNUSparse,
    ] {
        let bytes = release(&[("example-1.2.3/special", b"", kind)])?;
        let fixture = Fixture::new(&bytes)?;
        assert!(fixture.prepare(&bytes).is_err(), "{kind:?}");
    }
    for entries in [
        vec![(
            "example-1.2.3/src/lib.rs",
            b"duplicate".as_slice(),
            tar::EntryType::Regular,
        )],
        vec![(
            "example-1.2.3/SRC/another.rs",
            b"case prefix".as_slice(),
            tar::EntryType::Regular,
        )],
    ] {
        let bytes = release(&entries)?;
        let fixture = Fixture::new(&bytes)?;
        assert!(fixture.prepare(&bytes).is_err());
    }
    Ok(())
}

#[test]
fn trailing_gzip_and_tar_data_cannot_become_verified_cargo_sources() -> Result<()> {
    let original = release(&[])?;
    for bytes in [
        [original.as_slice(), b"trailing"].concat(),
        [original.as_slice(), original.as_slice()].concat(),
        original[..original.len() - 1].to_vec(),
    ] {
        let fixture = Fixture::new(&bytes)?;
        assert!(fixture.prepare(&bytes).is_err());
    }
    let mut tar = Vec::new();
    std::io::Read::read_to_end(
        &mut flate2::read::GzDecoder::new(original.as_slice()),
        &mut tar,
    )?;
    tar.extend_from_slice(b"not padding");
    let mut gzip = GzEncoder::new(Vec::new(), Compression::default());
    gzip.write_all(&tar)?;
    let bytes = gzip.finish()?;
    let fixture = Fixture::new(&bytes)?;
    assert!(fixture.prepare(&bytes).is_err());
    Ok(())
}

#[test]
fn changed_opened_archives_and_prepared_sources_are_refused() -> Result<()> {
    let bytes = release(&[])?;
    let fixture = Fixture::new(&bytes)?;
    let workload = CargoWorkload::capture(&fixture.root)?;
    let plan = workload.plan()?;
    let verified = Cache::new(fixture.cache.clone()).acquire(
        &plan,
        &Bytes {
            bytes: &bytes,
            calls: Cell::new(0),
        },
    )?;
    let expected = plan
        .artifacts()
        .first()
        .context("required crate")?
        .expected();
    fs::write(
        fixture.cache.join(format!("{}.blob", expected.hex())),
        b"changed after verification",
    )?;
    assert!(verified.prepare_cargo(&workload, &fixture.parent).is_err());
    let second = Fixture::new(&bytes)?;
    let (workload, prepared) = second.prepare(&bytes)?;
    fs::write(
        prepared.directory().join("vendor/example-1.2.3/src/lib.rs"),
        b"changed source",
    )?;
    let mut cargo = platform::tool::Tool::Cargo.command();
    assert!(prepared.configure(&workload, &mut cargo).is_err());
    assert_eq!(cargo.get_args().count(), 0);
    Ok(())
}

#[test]
fn cargo_long_path_metadata_is_bounded_and_keeps_exact_package_identity() -> Result<()> {
    let path = format!("example-1.2.3/src/{}.rs", "x".repeat(110));
    let bytes = release(&[(
        &path,
        b"// additional package file\n",
        tar::EntryType::Regular,
    )])?;
    let fixture = Fixture::new(&bytes)?;
    let (_, prepared) = fixture.prepare(&bytes)?;
    assert!(prepared.directory().join("vendor").join(&path).is_file());
    let bytes = archive(&[(
        "example-1.2.3/Cargo.toml",
        b"[package]\nname='other'\nversion='1.2.3'\n",
        tar::EntryType::Regular,
    )])?;
    let fixture = Fixture::new(&bytes)?;
    assert!(fixture.prepare(&bytes).is_err());
    Ok(())
}

#[test]
fn finite_replay_uses_the_native_decoder_without_widening_its_budget() -> Result<()> {
    let bytes = release(&[])?;
    let fixture = Fixture::new(&bytes)?;
    let workload = CargoWorkload::capture(&fixture.root)?;
    let plan = workload.plan()?;
    let verified = Cache::new(fixture.cache.clone()).acquire(
        &plan,
        &Bytes {
            bytes: &bytes,
            calls: Cell::new(0),
        },
    )?;
    let prepared =
        verified.prepare_cargo_with_budget(&workload, &fixture.parent, CargoBudget::Replay)?;
    prepared.configure(&workload, &mut platform::tool::Tool::Cargo.command())?;
    let large = vec![b'x'; 300 * 1024];
    let bytes = release(&[("example-1.2.3/src/large", &large, tar::EntryType::Regular)])?;
    let fixture = Fixture::new(&bytes)?;
    let workload = CargoWorkload::capture(&fixture.root)?;
    let plan = workload.plan()?;
    let cache = Cache::new(fixture.cache.clone());
    cache.acquire(
        &plan,
        &Bytes {
            bytes: &bytes,
            calls: Cell::new(0),
        },
    )?;
    assert!(
        cache
            .verify(&plan)?
            .prepare_cargo_with_budget(&workload, &fixture.parent, CargoBudget::Replay)
            .is_err()
    );
    cache
        .verify(&plan)?
        .prepare_cargo(&workload, &fixture.parent)?;
    Ok(())
}
