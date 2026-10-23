//! The files that produce the installed tools, and one hash over them.
//!
//! The installed tools link the platform crate, which embeds one stamp over the sources of every crate they build from.
//! Their `include_str!` calls compile the policy files into each executable.
//! `build.rs` and the library compile this same file.
//! The hash a tool embeds at build time and the hash of a checkout at run time agree on which files count.

use sha2::{Digest, Sha256};
use std::fs;
use std::io;
use std::path::Path;

/// The crates whose sources build the installed tools.
pub const CRATES: [&str; 9] = [
    "crates/coderabbit-guard",
    "crates/dotfiles",
    "crates/dotguard",
    "crates/model",
    "crates/platform",
    "crates/pr-workflow",
    "crates/prose",
    "crates/rules",
    "crates/skill-ops",
];

/// Files outside the crates' `src` directories that the compiler includes or that decide how the tools build.
const SHARED: [&str; 48] = [
    ".cargo/config.toml",
    ".chezmoiroot",
    ".github/workflows/ci.yml",
    ".mise/locks/pypi-ansible-core/2.21.5/pyproject.toml",
    ".mise/locks/pypi-ansible-core/2.21.5/uv.lock",
    ".mise/locks/yamllint/1.38.0/pyproject.toml",
    ".mise/locks/yamllint/1.38.0/uv.lock",
    "Cargo.lock",
    "Cargo.toml",
    "bun.lock",
    "clippy.toml",
    "crates/dotfiles/assets/elevate.ps1",
    "crates/dotfiles/assets/psgallery.ps1",
    "crates/dotfiles/assets/repository/LICENSE-APACHE",
    "crates/dotfiles/assets/repository/LICENSE-MIT",
    "crates/dotfiles/assets/repository/README.template",
    "crates/dotfiles/assets/sshd-host.ps1",
    "crates/dotfiles/tests/fixtures/rust-src-nightly-library-cargo-lock.txt",
    "crates/dotguard/tests/fixtures/renovate-local-44.52.json",
    "crates/dotguard/tests/support/fixture_git.rs",
    "crates/platform/build.rs",
    "crates/platform/tests/support/fixture_process.rs",
    "crates/rules/clippy.toml",
    "fuzz/corpus/answer_plan/lexical",
    "home/.chezmoidata/modern.toml",
    "home/.chezmoidata/versions.toml",
    "home/.chezmoitemplates/partials/njutest-mise.toml",
    "home/.chezmoiignore",
    "home/dot_agents/skills/pull-request/assets/workflow-policy.json",
    "home/dot_config/dotfiles/bounded-tools.toml",
    "home/dot_config/opencode/plugins/skill-ops.ts",
    "home/dot_config/mise/config.njutest.toml.tmpl",
    "lefthook.yml",
    "mise.lock",
    "mise.toml",
    "mise.njutest.toml",
    "package.json",
    "policy/obligations.toml",
    "specs/lean/lean-toolchain",
    "tsconfig.json",
    "xtask/proofs/Dockerfile",
    "xtask/schema/njutest/manifest.json",
    "xtask/schema/njutest/njutest-assurance-report-v1.json",
    "xtask/schema/njutest/rust-mutants-catalog-v1.json",
    "xtask/schema/njutest/rust-mutants-run-report-v1.json",
    "xtask/src/njutest_assessment.rs",
    "xtask/src/njutest_producer.rs",
    "xtask/src/skill_proofs.rs",
];

fn collect(root: &Path, relative: &str, found: &mut Vec<String>) -> io::Result<()> {
    for entry in fs::read_dir(root.join(relative))? {
        let entry = entry?;
        let path = format!("{relative}/{}", entry.file_name().to_string_lossy());
        if entry.file_type()?.is_dir() {
            collect(root, &path, found)?;
        } else {
            found.push(path);
        }
    }
    Ok(())
}

/// The checkout-relative paths that build the tools, sorted and slash-separated on every host.
///
/// # Errors
///
/// Returns an error when it can't read the `src` directory of a crate in [`CRATES`], or one of its entries.
pub fn inputs(root: &Path) -> io::Result<Vec<String>> {
    let mut found: Vec<String> = SHARED.map(String::from).into();
    for krate in CRATES {
        found.push(format!("{krate}/Cargo.toml"));
        collect(root, &format!("{krate}/src"), &mut found)?;
    }
    found.sort();
    Ok(found)
}

/// A hash over every input's path and content with line endings normalized, so a checkout converted between LF and CRLF hashes the same.
/// A missing input hashes as absent, which differs from every present file.
///
/// # Errors
///
/// Returns an error when it can't list the inputs or can't read an input that exists.
pub fn content_hash(root: &Path) -> io::Result<String> {
    let mut hasher = Sha256::new();
    for path in inputs(root)? {
        hasher.update(path.as_bytes());
        match fs::read(root.join(&path)) {
            Ok(bytes) => {
                hasher.update(b"\0present\0");
                let mut previous = 0;
                for (index, byte) in bytes.iter().enumerate() {
                    if *byte == b'\r' && bytes.get(index + 1) == Some(&b'\n') {
                        hasher.update(&bytes[previous..index]);
                        previous = index + 1;
                    }
                }
                hasher.update(&bytes[previous..]);
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                hasher.update(b"\0absent\0");
            }
            Err(error) => return Err(error),
        }
        hasher.update(b"\0");
    }
    Ok(hex(&hasher.finalize()))
}

const HEX_RADIX: u8 = 16;

/// Lowercase hexadecimal, two digits per byte.
#[must_use]
pub fn hex(bytes: &[u8]) -> String {
    bytes
        .iter()
        .flat_map(|byte| [byte / HEX_RADIX, byte % HEX_RADIX])
        .filter_map(|digit| char::from_digit(u32::from(digit), u32::from(HEX_RADIX)))
        .collect()
}
