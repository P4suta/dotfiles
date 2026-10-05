//! Every line of the managed global agent instructions is classified in a tracked audit.
//!
//! A retained line is either judgment that no mechanism can enforce or a named follow-up that will mechanize it.
//! The check refuses an unclassified line, a stale entry, a removed line without a named holder, and an instruction file that renders text outside the audited partials.

use crate::instruction_rules::{
    Class, Definition, Holder, Line, admitted, follow_up, removed_row_held,
};
use anyhow::{Context, Result, bail};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::Path;

/// The partials that every managed `CLAUDE.md` and `AGENTS.md` renders.
pub const PARTIALS: [&str; 3] = ["agent_policy", "prose_policy", "remote_machines"];

/// The tracked audit that classifies every partial line.
pub const AUDIT: &str = "docs/agent-instruction-audit.md";

/// The managed instruction files, each a root dispatcher over the same path inside every native profile.
pub const PROFILE_FILES: [&str; 3] = [
    "dot_claude/CLAUDE.md.tmpl",
    "dot_codex/AGENTS.md.tmpl",
    "dot_config/opencode/AGENTS.md.tmpl",
];

/// File names that agent clients load as instructions; any other location holding one is unaudited.
const INSTRUCTION_NAMES: [&str; 4] = [
    "CLAUDE.md",
    "AGENTS.md",
    "GEMINI.md",
    "copilot-instructions.md",
];

const PROFILES: &str = ".chezmoitemplates/profiles";
const RETAINED: &str = "## Retained lines";
const REMOVED: &str = "## Removed lines";
const FOLLOW_UPS: &str = "## Follow-ups";

fn shape(line: &str) -> Line {
    let start = line.trim_start();
    if start.starts_with('#') {
        Line::Heading
    } else if start.starts_with("{{") {
        Line::Directive
    } else {
        Line::Text
    }
}

/// Every non-blank line with its one-based number.
fn instruction_lines(text: &str) -> impl Iterator<Item = (usize, &str)> {
    text.lines().enumerate().filter_map(|(index, line)| {
        let line = line.trim_end();
        (!line.trim().is_empty()).then_some((index + 1, line))
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

fn class_of(class: Option<&str>) -> Class {
    let Some(class) = class else {
        return Class::Missing;
    };
    let reasoned = |prefix: &str| {
        class
            .strip_prefix(prefix)
            .is_some_and(|reason| !reason.trim().is_empty())
    };
    if reasoned("Judgment: ") {
        Class::Judgment
    } else if follow_up_id(class).is_some() {
        Class::FollowUp
    } else if reasoned("Heading: ") {
        Class::Heading
    } else if reasoned("Directive: ") {
        Class::Directive
    } else {
        Class::Missing
    }
}

struct Row<'a> {
    line: usize,
    cells: Vec<&'a str>,
}

struct Audit<'a> {
    entries: Vec<Entry<'a>>,
    definitions: BTreeMap<&'a str, Vec<usize>>,
    rows: Vec<Row<'a>>,
}

fn parse(audit: &str) -> Audit<'_> {
    let mut parsed = Audit {
        entries: Vec::new(),
        definitions: BTreeMap::new(),
        rows: Vec::new(),
    };
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
            parsed.definitions.entry(id).or_default().push(index + 1);
        }
        if section == REMOVED && line.starts_with('|') {
            let cells: Vec<&str> = line.trim_matches('|').split('|').map(str::trim).collect();
            if !cells.iter().all(|cell| cell.chars().all(|c| c == '-')) {
                parsed.rows.push(Row {
                    line: index + 1,
                    cells,
                });
            }
        }
        if section != RETAINED || line.trim().is_empty() {
            continue;
        }
        if let Some(heading) = line.strip_prefix("### ") {
            partial = heading.trim().trim_matches('`');
            pending = false;
        } else if let Some(quote) = line.strip_prefix("> ") {
            parsed.entries.push(Entry {
                partial,
                quote,
                line: index + 1,
                class: None,
            });
            pending = true;
        } else if pending {
            if let Some(entry) = parsed.entries.last_mut() {
                entry.class = Some(line);
            }
            pending = false;
        }
    }
    // The table header row names the columns.
    if !parsed.rows.is_empty() {
        parsed.rows.remove(0);
    }
    parsed
}

fn code_spans(text: &str) -> impl Iterator<Item = &str> {
    text.split('`').skip(1).step_by(2)
}

fn is_path(span: &str) -> bool {
    span.contains('/')
        && !span.starts_with(['~', '/', '-'])
        && !span.contains(|c: char| c.is_whitespace() || "<>*{}:\"".contains(c))
}

fn holder(cell: &str) -> Option<Holder> {
    match cell {
        "Gate" => Some(Holder::Gate),
        "Skill" => Some(Holder::Skill),
        "Gate and skill" => Some(Holder::GateAndSkill),
        "Merged" => Some(Holder::Merged),
        _ => None,
    }
}

/// Refusals for partial lines the audit does not classify, audit entries that no longer match, and removed lines without a named holder.
///
/// `exists` answers whether a repository-relative path exists, and `skills` lists the shared skill names.
pub fn findings(
    partials: &[(&str, &str)],
    audit: &str,
    exists: &dyn Fn(&str) -> bool,
    skills: &[&str],
) -> Vec<String> {
    let parsed = parse(audit);
    let mut references: BTreeMap<&str, usize> = BTreeMap::new();
    for entry in &parsed.entries {
        if let Some(id) = entry.class.and_then(follow_up_id) {
            *references.entry(id).or_default() += 1;
        }
    }
    let mut quoted: BTreeMap<(&str, &str), usize> = BTreeMap::new();
    let mut result = Vec::new();
    for entry in &parsed.entries {
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
        let defined = entry
            .class
            .and_then(follow_up_id)
            .is_some_and(|id| parsed.definitions.contains_key(id));
        if !present {
            result.push(format!(
                "{AUDIT}:{}: the quoted line `{}` is no longer in `{}`; remove the quote or restore the line, then run `just check`.",
                entry.line, entry.quote, entry.partial
            ));
        } else if !admitted(shape(entry.quote), class_of(entry.class), defined) {
            let expected = match shape(entry.quote) {
                Line::Text => format!(
                    "a following `Judgment: <why no mechanism can enforce it>` line or a `Follow-up F<n>: <what exists now>` line naming a `### F<n>:` entry under `{FOLLOW_UPS}`"
                ),
                Line::Heading => "a following `Heading: <what it groups>` line".to_owned(),
                Line::Directive => "a following `Directive: <what it renders>` line".to_owned(),
            };
            result.push(format!(
                "{AUDIT}:{}: `{}` needs {expected}; add it, then run `just check`.",
                entry.line, entry.quote
            ));
        }
    }
    for (id, lines) in &parsed.definitions {
        let referenced = references.get(id).copied().unwrap_or_default();
        match follow_up(lines.len(), referenced) {
            Definition::Duplicate => result.push(format!(
                "{AUDIT}:{}: `### F{id}:` is defined more than once; keep one definition, then run `just check`.",
                lines[1]
            )),
            Definition::Unreferenced => result.push(format!(
                "{AUDIT}:{}: `### F{id}:` is referenced by no retained line; remove it or classify the line it mechanizes as `Follow-up F{id}:`, then run `just check`.",
                lines[0]
            )),
            Definition::Current | Definition::Undefined => {}
        }
    }
    for row in &parsed.rows {
        let [_, held_by, mechanism] = row.cells[..] else {
            result.push(format!(
                "{AUDIT}:{}: a removed-line row needs three cells (former line, held by, mechanism); fix the row, then run `just check`.",
                row.line
            ));
            continue;
        };
        let names_source = code_spans(mechanism).any(|span| is_path(span) && exists(span));
        let names_skill = code_spans(mechanism).any(|span| skills.contains(&span));
        let names_retained = mechanism.contains("retained");
        if !removed_row_held(holder(held_by), names_source, names_skill, names_retained) {
            let need = match holder(held_by) {
                None => "a Held by cell of `Gate`, `Skill`, `Gate and skill`, or `Merged`",
                Some(Holder::Gate) => "a mechanism cell naming the gate's source file",
                Some(Holder::Skill) => "a mechanism cell naming an existing skill",
                Some(Holder::GateAndSkill) => {
                    "a mechanism cell naming the gate's source file and an existing skill"
                }
                Some(Holder::Merged) => "a mechanism cell naming the retained line it merged into",
            };
            result.push(format!(
                "{AUDIT}:{}: the removed line `{}` needs {need}; correct the row, then run `just check`.",
                row.line, row.cells[0]
            ));
        }
    }
    for (index, line) in audit.lines().enumerate() {
        for span in code_spans(line).filter(|span| is_path(span) && !exists(span)) {
            result.push(format!(
                "{AUDIT}:{}: `{span}` does not exist; name the current path, then run `just check`.",
                index + 1
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

/// A dispatcher action that renders nothing itself or selects its own file in one profile.
fn dispatcher_action(action: &str, file: &str) -> bool {
    let action = action.trim_matches('-').trim();
    let selects_profile = action
        .strip_prefix("includeTemplate \"profiles/")
        .and_then(|rest| rest.split_once('"'))
        .and_then(|(target, _)| target.split_once('/'))
        .is_some_and(|(profile, target)| {
            !profile.is_empty() && !profile.contains('/') && target == file
        });
    selects_profile
        || (action.starts_with('$') && action.contains(":="))
        || action.starts_with("if ")
        || action.starts_with("else if ")
        || action == "else"
        || action == "end"
        || action.starts_with("fail \"")
}

/// Refusals for a root dispatcher that renders text or includes anything besides its own file in a native profile.
pub fn dispatcher_findings(path: &str, text: &str) -> Vec<String> {
    let mut result = Vec::new();
    'lines: for (index, line) in text.lines().enumerate() {
        let mut refuse = |what: &str| {
            result.push(format!(
                "{path}:{}: {what}; a dispatcher only selects `profiles/<profile>/{path}`, so move the text into an audited partial, then run `just check`.",
                index + 1
            ));
        };
        let mut rest = line;
        while let Some(start) = rest.find("{{") {
            if !rest[..start].trim().is_empty() {
                refuse(&format!("`{}` renders text", rest[..start].trim()));
                continue 'lines;
            }
            let after = &rest[start + 2..];
            let Some(end) = after.find("}}") else {
                refuse("an action is not closed");
                continue 'lines;
            };
            if !dispatcher_action(&after[..end], path) {
                refuse(&format!(
                    "`{}` is not a profile selection",
                    after[..end].trim()
                ));
                continue 'lines;
            }
            rest = &after[end + 2..];
        }
        if !rest.trim().is_empty() {
            refuse(&format!("`{}` renders text", rest.trim()));
        }
    }
    result
}

fn relative(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}

fn read(root: &Path, relative: &str) -> Result<String> {
    fs::read_to_string(root.join(relative)).with_context(|| {
        format!("{relative}: cannot be read; restore the file, then run `just check`")
    })
}

/// Instruction files anywhere in the source tree, by the names agent clients load.
fn instruction_files(root: &Path) -> Result<Vec<String>> {
    let mut found = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(directory) = stack.pop() {
        let entries = fs::read_dir(&directory).with_context(|| {
            format!(
                "{}: cannot be listed; restore read access, then run `just check`",
                relative(root, &directory)
            )
        })?;
        for entry in entries {
            let path = entry
                .with_context(|| {
                    format!(
                        "{}: an entry cannot be read; restore read access, then run `just check`",
                        relative(root, &directory)
                    )
                })?
                .path();
            let name = path
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or_default();
            if path.is_dir() {
                if !matches!(name, ".git" | "target" | "node_modules") {
                    stack.push(path);
                }
            } else if INSTRUCTION_NAMES
                .iter()
                .any(|known| name.strip_suffix(".tmpl").unwrap_or(name).ends_with(known))
            {
                found.push(relative(root, &path));
            }
        }
    }
    found.sort();
    Ok(found)
}

fn profile_file(path: &str) -> bool {
    path.strip_prefix(PROFILES)
        .and_then(|rest| rest.strip_prefix('/'))
        .and_then(|rest| rest.split_once('/'))
        .is_some_and(|(profile, file)| !profile.is_empty() && PROFILE_FILES.contains(&file))
}

fn shared_skills(root: &Path) -> Result<Vec<String>> {
    let directory = root.join("dot_agents/skills");
    let mut names = BTreeSet::new();
    for entry in fs::read_dir(&directory).with_context(|| {
        "dot_agents/skills: cannot be listed; restore the shared skill tree, then run `just check`"
    })? {
        let entry = entry.context(
            "dot_agents/skills: an entry cannot be read; restore read access, then run `just check`",
        )?;
        if entry.path().is_dir()
            && let Ok(name) = entry.file_name().into_string()
        {
            names.insert(name);
        }
    }
    Ok(names.into_iter().collect())
}

/// Refuse unless every managed instruction line is classified in the audit.
pub fn check(root: &Path) -> Result<()> {
    let partials = PARTIALS
        .iter()
        .map(|name| Ok((*name, read(root, &format!(".chezmoitemplates/{name}"))?)))
        .collect::<Result<Vec<_>>>()?;
    let borrowed: Vec<(&str, &str)> = partials
        .iter()
        .map(|(name, text)| (*name, text.as_str()))
        .collect();
    let skills = shared_skills(root)?;
    let skills: Vec<&str> = skills.iter().map(String::as_str).collect();
    let exists = |path: &str| root.join(path).exists();
    let mut result = findings(&borrowed, &read(root, AUDIT)?, &exists, &skills);
    let mut profiles = 0;
    for path in instruction_files(root)? {
        if PROFILE_FILES.contains(&path.as_str()) {
            result.extend(dispatcher_findings(&path, &read(root, &path)?));
        } else if profile_file(&path) {
            profiles += 1;
            result.extend(profile_findings(&path, &read(root, &path)?, &PARTIALS));
        } else {
            result.push(format!(
                "{path}: an agent instruction file outside the audit; render it through a dispatcher from `{PROFILES}/<profile>/` and add it to PROFILE_FILES in xtask/src/instruction_audit.rs, or remove it, then run `just check`."
            ));
        }
    }
    if profiles == 0 {
        result.push(format!(
            "{PROFILES}: no managed instruction template ({}) was found in any profile; restore the profile templates, then run `just check`.",
            PROFILE_FILES.join(", ")
        ));
    }
    if !result.is_empty() {
        bail!("agent instruction audit refused:\n{}", result.join("\n"));
    }
    Ok(())
}
