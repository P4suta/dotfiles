//! Resolves generated TypeScript from the same repository attributes as the surveyed source.

use crate::shape_rules::{CatalogInput, GeneratedMarker, catalog_input};
use crate::tool::Tool;
use anyhow::{Context, Result, ensure};
use std::collections::{BTreeMap, BTreeSet};
use std::io::{Read, Write};
use std::path::Path;
use std::process::{Command, Stdio};

const ATTRIBUTE_FILE: &str = ".gitattributes";
const GENERATED_ATTRIBUTE: &str = "linguist-generated";
const FIELDS_PER_ATTRIBUTE: usize = 3;

pub(super) fn is_attributes_path(path: &str) -> bool {
    path.rsplit('/').next() == Some(ATTRIBUTE_FILE)
}

fn typescript(path: &str) -> bool {
    path.rsplit_once('.')
        .is_some_and(|(_, extension)| matches!(extension, "ts" | "tsx" | "mts" | "cts"))
}

/// Decodes exactly one requested generated marker per path, preserving their order.
///
/// # Errors
///
/// Refuses missing, extra, reordered, mislabeled, or non-UTF-8 records.
pub fn decode(output: &[u8], paths: &[&str]) -> Result<Vec<GeneratedMarker>> {
    let expected = paths
        .len()
        .checked_mul(FIELDS_PER_ATTRIBUTE)
        .and_then(|fields| fields.checked_add(1))
        .context("generated attribute request is too large")?;
    let fields: Vec<&[u8]> = output.split(|byte| *byte == 0).collect();
    ensure!(
        fields.len() == expected && fields.last() == Some(&&b""[..]),
        "git check-attr returned an unexpected generated record count"
    );
    let mut markers = Vec::with_capacity(paths.len());
    for ([path, name, value], expected_path) in fields
        .as_chunks::<FIELDS_PER_ATTRIBUTE>()
        .0
        .iter()
        .zip(paths)
    {
        ensure!(
            !expected_path.is_empty()
                && !expected_path.contains('\0')
                && *path == expected_path.as_bytes()
                && *name == GENERATED_ATTRIBUTE.as_bytes(),
            "git check-attr returned a mismatched generated record"
        );
        let value = std::str::from_utf8(value).context("generated attribute is not UTF-8")?;
        markers.push(match value {
            "set" => GeneratedMarker::Set,
            "true" => GeneratedMarker::True,
            "false" => GeneratedMarker::False,
            "unset" => GeneratedMarker::Unset,
            "unspecified" => GeneratedMarker::Unspecified,
            _ => GeneratedMarker::Other,
        });
    }
    Ok(markers)
}

fn isolated_git(directory: &Path, empty: &Path) -> Command {
    let mut command = Tool::Git.command();
    command
        .arg("-C")
        .arg(directory)
        .env("GIT_CONFIG_GLOBAL", empty)
        .env("GIT_CONFIG_SYSTEM", empty)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_ATTR_NOSYSTEM", "1")
        .env_remove("GIT_ATTR_SOURCE")
        .arg("-c")
        .arg(format!("core.attributesFile={}", empty.display()));
    command
}

pub(super) fn source_paths(
    scratch: &Path,
    scanned: &[String],
    files: &BTreeMap<String, Vec<u8>>,
) -> Result<Vec<String>> {
    let attributes: Vec<_> = files
        .iter()
        .filter(|(path, _)| is_attributes_path(path))
        .collect();
    if !scanned.iter().any(|path| typescript(path)) || attributes.is_empty() {
        return Ok(scanned.to_vec());
    }
    let paths: Vec<&str> = scanned.iter().map(String::as_str).collect();
    for (path, bytes) in attributes {
        let target = scratch.join(path);
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(target, bytes)?;
    }
    let empty = scratch.join(".git-global-empty");
    let template = scratch.join(".git-template-empty");
    std::fs::write(&empty, b"")?;
    std::fs::create_dir(&template)?;
    let initialized = isolated_git(scratch, &empty)
        .args(["init", "--quiet", "--template"])
        .arg(&template)
        .output()
        .context("initialize the isolated attribute repository")?;
    ensure!(
        initialized.status.success(),
        "cannot initialize the isolated attribute repository: {}",
        String::from_utf8_lossy(&initialized.stderr).trim()
    );
    let mut input = Vec::new();
    for path in &paths {
        input.extend_from_slice(path.as_bytes());
        input.push(0);
    }
    let mut child = isolated_git(scratch, &empty)
        .args(["check-attr", "-z", "--stdin", GENERATED_ATTRIBUTE])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .context("resolve the surveyed generated attributes")?;
    let mut stdin = child.stdin.take().context("Git input is unavailable")?;
    let writer = std::thread::spawn(move || stdin.write_all(&input));
    let mut output = Vec::new();
    child
        .stdout
        .take()
        .context("Git output is unavailable")?
        .read_to_end(&mut output)?;
    writer
        .join()
        .map_err(|_| anyhow::anyhow!("the generated attribute writer panicked"))??;
    ensure!(child.wait()?.success(), "git check-attr failed");
    let markers = decode(&output, &paths)?;
    let generated: BTreeSet<&str> = paths
        .into_iter()
        .zip(markers)
        .filter_map(|(path, marker)| {
            (catalog_input(typescript(path), marker) == CatalogInput::Generated).then_some(path)
        })
        .collect();
    Ok(scanned
        .iter()
        .filter(|path| !generated.contains(path.as_str()))
        .cloned()
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generated_records_require_the_requested_paths_and_complete_protocol() {
        let paths = ["src/client.ts", "src/client with 'quotes'.ts"];
        let valid = b"src/client.ts\0linguist-generated\0set\0src/client with 'quotes'.ts\0linguist-generated\0false\0";
        assert_eq!(
            decode(valid, &paths).unwrap(),
            [GeneratedMarker::Set, GeneratedMarker::False]
        );
        for invalid in [
            &b""[..],
            &valid[..valid.len() - 1],
            b"src/client.ts\0linguist-generated\0set\0",
            b"src/client.ts\0other\0set\0src/client with 'quotes'.ts\0linguist-generated\0false\0",
            b"src/client.ts\0linguist-generated\0set\0src/client.ts\0linguist-generated\0set\0",
            b"src/client with 'quotes'.ts\0linguist-generated\0set\0src/client.ts\0linguist-generated\0set\0",
            b"src/client.ts\0linguist-generated\0\xff\0src/client with 'quotes'.ts\0linguist-generated\0false\0",
        ] {
            assert!(decode(invalid, &paths).is_err(), "{invalid:?}");
        }
        assert!(decode(b"\0linguist-generated\0set\0", &[""]).is_err());
        assert!(decode(b"a\0b\0linguist-generated\0set\0", &["a\0b"]).is_err());
    }

    #[test]
    fn the_field_separated_corpus_replays_positive_negative_and_unknown_markers() {
        let paths = ["src/client.ts", "src/client with 'quotes'.ts"];
        let corpus = [
            (
                &include_bytes!("../../../../fuzz/corpus/generated_attributes/valid")[..],
                Some([GeneratedMarker::Set, GeneratedMarker::False]),
            ),
            (
                &include_bytes!("../../../../fuzz/corpus/generated_attributes/wrong-path")[..],
                None,
            ),
            (
                &include_bytes!("../../../../fuzz/corpus/generated_attributes/unknown-value")[..],
                Some([GeneratedMarker::Other, GeneratedMarker::Unspecified]),
            ),
        ];
        for (input, expected) in corpus {
            let output: Vec<_> = input
                .iter()
                .map(|byte| if *byte == b'\n' { 0 } else { *byte })
                .collect();
            match expected {
                Some(markers) => assert_eq!(decode(&output, &paths).unwrap(), markers),
                None => assert!(decode(&output, &paths).is_err()),
            }
        }
    }
}
