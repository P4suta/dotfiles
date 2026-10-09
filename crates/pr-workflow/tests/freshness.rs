use anyhow::Result;
use platform::build_inputs::{CRATES, content_hash, inputs};
use platform::freshness::{
    Built, Embedded, SOURCE_VARIABLE, embedded, lagging, locate, record_installation, verify,
};
use platform::refusal::Refusal;
use platform::stale_rules::{Freshness, Origin, freshness, origin, reinstalls, runs};
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
    for source in tool_sources(&root)? {
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
    let checkout = fixture("pub fn rule() {}")?;
    assert_eq!(lagging(home.path(), checkout.path())?, Vec::<Built>::new());
    record_installation(home.path(), checkout.path(), Built::SkillOps)?;
    assert_eq!(lagging(home.path(), checkout.path())?, Vec::<Built>::new());
    fs::write(checkout.path().join(LIBRARY), "pub fn rule() { 2; }")?;
    assert_eq!(
        lagging(home.path(), checkout.path())?,
        vec![Built::SkillOps]
    );
    record_installation(home.path(), checkout.path(), Built::SkillOps)?;
    assert_eq!(lagging(home.path(), checkout.path())?, Vec::<Built>::new());
    Ok(())
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
    assert_eq!(embedded().hash.len(), 64, "the build embeds a sha-256 hash");
    Ok(())
}
