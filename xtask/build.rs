//! Embeds the revision, checkout, and content hash the installed tools are built from, so a tool can notice that its sources moved on.

#[path = "src/build_inputs.rs"]
mod build_inputs;

use build_inputs::{content_hash, inputs};
use std::path::PathBuf;

/// The checkout's commit, or `unknown` where Git or the history is absent, such as a source copy in a container.
#[allow(clippy::disallowed_methods)]
fn revision(root: &std::path::Path) -> String {
    std::process::Command::new("git")
        .arg("-C")
        .arg(root)
        .args(["rev-parse", "HEAD"])
        .output()
        .ok()
        .filter(|output| output.status.success())
        .and_then(|output| String::from_utf8(output.stdout).ok())
        .map_or_else(|| "unknown".into(), |text| text.trim().to_owned())
}

fn main() {
    let manifest = PathBuf::from(
        std::env::var_os("CARGO_MANIFEST_DIR").expect("cargo sets the manifest directory"),
    );
    let root = manifest.parent().expect("xtask sits inside its checkout");
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=src");
    for path in inputs(root).expect("the build inputs are readable") {
        println!("cargo:rerun-if-changed=../{path}");
    }
    println!("cargo:rustc-env=DOTFILES_BUILD_REVISION={}", revision(root));
    println!("cargo:rustc-env=DOTFILES_BUILD_ROOT={}", root.display());
    println!(
        "cargo:rustc-env=DOTFILES_BUILD_HASH={}",
        content_hash(root).expect("the build inputs are readable")
    );
}
