//! Embeds the revision, checkout, and content hash of the sources that produce each installed tool.
//! A tool can then notice that its sources changed.

#[path = "src/build_inputs.rs"]
mod build_inputs;

use build_inputs::{content_hash, inputs};
use std::path::PathBuf;

/// The commit of the checkout, or `unknown` when the environment lacks Git or the history, such as in a source copy in a container.
#[expect(
    clippy::disallowed_methods,
    reason = "a build script cannot use the crate's tool registry"
)]
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
