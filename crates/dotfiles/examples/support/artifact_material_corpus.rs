use anyhow::{Context, Result, ensure};
use dotfiles::artifacts::{
    Digest, Inputs, KaniBundleMetadata, MiseArchiveWorkload, MiseBootstrapIndex, MiseDownloadPin,
    NativeConfiguration, NativeRustCatalog, RustDistributionMetadata, Target,
    replay_stdlib_archive,
};
use flate2::{Compression, write::GzEncoder};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub const INPUT_BYTE_LIMIT: usize = 16 * 1024;
pub const SOURCE_MANIFEST: &str = "Cargo.toml";
pub const SOURCE_LOCK: &str = "Cargo.lock";
pub const FUZZ_MANIFEST: &str = "fuzz/Cargo.toml";
pub const FUZZ_LOCK: &str = "fuzz/Cargo.lock";
pub const CONFIG: &str = ".cargo/config.toml";
pub const LEGACY_CONFIG: &str = ".cargo/config";
pub const SOURCE_FILES: [&str; 6] = [
    SOURCE_MANIFEST,
    SOURCE_LOCK,
    FUZZ_MANIFEST,
    FUZZ_LOCK,
    CONFIG,
    LEGACY_CONFIG,
];
const CRATE_MANIFEST: &str = "example-1.2.3/Cargo.toml";
const CRATE_SOURCE: &str = "example-1.2.3/src/lib.rs";
const CRATES_IO: &str = "registry+https://github.com/rust-lang/crates.io-index";
const CRATE_NAME: &str = "example";
const CRATE_VERSION: &str = "1.2.3";
const TAR_NAME_BYTES: usize = 100;
const ARCHIVE_FILE_MODE: u32 = 0o644;
const BLAKE3_HEX_DIGITS: usize = 64;
const NATIVE_VERSIONS: &str = "[mise]\nversion='2026.10.6'\n[mise.sha256]\nwindows-x64='ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad'\n";
const NATIVE_CONFIGURATION: &str = "[tools]\nglobal='1.2.3'\n";
const NATIVE_LOCK: &str = "lockfile_version=3\n[[tools.global]]\nversion='1.2.3'\nbackend='aqua:owner/global'\nspecifiers=['1.2.3']\n[tools.global.\"platforms.windows-x64\"]\nurl='https://example.com/global.zip'\nchecksum='sha256:ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad'\n";
const MISE_CONFIGURATION: &str = "[tools]\n'http:example'={version='1.2.3',url='https://example.com/example.zip',checksum='sha256:ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad'}\n";
const MISE_LOCK: &str = "lockfile_version=3\n[[tools.'http:example']]\nversion='1.2.3'\nbackend='http:example'\nspecifiers=['1.2.3']\n[tools.'http:example'.options]\n[tools.'http:example'.\"platforms.windows-x64\"]\nurl='https://example.com/example.zip'\nchecksum='sha256:ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad'\n";
const MISE_VERSIONS: &str = "[mise]\nversion='2026.10.7'\n[mise.sha256]\nwindows-x64='152086b35c0e0adc182718f6f280f6b7f58b97535d5070fb6135f4e524a0d0e1'\n";
const MISE_PIN: &str = "{\"blake3\":\"ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad\",\"effective_filename\":\"example.zip\"}";
const MISE_ZIP_INDEX: &str =
    "{\"count\":1,\"entries\":[{\"name\":\"mise/bin/mise.exe\",\"length\":37,\"attributes\":0}]}";
pub const MISE_CASE_COUNT: usize = 16;
const EXISTING_RUST_CASE_COUNT: usize = 11;
pub const STDLIB_CASE_COUNT: usize = 6;
pub const RUST_CASE_COUNT: usize = EXISTING_RUST_CASE_COUNT + STDLIB_CASE_COUNT;
const STDLIB_ARCHIVE_MEMBER: &str =
    "rust-src-nightly/rust-src/lib/rustlib/src/rust/library/Cargo.lock";
const STDLIB_MISSING_MEMBER: &str = "rust-src-nightly/README";
const STDLIB_LINK_TARGET: &str = "foreign-lock";
const RUST_CATALOG: &str = include_str!("../../../../policy/native-materials.toml");
const RUST_RELEASE_ORIGIN: &str = "https://static.rust-lang.org";
const FOREIGN_RELEASE_ORIGIN: &str = "https://example.com";
const RUST_HOST: &str = "x86_64-pc-windows-msvc";
const RUST_COMPONENT_MANIFEST: &str = "manifest-version='2'\ndate='2026-10-01'\n[pkg.rust]\nversion='1.99.0 (fixture 2026-09-28)'\n[pkg.rustc]\nversion='1.99.0 (fixture 2026-09-28)'\n[pkg.rustc.target.x86_64-pc-windows-msvc]\navailable=true\nurl='https://static.rust-lang.org/dist/2026-10-01/rustc-1.99.0-x86_64-pc-windows-msvc.tar.gz'\nhash='ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad'\n";

#[derive(Debug, Deserialize, Serialize)]
#[serde(tag = "kind", deny_unknown_fields)]
pub enum RustMetadata {
    Catalog {
        text: String,
    },
    Distribution {
        manifest: String,
        host: String,
        components: Vec<String>,
    },
    Bundle {
        bytes: Vec<u8>,
    },
    StdlibArchive {
        bytes: Vec<u8>,
    },
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Native {
    pub configuration: String,
    pub lock: String,
    pub answers: String,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Mise {
    pub configuration: String,
    pub lock: String,
    pub versions: String,
    pub tools: Vec<String>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Packet {
    pub files: BTreeMap<String, String>,
    pub archive: Option<Vec<u8>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub native: Option<Native>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mise: Option<Mise>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mise_pin: Option<Vec<u8>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mise_bootstrap_index: Option<Vec<u8>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rust: Option<RustMetadata>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Expected {
    Plan,
    Materials,
    Refused,
}

#[derive(Debug)]
pub struct Case {
    pub name: &'static str,
    pub bytes: Vec<u8>,
    pub expected: Expected,
}

pub fn validate_native(packet: &Packet) -> Result<()> {
    if let Some(native) = &packet.native {
        NativeConfiguration::profile(&native.answers, Target::WindowsX64)?;
        let inputs = Inputs {
            project: &native.configuration,
            mise_lock: &native.lock,
            cargo_lock: packet
                .files
                .get(SOURCE_LOCK)
                .context("native replay needs its Cargo lock")?,
            versions: NATIVE_VERSIONS,
        };
        let configuration = NativeConfiguration::parse(inputs, Target::WindowsX64)?;
        configuration.complete()?;
        ensure!(
            serde_json::to_vec(&configuration)?
                == serde_json::to_vec(&NativeConfiguration::parse(inputs, Target::WindowsX64)?)?,
            "native material decoding is not deterministic"
        );
    }
    validate_rust(packet)
}

fn validate_rust(packet: &Packet) -> Result<()> {
    match &packet.rust {
        None => {}
        Some(RustMetadata::Catalog { text }) => {
            NativeRustCatalog::parse(text)?;
        }
        Some(RustMetadata::Distribution {
            manifest,
            host,
            components,
        }) => {
            RustDistributionMetadata::parse(manifest, host, components)?;
        }
        Some(RustMetadata::Bundle { bytes }) => {
            KaniBundleMetadata::parse_replay(bytes)?;
        }
        Some(RustMetadata::StdlibArchive { bytes }) => replay_stdlib_archive(bytes)?,
    }
    Ok(())
}

pub fn has_mise(packet: &Packet) -> bool {
    packet.mise.is_some() || packet.mise_pin.is_some() || packet.mise_bootstrap_index.is_some()
}

pub fn validate_mise(packet: &Packet) -> Result<()> {
    if let Some(mise) = &packet.mise {
        let scope = tempfile::tempdir()?;
        let result = (|| {
            let root = platform::paths::canonical(scope.path())?;
            std::fs::create_dir_all(root.join("home/.chezmoidata"))?;
            for (name, text) in [
                ("mise.toml", mise.configuration.as_str()),
                ("mise.lock", mise.lock.as_str()),
                ("home/.chezmoidata/versions.toml", mise.versions.as_str()),
                ("Cargo.lock", "version=4\n"),
            ] {
                ensure!(
                    text.len() <= INPUT_BYTE_LIMIT,
                    "mise replay input exceeds its bound"
                );
                std::fs::write(root.join(name), text)?;
            }
            let workload = MiseArchiveWorkload::capture(&root, Target::WindowsX64, &mise.tools)?;
            workload.complete()
        })();
        platform::cleanup::report(scope.close(), "remove the mise material decoder replay");
        result?;
    }
    if let Some(pin) = &packet.mise_pin {
        let decoded = MiseDownloadPin::parse(pin)?;
        ensure!(
            decoded.blake3().len() == BLAKE3_HEX_DIGITS,
            "decoded pin has no exact hash"
        );
    }
    if let Some(index) = &packet.mise_bootstrap_index {
        ensure!(
            MiseBootstrapIndex::parse(index)?.member_count() > 0,
            "decoded bootstrap has no member inventory"
        );
    }
    Ok(())
}

fn archive(extra: Option<(&str, tar::EntryType)>) -> Result<Vec<u8>> {
    let encoder = GzEncoder::new(Vec::new(), Compression::default());
    let mut builder = tar::Builder::new(encoder);
    for (path, contents, kind) in [
        (
            CRATE_MANIFEST,
            b"[package]\nname='example'\nversion='1.2.3'\nedition='2024'\n".as_slice(),
            tar::EntryType::Regular,
        ),
        (
            CRATE_SOURCE,
            b"pub fn value() -> u32 { 37 }\n".as_slice(),
            tar::EntryType::Regular,
        ),
    ]
    .into_iter()
    .chain(extra.map(|(path, kind)| (path, b"".as_slice(), kind)))
    {
        let mut header = tar::Header::new_ustar();
        header.set_size(contents.len() as u64);
        header.set_mode(ARCHIVE_FILE_MODE);
        header.set_entry_type(kind);
        header.as_mut_bytes()[..TAR_NAME_BYTES].fill(0);
        header.as_mut_bytes()[..path.len()].copy_from_slice(path.as_bytes());
        header.set_cksum();
        builder.append(&header, contents)?;
    }
    Ok(builder.into_inner()?.finish()?)
}

fn packet(archive: Option<Vec<u8>>) -> Packet {
    let digest = archive
        .as_deref()
        .map_or_else(|| Digest::of(b"metadata only"), Digest::of);
    let lock = format!(
        "version=4\n[[package]]\nname='{CRATE_NAME}'\nversion='{CRATE_VERSION}'\nsource='{CRATES_IO}'\nchecksum='{}'\n",
        digest.hex()
    );
    Packet {
        files: [
            (SOURCE_MANIFEST.into(), "[workspace]\n".into()),
            (SOURCE_LOCK.into(), lock),
        ]
        .into(),
        archive,
        native: None,
        mise: None,
        mise_pin: None,
        mise_bootstrap_index: None,
        rust: None,
    }
}

fn case(name: &'static str, packet: &Packet, expected: Expected) -> Result<Case> {
    ensure!(
        packet
            .files
            .keys()
            .all(|path| SOURCE_FILES.contains(&path.as_str())),
        "corpus source paths must stay in the maintained fixture"
    );
    Ok(Case {
        name,
        bytes: serde_json::to_vec(packet)?,
        expected,
    })
}

pub fn cases() -> Result<Vec<Case>> {
    let valid = packet(Some(archive(None)?));
    let mut cases = vec![
        case("archive-valid", &valid, Expected::Materials)?,
        case("source-valid", &packet(None), Expected::Plan)?,
    ];
    let mut nested = packet(None);
    nested
        .files
        .insert(FUZZ_MANIFEST.into(), "[workspace]\n".into());
    let nested_lock = nested
        .files
        .get(SOURCE_LOCK)
        .context("maintained corpus root lock")?
        .clone();
    nested.files.insert(FUZZ_LOCK.into(), nested_lock);
    nested
        .files
        .insert(CONFIG.into(), "[alias]\nverify='check --locked'\n".into());
    cases.push(case("source-independent-fuzz", &nested, Expected::Plan)?);
    for (name, path, kind) in [
        (
            "archive-traversal",
            "example-1.2.3/../escape",
            tar::EntryType::Regular,
        ),
        (
            "archive-device",
            "example-1.2.3/CON.txt",
            tar::EntryType::Regular,
        ),
        (
            "archive-symlink",
            "example-1.2.3/link",
            tar::EntryType::Symlink,
        ),
        (
            "archive-case-prefix",
            "example-1.2.3/SRC/other.rs",
            tar::EntryType::Regular,
        ),
    ] {
        cases.push(case(
            name,
            &packet(Some(archive(Some((path, kind)))?)),
            Expected::Refused,
        )?);
    }
    let mut poison = packet(Some(b"poisoned acquired bytes".to_vec()));
    poison.files = valid.files.clone();
    cases.push(case(
        "archive-checksum-mismatch",
        &poison,
        Expected::Refused,
    )?);
    let mut trailing = archive(None)?;
    trailing.extend_from_slice(b"trailing");
    cases.push(case(
        "archive-trailing-member",
        &packet(Some(trailing)),
        Expected::Refused,
    )?);
    let mut missing = packet(None);
    missing
        .files
        .insert(FUZZ_MANIFEST.into(), "[workspace]\n".into());
    cases.push(case("source-missing-lock", &missing, Expected::Refused)?);
    let mut malformed = packet(None);
    malformed
        .files
        .insert(SOURCE_LOCK.into(), "not a lock".into());
    cases.push(case(
        "source-malformed-lock",
        &malformed,
        Expected::Refused,
    )?);
    let mut override_config = packet(None);
    override_config.files.insert(
        LEGACY_CONFIG.into(),
        "[source.crates-io]\nreplace-with='unverified'\n".into(),
    );
    cases.push(case(
        "source-legacy-override",
        &override_config,
        Expected::Refused,
    )?);
    cases.extend(native_cases()?);
    cases.extend(mise_cases()?);
    cases.extend(rust_cases()?);
    Ok(cases)
}

fn rust_cases() -> Result<Vec<Case>> {
    let mut cases = Vec::new();
    for (name, text, expected) in [
        (
            "rust-catalog-valid",
            RUST_CATALOG.to_owned(),
            Expected::Plan,
        ),
        (
            "rust-catalog-unknown",
            format!("unknown=true\n{RUST_CATALOG}"),
            Expected::Refused,
        ),
        (
            "rust-catalog-origin",
            RUST_CATALOG.replace(RUST_RELEASE_ORIGIN, FOREIGN_RELEASE_ORIGIN),
            Expected::Refused,
        ),
    ] {
        let mut input = packet(None);
        input.rust = Some(RustMetadata::Catalog { text });
        cases.push(case(name, &input, expected)?);
    }
    for (name, manifest, host, expected) in [
        (
            "rust-distribution-valid",
            RUST_COMPONENT_MANIFEST.to_owned(),
            RUST_HOST,
            Expected::Plan,
        ),
        (
            "rust-distribution-unavailable",
            RUST_COMPONENT_MANIFEST.replace("available=true", "available=false"),
            RUST_HOST,
            Expected::Refused,
        ),
        (
            "rust-distribution-no-hash",
            RUST_COMPONENT_MANIFEST
                .lines()
                .filter(|line| !line.starts_with("hash="))
                .collect::<Vec<_>>()
                .join("\n"),
            RUST_HOST,
            Expected::Refused,
        ),
        (
            "rust-distribution-origin",
            RUST_COMPONENT_MANIFEST.replace(RUST_RELEASE_ORIGIN, FOREIGN_RELEASE_ORIGIN),
            RUST_HOST,
            Expected::Refused,
        ),
        (
            "rust-distribution-encoded-path",
            RUST_COMPONENT_MANIFEST.replace("dist/2026", "dist/%2e%2e/2026"),
            RUST_HOST,
            Expected::Refused,
        ),
        (
            "rust-distribution-unknown-host",
            RUST_COMPONENT_MANIFEST.to_owned(),
            "unknown-host",
            Expected::Refused,
        ),
    ] {
        let mut input = packet(None);
        input.rust = Some(RustMetadata::Distribution {
            manifest,
            host: host.into(),
            components: vec!["rustc".into()],
        });
        cases.push(case(name, &input, expected)?);
    }
    for (name, linked, expected) in [
        ("rust-kani-bundle-valid", false, Expected::Plan),
        ("rust-kani-bundle-linked", true, Expected::Refused),
    ] {
        let mut input = packet(None);
        input.rust = Some(RustMetadata::Bundle {
            bytes: bundle(linked)?,
        });
        cases.push(case(name, &input, expected)?);
    }
    cases.extend(stdlib_cases()?);
    ensure!(
        cases.len() == RUST_CASE_COUNT,
        "maintained Rust metadata case count differs"
    );
    Ok(cases)
}

fn stdlib_cases() -> Result<Vec<Case>> {
    let lock = format!(
        "version=4\n[[package]]\nname={CRATE_NAME:?}\nversion={CRATE_VERSION:?}\nsource={CRATES_IO:?}\nchecksum={:?}\n",
        Digest::of(b"metadata only").hex()
    );
    let entry = (
        STDLIB_ARCHIVE_MEMBER,
        lock.as_bytes(),
        tar::EntryType::Regular,
    );
    let valid = stdlib_archive(&[entry])?;
    let mut trailing = valid.clone();
    trailing.extend_from_slice(b"foreign gzip trailing data");
    let mut cases = Vec::new();
    for (name, bytes, expected) in [
        ("rust-stdlib-valid", valid, Expected::Plan),
        (
            "rust-stdlib-missing",
            stdlib_archive(&[(STDLIB_MISSING_MEMBER, b"no lock", tar::EntryType::Regular)])?,
            Expected::Refused,
        ),
        (
            "rust-stdlib-duplicate",
            stdlib_archive(&[entry, entry])?,
            Expected::Refused,
        ),
        (
            "rust-stdlib-linked",
            stdlib_archive(&[(STDLIB_ARCHIVE_MEMBER, b"", tar::EntryType::Symlink)])?,
            Expected::Refused,
        ),
        ("rust-stdlib-trailing", trailing, Expected::Refused),
        (
            "rust-stdlib-malformed",
            stdlib_archive(&[(
                STDLIB_ARCHIVE_MEMBER,
                b"not a Cargo lock",
                tar::EntryType::Regular,
            )])?,
            Expected::Refused,
        ),
    ] {
        let mut input = packet(None);
        input.rust = Some(RustMetadata::StdlibArchive { bytes });
        cases.push(case(name, &input, expected)?);
    }
    ensure!(
        cases.len() == STDLIB_CASE_COUNT,
        "maintained stdlib decoder case count differs"
    );
    Ok(cases)
}

fn stdlib_archive(entries: &[(&str, &[u8], tar::EntryType)]) -> Result<Vec<u8>> {
    let mut builder = tar::Builder::new(GzEncoder::new(Vec::new(), Compression::default()));
    for (path, bytes, kind) in entries {
        let mut header = tar::Header::new_ustar();
        header.set_size(u64::try_from(bytes.len())?);
        header.set_mode(ARCHIVE_FILE_MODE);
        header.set_entry_type(*kind);
        if kind.is_symlink() {
            header.set_link_name(STDLIB_LINK_TARGET)?;
        }
        header.set_cksum();
        builder.append_data(&mut header, path, *bytes)?;
    }
    Ok(builder.into_inner()?.finish()?)
}

fn bundle(linked: bool) -> Result<Vec<u8>> {
    let mut builder = tar::Builder::new(GzEncoder::new(Vec::new(), Compression::default()));
    for (path, bytes) in [
        (
            "kani-fixture/rust-toolchain-version",
            b"nightly-2026-08-21\n".as_slice(),
        ),
        (
            "kani-fixture/rustc-version",
            b"rustc 1.99.0-nightly (fixture 2026-08-20)\n".as_slice(),
        ),
    ] {
        let mut header = tar::Header::new_ustar();
        header.set_size(bytes.len() as u64);
        header.set_mode(ARCHIVE_FILE_MODE);
        header.set_cksum();
        builder.append_data(&mut header, path, bytes)?;
    }
    if linked {
        let mut header = tar::Header::new_ustar();
        header.set_size(0);
        header.set_mode(ARCHIVE_FILE_MODE);
        header.set_entry_type(tar::EntryType::Symlink);
        header.set_link_name("../../outside")?;
        header.set_cksum();
        builder.append_data(&mut header, "kani-fixture/toolchain", b"".as_slice())?;
    }
    Ok(builder.into_inner()?.finish()?)
}

fn mise_cases() -> Result<Vec<Case>> {
    let mut cases = mise_configuration_cases()?;
    cases.extend(mise_pin_cases()?);
    cases.extend(mise_bootstrap_cases()?);
    ensure!(
        cases.len() == MISE_CASE_COUNT,
        "maintained mise decoder case count differs"
    );
    Ok(cases)
}

fn mise_configuration_cases() -> Result<Vec<Case>> {
    let mut cases = Vec::new();
    for (name, configuration, lock, versions, expected) in [
        (
            "mise-archive-scope-valid",
            MISE_CONFIGURATION.into(),
            MISE_LOCK.into(),
            MISE_VERSIONS.into(),
            Expected::Plan,
        ),
        (
            "mise-archive-old-engine",
            MISE_CONFIGURATION.into(),
            MISE_LOCK.into(),
            MISE_VERSIONS.replace("2026.10.7", "2026.10.6"),
            Expected::Refused,
        ),
        (
            "mise-archive-provider-unclosed",
            MISE_CONFIGURATION.replace("http:example", "aqua:example"),
            MISE_LOCK.replace("http:example", "aqua:example"),
            MISE_VERSIONS.into(),
            Expected::Refused,
        ),
        (
            "mise-archive-lock-missing",
            MISE_CONFIGURATION.into(),
            "lockfile_version=3\n[tools]\n".into(),
            MISE_VERSIONS.into(),
            Expected::Refused,
        ),
    ] {
        let mut input = packet(None);
        input.mise = Some(Mise {
            tools: vec![
                if configuration.contains("aqua:example") {
                    "aqua:example"
                } else {
                    "http:example"
                }
                .into(),
            ],
            configuration,
            lock,
            versions,
        });
        cases.push(case(name, &input, expected)?);
    }
    Ok(cases)
}

fn mise_pin_cases() -> Result<Vec<Case>> {
    let mut cases = Vec::new();
    for (name, pin, expected) in [
        ("mise-pin-valid", MISE_PIN.to_owned(), Expected::Plan),
        (
            "mise-pin-unknown",
            MISE_PIN.replace('}', ",\"unknown\":true}"),
            Expected::Refused,
        ),
        (
            "mise-pin-repeated",
            MISE_PIN.replace('{', "{\"blake3\":\"wrong\","),
            Expected::Refused,
        ),
        (
            "mise-pin-short-hash",
            MISE_PIN.replace(
                "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad",
                "bad",
            ),
            Expected::Refused,
        ),
        (
            "mise-pin-device",
            MISE_PIN.replace("example.zip", "CON.zip"),
            Expected::Refused,
        ),
        (
            "mise-pin-traversal",
            MISE_PIN.replace("example.zip", "../example.zip"),
            Expected::Refused,
        ),
    ] {
        let mut input = packet(None);
        input.mise_pin = Some(pin.into_bytes());
        cases.push(case(name, &input, expected)?);
    }
    Ok(cases)
}

fn mise_bootstrap_cases() -> Result<Vec<Case>> {
    let mut cases = Vec::new();
    for (name, index, expected) in [
        ("mise-bootstrap-index-valid", MISE_ZIP_INDEX.to_owned(), Expected::Plan),
        ("mise-bootstrap-index-incomplete", MISE_ZIP_INDEX.replace("\"count\":1", "\"count\":2"), Expected::Refused),
        ("mise-bootstrap-index-traversal", MISE_ZIP_INDEX.replace("mise/bin/mise.exe", "mise/../mise.exe"), Expected::Refused),
        ("mise-bootstrap-index-symlink", MISE_ZIP_INDEX.replace("\"attributes\":0", "\"attributes\":2684354560"), Expected::Refused),
        ("mise-bootstrap-index-case", "{\"count\":2,\"entries\":[{\"name\":\"mise/bin/mise.exe\",\"length\":37,\"attributes\":0},{\"name\":\"MISE/bin/other.exe\",\"length\":1,\"attributes\":0}]}".into(), Expected::Refused),
        ("mise-bootstrap-index-wrong-member", MISE_ZIP_INDEX.replace("mise/bin/mise.exe", "mise/bin/other.exe"), Expected::Refused),
    ] {
        let mut input = packet(None);
        input.mise_bootstrap_index = Some(index.into_bytes());
        cases.push(case(name, &input, expected)?);
    }
    Ok(cases)
}

fn native_cases() -> Result<Vec<Case>> {
    let mut cases = Vec::new();
    for (name, configuration, lock, answers, expected) in [
        (
            "native-configuration-valid",
            NATIVE_CONFIGURATION.to_owned(),
            NATIVE_LOCK.to_owned(),
            "[data]\nprofile='windows'\n",
            Expected::Plan,
        ),
        (
            "native-provider-unclosed",
            format!("{NATIVE_CONFIGURATION}\n[tool_alias]\nglobal='asdf:owner/plugin'\n"),
            NATIVE_LOCK.to_owned(),
            "[data]\nprofile='windows'\n",
            Expected::Refused,
        ),
        (
            "native-provider-unknown",
            NATIVE_CONFIGURATION.to_owned(),
            NATIVE_LOCK.replace("aqua:owner/global", "vfox:owner/global"),
            "[data]\nprofile='windows'\n",
            Expected::Refused,
        ),
        (
            "native-namespace-type",
            format!("tool_alias=false\n{NATIVE_CONFIGURATION}"),
            NATIVE_LOCK.to_owned(),
            "[data]\nprofile='windows'\n",
            Expected::Refused,
        ),
        (
            "native-lock-omission",
            "[tools]\nglobal='1.2.3'\nmissing='9.0.0'\n".to_owned(),
            NATIVE_LOCK.to_owned(),
            "[data]\nprofile='windows'\n",
            Expected::Refused,
        ),
        (
            "native-lock-version",
            NATIVE_CONFIGURATION.to_owned(),
            NATIVE_LOCK.replace("version='1.2.3'", "version='1.2.4'"),
            "[data]\nprofile='windows'\n",
            Expected::Refused,
        ),
        (
            "native-answers-profile",
            NATIVE_CONFIGURATION.to_owned(),
            NATIVE_LOCK.to_owned(),
            "[data]\nprofile='mac'\n",
            Expected::Refused,
        ),
        (
            "native-answers-unknown",
            NATIVE_CONFIGURATION.to_owned(),
            NATIVE_LOCK.to_owned(),
            "[data]\nprofile='unknown'\n",
            Expected::Refused,
        ),
        (
            "native-answers-type",
            NATIVE_CONFIGURATION.to_owned(),
            NATIVE_LOCK.to_owned(),
            "data=false\n",
            Expected::Refused,
        ),
    ] {
        let mut input = packet(None);
        input.native = Some(Native {
            configuration,
            lock,
            answers: answers.into(),
        });
        cases.push(case(name, &input, expected)?);
    }
    Ok(cases)
}
