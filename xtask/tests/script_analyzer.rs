//! Every PowerShell gate and the Windows setup load one pinned `PSScriptAnalyzer` release, so no copy of its version can drift.

use std::fs;
use std::path::{Path, PathBuf};

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .to_path_buf()
}

fn sources(directory: &Path, found: &mut Vec<PathBuf>) {
    for entry in fs::read_dir(directory).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            if path.file_name().is_none_or(|name| name != "target") {
                sources(&path, found);
            }
        } else if path
            .extension()
            .is_some_and(|extension| matches!(extension.to_str(), Some("rs" | "ps1" | "toml")))
        {
            found.push(path);
        }
    }
}

#[test]
fn only_the_pin_names_the_analyzer_version() {
    let root = root();
    let pin = root.join("crates/platform/src/script_analyzer.rs");
    let version = platform::script_analyzer::VERSION;
    let mut found = Vec::new();
    for directory in ["crates", "xtask", "home", "policy"] {
        sources(&root.join(directory), &mut found);
    }
    let drifted: Vec<String> = found
        .iter()
        .filter(|path| **path != pin)
        .filter(|path| {
            let text = fs::read_to_string(path).unwrap_or_default();
            text.lines()
                .any(|line| line.contains("PSScriptAnalyzer") && line.contains(version))
        })
        .map(|path| path.display().to_string())
        .collect();
    assert!(
        drifted.is_empty(),
        "name platform::script_analyzer::VERSION instead: {drifted:?}"
    );
}

#[test]
fn a_lint_probe_that_draws_no_finding_fails_and_one_that_does_passes() {
    let root = root();
    let manifest = |probe: &str| {
        xtask::obligations::parse_manifest(&format!(
            "[[obligation]]\nverifier = \"lint\"\ntool = \"shellcheck\"\nsettings = \"policy/shellcheckrc\"\nprobe = '''\n{probe}'''\nfinding = \"SC2086\"\n"
        ))
        .unwrap()
    };
    let quoted = xtask::quality::lint_probes(&root, &manifest("#!/bin/sh\necho \"$1\"\n")).unwrap();
    assert_eq!(quoted.len(), 1, "{quoted:?}");
    let unquoted = xtask::quality::lint_probes(&root, &manifest("#!/bin/sh\necho $1\n")).unwrap();
    assert!(unquoted.is_empty(), "{unquoted:?}");
    assert_eq!(
        xtask::quality::shellcheck_findings(&root, ".bashrc", "echo $1\n").unwrap(),
        ["SC2086"],
        "a startup file without a shebang is checked as its shell"
    );
}

#[test]
fn the_unpinned_gallery_helper_never_installs_the_analyzer() {
    let tools =
        fs::read_to_string(root().join("crates/dotfiles/src/windows/setup/tools.rs")).unwrap();
    let modules = tools
        .lines()
        .find(|line| line.starts_with("const PS_MODULES"))
        .unwrap();
    assert!(
        !modules.contains(platform::script_analyzer::MODULE),
        "{modules}"
    );
    assert!(tools.contains("platform::script_analyzer::save"));
    assert!(tools.contains("platform::script_analyzer::verified"));
}
