//! Architecture decision records carry front matter with a closed status vocabulary, and `docs/adr/README.md` indexes them.
//! `cargo xtask gen` writes the index, and `cargo xtask gen --check` refuses a stale index or a status that disagrees with its links.

use crate::adr_rules::{Status, Verdict, verdict};
use anyhow::{Context, Result, bail, ensure};
use std::fs;
use std::path::Path;

pub const DIRECTORY: &str = "docs/adr";
pub const INDEX: &str = "docs/adr/README.md";

#[derive(Debug, PartialEq)]
pub struct Record {
    pub file: String,
    pub title: String,
    pub status: Status,
    /// The later records that amended or superseded this one.
    pub changed_by: Vec<String>,
}

/// Reads the front matter and title of one record.
pub fn parse(file: &str, text: &str) -> Result<Record> {
    let (header, body) = crate::split_frontmatter(text)
        .with_context(|| format!("{file}: start with front matter that names its status"))?;
    let mut status = None;
    let mut changed_by = Vec::new();
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
            other => bail!("{file}: front matter key `{other}` is not status or changed_by"),
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
    })
}

pub fn records(root: &Path) -> Result<Vec<Record>> {
    let mut found = Vec::new();
    for entry in fs::read_dir(root.join(DIRECTORY))? {
        let entry = entry?;
        let file = entry.file_name().to_string_lossy().into_owned();
        if file.ends_with(".md") && file != "README.md" {
            found.push(parse(&file, &fs::read_to_string(entry.path())?)?);
        }
    }
    found.sort_by(|left, right| left.file.cmp(&right.file));
    Ok(found)
}

/// Every link names an existing record, and each status agrees with its links.
pub fn findings(records: &[Record]) -> Vec<String> {
    let mut found = Vec::new();
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

fn number(file: &str) -> &str {
    file.split('-').next().unwrap_or(file)
}

/// The index the records produce.
pub fn index(records: &[Record]) -> String {
    let mut text = String::from(
        "# Architecture decisions\n\n`just gen` writes this index from the front matter of each record.\n\n| Record | Status | Changed by |\n| --- | --- | --- |\n",
    );
    for record in records {
        let changed: Vec<String> = record
            .changed_by
            .iter()
            .map(|file| format!("[{}]({file})", number(file)))
            .collect();
        text.push_str(&format!(
            "| [{} {}]({}) | {} | {} |\n",
            number(&record.file),
            record.title,
            record.file,
            record.status.name(),
            changed.join(", ")
        ));
    }
    text
}

/// Writes the index, or with `check` refuses a stale one, after refusing records whose status disagrees with their links.
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
        assert!(findings(&records).is_empty());
        let text = index(&records);
        assert!(text.contains("| [0001 First record](0001-a.md) | amended | [0002](0002-b.md) |"));
        assert!(text.contains("| [0002 Second](0002-b.md) | accepted |  |"));
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
