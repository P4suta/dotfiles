use anyhow::{Context, Result, ensure};
use dotfiles::artifacts::{Cache, CargoWorkload, Digest, Fetch, FetchFailure};
use flate2::{Compression, write::GzEncoder};
use std::cell::Cell;
use std::fs;
use std::path::{Path, PathBuf};

const CACHE: &str = ".cache/dotfiles/artifacts";
const NAME: &str = "host-fixture-material";
const VERSION: &str = "1.2.3";
const REGISTRY: &str = "registry+https://github.com/rust-lang/crates.io-index";
const ARCHIVE_SOURCE: &str =
    "https://static.crates.io/crates/host-fixture-material/host-fixture-material-1.2.3.crate";

struct Bytes<'a> {
    archive: &'a [u8],
    calls: Cell<usize>,
}

impl Fetch for Bytes<'_> {
    fn fetch(&self, source: &str, destination: &Path) -> Result<(), FetchFailure> {
        if source != ARCHIVE_SOURCE {
            return Err(FetchFailure::Refused(
                "unexpected fixture material source".into(),
            ));
        }
        self.calls.set(self.calls.get() + 1);
        fs::write(destination, self.archive)
            .map_err(|error| FetchFailure::Refused(error.to_string()))
    }
}

fn archive() -> Result<Vec<u8>> {
    let gzip = GzEncoder::new(Vec::new(), Compression::default());
    let mut builder = tar::Builder::new(gzip);
    let manifest = format!("[package]\nname={NAME:?}\nversion={VERSION:?}\nedition='2024'\n");
    for (name, bytes) in [
        ("Cargo.toml", manifest.as_bytes()),
        ("src/lib.rs", b"pub fn value() -> u32 { 37 }\n".as_slice()),
    ] {
        let mut header = tar::Header::new_ustar();
        header.set_size(u64::try_from(bytes.len())?);
        header.set_mode(0o644);
        header.set_entry_type(tar::EntryType::Regular);
        header.set_cksum();
        builder.append_data(&mut header, format!("{NAME}-{VERSION}/{name}"), bytes)?;
    }
    Ok(builder.into_inner()?.finish()?)
}

pub fn seed(root: &Path, home: &Path) -> Result<PathBuf> {
    let checksum = Digest::of(&archive()?).hex();
    fs::create_dir_all(root.join("src"))?;
    fs::write(
        root.join("src/lib.rs"),
        "pub fn value() -> u32 { host_fixture_material::value() }\n",
    )?;
    fs::write(
        root.join("Cargo.toml"),
        format!(
            "[package]\nname='host-fixture'\nversion='0.1.0'\nedition='2024'\n[workspace]\n[dependencies]\n{NAME}='={VERSION}'\n"
        ),
    )?;
    fs::write(
        root.join("Cargo.lock"),
        format!(
            "version=4\n[[package]]\nname='host-fixture'\nversion='0.1.0'\ndependencies=['{NAME}']\n[[package]]\nname='{NAME}'\nversion='{VERSION}'\nsource='{REGISTRY}'\nchecksum='{checksum}'\n"
        ),
    )?;
    seed_cache(root, home)
}

pub fn seed_cache(root: &Path, home: &Path) -> Result<PathBuf> {
    let bytes = archive()?;
    let workload = CargoWorkload::capture(root)?;
    let plan = workload.plan()?;
    let [material] = plan.artifacts() else {
        anyhow::bail!("host fixture requires exactly one Cargo material");
    };
    ensure!(
        material.subject() == format!("crate:{NAME}@{VERSION}")
            && material.expected() == Digest::of(&bytes),
        "host fixture Cargo lock differs from its generated material"
    );
    let cache = home.join(CACHE);
    let fetch = Bytes {
        archive: &bytes,
        calls: Cell::new(0),
    };
    Cache::new(cache.clone()).acquire(&plan, &fetch)?;
    ensure!(
        fetch.calls.get() <= 1,
        "fixture material acquisition repeated"
    );
    Cache::new(cache.clone()).verify(&plan)?;
    let blob = cache.join(format!("{}.blob", material.expected().hex()));
    ensure!(
        Digest::of(&fs::read(&blob).context("read acquired host fixture material")?)
            == material.expected(),
        "host fixture material changed after verification"
    );
    Ok(blob)
}
