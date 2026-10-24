use anyhow::{Result, ensure};
use platform::source_identity::{self, Repository};
use std::fs;
use std::path::Path;

#[path = "../../dotguard/tests/support/fixture_git.rs"]
mod fixture_git;

const CHILD_ROOT: &str = "DOTFILES_SOURCE_IDENTITY_FIXTURE";
const UPSTREAM: &str = "upstream/project";
const FORK: &str = "contributor/project";

fn git(root: &Path, global: &Path, arguments: &[&str]) -> Result<String> {
    let output = fixture_git::command("git", global)
        .current_dir(root)
        .args(arguments)
        .output()?;
    ensure!(
        output.status.success(),
        "fixture Git {arguments:?}: {}\n{}\n{}",
        output.status,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    Ok(String::from_utf8(output.stdout)?.trim().to_owned())
}

fn metadata(repository: &str, network: Option<u64>) -> Result<Repository> {
    let (id, owner_id) = if repository == UPSTREAM {
        (1, 10)
    } else {
        (2, 20)
    };
    let owner = repository.split('/').next().unwrap();
    let source = network.map(|id| serde_json::json!({"id":id,"full_name":UPSTREAM}));
    Ok(serde_json::from_value(serde_json::json!({
        "id":id,"full_name":repository,"owner":{"id":owner_id,"login":owner},
        "fork":repository != UPSTREAM,"source":source
    }))?)
}

#[test]
fn remote_identity_binding_requires_no_contributor_checkout() -> Result<()> {
    let mut reads = Vec::new();
    let binding = source_identity::bind(FORK, UPSTREAM, |repository| {
        reads.push(repository.to_owned());
        metadata(repository, (repository != UPSTREAM).then_some(1))
    })?;
    assert_eq!(reads, [UPSTREAM, FORK]);
    assert_eq!(binding.source(), FORK);
    assert_eq!(binding.source_id(), 2);
    assert_eq!(binding.target(), UPSTREAM);
    assert_eq!(binding.target_id(), 1);
    assert!(!binding.same_repository());
    let same = source_identity::bind(UPSTREAM, UPSTREAM, |repository| metadata(repository, None))?;
    assert!(same.same_repository());
    assert_eq!(same.source_id(), same.target_id());
    for network in [None, Some(0), Some(2), Some(3)] {
        assert!(
            source_identity::bind(FORK, UPSTREAM, |repository| {
                metadata(
                    repository,
                    (repository != UPSTREAM).then_some(network).flatten(),
                )
            })
            .is_err()
        );
    }
    for (source, target) in [("../project", UPSTREAM), (FORK, "upstream/project/extra")] {
        assert!(
            source_identity::bind(source, target, |_| {
                panic!("invalid repository name must stop before API reads")
            })
            .is_err()
        );
    }
    Ok(())
}

fn verify_source(root: &Path) -> Result<()> {
    let mut reads = Vec::new();
    let checked = source_identity::capture(root, UPSTREAM, |repository| {
        reads.push(repository.to_owned());
        metadata(repository, (repository != UPSTREAM).then_some(1))
    })?;
    assert_eq!(reads, [UPSTREAM, FORK]);
    assert_eq!(checked.binding().source(), FORK);
    assert_eq!(checked.binding().target(), UPSTREAM);
    assert!(!checked.binding().same_repository());
    assert_eq!(
        source_identity::head_branch(root, checked.binding(), "contributor:feature")?,
        "feature"
    );
    assert!(source_identity::head_branch(root, checked.binding(), "unrelated:feature").is_err());
    assert!(
        source_identity::head_branch(root, checked.binding(), "contributor:feature:other").is_err()
    );
    for network in [None, Some(0), Some(3)] {
        assert!(
            source_identity::capture(root, UPSTREAM, |repository| {
                metadata(
                    repository,
                    (repository != UPSTREAM).then_some(network).flatten(),
                )
            })
            .is_err()
        );
    }
    assert!(
        source_identity::capture(root, UPSTREAM, |repository| {
            let mut metadata = metadata(repository, (repository != UPSTREAM).then_some(1))?;
            if repository == FORK {
                metadata.id = 0;
            }
            Ok(metadata)
        })
        .is_err()
    );
    assert!(
        source_identity::capture(root, UPSTREAM, |repository| {
            let mut metadata = metadata(repository, (repository != UPSTREAM).then_some(1))?;
            if repository == FORK {
                metadata.full_name = "unrelated/project".into();
            }
            Ok(metadata)
        })
        .is_err()
    );
    let mut reads = 0;
    let same = source_identity::capture(root, FORK, |repository| {
        reads += 1;
        metadata(repository, None)
    })?;
    assert!(same.binding().same_repository());
    assert_eq!(reads, 1);
    let global = root.parent().unwrap().join("gitconfig");
    git(
        root,
        &global,
        &[
            "remote",
            "set-url",
            "origin",
            "https://github.com/unrelated/project.git",
        ],
    )?;
    assert!(checked.require_current().is_err());
    git(
        root,
        &global,
        &[
            "remote",
            "set-url",
            "origin",
            &format!("https://github.com/{FORK}.git"),
        ],
    )?;
    Ok(())
}

#[test]
fn real_checkout_identity_accepts_only_verified_source_target_networks() -> Result<()> {
    if let Some(root) = std::env::var_os(CHILD_ROOT) {
        return verify_source(Path::new(&root));
    }
    let scope = tempfile::tempdir()?;
    let global = scope.path().join("gitconfig");
    fs::write(
        &global,
        "[user]\n name = Fixture\n email = fixture@example.com\n[commit]\n gpgsign = false\n",
    )?;
    let root = scope.path().join("fork");
    fs::create_dir(&root)?;
    git(
        &root,
        &global,
        &["init", "--quiet", "--initial-branch=feature"],
    )?;
    fs::write(
        root.join("source.txt"),
        "the same source can be copied into an unrelated repository",
    )?;
    git(&root, &global, &["add", "source.txt"])?;
    let tree = git(&root, &global, &["write-tree"])?;
    let head = git(&root, &global, &["commit-tree", &tree, "-m", "source"])?;
    git(&root, &global, &["update-ref", "HEAD", &head])?;
    git(
        &root,
        &global,
        &[
            "remote",
            "add",
            "origin",
            &format!("https://github.com/{FORK}.git"),
        ],
    )?;
    let output = fixture_git::command(std::env::current_exe()?, &global)
        .args([
            "--exact",
            "real_checkout_identity_accepts_only_verified_source_target_networks",
            "--nocapture",
        ])
        .env(CHILD_ROOT, &root)
        .output()?;
    ensure!(
        output.status.success(),
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(git(&root, &global, &["rev-parse", "HEAD"])?, head);
    assert_eq!(
        git(&root, &global, &["remote", "get-url", "origin"])?,
        format!("https://github.com/{FORK}.git")
    );
    Ok(())
}
