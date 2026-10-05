//! The files the installed tools are built from, and one hash over them.
//!
//! The executables share one library whose `include_str!` calls compile every binary's source and the policy files into each of them, so one set of inputs builds all of them.
//! `build.rs` and the library compile this same file, so the hash a tool embeds when it is built and the hash of a checkout when it runs cannot disagree about which files count.

use sha2::{Digest, Sha256};
use std::fs;
use std::io;
use std::path::Path;

/// Files outside `xtask/src` that are compiled in or that decide how the tools build.
const SHARED: [&str; 14] = [
    ".chezmoiignore",
    ".github/workflows/required.yml",
    "bun.lock",
    "dot_agents/skills/pull-request/assets/workflow-policy.json",
    "dot_config/opencode/plugins/skill-ops.ts",
    "guard/src/refusal.rs",
    "lefthook.yml",
    "mise.toml",
    "package.json",
    "tsconfig.json",
    "xtask/Cargo.lock",
    "xtask/Cargo.toml",
    "xtask/build.rs",
    "xtask/proofs/Dockerfile",
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
pub fn inputs(root: &Path) -> io::Result<Vec<String>> {
    let mut found: Vec<String> = SHARED.map(String::from).into();
    collect(root, "xtask/src", &mut found)?;
    found.sort();
    Ok(found)
}

/// A hash over every input's path and content with line endings normalized, so a checkout converted between LF and CRLF hashes the same.
/// A missing input hashes as absent, which differs from every present file.
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
    Ok(hasher
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect())
}
