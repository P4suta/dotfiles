//! The target manifest as a model: which profiles receive, ignore, and remove each target, which setup scripts a host skips, and which tools each step needs.
//! The findings hold over the typed data of `home/.chezmoidata` without rendering, and each names its entry and its profile.
//! `alloy_instance` writes the same manifest as an instance of `specs/manifest.als`, whose checks prove the validators' assertions on it.

use crate::ignore_rules::matches;
use crate::profile_rules::Profile;
use platform::step::{Family, Step};
use platform::tool::Provision;
use serde_json::Value;
use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::Path;

/// The shared listing of host state that every profile ignores.
pub const HOST_STATE: &str = "home/.chezmoitemplates/partials/host-state-ignore";

/// Where `just gen` writes the real manifest as an Alloy instance of `specs/manifest.als`.
pub const ALLOY_INSTANCE: &str = "specs/manifest_instance.als";

/// The scope of every command of the instance, which `policy/obligations.toml` pins.
const INSTANCE_SCOPE: usize = 4;

/// The rendered Git hooks of the source state, each named `executable_<hook>.tmpl`.
const HOOKS: &str = "home/dot_config/git/hooks";

/// The target of each rendered Git hook.
const HOOK_TARGET: &str = ".config/git/hooks";

/// The manifest of one checkout.
#[derive(Debug, Default)]
pub struct Manifest {
    /// Each named set of profiles.
    pub sets: BTreeMap<String, Vec<String>>,
    /// Targets and setup launchers that only the profiles of their set receive.
    pub restricted: BTreeMap<String, String>,
    /// Retired targets that the profiles of their set remove.
    pub retired: BTreeMap<String, String>,
    /// The host state patterns that every profile ignores.
    pub host_state: Vec<String>,
    /// The launcher file names of each OS family directory.
    pub launchers: BTreeMap<&'static str, Vec<String>>,
}

impl Manifest {
    /// The manifest of the merged chezmoi data, the host state listing, and the launcher directories.
    #[must_use]
    pub fn new(
        data: &Value,
        host_state: &str,
        launchers: BTreeMap<&'static str, Vec<String>>,
    ) -> Self {
        let table = |name: &str| -> BTreeMap<String, String> {
            data[name]
                .as_object()
                .into_iter()
                .flatten()
                .filter_map(|(key, set)| Some((key.clone(), set.as_str()?.to_owned())))
                .collect()
        };
        let mut restricted = table("targets");
        restricted.extend(table("scripts"));
        Self {
            sets: data["profiles"]
                .as_object()
                .into_iter()
                .flatten()
                .map(|(name, members)| {
                    (
                        name.clone(),
                        members
                            .as_array()
                            .into_iter()
                            .flatten()
                            .filter_map(Value::as_str)
                            .map(str::to_owned)
                            .collect(),
                    )
                })
                .collect(),
            restricted,
            retired: table("retired"),
            host_state: host_state
                .lines()
                .map(str::trim)
                .filter(|line| {
                    !line.is_empty() && !line.starts_with('#') && !line.starts_with("{{")
                })
                .map(str::to_owned)
                .collect(),
            launchers,
        }
    }

    fn holds(&self, set: &str, profile: Profile) -> bool {
        self.sets
            .get(set)
            .is_some_and(|members| members.iter().any(|member| member == profile.name()))
    }

    /// The restricted targets `profile` receives, without the wildcard entries that only select a tree.
    fn received(&self, profile: Profile) -> impl Iterator<Item = &String> {
        self.restricted
            .iter()
            .filter(move |(target, set)| !target.contains('*') && self.holds(set, profile))
            .map(|(target, _)| target)
    }

    /// The patterns `.chezmoiignore` holds on `profile`: every restricted entry it doesn't receive, and the host state.
    fn ignored(&self, profile: Profile) -> impl Iterator<Item = &String> {
        self.restricted
            .iter()
            .filter(move |(_, set)| !self.holds(set, profile))
            .map(|(target, _)| target)
            .chain(&self.host_state)
    }
}

/// Whether `pattern` matches the target or a parent directory.
fn covers(pattern: &str, target: &str) -> bool {
    matches(pattern.as_bytes(), target.as_bytes())
        || target
            .char_indices()
            .filter(|(_, character)| *character == '/')
            .any(|(index, _)| matches(pattern.as_bytes(), &target.as_bytes()[..index]))
}

/// The entries that ignore a target its profile receives, which would leave the target unrendered on that host.
#[must_use]
pub fn over_broad_ignores(manifest: &Manifest) -> Vec<String> {
    let mut found = Vec::new();
    for profile in Profile::ALL {
        for target in manifest.received(profile) {
            for pattern in manifest.ignored(profile) {
                if covers(pattern, target) {
                    found.push(format!(
                        "{}: the ignore entry {pattern} hides {target}, which the profile receives",
                        profile.name()
                    ));
                }
            }
        }
    }
    found
}

/// The retired entries that would remove a target their profile still receives.
#[must_use]
pub fn removed_and_managed(manifest: &Manifest) -> Vec<String> {
    let mut found = Vec::new();
    for profile in Profile::ALL {
        for (retired, set) in &manifest.retired {
            if !manifest.holds(set, profile) {
                continue;
            }
            for target in manifest.received(profile) {
                if covers(retired, target) {
                    found.push(format!(
                        "{}: the retired entry {retired} removes {target}, which the profile receives",
                        profile.name()
                    ));
                }
            }
        }
    }
    found
}

/// The restricted entries that cover a rendered Git hook, which some profile would then run without.
#[must_use]
pub fn restricted_hooks(manifest: &Manifest, hooks: &[String]) -> Vec<String> {
    let mut found = Vec::new();
    for hook in hooks {
        let target = format!("{HOOK_TARGET}/{hook}");
        for (pattern, set) in &manifest.restricted {
            if covers(pattern, &target) {
                found.push(format!(
                    "the entry {pattern} limits the hook {hook} to the profiles of {set}"
                ));
            }
        }
    }
    found
}

/// The step a launcher script name such as `ocaml.sh` runs.
fn step_of(script: &str) -> Option<Step> {
    let name = script.rsplit_once('.').map_or(script, |(name, _)| name);
    Step::ALL.into_iter().find(|step| step.name() == name)
}

/// Whether `family` holds the launcher `script`, named `<prefix>-<NN>-<script>.tmpl`.
fn launches(manifest: &Manifest, family: Family, script: &str) -> bool {
    manifest
        .launchers
        .get(family.directory())
        .into_iter()
        .flatten()
        .any(|file| file.ends_with(&format!("-{script}.tmpl")))
}

/// The skipped scripts that name no launcher of their profile, and those whose skip orphans a step that still runs.
#[must_use]
pub fn skip_findings(manifest: &Manifest, profile: Profile, skipped: &[&str]) -> Vec<String> {
    let family = Family::ALL
        .into_iter()
        .find(|family| family.profiles().contains(&profile));
    let mut found = Vec::new();
    let mut steps = Vec::new();
    for script in skipped {
        match (step_of(script), family) {
            (Some(step), Some(family))
                if launches(manifest, family, script) && step.runs_on(profile) =>
            {
                steps.push(step);
            }
            _ => found.push(format!(
                "{}: the skip {script} names no setup launcher the profile runs",
                profile.name()
            )),
        }
    }
    // Adapters runs after every other step only to come last, so a skip leaves it nothing it relies on.
    for step in Step::ALL {
        if step.runs_on(profile) && step != Step::Adapters && !steps.contains(&step) {
            for prerequisite in step.prerequisites() {
                if steps.contains(&prerequisite) {
                    found.push(format!(
                        "{}: skipping {} orphans {}, which still runs after it",
                        profile.name(),
                        prerequisite.name(),
                        step.name()
                    ));
                }
            }
        }
    }
    found
}

/// The atoms `<prefix>0` up to `<prefix><count - 1>`, joined by `separator`.
fn atoms(prefix: &str, count: usize, separator: &str) -> String {
    (0..count)
        .map(|index| format!("{prefix}{index}"))
        .collect::<Vec<_>>()
        .join(separator)
}

/// The targets, profile sets, retired entries, hooks, and parent paths of the real manifest.
struct Instance<'a> {
    manifest: &'a Manifest,
    targets: Vec<(String, Option<&'a String>)>,
    retired: Vec<(&'a String, &'a String)>,
    hooks: Vec<usize>,
    paths: Vec<String>,
}

/// The one relation entry a counterexample changes.
#[derive(Clone, Copy)]
enum Change {
    Overlap,
    Hook(usize),
    Mask(usize, Option<Profile>),
}

/// A variant of the real manifest that changes one entry, which the validator of `assertion` refuses.
struct Plant {
    name: &'static str,
    assertion: &'static str,
    description: &'static str,
    change: Change,
}

fn relation(
    text: &mut String,
    name: &str,
    signature: &str,
    tuples: &[String],
) -> anyhow::Result<()> {
    let value = if tuples.is_empty() {
        "none".to_owned()
    } else {
        tuples.join(" + ")
    };
    writeln!(text, "fun {name}: {signature} {{ {value} }}")?;
    Ok(())
}

fn assertion(name: &str, variant: &str) -> String {
    let arguments = match name {
        "disjoint" => format!("{variant}Receives, paths, parents, {variant}Removes, {variant}At"),
        "noEmptyDirectory" => format!("{variant}Receives, paths, parents"),
        "hookBijection" => format!("{variant}Files"),
        _ => format!("{variant}Files, {variant}Receives"),
    };
    format!("checks/{name}[{arguments}]")
}

impl<'a> Instance<'a> {
    fn new(manifest: &'a Manifest, hooks: &[String]) -> Self {
        let mut targets: Vec<(String, Option<&String>)> = manifest
            .restricted
            .iter()
            .filter(|(target, _)| !target.contains('*'))
            .map(|(target, set)| (target.clone(), Some(set)))
            .collect();
        let mut hook_targets = Vec::new();
        for hook in hooks {
            let target = format!("{HOOK_TARGET}/{hook}");
            let set = manifest
                .restricted
                .iter()
                .find(|(pattern, _)| covers(pattern, &target))
                .map(|(_, set)| set);
            hook_targets.push(targets.len());
            targets.push((target, set));
        }
        let mut paths: Vec<String> = Vec::new();
        for name in targets
            .iter()
            .map(|(target, _)| target)
            .chain(manifest.retired.keys())
        {
            let mut at = name.as_str();
            loop {
                if !paths.iter().any(|known| known == at) {
                    paths.push(at.to_owned());
                }
                match at.rsplit_once('/') {
                    Some((parent, _)) => at = parent,
                    None => break,
                }
            }
        }
        paths.sort();
        Self {
            manifest,
            targets,
            retired: manifest.retired.iter().collect(),
            hooks: hook_targets,
            paths,
        }
    }

    fn path(&self, name: &str) -> String {
        self.paths
            .iter()
            .position(|known| known == name)
            .map_or_else(|| "none".to_owned(), |index| format!("P{index}"))
    }

    fn profiles(&self, set: Option<&String>) -> String {
        let names: Vec<&str> = Profile::ALL
            .into_iter()
            .filter(|profile| set.is_none_or(|set| self.manifest.holds(set, *profile)))
            .map(Profile::name)
            .collect();
        if names.is_empty() {
            "none".to_owned()
        } else {
            names.join(" + ")
        }
    }

    /// Each relation of the real manifest, with one entry changed for a counterexample.
    fn relations(
        &self,
        text: &mut String,
        variant: &str,
        change: Option<Change>,
    ) -> anyhow::Result<()> {
        let receives: Vec<String> = self
            .targets
            .iter()
            .enumerate()
            .map(|(index, (_, set))| {
                let profiles = match change {
                    Some(Change::Mask(target, profile)) if target == index => {
                        profile.map_or("Profile", Profile::name).to_owned()
                    }
                    _ => self.profiles(*set),
                };
                format!("T{index} -> ({profiles})")
            })
            .collect();
        let files: Vec<String> = self
            .hooks
            .iter()
            .enumerate()
            .map(|(index, target)| {
                let target = match change {
                    Some(Change::Hook(first)) if index == 1 => first,
                    _ => *target,
                };
                format!("H{index} -> T{target}")
            })
            .collect();
        let mut at = Vec::new();
        let mut removes = Vec::new();
        for (index, (name, set)) in self.retired.iter().enumerate() {
            let (path, profiles) = if matches!(change, Some(Change::Overlap)) && index == 0 {
                self.targets.first().map_or_else(
                    || ("none".to_owned(), "none".to_owned()),
                    |(target, set)| (self.path(target), self.profiles(*set)),
                )
            } else {
                (self.path(name), self.profiles(Some(set)))
            };
            at.push(format!("R{index} -> {path}"));
            removes.push(format!("R{index} -> ({profiles})"));
        }
        for (suffix, signature, tuples) in [
            ("Receives", "Target -> Profile", receives),
            ("Files", "Hook -> Target", files),
            ("At", "Retired -> Path", at),
            ("Removes", "Retired -> Profile", removes),
        ] {
            relation(text, &format!("{variant}{suffix}"), signature, &tuples)?;
        }
        writeln!(
            text,
            "pred {variant} {{ checks/tree[parents] and checks/unique[paths] }}"
        )?;
        Ok(())
    }

    /// A restricted directory whose leaves some profile lacks, which a manifest giving it to every profile would leave empty there.
    fn sparse_directory(&self) -> Option<usize> {
        self.targets.iter().position(|(target, set)| {
            let leaves: Vec<Option<&String>> = self
                .targets
                .iter()
                .filter(|(leaf, _)| leaf.starts_with(&format!("{target}/")))
                .map(|(_, set)| *set)
                .collect();
            set.is_some()
                && !leaves.is_empty()
                && Profile::ALL.into_iter().any(|profile| {
                    !leaves
                        .iter()
                        .any(|leaf| leaf.is_none_or(|leaf| self.manifest.holds(leaf, profile)))
                })
        })
    }

    /// One planted defect per assertion of `specs/manifest.als`.
    fn plants(&self) -> Vec<Plant> {
        let hook = self.hooks.first().copied().unwrap_or_default();
        let mut plants = vec![
            Plant {
                name: "plantedOverlap",
                assertion: "disjoint",
                description: "the first retired entry moved onto the first target",
                change: Change::Overlap,
            },
            Plant {
                name: "plantedHook",
                assertion: "hookBijection",
                description: "the second hook run from the first hook's file",
                change: Change::Hook(hook),
            },
            Plant {
                name: "plantedMask",
                assertion: "uniformHookMask",
                description: "the first hook's file received on the Mac alone",
                change: Change::Mask(hook, Some(Profile::Mac)),
            },
        ];
        if let Some(directory) = self.sparse_directory() {
            plants.push(Plant {
                name: "plantedEmpty",
                assertion: "noEmptyDirectory",
                description: "a restricted directory given to every profile",
                change: Change::Mask(directory, None),
            });
        }
        plants
    }

    fn render(self) -> anyhow::Result<String> {
        let mut text = String::from(
            "// The real target manifest of `home/.chezmoidata` and the rendered Git hooks, which `just gen` writes from `xtask/src/manifest_model.rs`.\nmodule manifest_instance\nopen manifest_checks[Profile, Path, Target, Retired, Hook] as checks\n\nabstract sig Profile {}\none sig mac, linux, windows, wsl extends Profile {}\nabstract sig Path {}\nabstract sig Target {}\nabstract sig Retired {}\nabstract sig Hook {}\n\n",
        );
        for (index, name) in self.paths.iter().enumerate() {
            writeln!(text, "// P{index} is {name}")?;
        }
        for (prefix, count, sig) in [
            ("P", self.paths.len(), "Path"),
            ("T", self.targets.len(), "Target"),
            ("R", self.retired.len(), "Retired"),
            ("H", self.hooks.len(), "Hook"),
        ] {
            writeln!(
                text,
                "one sig {} extends {sig} {{}}",
                atoms(prefix, count, ", ")
            )?;
        }
        // Literal relations let the solver expand closure over the fixed data, with no dense relation to guess first.
        let parents: Vec<String> = self
            .paths
            .iter()
            .filter_map(|name| {
                name.rsplit_once('/')
                    .map(|(parent, _)| format!("{} -> {}", self.path(name), self.path(parent)))
            })
            .collect();
        let targets: Vec<String> = self
            .targets
            .iter()
            .enumerate()
            .map(|(index, (name, _))| format!("T{index} -> {}", self.path(name)))
            .collect();
        text.push('\n');
        relation(&mut text, "parents", "Path -> Path", &parents)?;
        relation(&mut text, "paths", "Target -> Path", &targets)?;
        self.relations(&mut text, "real", None)?;
        let plants = self.plants();
        for plant in &plants {
            writeln!(
                text,
                "\n// The real manifest with {}, which a validator refuses.",
                plant.description
            )?;
            self.relations(&mut text, plant.name, Some(plant.change))?;
        }
        // The one-sig atoms bound every abstract sig exactly, so the scope only covers the four profiles and stays fixed as the manifest grows.
        let scope = INSTANCE_SCOPE;
        text.push('\n');
        for plant in &plants {
            writeln!(
                text,
                "Real_{}: check {{ real implies {} }} for {scope}",
                plant.assertion,
                assertion(plant.assertion, "real")
            )?;
            writeln!(
                text,
                "Planted_{}: check {{ {} implies {} }} for {scope}",
                plant.assertion,
                plant.name,
                assertion(plant.assertion, plant.name)
            )?;
        }
        writeln!(text, "RealManifest: run {{ real }} for {scope}")?;
        Ok(text)
    }
}

/// The real manifest as an Alloy module: every concrete restricted target, every rendered hook file, which every profile receives, and every retired target.
/// `real` pins each relation, and each planted variant changes one fact, so each assertion of `specs/manifest.als` holds on the first and fails on its variant.
///
/// # Errors
///
/// Returns an error when writing a fact fails.
pub fn alloy_instance(manifest: &Manifest, hooks: &[String]) -> anyhow::Result<String> {
    Instance::new(manifest, hooks).render()
}

/// The hooks that the source state renders, by name.
///
/// # Errors
///
/// Returns an error when reading the directory fails.
pub fn rendered_hooks(root: &Path) -> anyhow::Result<Vec<String>> {
    let mut hooks = Vec::new();
    for entry in std::fs::read_dir(root.join(HOOKS))? {
        let name = entry?.file_name().to_string_lossy().into_owned();
        if let Some(hook) = name
            .strip_prefix("executable_")
            .and_then(|name| name.strip_suffix(".tmpl"))
        {
            hooks.push(hook.to_owned());
        }
    }
    hooks.sort();
    Ok(hooks)
}

/// The manifest of the checkout at `root`.
///
/// # Errors
///
/// Returns an error when reading the data, host state listing, or launcher directories fails.
pub fn checkout(root: &Path) -> anyhow::Result<Manifest> {
    let data = model::data::load(&root.join("home").join(model::data::DIRECTORY))
        .map_err(anyhow::Error::msg)?;
    let mut launchers = BTreeMap::new();
    for family in Family::ALL {
        let mut files = Vec::new();
        for entry in std::fs::read_dir(root.join("home/.chezmoiscripts").join(family.directory()))?
        {
            files.push(entry?.file_name().to_string_lossy().into_owned());
        }
        launchers.insert(family.directory(), files);
    }
    Ok(Manifest::new(
        &data,
        &std::fs::read_to_string(root.join(HOST_STATE))?,
        launchers,
    ))
}

/// Writes the Alloy instance of the real manifest, or with `check` refuses a stale one.
///
/// # Errors
///
/// Returns an error when reading the checkout or writing fails, or with `check` when the committed instance differs.
pub fn instance(root: &Path, check: bool) -> anyhow::Result<()> {
    let wanted = alloy_instance(&checkout(root)?, &rendered_hooks(root)?)?;
    let file = root.join(ALLOY_INSTANCE);
    if check {
        anyhow::ensure!(
            std::fs::read_to_string(&file).is_ok_and(|current| current == wanted),
            "{ALLOY_INSTANCE} differs from the manifest; run `just gen` and commit the result"
        );
    } else {
        std::fs::write(&file, wanted)?;
    }
    Ok(())
}

/// Checks every profile's manifest, setup skips, and provisioned tools before rendering.
///
/// # Errors
///
/// Returns an error when reading the checkout fails or a validator names a conflicting entry.
pub fn verify(root: &Path, data: &Value) -> anyhow::Result<()> {
    let manifest = checkout(root)?;
    let mut findings = over_broad_ignores(&manifest);
    findings.extend(removed_and_managed(&manifest));
    findings.extend(restricted_hooks(&manifest, &rendered_hooks(root)?));
    findings.extend(unprovided(data));
    for profile in Profile::ALL {
        let skipped: Vec<&str> = crate::rehearsal::omitted(profile)
            .iter()
            .map(|skip| skip.script)
            .collect();
        findings.extend(skip_findings(&manifest, profile, &skipped));
    }
    anyhow::ensure!(findings.is_empty(), "{}", findings.join("\n"));
    instance(root, true)
}

/// The tools a step needs on a profile that the profile's platform data doesn't install.
#[must_use]
pub fn unprovided(data: &Value) -> Vec<String> {
    let mut found = Vec::new();
    for profile in Profile::ALL {
        let platform = &data["platforms"][profile.name()];
        for step in Step::ALL.into_iter().filter(|step| step.runs_on(profile)) {
            for tool in step.requires(profile) {
                let packaged = tool.package(profile).is_some_and(|(pointer, entry)| {
                    let list = platform.pointer(pointer);
                    list.and_then(Value::as_array)
                        .is_some_and(|items| items.iter().any(|item| item == entry))
                        || list
                            .and_then(Value::as_object)
                            .is_some_and(|items| items.get(entry).is_some_and(|spec| spec != ""))
                });
                let mise_listed = platform["tools"].as_object().is_some_and(|sets| {
                    sets.values()
                        .filter_map(Value::as_array)
                        .flatten()
                        .any(|item| item == tool.program())
                });
                // The tools step installs only what the profile lists, so only a dedicated setup step provides a tool by running.
                let installed_by_step = matches!(
                    tool.provision(),
                    Provision::Setup(installer) if installer != Step::Tools && installer.runs_on(profile)
                );
                let provided = packaged || mise_listed || installed_by_step;
                if !provided {
                    found.push(format!(
                        "{}: the step {} needs {}, which the profile doesn't install",
                        profile.name(),
                        step.name(),
                        tool.program()
                    ));
                }
            }
        }
    }
    found
}
