use anyhow::{Context, Result};
use platform::build_inputs::{CRATES, content_hash, inputs};
use platform::freshness::{
    BUILD_IDENTITY_ARGUMENT, Built, DELEGATED_VARIABLE, Embedded, SOURCE_VARIABLE, embedded,
    lagging, locate, record_installation, verify, verify_artifact,
};
use platform::refusal::Refusal;
use platform::stale_rules::{Freshness, Origin, delegates, freshness, origin, reinstalls, runs};
use std::collections::BTreeSet;
use std::fs;
use std::path::{Component, Path, PathBuf};

#[path = "../../platform/tests/support/fixture_process.rs"]
mod fixture_process;

fn repository() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .expect("the crate sits two levels inside its checkout")
        .to_path_buf()
}

/// The source file of each installed executable.
fn executable(built: Built) -> &'static str {
    match built {
        Built::Coderabbit => "crates/coderabbit-guard/src/main.rs",
        Built::PrWorkflow => "crates/pr-workflow/src/main.rs",
        Built::SkillOps => "crates/skill-ops/src/main.rs",
    }
}

/// The library every installed tool links, whose source the fixtures change.
const LIBRARY: &str = "crates/platform/src/lib.rs";

/// A checkout that holds a subset of the files that produce the tools, enough to hash.
fn fixture(source: &str) -> Result<tempfile::TempDir> {
    let root = tempfile::tempdir()?;
    for krate in CRATES {
        fs::create_dir_all(root.path().join(krate).join("src"))?;
    }
    fs::write(
        root.path().join("crates/platform/Cargo.toml"),
        "[package]\nname = \"platform\"\n",
    )?;
    fs::write(root.path().join(LIBRARY), source)?;
    for built in Built::ALL {
        fs::write(root.path().join(executable(built)), "fn main() {}")?;
    }
    Ok(root)
}

fn built_from(root: &Path) -> Result<Embedded> {
    Ok(Embedded {
        revision: "0123456789abcdef".into(),
        root: root.to_path_buf(),
        hash: content_hash(root)?,
    })
}

fn record_fixture(home: &Path, root: &Path, built: Built) -> Result<()> {
    let directory = home.join(".local/state/dotfiles/installed");
    fs::create_dir_all(&directory)?;
    let record = serde_json::json!({
        "schema": 1,
        "build": {
            "revision": "fixture",
            "root": platform::paths::canonical(root)?,
            "hash": content_hash(root)?,
        },
    });
    fs::write(directory.join(built.binary()), serde_json::to_vec(&record)?)?;
    Ok(())
}

fn enable_skill_ops(home: &Path) -> Result<()> {
    let consent = home.join(platform::incidents::EXPERIMENTAL);
    fs::create_dir_all(consent.parent().unwrap())?;
    fs::write(consent, rules::incident_rules::EXPERIMENTAL_OPT_IN)?;
    Ok(())
}

fn rust_files(directory: PathBuf) -> Result<Vec<PathBuf>> {
    let mut found = Vec::new();
    let mut pending = vec![directory];
    while let Some(directory) = pending.pop() {
        for entry in fs::read_dir(directory)? {
            let path = entry?.path();
            if path.is_dir() {
                pending.push(path);
            } else if path.extension().is_some_and(|extension| extension == "rs") {
                found.push(path);
            }
        }
    }
    Ok(found)
}

/// Every Rust source of the crates the installed tools build from.
fn tool_sources(root: &Path) -> Result<Vec<PathBuf>> {
    let mut found = Vec::new();
    for krate in CRATES {
        found.extend(rust_files(root.join(krate).join("src"))?);
    }
    Ok(found)
}

fn normalize(path: &Path) -> PathBuf {
    let mut result = PathBuf::new();
    for component in path.components() {
        match component {
            Component::ParentDir => {
                result.pop();
            }
            Component::CurDir => {}
            other => result.push(other),
        }
    }
    result
}

#[test]
fn only_a_found_differing_build_is_stale() {
    assert_eq!(freshness(true, false), Freshness::Stale);
    assert_eq!(freshness(true, true), Freshness::Current);
    assert_eq!(freshness(false, false), Freshness::Unverified);
    assert_eq!(freshness(false, true), Freshness::Unverified);
    assert!(!runs(Freshness::Stale));
    assert!(runs(Freshness::Current));
    assert!(runs(Freshness::Unverified));
}

#[test]
fn the_nearest_named_checkout_wins() {
    assert_eq!(origin(true, true, true), Some(Origin::Explicit));
    assert_eq!(origin(false, true, true), Some(Origin::WorkingCopy));
    assert_eq!(origin(false, false, true), Some(Origin::BuildCheckout));
    assert_eq!(origin(false, false, false), None);
}

#[test]
fn only_a_recorded_lagging_installation_is_rebuilt() {
    assert!(reinstalls(true, true, false));
    assert!(!reinstalls(true, true, true));
    assert!(!reinstalls(true, false, false));
    assert!(!reinstalls(false, true, false));
}

#[test]
fn a_fixture_installation_older_than_its_sources_is_refused_with_the_install_action() -> Result<()>
{
    for built in Built::ALL {
        let checkout = fixture("pub fn rule() {}")?;
        let installed = built_from(checkout.path())?;
        verify(built, &installed, None, Some(checkout.path()))?;
        fs::write(
            checkout.path().join(LIBRARY),
            "pub fn rule() { /* merged */ }",
        )?;
        let error = verify(built, &installed, None, Some(checkout.path()))
            .expect_err("a build older than its sources must be refused");
        let refusal = error
            .downcast_ref::<Refusal>()
            .expect("the refusal is structured");
        assert_eq!(refusal.rule, "tool.stale");
        assert_eq!(
            refusal.next.as_deref(),
            Some(format!("just {}", built.install_task()).as_str())
        );
        assert!(refusal.is_complete());
    }
    Ok(())
}

#[test]
fn a_build_is_compared_with_the_working_copy_before_the_checkout_it_was_built_in() -> Result<()> {
    let built_in = fixture("pub fn rule() {}")?;
    let installed = built_from(built_in.path())?;
    let newer = fixture("pub fn rule() { /* newer */ }")?;
    let nested = newer.path().join("crates/platform/src");
    assert!(verify(Built::PrWorkflow, &installed, None, Some(&nested)).is_err());
    verify(Built::PrWorkflow, &installed, None, Some(built_in.path()))?;
    let elsewhere = tempfile::tempdir()?;
    verify(Built::PrWorkflow, &installed, None, Some(elsewhere.path()))?;
    assert!(
        verify(
            Built::PrWorkflow,
            &installed,
            Some(newer.path()),
            Some(elsewhere.path())
        )
        .is_err()
    );
    Ok(())
}

#[test]
fn nothing_is_refused_when_no_checkout_can_be_found() -> Result<()> {
    let gone = tempfile::tempdir()?;
    let installed = Embedded {
        revision: "unknown".into(),
        root: gone.path().join("deleted"),
        hash: "0".repeat(64),
    };
    let elsewhere = tempfile::tempdir()?;
    assert!(locate(None, Some(elsewhere.path()), &installed.root).is_none());
    verify(Built::SkillOps, &installed, None, Some(elsewhere.path()))?;
    Ok(())
}

#[test]
fn the_inputs_cover_every_executable_the_tools_compile_in() -> Result<()> {
    let listed = inputs(&repository())?;
    for built in Built::ALL {
        assert!(
            listed.contains(&executable(built).to_owned()),
            "{} is not a build input",
            built.binary()
        );
    }
    assert!(listed.contains(&LIBRARY.to_owned()));
    assert!(listed.contains(&"crates/platform/build.rs".to_owned()));
    let mut sorted = listed.clone();
    sorted.sort();
    assert_eq!(listed, sorted);
    Ok(())
}

#[test]
fn every_local_dependency_of_an_installed_tool_is_a_build_input() -> Result<()> {
    let scope = tempfile::tempdir()?;
    let root = repository();
    let output = fixture_process::command("cargo", scope.path())
        .current_dir(&root)
        .args([
            "metadata",
            "--offline",
            "--no-deps",
            "--format-version",
            "1",
        ])
        .output()?;
    assert!(output.status.success());
    let metadata: serde_json::Value = serde_json::from_slice(&output.stdout)?;
    let packages = metadata["packages"].as_array().unwrap();
    let listed = inputs(&root)?;
    let mut pending = vec!["coderabbit-guard", "pr-workflow", "skill-ops"];
    let mut visited = BTreeSet::new();
    while let Some(name) = pending.pop() {
        if !visited.insert(name) {
            continue;
        }
        let package = packages
            .iter()
            .find(|package| package["name"] == name)
            .unwrap();
        let manifest = Path::new(package["manifest_path"].as_str().unwrap());
        let directory = manifest.parent().unwrap();
        for source in rust_files(directory.join("src"))? {
            let source = source
                .strip_prefix(&root)?
                .to_string_lossy()
                .replace('\\', "/");
            assert!(
                listed.contains(&source),
                "{name} compiles {source} outside its freshness stamp"
            );
        }
        for dependency in package["dependencies"].as_array().unwrap() {
            if dependency["kind"].is_null() && dependency["path"].is_string() {
                pending.push(dependency["name"].as_str().unwrap());
            }
        }
    }
    assert!(visited.contains("dotguard"));
    Ok(())
}

#[test]
fn a_checkout_converted_between_line_endings_hashes_the_same() -> Result<()> {
    let checkout = fixture("a\nb\n")?;
    let unix = content_hash(checkout.path())?;
    fs::write(checkout.path().join(LIBRARY), "a\r\nb\r\n")?;
    assert_eq!(unix, content_hash(checkout.path())?);
    fs::write(checkout.path().join(LIBRARY), "a\rb\n")?;
    assert_ne!(unix, content_hash(checkout.path())?);
    Ok(())
}

#[test]
fn every_embedded_file_is_a_build_input() -> Result<()> {
    let root = repository();
    let listed = inputs(&root)?;
    let mut checked = 0;
    let mut missing = Vec::new();
    let mut pending = tool_sources(&root)?;
    let mut visited = BTreeSet::new();
    while let Some(source) = pending.pop() {
        if !visited.insert(source.clone()) {
            continue;
        }
        let text = fs::read_to_string(&source)?;
        for line in text.lines().map(str::trim_start) {
            if line.starts_with("//") {
                continue;
            }
            for marker in ["include_str!(\"", "include_bytes!(\"", "#[path = \""] {
                let Some((_, rest)) = line.split_once(marker) else {
                    continue;
                };
                let literal = rest.split('"').next().expect("a closed literal");
                let resolved = normalize(
                    &source
                        .parent()
                        .expect("a file has a directory")
                        .join(literal),
                );
                if resolved
                    .extension()
                    .is_some_and(|extension| extension == "rs")
                {
                    pending.push(resolved.clone());
                }
                let resolved = resolved
                    .strip_prefix(&root)?
                    .to_string_lossy()
                    .replace('\\', "/");
                if !listed.contains(&resolved) {
                    missing.push(format!("{} compiles in {resolved}", source.display()));
                }
                checked += 1;
            }
        }
    }
    assert!(checked > 0, "no embedded files were found to check");
    assert!(
        missing.is_empty(),
        "embedded files outside build_inputs::SHARED:\n{}",
        missing.join("\n")
    );
    Ok(())
}

#[test]
fn no_installer_reads_the_default_chezmoi_source() -> Result<()> {
    let root = repository();
    let mut offenders = Vec::new();
    let mut sources = rust_files(root.join("xtask/src"))?;
    sources.extend(tool_sources(&root)?);
    for path in sources {
        let name = path
            .file_name()
            .expect("a file name")
            .to_string_lossy()
            .into_owned();
        let text = fs::read_to_string(&path)?;
        // Only the registry builds chezmoi commands, and each one names the invoking checkout.
        if !matches!(name.as_str(), "tool.rs" | "terminal.rs" | "freshness.rs")
            && (text.contains("Tool::Chezmoi.command")
                || text.contains("Tool::Chezmoi\n")
                || text.contains("source-path ~/"))
        {
            offenders.push(name);
        }
    }
    assert!(
        offenders.is_empty(),
        "build chezmoi commands with Tool::chezmoi_in(checkout): {offenders:?}"
    );
    Ok(())
}

#[test]
fn a_recorded_installation_is_rebuilt_only_after_its_sources_change() -> Result<()> {
    let home = tempfile::tempdir()?;
    enable_skill_ops(home.path())?;
    let checkout = fixture("pub fn rule() {}")?;
    assert_eq!(lagging(home.path(), checkout.path())?, Vec::<Built>::new());
    record_fixture(home.path(), checkout.path(), Built::SkillOps)?;
    assert_eq!(lagging(home.path(), checkout.path())?, Vec::<Built>::new());
    fs::write(checkout.path().join(LIBRARY), "pub fn rule() { 2; }")?;
    assert_eq!(
        lagging(home.path(), checkout.path())?,
        vec![Built::SkillOps]
    );
    record_fixture(home.path(), checkout.path(), Built::SkillOps)?;
    assert_eq!(lagging(home.path(), checkout.path())?, Vec::<Built>::new());
    Ok(())
}

#[test]
fn an_unrelated_checkout_never_takes_over_the_primary_installation() -> Result<()> {
    let home = tempfile::tempdir()?;
    enable_skill_ops(home.path())?;
    let primary = fixture("pub fn current() {}")?;
    let secondary = fixture("pub fn current() {}")?;
    record_fixture(home.path(), primary.path(), Built::SkillOps)?;
    let record = home
        .path()
        .join(".local/state/dotfiles/installed/skill-ops");
    let original = fs::read(&record)?;
    fs::write(secondary.path().join(LIBRARY), "pub fn secondary() {}")?;
    assert_eq!(lagging(home.path(), secondary.path())?, Vec::<Built>::new());
    platform::freshness::reinstall_lagging(home.path(), secondary.path())?;
    assert_eq!(fs::read(&record)?, original);
    fs::write(primary.path().join(LIBRARY), "pub fn primary_changed() {}")?;
    assert_eq!(lagging(home.path(), secondary.path())?, Vec::<Built>::new());
    assert_eq!(lagging(home.path(), primary.path())?, [Built::SkillOps]);
    record_fixture(home.path(), secondary.path(), Built::SkillOps)?;
    assert_eq!(lagging(home.path(), primary.path())?, Vec::<Built>::new());
    fs::write(
        secondary.path().join(LIBRARY),
        "pub fn secondary_changed() {}",
    )?;
    assert_eq!(lagging(home.path(), secondary.path())?, [Built::SkillOps]);
    Ok(())
}

#[test]
fn unbound_or_moved_records_require_explicit_installation_without_guessing_origin() -> Result<()> {
    let home = tempfile::tempdir().context("create installation home")?;
    enable_skill_ops(home.path()).context("enable skill-ops in the installation home")?;
    let checkout = fixture("pub fn current() {}").context("create source checkout")?;
    let directory = home.path().join(".local/state/dotfiles/installed");
    fs::create_dir_all(&directory)
        .with_context(|| format!("create installation records in {}", directory.display()))?;
    let path = directory.join(Built::SkillOps.binary());
    for invalid in [content_hash(checkout.path())?, "malformed".into()] {
        fs::write(&path, invalid)
            .with_context(|| format!("write invalid installation record {}", path.display()))?;
        let error = lagging(home.path(), checkout.path()).unwrap_err();
        assert_eq!(
            error.downcast_ref::<Refusal>().unwrap().rule,
            "tool.installation-origin"
        );
    }
    record_fixture(home.path(), checkout.path(), Built::SkillOps)
        .context("record the source checkout before moving it")?;
    let moved = tempfile::tempdir().context("create checkout relocation parent")?;
    let relocated = moved.path().join("relocated");
    fs::rename(checkout.path(), &relocated).with_context(|| {
        format!(
            "move source checkout {} to {}",
            checkout.path().display(),
            relocated.display()
        )
    })?;
    let error = lagging(home.path(), &relocated).unwrap_err();
    assert_eq!(
        error.downcast_ref::<Refusal>().unwrap().rule,
        "tool.installation-origin"
    );
    fs::remove_file(&path)
        .with_context(|| format!("remove installation record {}", path.display()))?;
    assert_eq!(lagging(home.path(), &relocated)?, Vec::<Built>::new());
    Ok(())
}

#[test]
fn artifact_readback_binds_installation_to_the_actual_build_instead_of_caller_checkout()
-> Result<()> {
    let home = tempfile::tempdir()?;
    let source = repository();
    let artifact = Path::new(env!("CARGO_BIN_EXE_pr-workflow"));
    let stamp = verify_artifact(&source, artifact, Built::PrWorkflow)?;
    assert_eq!(stamp.root, platform::paths::canonical(&source)?);
    let installed = Built::PrWorkflow.installed_binary(home.path());
    fs::create_dir_all(installed.parent().unwrap())?;
    fs::copy(artifact, installed)?;
    record_installation(home.path(), &source, Built::PrWorkflow)?;
    let record = home
        .path()
        .join(".local/state/dotfiles/installed/pr-workflow");
    let original = fs::read(&record)?;
    let foreign = fixture("pub fn unrelated() {}")?;
    assert!(verify_artifact(foreign.path(), artifact, Built::PrWorkflow).is_err());
    assert!(record_installation(home.path(), foreign.path(), Built::PrWorkflow).is_err());
    assert_eq!(fs::read(&record)?, original);
    assert_eq!(lagging(home.path(), foreign.path())?, Vec::<Built>::new());
    Ok(())
}

#[test]
fn reserved_metadata_is_read_only_and_does_not_grant_freshness() -> Result<()> {
    let scope = tempfile::tempdir()?;
    let foreign = fixture("pub fn unrelated() {}")?;
    let program = env!("CARGO_BIN_EXE_pr-workflow");
    let metadata = fixture_process::command(program, scope.path())
        .arg(BUILD_IDENTITY_ARGUMENT)
        .env(SOURCE_VARIABLE, foreign.path())
        .output()?;
    assert!(metadata.status.success());
    let stamp: Embedded = serde_json::from_slice(&metadata.stdout)?;
    assert_eq!(stamp, embedded());
    let normal = fixture_process::command(program, scope.path())
        .arg("--help")
        .env(SOURCE_VARIABLE, foreign.path())
        .output()?;
    assert!(!normal.status.success());
    let extra = fixture_process::command(program, scope.path())
        .args([BUILD_IDENTITY_ARGUMENT, "unexpected"])
        .env(SOURCE_VARIABLE, foreign.path())
        .output()?;
    assert!(!extra.status.success());
    Ok(())
}

#[test]
fn a_build_handed_a_run_by_a_stale_working_copy_never_hands_it_on_again() -> Result<()> {
    let scope = tempfile::tempdir()?;
    let newer = fixture("pub fn rule() { /* merged after the build */ }")?;
    let handed = fixture_process::command(env!("CARGO_BIN_EXE_pr-workflow"), scope.path())
        .arg("--help")
        .current_dir(newer.path())
        .env_remove(SOURCE_VARIABLE)
        .env(DELEGATED_VARIABLE, "1")
        .output()?;
    assert!(!handed.status.success());
    let stderr = String::from_utf8_lossy(&handed.stderr);
    let refusal = Refusal::find(&stderr).expect("the handed run emits a structured refusal");
    assert_eq!(refusal.rule, "tool.stale", "{stderr}");
    assert!(
        stderr.contains("the checkout this command runs in"),
        "{stderr}"
    );
    Ok(())
}

#[test]
fn only_a_stale_working_copy_hands_the_run_to_its_own_build() {
    let stale = freshness(true, false);
    assert!(delegates(stale, Origin::WorkingCopy, false));
    assert!(!delegates(stale, Origin::WorkingCopy, true));
    assert!(!delegates(stale, Origin::Explicit, false));
    assert!(!delegates(stale, Origin::BuildCheckout, false));
    assert!(!delegates(
        freshness(true, true),
        Origin::WorkingCopy,
        false
    ));
    assert_eq!(Built::PrWorkflow.package(), "pr-workflow");
    assert_eq!(Built::Coderabbit.package(), "coderabbit-guard");
}

#[test]
fn the_installed_binary_refuses_stale_sources_and_runs_current_ones() -> Result<()> {
    let scope = tempfile::tempdir()?;
    let program = env!("CARGO_BIN_EXE_pr-workflow");
    let current = fixture_process::command(program, scope.path())
        .arg("--help")
        .env(SOURCE_VARIABLE, repository())
        .output()?;
    assert!(
        current.status.success(),
        "a current build runs: {}",
        String::from_utf8_lossy(&current.stderr)
    );
    let newer = fixture("pub fn rule() { /* merged after the build */ }")?;
    let stale = fixture_process::command(program, scope.path())
        .arg("--help")
        .env(SOURCE_VARIABLE, newer.path())
        .output()?;
    assert!(!stale.status.success());
    let stderr = String::from_utf8_lossy(&stale.stderr);
    assert!(stderr.contains("tool.stale"), "{stderr}");
    assert!(stderr.contains("just install-pr-workflow"), "{stderr}");
    let refusal = Refusal::find(&stderr).expect("the stale binary emits a structured refusal");
    println!("\n{}", refusal.line());
    assert_eq!(embedded().hash.len(), 64, "the build embeds a sha-256 hash");
    Ok(())
}
