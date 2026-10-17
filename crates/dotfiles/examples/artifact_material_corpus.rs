#[path = "support/artifact_material_corpus.rs"]
mod corpus;

use anyhow::{Context, Result, ensure};
use std::fs;
use std::path::PathBuf;

const DESTINATION: &str = "fuzz/corpus/artifact_materials";

fn validate_metadata(case: &corpus::Case) -> Result<()> {
    let packet: corpus::Packet = serde_json::from_slice(&case.bytes)?;
    if packet.native.is_some() || corpus::has_mise(&packet) || packet.rust.is_some() {
        let actual = corpus::validate_native(&packet).and_then(|()| corpus::validate_mise(&packet));
        match case.expected {
            corpus::Expected::Plan => actual.with_context(|| case.name)?,
            corpus::Expected::Refused => ensure!(actual.is_err(), "{} must refuse", case.name),
            corpus::Expected::Materials => {
                anyhow::bail!("metadata cannot assert consumed materials")
            }
        }
    }
    Ok(())
}

fn main() -> Result<()> {
    let mut arguments = std::env::args_os().skip(1);
    let destination = arguments
        .next()
        .map_or_else(|| PathBuf::from(DESTINATION), PathBuf::from);
    ensure!(
        arguments.next().is_none(),
        "the corpus generator accepts one destination"
    );
    fs::create_dir_all(&destination).context("create the artifact corpus destination")?;
    let metadata = fs::symlink_metadata(&destination)?;
    ensure!(
        metadata.is_dir() && !metadata.file_type().is_symlink(),
        "artifact corpus destination must be a real directory"
    );
    for case in corpus::cases()? {
        ensure!(
            case.bytes.len() <= corpus::INPUT_BYTE_LIMIT,
            "artifact corpus input exceeds its replay bound"
        );
        validate_metadata(&case)?;
        let file = destination.join(case.name);
        match fs::symlink_metadata(&file) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error).context("inspect the artifact corpus file"),
            Ok(metadata) => ensure!(
                metadata.is_file() && !metadata.file_type().is_symlink(),
                "artifact corpus file must be a regular owned file"
            ),
        }
        fs::write(file, &case.bytes)?;
        println!(
            "{} {:?} {}",
            case.name,
            case.expected,
            dotfiles::artifacts::Digest::of(&case.bytes).hex()
        );
    }
    Ok(())
}
