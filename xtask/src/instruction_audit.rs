//! Every line of the managed global agent instructions is classified in a tracked audit.
//!
//! A retained line is either judgment that no mechanism can enforce or a named follow-up that will mechanize it.
//! The check refuses an unclassified line, a quote of a line that no longer exists, and a profile template that adds text outside the audited partials.

use anyhow::{Context, Result, bail};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::Path;

/// The partials that every managed `CLAUDE.md` and `AGENTS.md` renders.
pub const PARTIALS: [&str; 3] = ["agent_policy", "prose_policy", "remote_machines"];

/// The tracked audit that classifies every partial line.
pub const AUDIT: &str = "docs/agent-instruction-audit.md";

/// The managed instruction files inside each native profile.
const PROFILE_FILES: [&str; 3] = [
    "dot_claude/CLAUDE.md.tmpl",
    "dot_codex/AGENTS.md.tmpl",
    "dot_config/opencode/AGENTS.md.tmpl",
];

const RETAINED: &str = "## Retained lines";
const FOLLOW_UPS: &str = "## Follow-ups";

/// Instruction lines with their one-based numbers; headings and template directives carry no instruction.
fn instruction_lines(text: &str) -> impl Iterator<Item = (usize, &str)> {
    text.lines().enumerate().filter_map(|(index, line)| {
        let line = line.trim_end();
        (!line.trim().is_empty() && !line.starts_with('#') && !line.trim_start().starts_with("{{"))
            .then_some((index + 1, line))
    })
}

struct Entry<'a> {
    partial: &'a str,
    quote: &'a str,
    line: usize,
    class: Option<&'a str>,
}

fn follow_up_id(class: &str) -> Option<&str> {
    let rest = class.strip_prefix("Follow-up F")?;
    let (digits, text) = rest.split_once(": ")?;
    (!digits.is_empty() && digits.bytes().all(|b| b.is_ascii_digit()) && !text.trim().is_empty())
        .then_some(digits)
}

fn parse(audit: &str) -> (Vec<Entry<'_>>, BTreeSet<&str>) {
    let mut entries: Vec<Entry<'_>> = Vec::new();
    let mut defined = BTreeSet::new();
    let mut section = "";
    let mut partial = "";
    let mut pending = false;
    for (index, line) in audit.lines().enumerate() {
        let line = line.trim_end();
        if line.starts_with("## ") {
            section = line;
            pending = false;
            continue;
        }
        if section == FOLLOW_UPS
            && let Some(id) = line
                .strip_prefix("### F")
                .and_then(|rest| rest.split_once(':'))
                .map(|(id, _)| id)
                .filter(|id| !id.is_empty() && id.bytes().all(|b| b.is_ascii_digit()))
        {
            defined.insert(id);
        }
        if section != RETAINED || line.trim().is_empty() {
            continue;
        }
        if let Some(heading) = line.strip_prefix("### ") {
            partial = heading.trim().trim_matches('`');
            pending = false;
        } else if let Some(quote) = line.strip_prefix("> ") {
            entries.push(Entry {
                partial,
                quote,
                line: index + 1,
                class: None,
            });
            pending = true;
        } else if pending {
            if let Some(entry) = entries.last_mut() {
                entry.class = Some(line);
            }
            pending = false;
        }
    }
    (entries, defined)
}

fn classified(class: Option<&str>, defined: &BTreeSet<&str>) -> bool {
    class.is_some_and(|class| {
        class
            .strip_prefix("Judgment: ")
            .is_some_and(|reason| !reason.trim().is_empty())
            || follow_up_id(class).is_some_and(|id| defined.contains(id))
    })
}

/// Refusals for partial lines the audit does not classify and audit entries that do not match a partial line.
pub fn findings(partials: &[(&str, &str)], audit: &str) -> Vec<String> {
    let (entries, defined) = parse(audit);
    let mut quoted: BTreeMap<(&str, &str), usize> = BTreeMap::new();
    let mut result = Vec::new();
    for entry in &entries {
        let count = quoted.entry((entry.partial, entry.quote)).or_default();
        *count += 1;
        if *count > 1 {
            result.push(format!(
                "{AUDIT}:{}: `{}` is quoted more than once under `{}`; keep one entry, then run `just check`.",
                entry.line, entry.quote, entry.partial
            ));
            continue;
        }
        let present = partials.iter().any(|(path, text)| {
            *path == entry.partial && instruction_lines(text).any(|(_, line)| line == entry.quote)
        });
        if !present {
            result.push(format!(
                "{AUDIT}:{}: the quoted line `{}` is no longer in `{}`; remove the quote or restore the line, then run `just check`.",
                entry.line, entry.quote, entry.partial
            ));
        } else if !classified(entry.class, &defined) {
            result.push(format!(
                "{AUDIT}:{}: `{}` needs a following `Judgment: <why no mechanism can enforce it>` line or a `Follow-up F<n>: <mechanism>` line naming a `### F<n>:` entry under `{FOLLOW_UPS}`; add it, then run `just check`.",
                entry.line, entry.quote
            ));
        }
    }
    for (path, text) in partials {
        for (number, line) in instruction_lines(text) {
            if !quoted.contains_key(&(*path, line)) {
                result.push(format!(
                    "{path}:{number}: `{line}` has no entry in {AUDIT}; quote it under the `{path}` heading in `{RETAINED}` with its classification, or remove the line, then run `just check`."
                ));
            }
        }
    }
    result
}

/// Refusals for a profile instruction template that renders anything besides the audited partials.
pub fn profile_findings(path: &str, text: &str, partials: &[&str]) -> Vec<String> {
    text.lines()
        .enumerate()
        .filter(|(_, line)| !line.trim().is_empty())
        .filter(|(_, line)| {
            !line
                .trim()
                .strip_prefix("{{ includeTemplate \"")
                .and_then(|rest| rest.strip_suffix("\" . }}"))
                .is_some_and(|name| partials.contains(&name))
        })
        .map(|(index, line)| {
            format!(
                "{path}:{}: `{}` is not an include of an audited partial ({}); move the text into one and classify it in {AUDIT}, then run `just check`.",
                index + 1,
                line.trim(),
                partials.join(", ")
            )
        })
        .collect()
}

/// Refuse unless every managed instruction line is classified in the audit.
pub fn check(root: &Path) -> Result<()> {
    let read = |relative: &str| {
        fs::read_to_string(root.join(relative)).with_context(|| format!("read {relative}"))
    };
    let partials = PARTIALS
        .iter()
        .map(|name| Ok((*name, read(&format!(".chezmoitemplates/{name}"))?)))
        .collect::<Result<Vec<_>>>()?;
    let borrowed: Vec<(&str, &str)> = partials
        .iter()
        .map(|(name, text)| (*name, text.as_str()))
        .collect();
    let mut result = findings(&borrowed, &read(AUDIT)?);
    let mut profiles = 0;
    for profile in fs::read_dir(root.join(".chezmoitemplates/profiles"))? {
        let profile = profile?.path();
        for file in PROFILE_FILES {
            let path = profile.join(file);
            if path.is_file() {
                profiles += 1;
                let relative = path
                    .strip_prefix(root)?
                    .to_string_lossy()
                    .replace('\\', "/");
                result.extend(profile_findings(
                    &relative,
                    &fs::read_to_string(&path)?,
                    &PARTIALS,
                ));
            }
        }
    }
    if profiles == 0 {
        result.push(
            "no managed instruction template was found under .chezmoitemplates/profiles; restore the profile templates, then run `just check`."
                .to_owned(),
        );
    }
    if !result.is_empty() {
        bail!("agent instruction audit refused:\n{}", result.join("\n"));
    }
    Ok(())
}
