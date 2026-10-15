#![no_main]

#[path = "../../crates/dotfiles/examples/support/artifact_material_corpus.rs"]
mod corpus;

use anyhow::{Result, ensure};
use corpus::{Expected, INPUT_BYTE_LIMIT, Packet};
use dotfiles::artifacts::{Cache, CargoBudget, CargoWorkload, Fetch, FetchFailure};
use std::fs;
use std::path::Path;
use std::sync::LazyLock;

static CASES: LazyLock<Vec<corpus::Case>> =
    LazyLock::new(|| corpus::cases().expect("maintained artifact corpus generation"));

#[derive(Debug)]
struct Bytes<'a>(&'a [u8]);

impl Fetch for Bytes<'_> {
    fn fetch(&self, _source: &str, destination: &Path) -> Result<(), FetchFailure> {
        fs::write(destination, self.0).map_err(|error| FetchFailure::Refused(error.to_string()))
    }
}

fn inspect(data: &[u8]) -> Result<Expected> {
    ensure!(
        data.len() <= INPUT_BYTE_LIMIT,
        "artifact replay input exceeds its bound"
    );
    let packet: Packet = serde_json::from_slice(data)?;
    ensure!(
        packet.files.len() <= corpus::SOURCE_FILES.len(),
        "artifact replay source set exceeds its bound"
    );
    let scope = tempfile::tempdir()?;
    let result = (|| {
        let root = scope.path().join("source");
        let destination = scope.path().join("materials");
        fs::create_dir(&root)?;
        fs::create_dir(&destination)?;
        for (name, text) in &packet.files {
            ensure!(
                corpus::SOURCE_FILES.contains(&name.as_str()),
                "artifact replay source path is outside its closed fixture"
            );
            let path = root.join(name);
            fs::create_dir_all(path.parent().expect("owned replay file parent"))?;
            fs::write(path, text)?;
        }
        corpus::validate_native(&packet)?;
        corpus::validate_mise(&packet)?;
        let workload = CargoWorkload::capture(&root)?;
        let plan = workload.plan()?;
        plan.complete()?;
        let repeated = workload.plan()?;
        ensure!(
            plan.digest() == repeated.digest(),
            "current Cargo material plans do not round trip"
        );
        let Some(archive) = packet.archive else {
            return Ok(Expected::Plan);
        };
        ensure!(
            archive.len() <= INPUT_BYTE_LIMIT,
            "artifact replay archive exceeds its bound"
        );
        let verified = Cache::new(scope.path().join("cache")).acquire(&plan, &Bytes(&archive))?;
        let prepared =
            verified.prepare_cargo_with_budget(&workload, &destination, CargoBudget::Replay)?;
        ensure!(
            prepared.digest() == plan.digest(),
            "prepared Cargo inputs lost their source binding"
        );
        prepared.configure(&workload, &mut platform::tool::Tool::Cargo.command())?;
        Ok(Expected::Materials)
    })();
    platform::cleanup::report(scope.close(), "remove the artifact material replay");
    result
}

libfuzzer_sys::fuzz_target!(|data: &[u8]| {
    if data.len() > INPUT_BYTE_LIMIT {
        return;
    }
    let actual = inspect(data);
    if let Some(case) = CASES.iter().find(|case| case.bytes == data) {
        match case.expected {
            Expected::Refused => assert!(actual.is_err(), "{} must refuse", case.name),
            expected => assert_eq!(
                actual.expect("known valid artifact input must be accepted"),
                expected,
                "{}",
                case.name
            ),
        }
    }
});
