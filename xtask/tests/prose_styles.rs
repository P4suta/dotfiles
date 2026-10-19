//! Every profile applies the pinned Vale style packages byte for byte.

use anyhow::Result;
use std::fs;
use std::path::{Path, PathBuf};

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("..")
}

#[test]
fn every_profile_applies_the_style_packages_with_their_pinned_digests() -> Result<()> {
    let lock: toml::Value = toml::from_str(&fs::read_to_string(
        root().join("home/dot_config/prose/packages.toml"),
    )?)?;
    let packages = lock["package"].as_array().expect("pinned packages");
    for profile in xtask::profile_rules::Profile::ALL {
        let scope = tempfile::tempdir()?;
        let home = scope.path().join("home");
        fs::create_dir(&home)?;
        let config = scope.path().join("chezmoi.toml");
        fs::write(
            &config,
            xtask::rehearsal::configuration(&root(), profile, &home)?,
        )?;
        let output = xtask::profiles::chezmoi(&root(), scope.path(), &config, &home)
            .args([
                "apply",
                "--force",
                "--include",
                "dirs,files",
                "--parent-dirs",
            ])
            .arg(home.join(".config/prose/styles"))
            .output()?;
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        for package in packages {
            let name = package["name"].as_str().expect("package name");
            assert_eq!(
                prose::tree_digest(&home.join(".config/prose/styles").join(name))?,
                package["tree_sha256"].as_str().expect("tree digest"),
                "{name} applied on {}",
                profile.name()
            );
        }
    }
    Ok(())
}
