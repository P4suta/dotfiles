//! Embeds the revision, checkout, and content hash of the sources that produce each installed tool.
//! A tool can then notice that its sources changed.
//! The Git guard builds without the tools feature and embeds nothing.

#[cfg(feature = "tools")]
#[path = "src/build_inputs.rs"]
mod build_inputs;

/// The commit of the checkout, or `unknown` when the environment lacks Git or the history, such as in a source copy in a container.
#[cfg(feature = "tools")]
#[expect(
    clippy::disallowed_methods,
    reason = "a build script cannot use the crate's own process constructor"
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
    println!("cargo:rerun-if-changed=build.rs");
    #[cfg(feature = "tools")]
    {
        let manifest = std::path::PathBuf::from(
            std::env::var_os("CARGO_MANIFEST_DIR").expect("cargo sets the manifest directory"),
        );
        let root = manifest
            .ancestors()
            .nth(2)
            .expect("the platform crate sits two levels inside its checkout");
        for krate in build_inputs::CRATES {
            println!(
                "cargo:rerun-if-changed={}",
                root.join(krate).join("src").display()
            );
        }
        for path in build_inputs::inputs(root).expect("the build inputs are readable") {
            println!("cargo:rerun-if-changed={}", root.join(path).display());
        }
        println!("cargo:rustc-env=DOTFILES_BUILD_REVISION={}", revision(root));
        println!("cargo:rustc-env=DOTFILES_BUILD_ROOT={}", root.display());
        println!(
            "cargo:rustc-env=DOTFILES_BUILD_HASH={}",
            build_inputs::content_hash(root).expect("the build inputs are readable")
        );
    }
}
