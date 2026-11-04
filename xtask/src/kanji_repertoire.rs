//! The Japanese kanji repertoire: every Han code point with a source in the Japanese Industrial Standards (JIS) X 0208 or X 0213, derived from the Unihan database.
//!
//! `refresh` reads a verified `Unihan_IRGSources.txt` and writes the vendored code point list.
//! `table` turns that list into the Rust range table the content filter searches, or with `check` refuses a stale table.

use anyhow::{Context, Result, bail, ensure};
use sha2::{Digest, Sha256};
use std::fs;
use std::path::Path;

/// The vendored list of code points, one uppercase hexadecimal number per line after the header.
pub const SOURCE: &str = "data/unihan-jis-kanji.txt";
/// The generated range table the `dotguard` crate searches.
pub const GENERATED: &str = "crates/dotguard/src/jis_kanji_table.rs";
/// The Unihan release the list derives from.
pub const UNIHAN_VERSION: &str = "16.0.0";
/// The archive that carries the release.
pub const UNIHAN_ARCHIVE: &str = "https://www.unicode.org/Public/16.0.0/ucd/Unihan.zip";
/// The SHA-256 digest of `Unihan_IRGSources.txt` inside that archive.
pub const IRG_SOURCES_SHA256: &str =
    "4c993182773d942c42e8be5f93cfc117938bc5d6bb28de35e8a7f2957c12ad5e";
/// The `kIRG_JSource` prefixes that place a character in JIS X 0208 or X 0213.
const JIS_SOURCES: [&str; 7] = ["J0", "J3", "J3A", "J4", "J13", "J13A", "J14"];
const PROPERTY: &str = "kIRG_JSource";
const HEX_RADIX: u32 = 16;
const HEADER_MARKER: char = '#';
const CODE_POINT_PREFIX: &str = "U+";
const RANGES_PER_LINE: usize = 6;

/// Reads a verified `Unihan_IRGSources.txt` and writes the vendored code point list.
///
/// # Errors
///
/// Returns an error when the file is missing, its digest differs from the pinned release, or writing fails.
pub fn refresh(root: &Path, irg_sources: &Path) -> Result<()> {
    let bytes = fs::read(irg_sources).with_context(|| format!("read {}", irg_sources.display()))?;
    let digest = crate::build_inputs::hex(&Sha256::digest(&bytes));
    ensure!(
        digest == IRG_SOURCES_SHA256,
        "{} has digest {digest}, not the pinned Unihan {UNIHAN_VERSION} digest {IRG_SOURCES_SHA256}",
        irg_sources.display()
    );
    let text = String::from_utf8(bytes).context("Unihan_IRGSources.txt is not UTF-8")?;
    let code_points = jis_code_points(&text)?;
    fs::write(root.join(SOURCE), render_source(&code_points))?;
    Ok(())
}

/// Writes the range table from the vendored list, or with `check` refuses a stale table.
///
/// # Errors
///
/// Returns an error when the list holds a bad line, writing fails, or with `check` when the committed table differs.
pub fn table(root: &Path, check: bool) -> Result<()> {
    let source = fs::read_to_string(root.join(SOURCE))
        .with_context(|| format!("read {SOURCE}; run `just kanji-repertoire` to vendor it"))?;
    let wanted = render_table(&ranges(&parse_source(&source)?));
    let file = root.join(GENERATED);
    if check {
        ensure!(
            fs::read_to_string(&file).is_ok_and(|current| current == wanted),
            "{GENERATED} differs from what {SOURCE} gives; run `just gen` and commit the result"
        );
    } else {
        fs::write(&file, wanted)?;
    }
    Ok(())
}

/// The sorted, deduplicated code points whose `kIRG_JSource` names a JIS X 0208 or JIS X 0213 position.
fn jis_code_points(irg_sources: &str) -> Result<Vec<u32>> {
    let mut found = Vec::new();
    for line in irg_sources.lines() {
        let mut fields = line.split('\t');
        let (Some(code_point), Some(property), Some(value)) =
            (fields.next(), fields.next(), fields.next())
        else {
            continue;
        };
        if property != PROPERTY {
            continue;
        }
        let Some((source, _)) = value.split_once('-') else {
            bail!("{PROPERTY} value {value:?} has no source prefix");
        };
        if !JIS_SOURCES.contains(&source) {
            continue;
        }
        let digits = code_point
            .strip_prefix(CODE_POINT_PREFIX)
            .with_context(|| format!("code point {code_point:?} lacks the U+ prefix"))?;
        found.push(u32::from_str_radix(digits, HEX_RADIX)?);
    }
    ensure!(!found.is_empty(), "no {PROPERTY} entry names a JIS source");
    found.sort_unstable();
    found.dedup();
    Ok(found)
}

/// Joins lines with a trailing newline, the shape of every file this module writes.
fn lines(lines: impl IntoIterator<Item = String>) -> String {
    lines.into_iter().map(|line| line + "\n").collect()
}

fn render_source(code_points: &[u32]) -> String {
    let header = [
        format!(
            "{HEADER_MARKER} Code points whose kIRG_JSource is J0, J3, J3A, J4, J13, J13A, or J14 in Unihan {UNIHAN_VERSION}."
        ),
        format!(
            "{HEADER_MARKER} Source: {UNIHAN_ARCHIVE} (Unihan_IRGSources.txt sha256 {IRG_SOURCES_SHA256})."
        ),
        format!(
            "{HEADER_MARKER} Unicode data is used under the Unicode License v3 in data/UNICODE-LICENSE.txt."
        ),
        format!(
            "{HEADER_MARKER} Regenerate with `just kanji-repertoire`, which runs `cargo xtask gen --irg-sources`; `just gen` derives {GENERATED} from this file."
        ),
    ];
    lines(
        header.into_iter().chain(
            code_points
                .iter()
                .map(|code_point| format!("{code_point:04X}")),
        ),
    )
}

fn parse_source(text: &str) -> Result<Vec<u32>> {
    let mut code_points = Vec::new();
    for line in text.lines() {
        if line.starts_with(HEADER_MARKER) || line.trim().is_empty() {
            continue;
        }
        let value = u32::from_str_radix(line.trim(), HEX_RADIX)
            .with_context(|| format!("{SOURCE} holds a non-hexadecimal line {line:?}"))?;
        ensure!(
            code_points.last().is_none_or(|last| *last < value),
            "{SOURCE} must list code points in strictly increasing order; {line:?} breaks it"
        );
        code_points.push(value);
    }
    ensure!(!code_points.is_empty(), "{SOURCE} lists no code point");
    Ok(code_points)
}

/// Inclusive ranges of consecutive code points.
fn ranges(code_points: &[u32]) -> Vec<(u32, u32)> {
    let mut ranges: Vec<(u32, u32)> = Vec::new();
    for &value in code_points {
        match ranges.last_mut() {
            Some((_, high)) if *high + 1 == value => *high = value,
            _ => ranges.push((value, value)),
        }
    }
    ranges
}

fn render_table(ranges: &[(u32, u32)]) -> String {
    let total: u64 = ranges
        .iter()
        .map(|&(low, high)| u64::from(high - low) + 1)
        .sum();
    let header = [
        format!("//! Generated by `cargo xtask gen` from `{SOURCE}`; do not edit."),
        format!(
            "//! Inclusive ranges of the Han code points with a Japanese Industrial Standards source in Unihan {UNIHAN_VERSION}."
        ),
        String::new(),
        format!(
            "/// Sorted, non-overlapping, non-adjacent inclusive ranges covering {total} code points."
        ),
        "#[rustfmt::skip]".to_owned(),
        "pub const JIS_KANJI_RANGES: &[(u32, u32)] = &[".to_owned(),
    ];
    let rows = ranges.chunks(RANGES_PER_LINE).map(|chunk| {
        chunk
            .iter()
            .map(|&(low, high)| format!(" (0x{low:04X}, 0x{high:04X}),"))
            .fold("   ".to_owned(), |row, cell| row + &cell)
    });
    lines(
        header
            .into_iter()
            .chain(rows)
            .chain(std::iter::once("];".to_owned())),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn jis_sources_select_only_the_japanese_standards() -> Result<()> {
        let text = "U+4E00\tkIRG_JSource\tJ0-306C\nU+7F13\tkIRG_GSource\tG0-3B3A\nU+8FD0\tkIRG_JSource\tJ1-613D\nU+5220\tkIRG_JSource\tJMJ-007487\nU+5D0E\tkIRG_JSource\tJ0-3A6A\nU+4E00\tkIRG_JSource\tJ0-306C\n";
        assert_eq!(jis_code_points(text)?, vec![0x4E00, 0x5D0E]);
        Ok(())
    }

    #[test]
    fn ranges_merge_consecutive_code_points_only() {
        assert_eq!(
            ranges(&[0x4E00, 0x4E01, 0x4E02, 0x4E04, 0x9FFF]),
            vec![(0x4E00, 0x4E02), (0x4E04, 0x4E04), (0x9FFF, 0x9FFF)]
        );
    }

    #[test]
    fn source_round_trips_through_render_and_parse() -> Result<()> {
        let code_points = vec![0x4E00, 0x4E01, 0x2000B];
        assert_eq!(parse_source(&render_source(&code_points))?, code_points);
        Ok(())
    }

    #[test]
    fn source_refuses_unsorted_lines() {
        assert!(parse_source("4E01\n4E00\n").is_err());
    }

    #[test]
    fn the_committed_table_matches_the_vendored_list() -> Result<()> {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .context("workspace root")?;
        table(root, true)
    }
}
