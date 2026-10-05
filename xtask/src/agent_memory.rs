//! Persistent agent memory stays off in every managed client.
//! A client is managed when a profile renders its instruction file, and that profile must then render the client's own settings with every memory switch disabled.

use crate::profile_rules::{MEMORY_SWITCHES_PROVED, Memory, memory_off};
use anyhow::{Context, Result};
use serde::Deserialize;
use serde_json::Value;

/// Inputs that reach the file OpenCode's prompt tells an agent to keep memories in, at a project root and below it, for each tool that can read or write it.
const OPENCODE_PROBES: [(&str, &str); 6] = [
    ("read", ".github/instructions/memory.instruction.md"),
    ("read", "src/.github/instructions/memory.instruction.md"),
    ("edit", ".github/instructions/memory.instruction.md"),
    ("edit", "src/.github/instructions/memory.instruction.md"),
    ("bash", "cat .github/instructions/memory.instruction.md"),
    (
        "bash",
        "echo x >> src/.github/instructions/memory.instruction.md",
    ),
];

const CODEX_SWITCHES: [&str; 5] = [
    "features.memories",
    "features.external_agent_memory_import",
    "features.chronicle",
    "memories.generate_memories",
    "memories.use_memories",
];

const _: () = assert!(
    CODEX_SWITCHES.len() <= MEMORY_SWITCHES_PROVED
        && OPENCODE_PROBES.len() <= MEMORY_SWITCHES_PROVED
);

/// One OpenCode permission entry: an action for every input, or patterns in the order the file lists them.
#[derive(Deserialize)]
#[serde(untagged)]
enum Rule {
    Action(String),
    Patterns(Ordered),
    Other(serde::de::IgnoredAny),
}

/// A JSON object in file order, because OpenCode applies the last rule that matches.
struct Ordered(Vec<(String, Rule)>);

impl<'de> Deserialize<'de> for Ordered {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct Visitor;
        impl<'de> serde::de::Visitor<'de> for Visitor {
            type Value = Ordered;
            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                formatter.write_str("an object of permission rules")
            }
            fn visit_map<A: serde::de::MapAccess<'de>>(
                self,
                mut map: A,
            ) -> Result<Ordered, A::Error> {
                let mut entries = Vec::new();
                while let Some(entry) = map.next_entry()? {
                    entries.push(entry);
                }
                Ok(Ordered(entries))
            }
        }
        deserializer.deserialize_map(Visitor)
    }
}

#[derive(Deserialize)]
struct OpenCodeConfig {
    #[serde(default)]
    permission: Option<Ordered>,
}

/// OpenCode's wildcard: `*` matches any run of characters, `/` included, and `?` matches one.
fn wildcard(pattern: &str, input: &str) -> bool {
    let (pattern, input): (Vec<char>, Vec<char>) =
        (pattern.chars().collect(), input.chars().collect());
    let (mut p, mut i, mut star, mut mark) = (0, 0, None, 0);
    while i < input.len() {
        if p < pattern.len() && (pattern[p] == '?' || pattern[p] == input[i]) {
            p += 1;
            i += 1;
        } else if p < pattern.len() && pattern[p] == '*' {
            star = Some(p);
            mark = i;
            p += 1;
        } else if let Some(position) = star {
            p = position + 1;
            mark += 1;
            i = mark;
        } else {
            return false;
        }
    }
    pattern[p..].iter().all(|&c| c == '*')
}

/// The action of the last configured rule that matches the tool and its input, as OpenCode evaluates them.
fn opencode_action<'a>(permission: &'a Ordered, tool: &str, input: &str) -> Option<&'a str> {
    let mut action = None;
    for (name, rule) in &permission.0 {
        if !wildcard(name, tool) {
            continue;
        }
        match rule {
            Rule::Action(value) => action = Some(value.as_str()),
            Rule::Other(_) => {}
            Rule::Patterns(patterns) => {
                for (pattern, rule) in &patterns.0 {
                    if let Rule::Action(value) = rule
                        && wildcard(pattern, input)
                    {
                        action = Some(value.as_str());
                    }
                }
            }
        }
    }
    action
}

#[derive(Clone, Copy)]
enum Client {
    Claude,
    Codex,
    OpenCode,
}

impl Client {
    const ALL: [Self; 3] = [Self::Claude, Self::Codex, Self::OpenCode];

    fn instructions(self) -> &'static str {
        match self {
            Self::Claude => ".claude/CLAUDE.md",
            Self::Codex => ".codex/AGENTS.md",
            Self::OpenCode => ".config/opencode/AGENTS.md",
        }
    }

    fn settings(self) -> &'static str {
        match self {
            Self::Claude => ".claude/settings.json",
            Self::Codex => ".codex/config.toml",
            Self::OpenCode => ".config/opencode/opencode.json",
        }
    }

    fn switches(self, contents: &str) -> Result<Vec<(String, Memory)>> {
        Ok(match self {
            Self::Claude => {
                let value: Value = serde_json::from_str(contents)?;
                vec![(
                    "autoMemoryEnabled".into(),
                    flag(value.get("autoMemoryEnabled")),
                )]
            }
            Self::Codex => {
                let value: toml::Table = toml::from_str(contents)?;
                CODEX_SWITCHES
                    .iter()
                    .map(|path| {
                        let (table, key) = path.split_once('.').unwrap_or_default();
                        let setting = value
                            .get(table)
                            .and_then(|table| table.get(key))
                            .and_then(toml::Value::as_bool);
                        ((*path).into(), flag(setting.map(Value::Bool).as_ref()))
                    })
                    .collect()
            }
            Self::OpenCode => {
                let config: OpenCodeConfig = serde_json::from_str(contents)?;
                let none = Ordered(Vec::new());
                let permission = config.permission.as_ref().unwrap_or(&none);
                OPENCODE_PROBES
                    .iter()
                    .map(|(tool, input)| {
                        (
                            format!("permission.{tool}({input:?})"),
                            match opencode_action(permission, tool, input) {
                                Some("deny") => Memory::Disabled,
                                Some(_) => Memory::Enabled,
                                None => Memory::Unset,
                            },
                        )
                    })
                    .collect()
            }
        })
    }
}

fn flag(value: Option<&Value>) -> Memory {
    match value.and_then(Value::as_bool) {
        Some(false) => Memory::Disabled,
        Some(true) => Memory::Enabled,
        None => Memory::Unset,
    }
}

/// The memory switches a rendered profile leaves on or unset, from a `chezmoi dump` of its files.
pub fn violations(dump: &Value) -> Result<Vec<String>> {
    let mut found = Vec::new();
    for client in Client::ALL {
        if dump.get(client.instructions()).is_none() {
            continue;
        }
        let Some(contents) = dump
            .get(client.settings())
            .and_then(|entry| entry["contents"].as_str())
        else {
            found.push(format!(
                "{} is not rendered, so the client keeps its default memory",
                client.settings()
            ));
            continue;
        };
        let switches = client
            .switches(contents)
            .with_context(|| format!("parse rendered {}", client.settings()))?;
        let states: Vec<_> = switches.iter().map(|(_, state)| *state).collect();
        if !memory_off(&states) {
            found.extend(
                switches
                    .into_iter()
                    .filter(|(_, state)| *state != Memory::Disabled)
                    .map(|(name, state)| format!("{}: {name} is {state:?}", client.settings())),
            );
        }
    }
    Ok(found)
}
