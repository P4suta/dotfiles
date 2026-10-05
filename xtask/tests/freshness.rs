use anyhow::Result;
use dotfiles_xtask::build_inputs::{content_hash, inputs};
use dotfiles_xtask::freshness::{
    Built, Embedded, SOURCE_VARIABLE, embedded, lagging, locate, record_installation, verify,
};
use dotfiles_xtask::refusal::Refusal;
use dotfiles_xtask::stale_rules::{Freshness, Origin, freshness, origin, reinstalls, runs};
use dotfiles_xtask::tool::external;
use std::fs;
use std::path::{Component, Path, PathBuf};

fn repository() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("xtask sits inside its checkout")
        .to_path_buf()
}

/// A checkout holding only some of the files the tools are built from, enough to hash.
fn fixture(source: &str) -> Result<tempfile::TempDir> {
    let root = tempfile::tempdir()?;
    fs::create_dir_all(root.path().join("xtask/src/bin"))?;
    fs::write(
        root.path().join("xtask/Cargo.toml"),
        "[package]\nname = \"dotfiles-xtask\"\n",
    )?;
    fs::write(root.path().join("xtask/src/lib.rs"), source)?;
    fs::write(
        root.path().join("xtask/src/bin/skill-ops.rs"),
        "fn main() {}",
    )?;
    Ok(root)
}

fn built_from(root: &Path) -> Result<Embedded> {
    Ok(Embedded {
        revision: "0123456789abcdef".into(),
        root: root.to_path_buf(),
        hash: content_hash(root)?,
    })
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
    assert!(reinstalls(true, false));
    assert!(!reinstalls(true, true));
    assert!(!reinstalls(false, false));
}

#[test]
fn a_fixture_installation_older_than_its_sources_is_refused_with_the_install_action() -> Result<()>
{
    for built in Built::ALL {
        let checkout = fixture("pub fn rule() {}")?;
        let installed = built_from(checkout.path())?;
        verify(built, &installed, None, Some(checkout.path()))?;
        fs::write(
            checkout.path().join("xtask/src/lib.rs"),
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
            Some(format!("mise run {}", built.install_task()).as_str())
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
    let nested = newer.path().join("xtask/src");
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
fn the_inputs_cover_every_executable_the_library_compiles_in() -> Result<()> {
    let listed = inputs(&repository())?;
    for binary in Built::ALL {
        assert!(
            listed.contains(&format!("xtask/src/bin/{}.rs", binary.binary())),
            "{} is not a build input",
            binary.binary()
        );
    }
    assert!(listed.contains(&"xtask/src/lib.rs".to_owned()));
    assert!(listed.contains(&"xtask/build.rs".to_owned()));
    let mut sorted = listed.clone();
    sorted.sort();
    assert_eq!(listed, sorted);
    Ok(())
}

#[test]
fn a_checkout_converted_between_line_endings_hashes_the_same() -> Result<()> {
    let checkout = fixture("a\nb\n")?;
    let unix = content_hash(checkout.path())?;
    fs::write(checkout.path().join("xtask/src/lib.rs"), "a\r\nb\r\n")?;
    assert_eq!(unix, content_hash(checkout.path())?);
    fs::write(checkout.path().join("xtask/src/lib.rs"), "a\rb\n")?;
    assert_ne!(unix, content_hash(checkout.path())?);
    Ok(())
}

#[test]
fn every_embedded_file_is_a_build_input() -> Result<()> {
    let root = repository();
    let listed = inputs(&root)?;
    let mut checked = 0;
    for source in rust_files(root.join("xtask/src"))? {
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
                let resolved = resolved
                    .strip_prefix(&root)?
                    .to_string_lossy()
                    .replace('\\', "/");
                assert!(
                    listed.contains(&resolved),
                    "{} compiles in {resolved}, which the installed tools are not rebuilt from; add it to build_inputs::SHARED",
                    source.display()
                );
                checked += 1;
            }
        }
    }
    assert!(checked > 0, "no embedded files were found to check");
    Ok(())
}

#[test]
fn no_installer_reads_the_default_chezmoi_source() -> Result<()> {
    let mut offenders = Vec::new();
    for path in rust_files(repository().join("xtask/src"))? {
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
    let checkout = fixture("pub fn rule() {}")?;
    assert!(lagging(home.path(), checkout.path())?.is_empty());
    record_installation(home.path(), checkout.path(), Built::SkillOps)?;
    assert!(lagging(home.path(), checkout.path())?.is_empty());
    fs::write(
        checkout.path().join("xtask/src/lib.rs"),
        "pub fn rule() { 2; }",
    )?;
    assert_eq!(
        lagging(home.path(), checkout.path())?,
        vec![Built::SkillOps]
    );
    record_installation(home.path(), checkout.path(), Built::SkillOps)?;
    assert!(lagging(home.path(), checkout.path())?.is_empty());
    Ok(())
}

#[test]
fn the_installed_binary_refuses_stale_sources_and_runs_current_ones() -> Result<()> {
    let program = env!("CARGO_BIN_EXE_pr-workflow");
    let current = external(program)
        .arg("--help")
        .env(SOURCE_VARIABLE, repository())
        .output()?;
    assert!(
        current.status.success(),
        "a current build runs: {}",
        String::from_utf8_lossy(&current.stderr)
    );
    let newer = fixture("pub fn rule() { /* merged after the build */ }")?;
    let stale = external(program)
        .arg("--help")
        .env(SOURCE_VARIABLE, newer.path())
        .output()?;
    assert!(!stale.status.success());
    let stderr = String::from_utf8_lossy(&stale.stderr);
    assert!(stderr.contains("tool.stale"), "{stderr}");
    assert!(stderr.contains("mise run install:pr-workflow"), "{stderr}");
    assert_eq!(embedded().hash.len(), 64, "the build embeds a sha-256 hash");
    Ok(())
}
