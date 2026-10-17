use anyhow::{Context, Result};
#[cfg(windows)]
use dotfiles::artifacts::PreparedMiseArchives;
use dotfiles::artifacts::{
    Cache, Fetch, FetchFailure, Inputs, MiseArchiveWorkload, MiseBootstrapIndex, Plan, Reason,
    Request, Target,
};
use std::fs;
use std::path::{Path, PathBuf};

const VERSION: &str = "1.3.7";
const TOOL: &str = "http:choose";
const PUBLIC_RELEASE: &str = "https://github.com/theryangeary/choose/releases/download/v1.3.7/choose-x86_64-pc-windows-gnu.exe";
const RELEASE_SHA256: &str = "9233beade020c3e74854911a15aa973f7dc9f4253abe8856b8c273579a555903";
#[cfg(windows)]
const FIXTURE_BYTES: &str = "DOTFILES_VERIFIED_CHOOSE_RELEASE";
#[cfg(windows)]
const FIXTURE_MISE: &str = "DOTFILES_VERIFIED_MISE_RELEASE_ARCHIVE";
const BOOTSTRAP_URL: &str =
    "https://github.com/jdx/mise/releases/download/v2026.10.9/mise-v2026.10.9-windows-x64.zip";
const BOOTSTRAP_SHA256: &str = "bc813006b1bf6cf02f470f44fac775a43da8403266a6c4d68e28dad510dcda39";
const VERSIONS: &str = "[mise]\nversion='2026.10.9'\n[mise.sha256]\nwindows-x64='bc813006b1bf6cf02f470f44fac775a43da8403266a6c4d68e28dad510dcda39'\n";
const PUBLIC_BOOTSTRAP_INDEX: &[u8] =
    include_bytes!("fixtures/mise-v2026.10.9-bootstrap-index.json");
const PUBLIC_BOOTSTRAP_MEMBERS: usize = 6;
const PUBLIC_BOOTSTRAP_DIRECTORY: &str = "mise/";
#[cfg(windows)]
const DATA_DIRECTORY: &str = "data";
#[cfg(windows)]
const RELOCATED_INSTALLATION: &str = "owned-relocated-installation";
#[cfg(windows)]
const JUNCTION: &str = "[string]$Link=$args[0]\n[string]$Destination=$args[1]\n$ErrorActionPreference='Stop'\nNew-Item -ItemType Junction -Path $Link -Value $Destination | Out-Null";

#[path = "../examples/support/artifact_material_corpus.rs"]
mod corpus;

struct Fixture {
    _scope: tempfile::TempDir,
    root: PathBuf,
    cache: PathBuf,
    #[cfg(windows)]
    materials: PathBuf,
    tool: String,
}

impl Fixture {
    fn new(backend: &str, provenance: Option<&str>) -> Result<Self> {
        let scope = tempfile::tempdir()?;
        let owned = platform::paths::canonical(scope.path())?;
        let root = owned.join("source");
        let cache = owned.join("cache");
        let materials = owned.join("materials");
        fs::create_dir_all(root.join("home/.chezmoidata"))?;
        fs::create_dir(&materials)?;
        fs::write(root.join("Cargo.lock"), "version=4\n")?;
        fs::write(root.join("home/.chezmoidata/versions.toml"), VERSIONS)?;
        let tool = backend.to_owned();
        let options = if backend.starts_with("http:") {
            String::new()
        } else {
            format!("url={PUBLIC_RELEASE:?}\nchecksum='sha256:{RELEASE_SHA256}'\n")
        };
        fs::write(
            root.join("mise.toml"),
            format!(
                "[tools]\n{tool:?}={{version={VERSION:?},url={PUBLIC_RELEASE:?},checksum='sha256:{RELEASE_SHA256}'}}\n[settings]\nparanoid=true\nlockfile=true\n"
            ),
        )?;
        let provenance =
            provenance.map_or(String::new(), |value| format!("provenance={value:?}\n"));
        fs::write(
            root.join("mise.lock"),
            format!(
                "lockfile_version=3\n[[tools.{tool:?}]]\nversion={VERSION:?}\nbackend={backend:?}\nspecifiers=[{VERSION:?}]\n[tools.{tool:?}.options]\n{options}[tools.{tool:?}.\"platforms.windows-x64\"]\nurl={PUBLIC_RELEASE:?}\nchecksum='sha256:{RELEASE_SHA256}'\n{provenance}"
            ),
        )?;
        Ok(Self {
            _scope: scope,
            root,
            cache,
            #[cfg(windows)]
            materials,
            tool,
        })
    }

    fn workload(&self) -> Result<MiseArchiveWorkload> {
        MiseArchiveWorkload::capture(
            &self.root,
            Target::WindowsX64,
            std::slice::from_ref(&self.tool),
        )
    }

    #[cfg(windows)]
    fn prepare(&self) -> Result<(MiseArchiveWorkload, PreparedMiseArchives)> {
        let workload = self.workload()?;
        workload.complete()?;
        let asset = PathBuf::from(
            std::env::var_os(FIXTURE_BYTES)
                .context("supply the reviewed public choose release bytes")?,
        );
        let mise = PathBuf::from(
            std::env::var_os(FIXTURE_MISE)
                .context("supply the reviewed pinned mise release archive")?,
        );
        let fetched = Cache::new(self.cache.clone()).acquire(
            workload.plan(),
            &Release {
                choose: &asset,
                mise: &mise,
            },
        )?;
        let prepared = fetched.prepare_mise_archives(&workload, &self.materials)?;
        Ok((workload, prepared))
    }
}

#[derive(Debug)]
#[cfg(windows)]
struct Release<'a> {
    choose: &'a Path,
    mise: &'a Path,
}

#[cfg(windows)]
impl Fetch for Release<'_> {
    fn fetch(&self, source: &str, destination: &Path) -> Result<(), FetchFailure> {
        let origin = match source {
            PUBLIC_RELEASE => self.choose,
            BOOTSTRAP_URL => self.mise,
            _ => {
                return Err(FetchFailure::Refused(
                    "unexpected release fixture request".into(),
                ));
            }
        };
        fs::copy(origin, destination)
            .map(|_| ())
            .map_err(|error| FetchFailure::Refused(error.to_string()))
    }
}

#[test]
fn unsupported_provider_and_provenance_do_not_qualify_archive_installation() -> Result<()> {
    for (backend, provenance, reason) in [
        ("cargo:choose", None, Reason::UnsupportedBackend),
        (
            "http:choose",
            Some("github-attestations"),
            Reason::UnsupportedProvenance,
        ),
        (
            "github:theryangeary/choose",
            None,
            Reason::MissingDependency,
        ),
        ("aqua:theryangeary/choose", None, Reason::MissingDependency),
    ] {
        let fixture = Fixture::new(backend, provenance)?;
        let workload = fixture.workload()?;
        assert!(workload.complete().is_err());
        assert!(
            workload
                .unresolved()
                .iter()
                .chain(workload.plan().diagnostics())
                .any(|diagnostic| diagnostic.reason == reason)
        );
    }
    Ok(())
}

#[test]
fn old_download_abi_wrong_platform_and_changed_options_refuse_before_material_preparation()
-> Result<()> {
    let fixture = Fixture::new("http:choose", None)?;
    fixture.workload()?.complete()?;
    fs::write(
        fixture.root.join("home/.chezmoidata/versions.toml"),
        "[mise]\nversion='2026.10.6'\n",
    )?;
    let old = fixture.workload()?;
    assert!(old.complete().is_err());
    assert!(
        old.unresolved()
            .iter()
            .any(|diagnostic| diagnostic.subject == "mise-download-store-abi")
    );
    fs::write(
        fixture.root.join("home/.chezmoidata/versions.toml"),
        VERSIONS,
    )?;
    let wrong = MiseArchiveWorkload::capture(&fixture.root, Target::LinuxX64, &[TOOL.into()])?;
    assert!(wrong.complete().is_err());
    assert!(
        wrong
            .plan()
            .diagnostics()
            .iter()
            .any(|diagnostic| diagnostic.reason == Reason::MissingPlatform)
    );
    let configuration = fs::read_to_string(fixture.root.join("mise.toml"))?;
    fs::write(
        fixture.root.join("mise.toml"),
        configuration.replace(
            RELEASE_SHA256,
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad",
        ),
    )?;
    let changed = fixture.workload()?;
    assert!(changed.complete().is_err());
    assert!(
        changed
            .plan()
            .diagnostics()
            .iter()
            .any(|diagnostic| diagnostic.reason == Reason::VersionMismatch)
    );
    Ok(())
}

#[test]
fn canonical_http_lock_options_preserve_the_selected_artifact_binding() -> Result<()> {
    for (project_options, lock_options) in [
        ("", ""),
        (
            ",format='zip',strip_components=1,rename_exe='choose'",
            "format='zip'\nstrip_components='1'\nrename_exe='choose'\n",
        ),
        (",strip_components='01'", "strip_components='01'\n"),
    ] {
        let fixture = Fixture::new(TOOL, None)?;
        let project = fs::read_to_string(fixture.root.join("mise.toml"))?;
        let lock = fs::read_to_string(fixture.root.join("mise.lock"))?;
        fs::write(
            fixture.root.join("mise.toml"),
            project.replace('}', &format!("{project_options}}}")),
        )?;
        fs::write(
            fixture.root.join("mise.lock"),
            lock.replace(
                &format!("[tools.{TOOL:?}.options]\n"),
                &format!("[tools.{TOOL:?}.options]\n{lock_options}"),
            ),
        )?;
        fixture.workload()?.complete()?;
    }
    for (project_options, lock_options) in [
        (",format='zip'", "format='tar.gz'\n"),
        (",strip_components='01'", "strip_components='1'\n"),
        (",rename_exe='choose'", "rename_exe='other'\n"),
        (",strip_components=-1", "strip_components='-1'\n"),
        (",format=['zip']", "format='zip'\n"),
        (",headers='unknown'", "headers='unknown'\n"),
        ("", "format='zip'\n"),
    ] {
        let fixture = Fixture::new(TOOL, None)?;
        let project = fs::read_to_string(fixture.root.join("mise.toml"))?;
        let lock = fs::read_to_string(fixture.root.join("mise.lock"))?;
        fs::write(
            fixture.root.join("mise.toml"),
            project.replace('}', &format!("{project_options}}}")),
        )?;
        fs::write(
            fixture.root.join("mise.lock"),
            lock.replace(
                &format!("[tools.{TOOL:?}.options]\n"),
                &format!("[tools.{TOOL:?}.options]\n{lock_options}"),
            ),
        )?;
        let refused = fixture.workload()?;
        assert!(
            refused.complete().is_err(),
            "{project_options}/{lock_options}"
        );
        assert!(
            refused
                .plan()
                .diagnostics()
                .iter()
                .any(|diagnostic| diagnostic.reason == Reason::VersionMismatch)
        );
    }
    for (original, changed) in [
        (
            PUBLIC_RELEASE.to_owned(),
            format!("{PUBLIC_RELEASE}?changed"),
        ),
        (RELEASE_SHA256.to_owned(), "0".repeat(RELEASE_SHA256.len())),
    ] {
        let fixture = Fixture::new(TOOL, None)?;
        let project = fs::read_to_string(fixture.root.join("mise.toml"))?;
        fs::write(
            fixture.root.join("mise.toml"),
            project.replace(&original, &changed),
        )?;
        let refused = fixture.workload()?;
        assert!(refused.complete().is_err(), "{original}");
        assert!(
            refused
                .plan()
                .diagnostics()
                .iter()
                .any(|diagnostic| diagnostic.reason == Reason::VersionMismatch)
        );
    }
    Ok(())
}

#[test]
fn selected_archive_materials_cannot_be_verified_from_a_missing_or_poisoned_cache() -> Result<()> {
    let fixture = Fixture::new("http:choose", None)?;
    let workload = fixture.workload()?;
    workload.complete()?;
    assert!(
        Cache::new(fixture.cache.clone())
            .verify(workload.plan())
            .is_err()
    );
    fs::create_dir(&fixture.cache)?;
    fs::write(
        fixture.cache.join(format!("{RELEASE_SHA256}.blob")),
        b"poisoned material",
    )?;
    let poisoned = Cache::new(fixture.cache.clone())
        .verify(workload.plan())
        .expect_err("poisoned public release must refuse");
    assert!(format!("{poisoned:#}").contains("SHA-256 mismatch"));
    Ok(())
}

#[derive(Debug)]
struct FakeVersionProgram;

impl Fetch for FakeVersionProgram {
    fn fetch(&self, source: &str, destination: &Path) -> Result<(), FetchFailure> {
        if source != BOOTSTRAP_URL {
            return Err(FetchFailure::Refused(
                "unexpected bootstrap fixture request".into(),
            ));
        }
        fs::write(destination, b"@echo 2026.10.9\r\n")
            .map_err(|error| FetchFailure::Refused(error.to_string()))
    }
}

#[test]
fn a_self_reported_current_program_cannot_replace_the_verified_source_bootstrap() -> Result<()> {
    let fixture = Fixture::new("http:choose", None)?;
    let project = fs::read_to_string(fixture.root.join("mise.toml"))?;
    let lock = fs::read_to_string(fixture.root.join("mise.lock"))?;
    let plan = Plan::parse(
        Inputs {
            project: &project,
            mise_lock: &lock,
            cargo_lock: "version=4\n",
            versions: VERSIONS,
        },
        Request {
            targets: vec![Target::WindowsX64],
            tools: vec!["mise-bootstrap".into()],
        },
    )?;
    assert_eq!(plan.artifacts().len(), 1);
    assert_eq!(plan.artifacts()[0].expected().hex(), BOOTSTRAP_SHA256);
    let refusal = Cache::new(fixture.cache.clone())
        .acquire(&plan, &FakeVersionProgram)
        .expect_err("a program's version text cannot authenticate its release bytes");
    assert!(format!("{refusal:#}").contains("SHA-256 mismatch"));
    assert!(
        !fixture
            .cache
            .join(format!("{BOOTSTRAP_SHA256}.blob"))
            .exists()
    );
    Ok(())
}

#[test]
fn mise_metadata_corpus_uses_the_actual_consumer_decoders() -> Result<()> {
    let mut count = 0;
    let mut native = 0;
    for case in corpus::cases()? {
        let packet: corpus::Packet = serde_json::from_slice(&case.bytes)?;
        if !corpus::has_mise(&packet) && packet.native.is_none() {
            continue;
        }
        count += usize::from(corpus::has_mise(&packet));
        native += usize::from(packet.native.is_some());
        let actual = corpus::validate_native(&packet).and_then(|()| corpus::validate_mise(&packet));
        match case.expected {
            corpus::Expected::Plan => actual.with_context(|| case.name)?,
            corpus::Expected::Refused => assert!(actual.is_err(), "{}", case.name),
            corpus::Expected::Materials => {
                anyhow::bail!("mise metadata cannot assert installation")
            }
        }
    }
    assert_eq!(count, corpus::MISE_CASE_COUNT);
    assert_eq!(native, 9);
    Ok(())
}

#[test]
fn the_verified_public_bootstrap_index_preserves_its_real_directory_metadata() -> Result<()> {
    let index = MiseBootstrapIndex::parse(PUBLIC_BOOTSTRAP_INDEX)?;
    assert_eq!(index.member_count(), PUBLIC_BOOTSTRAP_MEMBERS);
    Ok(())
}

#[test]
fn bootstrap_directories_never_allow_data_or_linked_entries() -> Result<()> {
    for (field, value) in [("length", 1u64), ("attributes", 2_684_354_560)] {
        let mut packet: serde_json::Value = serde_json::from_slice(PUBLIC_BOOTSTRAP_INDEX)?;
        let directory = packet
            .get_mut("entries")
            .and_then(serde_json::Value::as_array_mut)
            .context("public index requires entries")?
            .iter_mut()
            .find(|entry| {
                entry.get("name").and_then(serde_json::Value::as_str)
                    == Some(PUBLIC_BOOTSTRAP_DIRECTORY)
            })
            .and_then(serde_json::Value::as_object_mut)
            .context("public index requires its actual directory member")?;
        directory.insert(field.into(), value.into());
        let bytes = serde_json::to_vec(&packet)?;
        assert!(
            MiseBootstrapIndex::parse(&bytes).is_err(),
            "{field} must refuse"
        );
    }
    Ok(())
}

#[cfg(windows)]
#[test]
#[ignore = "requires pinned mise 2026.10.9 and the reviewed public choose release"]
fn verified_public_release_installs_fresh_offline_and_runs_independent_live_health() -> Result<()> {
    let fixture = Fixture::new("http:choose", None)?;
    let (workload, prepared) = fixture.prepare()?;
    let installed = prepared.install(&workload)?;
    assert_eq!(installed.paths().len(), 1);
    let actual = installed.health(&prepared, &workload, TOOL, &["choose", "--version"])?;
    assert_eq!(actual.trim(), "choose 1.3.7");
    assert!(prepared.install(&workload).is_err());
    Ok(())
}

#[cfg(windows)]
#[test]
#[ignore = "requires the reviewed public choose and mise release bytes"]
fn captured_public_material_mutation_and_index_corruption_refuse_consumption() -> Result<()> {
    let fixture = Fixture::new("http:choose", None)?;
    let (workload, prepared) = fixture.prepare()?;
    let immutable = prepared.immutable_paths();
    let pins = immutable[1].join("pins/sha256");
    let index = pins.join(format!("{RELEASE_SHA256}.json"));
    let original_index = fs::read(&index)?;
    fs::write(
        &index,
        b"{\"blake3\":\"wrong\",\"effective_filename\":\"choose.exe\"}",
    )?;
    let corruption = prepared
        .install(&workload)
        .expect_err("invalid private pin entry must refuse before running the verified engine");
    assert!(format!("{corruption:#}").contains("private download pin index has invalid fields"));
    fs::write(&index, original_index)?;
    fs::write(immutable[3].join("mise.exe"), b"@echo 2026.10.9\r\n")?;
    let replaced = prepared
        .install(&workload)
        .expect_err("a replaced engine must refuse before any installer operation");
    assert!(format!("{replaced:#}").contains("prepared release archive inputs changed"));
    let original = fs::read_to_string(fixture.root.join("mise.toml"))?;
    fs::write(
        fixture.root.join("mise.toml"),
        format!("{original}\n# changed captured Source\n"),
    )?;
    let changed = prepared
        .install(&workload)
        .expect_err("changed captured Source must refuse before running the verified engine");
    assert!(format!("{changed:#}").contains("Source inputs changed before consumption"));
    Ok(())
}

#[cfg(windows)]
#[test]
#[ignore = "requires the reviewed public choose and mise release bytes"]
fn replacing_an_owned_installation_ancestor_refuses_live_health() -> Result<()> {
    for replace_data in [false, true] {
        let fixture = Fixture::new("http:choose", None)?;
        let (workload, prepared) = fixture.prepare()?;
        let installed = prepared.install(&workload)?;
        assert_eq!(
            installed
                .health(&prepared, &workload, TOOL, &["choose", "--version"])?
                .trim(),
            "choose 1.3.7"
        );
        let immutable = prepared.immutable_paths();
        let scope = immutable[0]
            .parent()
            .context("owned material scope is missing")?;
        let original = if replace_data {
            scope.join(DATA_DIRECTORY)
        } else {
            installed
                .paths()
                .get(TOOL)
                .context("actual selected installation is missing")?
                .clone()
        };
        let relocated = original
            .parent()
            .context("installation has no parent")?
            .join(RELOCATED_INSTALLATION);
        fs::rename(&original, &relocated)?;
        let created = platform::tool::Tool::Pwsh
            .command()
            .args([
                "-NoProfile",
                "-NonInteractive",
                "-CommandWithArgs",
                JUNCTION,
            ])
            .arg(&original)
            .arg(&relocated)
            .output()?;
        assert!(
            created.status.success(),
            "{}",
            String::from_utf8_lossy(&created.stderr)
        );
        assert!(
            installed
                .health(&prepared, &workload, TOOL, &["choose", "--version"])
                .is_err(),
            "a replaced owned installation ancestor cannot qualify live health"
        );
    }
    Ok(())
}
