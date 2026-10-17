//! Architecture decision records carry front matter with a closed status vocabulary, and `docs/adr/README.md` indexes them.
//! A record from before dated names keeps its number, and every later record goes by the name of its decision and a `date`, so concurrent branches never claim one identity.
//! `cargo xtask gen` writes the index, and `cargo xtask gen --check` refuses a stale index, a record without an identity, or a status that disagrees with its links.

use crate::adr_rules::{Identity, Status, Verdict, identity, verdict};
use anyhow::{Context, Result, bail, ensure};
use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

pub const DIRECTORY: &str = "docs/adr";
pub const INDEX: &str = "docs/adr/README.md";

/// The digits of a numbered record's prefix.
const NUMBER_DIGITS: usize = 4;
/// An ISO 8601 date, `YYYY-MM-DD`, and the positions of its hyphens.
const DATE_LENGTH: usize = 10;
const DATE_SEPARATORS: [usize; 2] = [4, 7];

#[derive(Debug, PartialEq)]
pub struct Record {
    pub file: String,
    pub title: String,
    pub status: Status,
    /// The later records that amended or superseded this one.
    pub changed_by: Vec<String>,
    /// The day of the decision of a record without a number, which places it in the index.
    pub date: Option<String>,
}

/// The number at the start of a numbered record's filename.
fn number(file: &str) -> Option<u16> {
    let (digits, _) = file.split_once('-')?;
    (digits.len() == NUMBER_DIGITS && digits.bytes().all(|byte| byte.is_ascii_digit()))
        .then(|| digits.parse().ok())
        .flatten()
}

fn is_date(text: &str) -> bool {
    text.len() == DATE_LENGTH
        && text.bytes().enumerate().all(|(index, byte)| {
            if DATE_SEPARATORS.contains(&index) {
                byte == b'-'
            } else {
                byte.is_ascii_digit()
            }
        })
}

/// Reads the front matter and title of one record.
pub(crate) fn parse(file: &str, text: &str) -> Result<Record> {
    let (header, body) = crate::split_frontmatter(text)
        .with_context(|| format!("{file}: start with front matter that names its status"))?;
    let mut status = None;
    let mut changed_by = Vec::new();
    let mut date = None;
    for line in header.lines() {
        let (key, value) = line
            .split_once(':')
            .with_context(|| format!("{file}: front matter line `{line}` names no key"))?;
        let value = value.trim();
        match key.trim() {
            "status" => {
                status = Some(
                    Status::ALL
                        .into_iter()
                        .find(|candidate| candidate.name() == value)
                        .with_context(|| {
                            format!(
                                "{file}: status `{value}` is not one of {}",
                                Status::ALL.map(Status::name).join(", ")
                            )
                        })?,
                );
            }
            "changed_by" => {
                changed_by = value
                    .trim_start_matches('[')
                    .trim_end_matches(']')
                    .split(',')
                    .map(str::trim)
                    .filter(|entry| !entry.is_empty())
                    .map(str::to_owned)
                    .collect();
            }
            "date" => {
                ensure!(
                    is_date(value),
                    "{file}: date `{value}` is not a YYYY-MM-DD day"
                );
                date = Some(value.to_owned());
            }
            other => {
                bail!("{file}: front matter key `{other}` is not status, changed_by, or date")
            }
        }
    }
    let status = status.with_context(|| format!("{file}: front matter needs a status"))?;
    let title = body
        .lines()
        .find_map(|line| line.strip_prefix("# "))
        .with_context(|| format!("{file}: needs a `# ` title"))?
        .trim()
        .to_owned();
    Ok(Record {
        file: file.to_owned(),
        title,
        status,
        changed_by,
        date,
    })
}

/// Numbered records first in number order, then dated ones by date and name, so the order never depends on which branch merged first.
fn order(records: &mut [Record]) {
    records.sort_by(|left, right| {
        (number(&left.file).is_none(), &left.date, &left.file).cmp(&(
            number(&right.file).is_none(),
            &right.date,
            &right.file,
        ))
    });
}

pub(crate) fn records(root: &Path) -> Result<Vec<Record>> {
    let mut found = Vec::new();
    for entry in fs::read_dir(root.join(DIRECTORY))? {
        let entry = entry?;
        let file = entry.file_name().to_string_lossy().into_owned();
        if crate::has_extension(&file, "md") && file != "README.md" {
            found.push(parse(&file, &fs::read_to_string(entry.path())?)?);
        }
    }
    order(&mut found);
    Ok(found)
}

/// Every record has one identity that no other record claims, every link names an existing record, and each status agrees with its links.
#[must_use]
pub fn findings(records: &[Record]) -> Vec<String> {
    let mut found = Vec::new();
    let mut numbers: BTreeMap<u16, Vec<&str>> = BTreeMap::new();
    for record in records {
        let numbered = number(&record.file);
        if let Some(numbered) = numbered {
            numbers.entry(numbered).or_default().push(&record.file);
        }
        if let Identity::Refused(reason) = identity(numbered, record.date.is_some()) {
            found.push(format!(
                "{}: {:?}; {}",
                record.file,
                reason,
                reason.next_action()
            ));
        }
    }
    for (claimed, files) in numbers {
        if files.len() > 1 {
            found.push(format!(
                "{} share the number {claimed:04}; give every record but the first a dated name, then run `just gen`",
                files.join(" and ")
            ));
        }
    }
    for record in records {
        for successor in &record.changed_by {
            if !records.iter().any(|other| &other.file == successor) {
                found.push(format!(
                    "{}: changed_by names {successor}, which no record in {DIRECTORY} has",
                    record.file
                ));
            }
        }
        match verdict(record.status, record.changed_by.len()) {
            Verdict::Consistent => {}
            Verdict::MissingSuccessor => found.push(format!(
                "{}: a record with status {} lists the records that changed it in changed_by",
                record.file,
                record.status.name()
            )),
            Verdict::HiddenSuccessor => found.push(format!(
                "{}: a record that later records changed has status amended or superseded, not {}",
                record.file,
                record.status.name()
            )),
        }
    }
    found
}

/// How the index names a record: its number, or its date.
fn label(file: &str, records: &[Record]) -> String {
    number(file).map_or_else(
        || {
            records
                .iter()
                .find(|record| record.file == file)
                .and_then(|record| record.date.clone())
                .unwrap_or_else(|| file.trim_end_matches(".md").to_owned())
        },
        |number| format!("{number:04}"),
    )
}

/// The index the records produce.
#[must_use]
pub fn index(records: &[Record]) -> String {
    let rows: Vec<String> = records
        .iter()
        .map(|record| {
            let changed: Vec<String> = record
                .changed_by
                .iter()
                .map(|file| format!("[{}]({file})", label(file, records)))
                .collect();
            format!(
                "| [{} {}]({}) | {} | {} |\n",
                label(&record.file, records),
                record.title,
                record.file,
                record.status.name(),
                changed.join(", ")
            )
        })
        .collect();
    format!(
        "# Architecture decisions\n\n`just gen` writes this index from the front matter of each record.\n\n| Record | Status | Changed by |\n| --- | --- | --- |\n{}",
        rows.concat()
    )
}

/// Writes the index, or with `check` refuses a stale one, after refusing records whose status disagrees with their links.
///
/// # Errors
///
/// Returns an error when it can't read or parse a record, a status disagrees with its links, or it can't write the index.
/// With `check`, it also fails when the index has gone stale.
pub fn generate(root: &Path, check: bool) -> Result<()> {
    let records = records(root)?;
    let found = findings(&records);
    ensure!(
        found.is_empty(),
        "architecture decision records disagree with their links:\n{}",
        found.join("\n")
    );
    let wanted = index(&records);
    let path = root.join(INDEX);
    if check {
        ensure!(
            fs::read_to_string(&path).is_ok_and(|current| current == wanted),
            "{INDEX} differs from the records; run `just gen` and commit the result"
        );
    } else {
        fs::write(&path, wanted)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const AMENDED: &str =
        "---\nstatus: amended\nchanged_by: [0002-b.md]\n---\n# First record\n\nBody.\n";

    #[test]
    fn a_record_reads_its_status_links_and_title() {
        let record = parse("0001-a.md", AMENDED).unwrap();
        assert_eq!(record.status, Status::Amended);
        assert_eq!(record.changed_by, ["0002-b.md"]);
        assert_eq!(record.title, "First record");
    }

    #[test]
    fn an_open_status_word_and_a_missing_link_target_are_refused() {
        let error = parse("0001-a.md", "---\nstatus: mostly accepted\n---\n# A\n")
            .unwrap_err()
            .to_string();
        assert!(error.contains("is not one of"), "{error}");
        let records = [parse("0001-a.md", AMENDED).unwrap()];
        let found = findings(&records);
        assert_eq!(found.len(), 1, "{found:?}");
        assert!(found[0].contains("0002-b.md"));
    }

    #[test]
    fn the_index_lists_every_record_with_its_links() {
        let records = [
            parse("0001-a.md", AMENDED).unwrap(),
            parse("0002-b.md", "---\nstatus: accepted\n---\n# Second\n").unwrap(),
        ];
        assert_eq!(findings(&records), Vec::<String>::new());
        let text = index(&records);
        assert!(text.contains("| [0001 First record](0001-a.md) | amended | [0002](0002-b.md) |"));
        assert!(text.contains("| [0002 Second](0002-b.md) | accepted |  |"));
    }

    const DATED: &str = "---\nstatus: accepted\ndate: 2026-10-07\n---\n# Dated\n";

    /// The records of a base branch plus the records two branches added, merged in the given order.
    fn merged(first: &str, second: &str) -> Vec<Record> {
        let mut found = vec![
            parse("0001-a.md", AMENDED).unwrap(),
            parse("0002-b.md", "---\nstatus: accepted\n---\n# Second\n").unwrap(),
        ];
        for file in [first, second] {
            found.push(parse(file, DATED).unwrap());
        }
        order(&mut found);
        found
    }

    #[test]
    fn two_branches_that_each_add_a_record_merge_in_either_order_without_a_collision() {
        let one = merged("one-decision.md", "another-decision.md");
        let other = merged("another-decision.md", "one-decision.md");
        assert_eq!(findings(&one), Vec::<String>::new());
        assert_eq!(index(&one), index(&other));
        assert!(index(&one).contains("| [2026-10-07 Dated](another-decision.md) | accepted |  |"));
    }

    #[test]
    fn a_shared_number_a_late_number_and_an_undated_name_are_refused_with_their_fix() {
        let records = [
            parse("0002-b.md", "---\nstatus: accepted\n---\n# B\n").unwrap(),
            parse("0002-c.md", "---\nstatus: accepted\n---\n# C\n").unwrap(),
            parse("0099-late.md", "---\nstatus: accepted\n---\n# Late\n").unwrap(),
            parse("undated.md", "---\nstatus: accepted\n---\n# Undated\n").unwrap(),
        ];
        let found = findings(&records).join("\n");
        assert!(
            found.contains("0002-b.md and 0002-c.md share the number 0002"),
            "{found}"
        );
        assert!(
            found.contains("0099-late.md: LateNumber; Drop the number"),
            "{found}"
        );
        assert!(found.contains("undated.md: Undated; Add `date"), "{found}");
        let error = parse("x.md", "---\nstatus: accepted\ndate: soon\n---\n# X\n")
            .unwrap_err()
            .to_string();
        assert!(error.contains("YYYY-MM-DD"), "{error}");
    }

    #[test]
    fn the_check_refuses_a_stale_index_until_gen_writes_it() {
        let root = tempfile::tempdir().unwrap();
        let directory = root.path().join(DIRECTORY);
        fs::create_dir_all(&directory).unwrap();
        fs::write(
            directory.join("0001-a.md"),
            "---\nstatus: accepted\n---\n# A\n",
        )
        .unwrap();
        fs::write(root.path().join(INDEX), "# Architecture decisions\n").unwrap();
        let error = generate(root.path(), true).unwrap_err().to_string();
        assert!(error.contains("just gen"), "{error}");
        generate(root.path(), false).unwrap();
        generate(root.path(), true).unwrap();
    }
}
