//! A tracked audit classifies every line of the managed global agent instructions and agent prompt files.
//!
//! A retained line holds judgment that no mechanism can enforce, or it names the follow-up that mechanizes it.
//! The check refuses an unclassified line, a stale entry, a removed line without a named holder, and an instruction file that renders text outside the audited sources.

use crate::instruction_rules::{
    Class, Definition, Holder, Line, admitted, follow_up, removed_row_held,
};
use anyhow::{Context, Result, bail};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

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

/// File names that agent clients load as instructions.
const INSTRUCTION_NAMES: [&str; 6] = [
    "CLAUDE.md",
    "CLAUDE.local.md",
    "AGENTS.md",
    "AGENTS.override.md",
    "GEMINI.md",
    "copilot-instructions.md",
];

/// Target directories whose Markdown files agent clients load as rules, agents, or commands.
const PROMPT_DIRECTORIES: [[&str; 2]; 8] = [
    [".claude", "rules"],
    [".claude", "agents"],
    [".claude", "commands"],
    [".codex", "prompts"],
    ["opencode", "agent"],
    ["opencode", "agents"],
    ["opencode", "command"],
    ["opencode", "commands"],
];

/// chezmoi source attributes that precede a target name.
const ATTRIBUTES: [&str; 17] = [
    "after_",
    "before_",
    "create_",
    "empty_",
    "encrypted_",
    "exact_",
    "executable_",
    "external_",
    "literal_",
    "modify_",
    "once_",
    "onchange_",
    "private_",
    "readonly_",
    "remove_",
    "run_",
    "symlink_",
];

/// Extensions of files that can hold a gate.
const GATE_SOURCES: [&str; 7] = [".rs", ".ts", ".js", ".json", ".toml", ".yml", ".yaml"];

const PROFILES: &str = ".chezmoitemplates/profiles";
const RETAINED: &str = "## Retained lines";
const REMOVED: &str = "## Removed lines";
const FOLLOW_UPS: &str = "## Follow-ups";

/// A template action that renders nothing: an assignment, a condition, or a comment.
fn control(action: &str) -> bool {
    let action = action.trim_matches('-').trim();
    let assignment = action.strip_prefix('$').is_some_and(|rest| {
        rest.split_once('=').is_some_and(|(name, _)| {
            let name = name.trim_end_matches(':').trim();
            !name.is_empty() && name.chars().all(char::is_alphanumeric)
        })
    });
    assignment
        || action.starts_with("if ")
        || action.starts_with("else if ")
        || action == "else"
        || action == "end"
        || (action.starts_with("/*") && action.ends_with("*/"))
}

fn shape(line: &str) -> Line {
    let line = line.trim();
    if line.contains("{{") || line.contains("}}") {
        let sole = line
            .strip_prefix("{{")
            .and_then(|rest| rest.strip_suffix("}}"))
            .filter(|action| !action.contains("{{") && !action.contains("}}"));
        if sole.is_some_and(control) {
            Line::Directive
        } else {
            Line::Output
        }
    } else if line.starts_with('#') {
        Line::Heading
    } else {
        Line::Text
    }
}

/// A source line with its one-based number and shape.
type Numbered<'a> = (usize, &'a str, Line);

/// Every non-blank line with its one-based number and shape.
/// A leading `---` block counts as front matter.
fn instruction_lines(text: &str) -> Vec<Numbered<'_>> {
    let mut front_matter = text.lines().next().map(str::trim_end) == Some("---");
    let mut result = Vec::new();
    for (index, line) in text.lines().enumerate() {
        let line = line.trim_end();
        let shape = if front_matter {
            if index > 0 && line == "---" {
                front_matter = false;
            }
            Line::Setting
        } else {
            shape(line)
        };
        if !line.trim().is_empty() {
            result.push((index + 1, line, shape));
        }
    }
    result
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
    } else if reasoned("Setting: ") {
        Class::Setting
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

fn gate_source(span: &str, exists: &dyn Fn(&str) -> bool) -> bool {
    is_path(span) && exists(span) && GATE_SOURCES.iter().any(|end| span.ends_with(end))
}

fn holder(cell: &str) -> Option<Holder> {
    match cell {
        "Gate" => Some(Holder::Gate),
        "Skill" => Some(Holder::Skill),
        "Gate and skill" => Some(Holder::GateAndSkill),
        "Merged" => Some(Holder::Merged),
        "Contradicted" => Some(Holder::Contradicted),
        _ => None,
    }
}

/// Refusals for source lines the audit doesn't classify, audit entries that no longer match, and removed lines without a named holder.
///
/// `sources` pairs each audited partial name or prompt path with its text.
/// `exists` answers whether a repository-relative path exists, and `skills` lists the shared skill names.
pub fn findings(
    sources: &[(&str, &str)],
    audit: &str,
    exists: &dyn Fn(&str) -> bool,
    skills: &[&str],
) -> Vec<String> {
    let parsed = parse(audit);
    let lines: Vec<(&str, Vec<Numbered<'_>>)> = sources
        .iter()
        .map(|(path, text)| (*path, instruction_lines(text)))
        .collect();
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
        let shapes: BTreeSet<Line> = lines
            .iter()
            .filter(|(path, _)| *path == entry.partial)
            .flat_map(|(_, lines)| lines.iter())
            .filter(|(_, line, _)| *line == entry.quote)
            .map(|(_, _, shape)| *shape)
            .collect();
        if shapes.is_empty() {
            result.push(format!(
                "{AUDIT}:{}: the quoted line `{}` is no longer in `{}`; remove the quote or restore the line, then run `just check`.",
                entry.line, entry.quote, entry.partial
            ));
            continue;
        }
        let defined = entry
            .class
            .and_then(follow_up_id)
            .is_some_and(|id| parsed.definitions.contains_key(id));
        let Some(shape) = shapes
            .iter()
            .copied()
            .find(|shape| !admitted(*shape, class_of(entry.class), defined))
        else {
            continue;
        };
        let expected = match shape {
            Line::Text => format!(
                "needs a following `Judgment: <why no mechanism can enforce it>` line or a `Follow-up F<n>: <what exists now>` line naming a `### F<n>:` entry under `{FOLLOW_UPS}`; add it"
            ),
            Line::Heading => "needs a following `Heading: <what it groups>` line; add it".to_owned(),
            Line::Directive => {
                "needs a following `Directive: <what it decides>` line; add it".to_owned()
            }
            Line::Setting => {
                "is front matter and needs a following `Setting: <what it configures>` line; add it"
                    .to_owned()
            }
            Line::Output => "renders template output that the audit cannot read; write the text as plain lines or remove the action".to_owned(),
        };
        result.push(format!(
            "{AUDIT}:{}: `{}` {expected}, then run `just check`.",
            entry.line, entry.quote
        ));
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
        let names_source = code_spans(mechanism).any(|span| gate_source(span, exists));
        let names_skill = code_spans(mechanism).any(|span| skills.contains(&span));
        let names_retained = code_spans(mechanism).any(|span| {
            span.contains(' ')
                && lines
                    .iter()
                    .flat_map(|(_, lines)| lines.iter())
                    .filter(|(_, line, shape)| *shape == Line::Text && line.contains(span))
                    .count()
                    == 1
        });
        if !removed_row_held(holder(held_by), names_source, names_skill, names_retained) {
            let need = match holder(held_by) {
                None => {
                    "a Held by cell of `Gate`, `Skill`, `Gate and skill`, `Merged`, or `Contradicted`"
                }
                Some(Holder::Gate) => {
                    "a mechanism cell naming the gate's source file (`.rs`, `.ts`, `.js`, `.json`, `.toml`, `.yml`, or `.yaml`)"
                }
                Some(Holder::Skill) => "a mechanism cell naming an existing skill",
                Some(Holder::GateAndSkill) => {
                    "a mechanism cell naming the gate's source file and an existing skill"
                }
                Some(Holder::Merged) => {
                    "a mechanism cell quoting, in a code span with a space, a fragment of exactly one retained line"
                }
                Some(Holder::Contradicted) => {
                    "a mechanism cell naming what it contradicted: an existing skill, or, in a code span with a space, a fragment of exactly one retained line"
                }
            };
            result.push(format!(
                "{AUDIT}:{}: the removed line `{}` needs {need}; correct the row, then run `just check`.",
                row.line, row.cells[0]
            ));
        }
    }
    // A quote reproduces a source line, so its code spans make no claim about this repository.
    for (index, line) in audit
        .lines()
        .enumerate()
        .filter(|(_, line)| !line.starts_with("> "))
    {
        for span in code_spans(line).filter(|span| is_path(span) && !exists(span)) {
            result.push(format!(
                "{AUDIT}:{}: `{span}` does not exist; name the current path, then run `just check`.",
                index + 1
            ));
        }
    }
    for (path, lines) in &lines {
        for (number, line, _) in lines {
            if !quoted.contains_key(&(*path, *line)) {
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

/// A dispatcher action that renders nothing itself or renders exactly its own file in one profile.
fn dispatcher_action(action: &str, file: &str) -> bool {
    let action = action.trim_matches('-').trim();
    let selects_profile = action
        .strip_prefix("includeTemplate \"profiles/")
        .and_then(|rest| rest.split_once('"'))
        .and_then(|(target, argument)| Some((target.split_once('/')?, argument.trim())))
        .is_some_and(|((profile, target), argument)| {
            let variable = argument
                .strip_prefix('$')
                .is_some_and(|name| !name.is_empty() && name.chars().all(char::is_alphanumeric));
            !profile.is_empty()
                && !profile.contains('/')
                && target == file
                && (argument == "." || variable)
        });
    selects_profile
        || control(action)
        || action
            .strip_prefix("fail \"")
            .and_then(|rest| rest.strip_suffix('"'))
            .is_some_and(|message| !message.contains('"'))
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

/// The entries of a source directory, with one refusal for any listing error.
fn listing(root: &Path, directory: &Path) -> Result<Vec<PathBuf>> {
    fs::read_dir(directory)
        .and_then(|entries| entries.map(|entry| entry.map(|entry| entry.path())).collect())
        .with_context(|| {
            format!(
                "{}: cannot be listed; restore the directory and its read access, then run `just check`",
                relative(root, directory)
            )
        })
}

/// The target path segments of a chezmoi source path.
fn target(path: &str) -> Vec<String> {
    path.split('/')
        .map(|segment| {
            let mut name = segment;
            while let Some(rest) = ATTRIBUTES
                .iter()
                .find_map(|prefix| name.strip_prefix(prefix))
            {
                name = rest;
            }
            let name = name.strip_suffix(".tmpl").unwrap_or(name);
            name.strip_prefix("dot_")
                .map_or_else(|| name.to_owned(), |rest| format!(".{rest}"))
        })
        .collect()
}

/// How an agent client loads a source file.
#[derive(Clone, Copy, PartialEq)]
enum Loaded {
    Instructions,
    Prompt,
}

fn loaded(path: &str) -> Option<Loaded> {
    let segments = target(path);
    let name = segments.last()?;
    if INSTRUCTION_NAMES.contains(&name.as_str()) {
        return Some(Loaded::Instructions);
    }
    let prompt = name.ends_with(".md")
        && segments.windows(3).any(|window| {
            PROMPT_DIRECTORIES
                .iter()
                .any(|[parent, directory]| window[0] == *parent && window[1] == *directory)
        });
    prompt.then_some(Loaded::Prompt)
}

/// Files anywhere in the source tree that agent clients load as instructions or prompts.
fn loaded_files(root: &Path) -> Result<Vec<(String, Loaded)>> {
    let mut found = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(directory) = stack.pop() {
        for path in listing(root, &directory)? {
            let name = path
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or_default();
            if path.is_dir() {
                if !matches!(name, ".git" | "target" | "node_modules") {
                    stack.push(path);
                }
            } else {
                let path = relative(root, &path);
                if let Some(kind) = loaded(&path) {
                    found.push((path, kind));
                }
            }
        }
    }
    found.sort_by(|left, right| left.0.cmp(&right.0));
    Ok(found)
}

fn profile_file(path: &str) -> bool {
    path.strip_prefix(PROFILES)
        .and_then(|rest| rest.strip_prefix('/'))
        .and_then(|rest| rest.split_once('/'))
        .is_some_and(|(profile, file)| !profile.is_empty() && PROFILE_FILES.contains(&file))
}

fn shared_skills(root: &Path) -> Result<Vec<String>> {
    let names: BTreeSet<String> = listing(root, &root.join("dot_agents/skills"))?
        .into_iter()
        .filter(|path| path.is_dir())
        .filter_map(|path| path.file_name()?.to_str().map(str::to_owned))
        .collect();
    Ok(names.into_iter().collect())
}

/// Refuses unless the audit classifies every managed instruction and prompt line.
pub fn check(root: &Path) -> Result<()> {
    let files = loaded_files(root)?;
    let mut sources = PARTIALS
        .iter()
        .map(|name| {
            Ok((
                (*name).to_owned(),
                read(root, &format!(".chezmoitemplates/{name}"))?,
            ))
        })
        .collect::<Result<Vec<_>>>()?;
    for (path, kind) in &files {
        if *kind == Loaded::Prompt {
            sources.push((path.clone(), read(root, path)?));
        }
    }
    let borrowed: Vec<(&str, &str)> = sources
        .iter()
        .map(|(name, text)| (name.as_str(), text.as_str()))
        .collect();
    let skills = shared_skills(root)?;
    let skills: Vec<&str> = skills.iter().map(String::as_str).collect();
    let exists = |path: &str| root.join(path).exists();
    let mut result = findings(&borrowed, &read(root, AUDIT)?, &exists, &skills);
    let mut profiles = 0;
    for (path, kind) in &files {
        if *kind == Loaded::Prompt {
            continue;
        }
        if PROFILE_FILES.contains(&path.as_str()) {
            result.extend(dispatcher_findings(path, &read(root, path)?));
        } else if profile_file(path) {
            profiles += 1;
            result.extend(profile_findings(path, &read(root, path)?, &PARTIALS));
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

#[cfg(test)]
mod tests {
    use super::listing;

    #[test]
    fn an_unlistable_directory_names_itself_and_the_next_action() {
        let root = tempfile::tempdir().expect("temporary repository");
        let message = format!(
            "{:#}",
            listing(root.path(), &root.path().join("dot_agents/skills"))
                .expect_err("a missing directory cannot be listed")
        );
        assert!(
            message.starts_with("dot_agents/skills: cannot be listed"),
            "{message}"
        );
        assert!(message.contains("just check"), "{message}");
    }
}
