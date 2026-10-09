//! Manifest refusals name conflicting entries and their profiles, including WSL.

use serde_json::Value;
use std::collections::BTreeMap;
use std::fs;
use std::path::Path;
use xtask::manifest_model::{
    HOST_STATE, Manifest, instance, over_broad_ignores, removed_and_managed, rendered_hooks,
    restricted_hooks, skip_findings, unprovided,
};
use xtask::profile_rules::Profile;

fn root() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap()
}

fn data() -> Value {
    model::data::load(&root().join("home").join(model::data::DIRECTORY)).unwrap()
}

fn manifest(data: &Value) -> Manifest {
    let mut launchers = BTreeMap::new();
    for family in platform::step::Family::ALL {
        let directory = root().join("home/.chezmoiscripts").join(family.directory());
        launchers.insert(
            family.directory(),
            fs::read_dir(directory)
                .unwrap()
                .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
                .collect(),
        );
    }
    Manifest::new(
        data,
        &fs::read_to_string(root().join(HOST_STATE)).unwrap(),
        launchers,
    )
}

#[test]
fn the_manifest_hides_removes_skips_and_needs_nothing_it_should_not() {
    let data = data();
    let manifest = manifest(&data);
    assert_eq!(over_broad_ignores(&manifest), Vec::<String>::new());
    assert_eq!(removed_and_managed(&manifest), Vec::<String>::new());
    for profile in Profile::ALL {
        let skipped: Vec<&str> = xtask::rehearsal::omitted(profile)
            .iter()
            .map(|skip| skip.script)
            .collect();
        assert_eq!(
            skip_findings(&manifest, profile, &skipped),
            Vec::<String>::new()
        );
    }
    assert_eq!(unprovided(&data), Vec::<String>::new());
    xtask::manifest_model::verify(root(), &data).unwrap();
}

#[test]
fn a_planted_over_broad_ignore_names_the_entry_and_the_profile() {
    let data = data();
    let mut manifest = manifest(&data);
    manifest.host_state.push(".config/herdr".to_owned());
    let found = over_broad_ignores(&manifest);
    assert!(
        found.iter().any(|finding| finding
            .starts_with("mac: the ignore entry .config/herdr hides .config/herdr/config.toml")),
        "{found:?}"
    );
    let mut manifest = self::manifest(&data);
    manifest
        .retired
        .insert(".config/herdr/config.toml".to_owned(), "desktop".to_owned());
    let found = removed_and_managed(&manifest);
    assert!(
        found.iter().any(|finding| finding
            .starts_with("linux: the retired entry .config/herdr/config.toml removes")),
        "{found:?}"
    );
}

/// The counterexample of `UniformHookMaskUnchecked` in `specs/manifest.als`: a restricted entry over a rendered hook.
#[test]
fn a_restricted_hook_names_the_entry_and_the_hook_and_the_alloy_instance_is_current() {
    let data = data();
    let hooks = rendered_hooks(root()).unwrap();
    assert!(hooks.contains(&"pre-commit".to_owned()), "{hooks:?}");
    let mut manifest = manifest(&data);
    assert_eq!(restricted_hooks(&manifest, &hooks), Vec::<String>::new());
    manifest
        .restricted
        .insert(".config/git/hooks".to_owned(), "desktop".to_owned());
    let found = restricted_hooks(&manifest, &hooks);
    assert!(
        found.contains(
            &"the entry .config/git/hooks limits the hook pre-commit to the profiles of desktop"
                .to_owned()
        ),
        "{found:?}"
    );
    instance(root(), true).unwrap();
}

#[test]
fn a_misspelled_or_orphaning_skip_names_the_entry_and_the_profile() {
    let data = data();
    let manifest = manifest(&data);
    assert_eq!(
        skip_findings(&manifest, Profile::Mac, &["ocamel.sh"]),
        ["mac: the skip ocamel.sh names no setup launcher the profile runs"]
    );
    assert_eq!(
        skip_findings(&manifest, Profile::Windows, &["ocaml.sh"]),
        ["windows: the skip ocaml.sh names no setup launcher the profile runs"]
    );
    let orphaned = skip_findings(&manifest, Profile::Linux, &["tools.sh"]);
    assert!(
        orphaned.iter().any(|finding| finding
            == "linux: skipping tools orphans integrations, which still runs after it"),
        "{orphaned:?}"
    );
}

#[test]
fn a_missing_package_names_the_step_the_tool_and_the_profile() {
    let mut data = data();
    data["platforms"]["linux"]["nix"]["packages"] = serde_json::json!({});
    let found = unprovided(&data);
    assert!(
        found.iter().any(|finding| finding
            == "linux: the step ocomment needs lefthook, which the profile doesn't install"),
        "{found:?}"
    );
}
