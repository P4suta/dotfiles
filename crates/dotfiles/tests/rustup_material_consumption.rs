use anyhow::{Context, Result};
use dotfiles::artifacts::{
    Cache, Fetch, FetchFailure, NativeRustCatalog, RustKaniWorkload, Target,
};
use std::cell::Cell;
use std::fs;
use std::path::{Path, PathBuf};

#[path = "../examples/support/artifact_material_corpus.rs"]
mod corpus;

const CATALOG: &str = include_str!("../../../policy/native-materials.toml");
const PROJECT: &str =
    "[tools]\nrust={version='1.99.0',components=['rust-src','clippy','rustfmt']}\n";
const LOCK: &str = "lockfile_version=3\n[[tools.rust]]\nversion='1.99.0'\nbackend='core:rust'\nspecifiers=['1.99.0']\n[tools.rust.options]\ncomponents='rust-src,clippy,rustfmt'\n";
const EXPECTED_METADATA_CASES: usize = 36 + corpus::STDLIB_CASE_COUNT;
const ORIGINAL_STDLIB_MEMBER: &str =
    "rust-src-nightly/rust-src/lib/rustlib/src/rust/library/Cargo.lock";
const ORIGINAL_STDLIB_LOCK: &[u8] =
    include_bytes!("fixtures/rust-src-nightly-library-cargo-lock.txt");
const ORIGINAL_STDLIB_SHA256: &str =
    "34656569ab979fdf259efffc99d2b68e253e73e0e6fba6762dc51e83df71aa76";
const REGULAR_ARCHIVE_FILE_MODE: u32 = 0o644;

struct Fixture {
    _owned: tempfile::TempDir,
    root: PathBuf,
    cache: PathBuf,
    materials: PathBuf,
}

impl Fixture {
    fn new() -> Result<Self> {
        let owned = tempfile::tempdir()?;
        let parent = platform::paths::canonical(owned.path())?;
        let root = parent.join("source");
        let cache = parent.join("cache");
        let materials = parent.join("materials");
        fs::create_dir(&root)?;
        fs::create_dir(root.join("policy"))?;
        fs::create_dir(&materials)?;
        for (path, bytes) in [
            ("mise.toml", PROJECT),
            ("mise.lock", LOCK),
            ("policy/native-materials.toml", CATALOG),
        ] {
            fs::write(root.join(path), bytes)?;
        }
        Ok(Self {
            _owned: owned,
            root,
            cache,
            materials,
        })
    }

    fn capture(&self) -> Result<RustKaniWorkload> {
        RustKaniWorkload::capture(&self.root, Target::WindowsX64)
    }
}

#[derive(Debug)]
struct Missing<'a>(&'a Cell<usize>);

impl Fetch for Missing<'_> {
    fn fetch(&self, _source: &str, _destination: &Path) -> Result<(), FetchFailure> {
        self.0.set(self.0.get() + 1);
        Err(FetchFailure::Refused(
            "controlled missing verified material".into(),
        ))
    }
}

#[derive(Debug)]
struct Poison;

impl Fetch for Poison {
    fn fetch(&self, _source: &str, destination: &Path) -> Result<(), FetchFailure> {
        fs::write(destination, b"controlled poisoned native release")
            .map_err(|error| FetchFailure::Refused(error.to_string()))
    }
}

#[test]
fn missing_and_poisoned_materials_cannot_reach_native_installation_or_the_body() -> Result<()> {
    let fixture = Fixture::new()?;
    let fetches = Cell::new(0);
    let body = Cell::new(0);
    let prepared = fixture.capture()?.prepare(
        &Cache::new(fixture.cache.clone()),
        &Missing(&fetches),
        &fixture.materials,
    );
    let installed = prepared.and_then(|prepared| {
        let installed = prepared.install()?;
        body.set(body.get() + 1);
        Ok(installed)
    });
    assert!(installed.is_err());
    assert_eq!(fetches.get(), 1);
    assert_eq!(body.get(), 0);
    let poisoned = fixture
        .capture()?
        .prepare(
            &Cache::new(fixture.cache.clone()),
            &Poison,
            &fixture.materials,
        )
        .expect_err("poisoned public release must refuse before installation");
    assert!(format!("{poisoned:#}").contains("SHA-256 mismatch"));
    assert_eq!(body.get(), 0);
    Ok(())
}

#[test]
fn current_pins_and_manifest_capture_are_checked_before_acquisition() -> Result<()> {
    NativeRustCatalog::parse(CATALOG)?;
    let fixture = Fixture::new()?;
    let captured = fixture.capture()?;
    fs::write(
        fixture.root.join("mise.toml"),
        PROJECT.replace("1.99.0", "1.98.0"),
    )?;
    let fetches = Cell::new(0);
    assert!(
        captured
            .prepare(
                &Cache::new(fixture.cache.clone()),
                &Missing(&fetches),
                &fixture.materials
            )
            .is_err()
    );
    assert_eq!(fetches.get(), 0);
    assert!(fixture.capture().is_err());
    fs::write(fixture.root.join("mise.toml"), PROJECT)?;
    fs::write(
        fixture.root.join("mise.lock"),
        LOCK.replace("core:rust", "cargo:rust"),
    )?;
    assert!(fixture.capture().is_err());
    fs::write(
        fixture.root.join("mise.lock"),
        LOCK.replace("rust-src,clippy,rustfmt", "rustfmt"),
    )?;
    assert!(fixture.capture().is_err());
    fs::write(fixture.root.join("mise.lock"), LOCK)?;
    fs::write(
        fixture.root.join("mise.toml"),
        PROJECT.replace("components=", "targets=['other-target'],components="),
    )?;
    assert!(fixture.capture().is_err());
    Ok(())
}

#[test]
fn stdlib_archive_decoder_accepts_the_original_lock_without_native_authority() -> Result<()> {
    assert_eq!(
        dotfiles::artifacts::Digest::of(ORIGINAL_STDLIB_LOCK).hex(),
        ORIGINAL_STDLIB_SHA256
    );
    let gzip = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
    let mut archive = tar::Builder::new(gzip);
    let mut header = tar::Header::new_ustar();
    header.set_size(u64::try_from(ORIGINAL_STDLIB_LOCK.len())?);
    header.set_mode(REGULAR_ARCHIVE_FILE_MODE);
    header.set_entry_type(tar::EntryType::Regular);
    header.set_cksum();
    archive.append_data(&mut header, ORIGINAL_STDLIB_MEMBER, ORIGINAL_STDLIB_LOCK)?;
    let bytes = archive.into_inner()?.finish()?;
    let packet = serde_json::from_value::<corpus::Packet>(serde_json::json!({
        "files": {},
        "archive": null,
        "rust": { "kind": "StdlibArchive", "bytes": bytes }
    }))
    .expect("the original regular stdlib lock must reach the maintained decoder");
    corpus::validate_native(&packet)?;
    Ok(())
}

#[test]
fn native_metadata_corpus_preserves_all_existing_and_new_parser_controls() -> Result<()> {
    let mut metadata = 0;
    let mut rust = 0;
    for case in corpus::cases()? {
        let packet: corpus::Packet = serde_json::from_slice(&case.bytes)?;
        if packet.native.is_none() && !corpus::has_mise(&packet) && packet.rust.is_none() {
            continue;
        }
        metadata += 1;
        rust += usize::from(packet.rust.is_some());
        assert!(case.bytes.len() <= corpus::INPUT_BYTE_LIMIT);
        let result = corpus::validate_native(&packet).and_then(|()| corpus::validate_mise(&packet));
        match case.expected {
            corpus::Expected::Plan => result.with_context(|| case.name)?,
            corpus::Expected::Refused => assert!(result.is_err(), "{}", case.name),
            corpus::Expected::Materials => {
                anyhow::bail!("metadata alone cannot assert installed or consumed materials")
            }
        }
    }
    assert_eq!(metadata, EXPECTED_METADATA_CASES);
    assert_eq!(rust, corpus::RUST_CASE_COUNT);
    assert_eq!(corpus::MISE_CASE_COUNT, 16);
    Ok(())
}
