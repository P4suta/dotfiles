//! The tools each profile installs, computed from the install sources, and the gate that holds every one to a version-bound skill or a no-skill decision.

use crate::tool_rules::{Coverage, Placement, coverage, placement};
use anyhow::{Context, Result, anyhow, ensure};
use model::Profile;
use model::data::{Data, Tools};
use platform::step::Step;
use serde::Deserialize;
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::Path;

/// The decisions this gate reads, relative to the checkout.
pub const POLICY: &str = "policy/tools.toml";

/// The Homebrew bundle, whose literal lines install tools beside the data-driven ones.
const BREWFILE: &str = "home/dot_config/homebrew/Brewfile.tmpl";

/// The shared skills, relative to the checkout.
const SKILLS: &str = "home/dot_agents/skills";
const TOOL_BINDINGS: &str = "tools.toml";

/// The profile sets and targets that restrict where a skill installs.
const TARGETS: &str = "home/.chezmoidata/targets.toml";
const PROFILE_SETS: &str = "home/.chezmoidata/profiles.toml";

/// The version the inventory records for a tool that no source pins.
pub const UNPINNED: &str = "latest";

/// Where a tool installs and at which versions.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Installed {
    pub profiles: BTreeSet<&'static str>,
    pub versions: BTreeSet<String>,
}

/// Every tool an install source places on a profile, by canonical name.
pub type Inventory = BTreeMap<String, Installed>;

/// Why a tool needs no skill, from a closed set.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub enum Reason {
    /// A single-purpose command whose `--help` covers every use, with no hazard that needs judgment.
    SelfDescribing,
    /// A language toolchain or package manager, which the project's manifest and the language's own documentation govern.
    Toolchain,
    /// A build dependency that no agent runs.
    Library,
    /// An interactive program for the owner, which agents never drive.
    Desktop,
    /// A preferred tool whose entry in the modern tool table holds the judgment: when to use it, the legacy cases, and the next action.
    ModernTable,
    /// A tool whose destructive use the Git guard or the agent hooks already refuse, and whose other uses its `--help` covers.
    Guarded,
}

/// One no-skill decision for a group of tools.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NoSkill {
    pub reason: Reason,
    pub why: String,
    pub tools: Vec<String>,
}

/// The decisions of the gate.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Policy {
    /// What each setup step installs, by step name, so profile scripts count as install sources.
    pub steps: BTreeMap<String, Vec<String>>,
    /// Tools that exist on only some platforms, with the profiles that may install them.
    #[serde(default)]
    pub platform_specific: BTreeMap<String, Vec<String>>,
    #[serde(default)]
    pub none: Vec<NoSkill>,
}

/// Reads the policy at `root`.
///
/// # Errors
///
/// Returns an error when reading or parsing the policy fails.
pub fn policy(root: &Path) -> Result<Policy> {
    toml::from_str(
        &fs::read_to_string(root.join(POLICY)).with_context(|| format!("read {POLICY}"))?,
    )
    .with_context(|| format!("{POLICY} is invalid"))
}

/// The canonical inventory name: `aqua:dalance/procs` names `procs`, and the renamed `github:pvolok/dekit` keeps the `mprocs` identity.
#[must_use]
pub fn name(spec: &str) -> String {
    let spec = spec.split_once(':').map_or(spec, |(_, rest)| rest);
    let spec = spec.rsplit('/').next().unwrap_or(spec);
    let name = spec.split('@').next().unwrap_or(spec).to_ascii_lowercase();
    match name.as_str() {
        "dekit" => "mprocs".to_owned(),
        "rg" => "ripgrep".to_owned(),
        "nu" => "nushell".to_owned(),
        _ => name,
    }
}

fn add(inventory: &mut Inventory, profiles: &[Profile], spec: &str, version: Option<&str>) {
    if spec.is_empty() {
        return;
    }
    let entry = inventory.entry(name(spec)).or_default();
    entry
        .profiles
        .extend(profiles.iter().map(|profile| profile.name()));
    entry
        .versions
        .insert(version.unwrap_or(UNPINNED).to_owned());
}

fn listed(tools: Option<&Tools>) -> Vec<String> {
    tools
        .into_iter()
        .flat_map(|tools| [&tools.common, &tools.personal, &tools.work])
        .flatten()
        .flatten()
        .cloned()
        .collect()
}

/// The `brew "NAME"` and `cask "NAME"` lines of the Homebrew bundle that no template renders.
fn brewfile(root: &Path) -> Result<Vec<String>> {
    let text =
        fs::read_to_string(root.join(BREWFILE)).with_context(|| format!("read {BREWFILE}"))?;
    Ok(text
        .lines()
        .filter(|line| !line.contains("{{"))
        .filter_map(|line| {
            line.strip_prefix("brew ")
                .or_else(|| line.strip_prefix("cask "))
                .map(|name| name.trim().trim_matches('"').to_owned())
        })
        .collect())
}

/// The inventory of the checkout at `root`: mise, Homebrew, Nix, Scoop, and `WinGet` per profile, the modern tool table, the mise release, and what the setup steps install.
///
/// # Errors
///
/// Returns an error when a data file or the Homebrew bundle fails to read, or the policy omits or invents a setup step.
pub fn inventory(root: &Path, policy: &Policy) -> Result<Inventory> {
    let data: Data = serde_json::from_value(
        model::data::load(&root.join("home").join(model::data::DIRECTORY))
            .map_err(|error| anyhow!(error))?,
    )?;
    let mut inventory = Inventory::new();
    let platforms = data.platforms.unwrap_or_default();
    let every = Profile::ALL;
    development(root, &mut inventory)?;
    if let Some(mac) = &platforms.mac {
        let versions = mac.tool_versions.clone().unwrap_or_default();
        for spec in listed(mac.tools.as_ref()) {
            add(
                &mut inventory,
                &[Profile::Mac],
                &spec,
                versions.get(&spec).map(String::as_str),
            );
        }
        let brew = mac.brew.clone().unwrap_or_default();
        for spec in brew
            .formulae
            .unwrap_or_default()
            .into_iter()
            .chain(brew.casks.unwrap_or_default())
            .chain(brewfile(root)?)
        {
            add(&mut inventory, &[Profile::Mac], &spec, None);
        }
    }
    if let Some(linux) = &platforms.linux {
        let versions = linux.tool_versions.clone().unwrap_or_default();
        for spec in listed(linux.tools.as_ref()) {
            add(
                &mut inventory,
                &[Profile::Linux],
                &spec,
                versions.get(&spec).map(String::as_str),
            );
        }
        for (package, fallback) in linux
            .nix
            .clone()
            .unwrap_or_default()
            .packages
            .unwrap_or_default()
        {
            add(&mut inventory, &[Profile::Linux], &package, None);
            add(&mut inventory, &[Profile::Linux], &fallback, None);
        }
        if let Some(coderabbit) = &data.coderabbit {
            add(
                &mut inventory,
                &[Profile::Linux],
                "coderabbit",
                coderabbit.version.as_deref(),
            );
        }
    }
    if let Some(windows) = &platforms.windows {
        for spec in listed(windows.tools.as_ref()) {
            add(&mut inventory, &[Profile::Windows], &spec, None);
        }
        for spec in windows
            .scoop
            .clone()
            .unwrap_or_default()
            .apps
            .unwrap_or_default()
            .into_iter()
            .chain(
                windows
                    .winget
                    .clone()
                    .unwrap_or_default()
                    .apps
                    .unwrap_or_default(),
            )
        {
            add(&mut inventory, &[Profile::Windows], &spec, None);
        }
    }
    if let Some(wsl) = &platforms.wsl {
        for spec in listed(wsl.tools.as_ref()) {
            add(&mut inventory, &[Profile::Wsl], &spec, None);
        }
    }
    if let Some(modern) = &data.modern {
        for tool in &modern.tools {
            add(&mut inventory, &every, &tool.tool, Some(&tool.version));
        }
    }
    if let Some(mise) = &data.mise {
        add(&mut inventory, &every, "mise", mise.version.as_deref());
    }
    steps(&mut inventory, policy)?;
    Ok(inventory)
}

fn development(root: &Path, inventory: &mut Inventory) -> Result<()> {
    let development: toml::Table = toml::from_str(&fs::read_to_string(root.join("mise.toml"))?)?;
    let tools = development
        .get("tools")
        .and_then(toml::Value::as_table)
        .context("mise.toml has no tools")?;
    for (spec, value) in tools {
        let version = value
            .as_str()
            .or_else(|| value.get("version").and_then(toml::Value::as_str))
            .context("a development tool has no version")?;
        let profiles: Vec<Profile> = Profile::ALL
            .into_iter()
            .filter(|profile| {
                value
                    .get("os")
                    .and_then(toml::Value::as_array)
                    .is_none_or(|systems| {
                        let system = match profile {
                            Profile::Mac => "macos",
                            Profile::Windows => "windows",
                            Profile::Linux | Profile::Wsl => "linux",
                        };
                        systems
                            .iter()
                            .any(|allowed| allowed.as_str() == Some(system))
                    })
            })
            .collect();
        add(inventory, &profiles, spec, Some(version));
    }
    Ok(())
}

fn bindings_current(installed: Option<&Installed>, skills: &[(&str, &str)]) -> bool {
    skills.iter().all(|(_, version)| {
        installed.is_some_and(|installed| {
            installed
                .versions
                .iter()
                .all(|pin| version.split('|').any(|reviewed| reviewed == pin))
        })
    })
}

/// Adds what each setup step installs, refusing a step the policy omits and a name that no step has.
fn steps(inventory: &mut Inventory, policy: &Policy) -> Result<()> {
    let names: BTreeSet<&str> = Step::ALL.iter().map(|step| step.name()).collect();
    for step in Step::ALL {
        let tools = policy.steps.get(step.name()).with_context(|| {
            format!(
                "the setup step {} has no entry under [steps] in {POLICY}; list the tools it installs, or none",
                step.name()
            )
        })?;
        for tool in tools {
            add(inventory, step.profiles(), tool, None);
        }
    }
    for step in policy.steps.keys() {
        ensure!(
            names.contains(step.as_str()),
            "[steps] in {POLICY} names {step}, which no setup step is; remove it"
        );
    }
    Ok(())
}

/// The versions a skill's `tools.toml` binds to its reviewed content.
///
/// # Errors
///
/// Returns an error when the binding file fails to parse or names an empty tool or version.
pub fn declared(skill: &str) -> Result<BTreeMap<String, String>> {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Bindings {
        #[serde(default)]
        tools: BTreeMap<String, String>,
    }
    let bindings: Bindings = toml::from_str(skill)?;
    ensure!(
        bindings
            .tools
            .iter()
            .all(|(tool, version)| !tool.trim().is_empty() && !version.trim().is_empty()),
        "a tool binding needs a name and reviewed version"
    );
    Ok(bindings.tools)
}

/// The targets that targets.toml restricts to a set of profiles.
#[derive(Deserialize)]
struct Targets {
    targets: BTreeMap<String, String>,
}

/// The profiles each named set of profiles holds.
fn profile_sets(root: &Path) -> Result<BTreeMap<String, BTreeSet<String>>> {
    #[derive(Deserialize)]
    struct Sets {
        profiles: BTreeMap<String, BTreeSet<String>>,
    }
    let sets: Sets = toml::from_str(&fs::read_to_string(root.join(PROFILE_SETS))?)?;
    Ok(sets.profiles)
}

/// The profiles a target installs on: every profile unless targets.toml restricts it to a set.
fn target_profiles(
    targets: &BTreeMap<String, String>,
    sets: &BTreeMap<String, BTreeSet<String>>,
    target: &str,
) -> Result<BTreeSet<String>> {
    match targets.get(target) {
        Some(set) => sets.get(set).cloned().with_context(|| {
            format!("{TARGETS} restricts {target} to {set}, which {PROFILE_SETS} lacks")
        }),
        None => Ok(Profile::ALL
            .iter()
            .map(|profile| profile.name().to_owned())
            .collect()),
    }
}

/// Every refusal of the coverage gate, from the inventory, each skill's declared tools, the no-skill decisions, and where each skill installs.
#[must_use]
pub fn refusals(
    inventory: &Inventory,
    skills: &BTreeMap<String, BTreeMap<String, String>>,
    policy: &Policy,
    placements: &BTreeMap<String, BTreeSet<String>>,
) -> Vec<String> {
    let mut refusals = Vec::new();
    let mut tools: BTreeSet<&str> = inventory.keys().map(String::as_str).collect();
    let mut decisions: BTreeMap<&str, usize> = BTreeMap::new();
    for decision in &policy.none {
        if decision.why.trim().is_empty() {
            refusals.push(format!(
                "a no-skill decision in {POLICY} for {} gives no why",
                decision.tools.join(", ")
            ));
        }
        for tool in &decision.tools {
            *decisions.entry(tool).or_default() += 1;
            tools.insert(tool);
        }
    }
    let mut declaring: BTreeMap<&str, Vec<(&str, &str)>> = BTreeMap::new();
    for (skill, declared) in skills {
        for (tool, version) in declared {
            declaring.entry(tool).or_default().push((skill, version));
            tools.insert(tool);
        }
    }
    for tool in tools {
        let installed = inventory.get(tool);
        let skills = declaring.get(tool).map(Vec::as_slice).unwrap_or_default();
        let current = bindings_current(installed, skills);
        match coverage(installed.is_some(), skills.len(), current, decisions.get(tool).copied().unwrap_or_default()) {
            Coverage::Covered => {}
            Coverage::Missing => refusals.push(format!(
                "{tool} installs on {} with no skill and no decision; bind its version in a skill's tools.toml, or add it to a no-skill decision in {POLICY} with its reason",
                installed.map(|installed| installed.profiles.iter().copied().collect::<Vec<_>>().join(", ")).unwrap_or_default()
            )),
            Coverage::Contradicted => refusals.push(format!(
                "{tool} has both a skill and a no-skill decision, or two decisions; keep one in {POLICY} or the skill"
            )),
            Coverage::Stale => {
                for (skill, version) in skills {
                    refusals.push(format!(
                        "{skill} was written against {tool} {version}, and the inventory now installs {}; review the skill for that version, update its tools.toml, and run `just decide {skill}`",
                        installed.map(|installed| installed.versions.iter().cloned().collect::<Vec<_>>().join(" or ")).unwrap_or_default()
                    ));
                }
            }
            Coverage::Orphaned => refusals.push(format!(
                "{tool} is named by a skill or a decision, but no install source places it; remove the name or install the tool"
            )),
        }
    }
    let every: BTreeSet<String> = Profile::ALL
        .iter()
        .map(|profile| profile.name().to_owned())
        .collect();
    for (skill, declared) in skills {
        let specific: Option<BTreeSet<String>> = declared
            .keys()
            .filter_map(|tool| policy.platform_specific.get(tool))
            .map(|profiles| profiles.iter().cloned().collect())
            .reduce(|left: BTreeSet<String>, right| left.union(&right).cloned().collect());
        let installs = placements
            .get(skill)
            .cloned()
            .unwrap_or_else(|| every.clone());
        let decision = placement(
            specific.is_some(),
            installs != every,
            specific.as_ref() == Some(&installs),
        );
        if decision == Placement::Misplaced {
            refusals.push(match specific {
                Some(profiles) => format!(
                    "{skill} covers a tool that only {} install, so restrict .agents/skills/{skill} and .claude/skills/{skill} to exactly those profiles in {TARGETS}",
                    profiles.into_iter().collect::<Vec<_>>().join(", ")
                ),
                None => format!(
                    "{skill} installs on fewer than every profile, but none of its tools is platform-specific in {POLICY}; remove its restriction from {TARGETS}"
                ),
            });
        }
    }
    for (tool, profiles) in &policy.platform_specific {
        let installed = inventory
            .get(tool.as_str())
            .map(|installed| &installed.profiles);
        let outside = installed
            .into_iter()
            .flatten()
            .filter(|profile| !profiles.iter().any(|allowed| allowed == *profile))
            .collect::<Vec<_>>();
        if !outside.is_empty() {
            refusals.push(format!(
                "{tool} is platform-specific to {} in {POLICY}, but installs on {} too",
                profiles.join(", "),
                outside.into_iter().copied().collect::<Vec<_>>().join(", ")
            ));
        }
    }
    refusals
}

/// Holds every inventoried tool to a version-bound skill or a no-skill decision, and every platform-specific skill to the profiles of its tool.
///
/// # Errors
///
/// Returns an error listing every refusal, or when an input fails to read or parse.
pub fn check(root: &Path) -> Result<usize> {
    let policy = policy(root)?;
    let inventory = inventory(root, &policy)?;
    let mut skills = BTreeMap::new();
    for entry in fs::read_dir(root.join(SKILLS))? {
        let entry = entry?;
        let name = entry.file_name().to_string_lossy().into_owned();
        let binding = entry.path().join(TOOL_BINDINGS);
        let text = if binding.try_exists()? {
            fs::read_to_string(binding)?
        } else {
            String::new()
        };
        skills.insert(
            name.clone(),
            declared(&text).with_context(|| format!("the skill {name}"))?,
        );
    }
    let targets: Targets = toml::from_str(&fs::read_to_string(root.join(TARGETS))?)?;
    let sets = profile_sets(root)?;
    let mut placements = BTreeMap::new();
    for name in skills.keys() {
        let agents = target_profiles(&targets.targets, &sets, &format!(".agents/skills/{name}"))?;
        let claude = target_profiles(&targets.targets, &sets, &format!(".claude/skills/{name}"))?;
        ensure!(
            agents == claude,
            "{TARGETS} places .agents/skills/{name} and .claude/skills/{name} on different profiles; give both the same set"
        );
        placements.insert(name.clone(), agents);
    }
    let refusals = refusals(&inventory, &skills, &policy, &placements);
    ensure!(
        refusals.is_empty(),
        "the skill catalog doesn't cover every installed tool:\n{}",
        refusals.join("\n")
    );
    Ok(inventory.len())
}
