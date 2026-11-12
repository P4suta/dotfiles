use anyhow::{Result, ensure};
use platform::upstream::{GitSource, git_original};
use std::path::Path;

#[path = "../../dotguard/tests/support/fixture_git.rs"]
mod fixture_git;

fn git(root: &Path, arguments: &[&str]) -> Result<String> {
    let output = fixture_git::command("git", &root.join("global"))
        .current_dir(root)
        .args(arguments)
        .output()?;
    ensure!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    Ok(String::from_utf8(output.stdout)?.trim().to_owned())
}

#[test]
fn a_repository_root_baseline_covers_top_level_files_and_refuses_overlaps() -> Result<()> {
    let temporary = tempfile::tempdir()?;
    let root = temporary.path();
    git(root, &["init", "--quiet"])?;
    std::fs::create_dir(root.join("src"))?;
    std::fs::write(root.join("src/library.rs"), "const VALUE: u8 = 1;\n")?;
    let original = "# Domain setup\n\nPublisher prose. More publisher prose.\n";
    std::fs::write(root.join("README.md"), original)?;
    git(root, &["add", "README.md", "src/library.rs"])?;
    git(
        root,
        &[
            "-c",
            "user.name=Fixture",
            "-c",
            "user.email=fixture@example.com",
            "commit",
            "--quiet",
            "-m",
            "Fixture",
        ],
    )?;
    let source = GitSource {
        repository: platform::upstream::repository(root)?,
        directory: ".".into(),
        revision: git(root, &["rev-parse", "HEAD"])?,
        origin: "https://github.com/example/library".into(),
    };
    let home = root.join("home");
    platform::upstream::register_git_in(&home, root, source.clone())?;
    assert_eq!(
        git_original(root, &source, "README.md")?,
        Some(original.as_bytes().to_vec())
    );
    let registry = home.join(".config/dotfiles/upstream-git.json");
    let registered = std::fs::read(&registry)?;
    let mut child = source;
    child.directory = "src".into();
    assert!(platform::upstream::register_git_in(&home, root, child).is_err());
    assert_eq!(std::fs::read(registry)?, registered);
    Ok(())
}

#[test]
fn a_pinned_commit_preserves_original_bytes_after_a_local_edit() -> Result<()> {
    let temporary = tempfile::tempdir()?;
    let root = temporary.path();
    git(root, &["init", "--quiet"])?;
    std::fs::create_dir(root.join("src"))?;
    std::fs::write(
        root.join("src/library.cs"),
        "// Publisher prose. More publisher prose.\n",
    )?;
    git(root, &["add", "src/library.cs"])?;
    git(
        root,
        &[
            "-c",
            "user.name=Fixture",
            "-c",
            "user.email=fixture@example.com",
            "commit",
            "--quiet",
            "-m",
            "Fixture",
        ],
    )?;
    let source = GitSource {
        repository: platform::upstream::repository(root)?,
        directory: "src".into(),
        revision: git(root, &["rev-parse", "HEAD"])?,
        origin: "https://github.com/example/library".into(),
    };
    let home = root.join("home");
    let configuration = home.join(".config/dotfiles");
    std::fs::create_dir_all(&configuration)?;
    let existing = br#"{"version":1,"sources":[],"preserved":[]}"#;
    std::fs::write(configuration.join("upstream.json"), existing)?;
    platform::upstream::register_git_in(&home, root, source.clone())?;
    assert_eq!(
        std::fs::read(configuration.join("upstream.json"))?,
        existing
    );
    let registered = std::fs::read(configuration.join("upstream-git.json"))?;
    platform::upstream::parse_git(&registered)?;
    std::fs::write(
        root.join("src/library.cs"),
        "// A new authored violation. Another one.\n",
    )?;
    assert_eq!(
        git_original(root, &source, "src/library.cs")?,
        Some(b"// Publisher prose. More publisher prose.\n".to_vec())
    );
    assert_eq!(git_original(root, &source, "src/added.cs")?, None);
    assert_eq!(git_original(root, &source, "tests/test.cs")?, None);
    let mut invalid = source.clone();
    invalid.revision = "HEAD".into();
    assert!(git_original(root, &invalid, "src/library.cs").is_err());
    invalid.revision = "0".repeat(40);
    assert!(git_original(root, &invalid, "src/library.cs").is_err());
    invalid = source.clone();
    invalid.directory = "../src".into();
    assert!(git_original(root, &invalid, "src/library.cs").is_err());
    assert!(git_original(root, &source, "src/../other.cs").is_err());
    Ok(())
}
