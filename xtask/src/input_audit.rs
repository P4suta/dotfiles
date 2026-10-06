//! A setup script reruns when its rendered body changes, so its body may depend only on the step's own inputs.
//! Each script hashes those inputs through the `inputs` partial, and every profile must render the same scripts under two chezmoi command lines.

use crate::input_rules::{Verdict, terms, verdict};
use anyhow::{Result, ensure};
use std::fs;
use std::path::Path;

const CALL: &str = "includeTemplate \"inputs\"";

/// The findings in one script template: a hash outside the `inputs` partial, and each `inputs` argument that reads the host or the whole context.
#[must_use]
pub fn findings(name: &str, text: &str) -> Vec<String> {
    let mut found = Vec::new();
    if text.contains("sha256sum") {
        found.push(format!(
            "{name}: hashes outside the inputs partial; pass the step's inputs to {CALL}"
        ));
    }
    for (offset, _) in text.match_indices(CALL) {
        let rest = &text[offset + CALL.len()..];
        let argument = rest.split("}}").next().unwrap_or_default();
        let argument = argument.trim().trim_end_matches('-');
        match verdict(&terms(argument)) {
            Verdict::Admit => {}
            Verdict::HostState => found.push(format!(
                "{name}: inputs read the host or the command line in {argument}; hash source data or a rendered partial instead"
            )),
            Verdict::WholeContext => found.push(format!(
                "{name}: inputs hash the whole context in {argument}; name the step's own data"
            )),
        }
    }
    found
}

fn scripts(root: &Path) -> Result<Vec<(String, String)>> {
    let mut found = Vec::new();
    let source = crate::source_state(root);
    let mut stack = vec![
        source.join(".chezmoiscripts"),
        source.join(".chezmoitemplates/profiles"),
    ];
    while let Some(directory) = stack.pop() {
        for entry in fs::read_dir(&directory)? {
            let path = entry?.path();
            if path.is_dir() {
                stack.push(path);
                continue;
            }
            let name = path.file_name().unwrap_or_default().to_string_lossy();
            if name.starts_with("run_") {
                let relative = path
                    .strip_prefix(root)?
                    .to_string_lossy()
                    .replace('\\', "/");
                found.push((relative, fs::read_to_string(&path)?));
            }
        }
    }
    found.sort();
    Ok(found)
}

/// Refuses a script that hashes the host, the command line, or the whole context, and a profile whose scripts change with the command line.
///
/// # Errors
///
/// Returns an error when any such script or profile exists.
/// Also returns an error when reading the setup scripts or rendering the profiles fails.
pub fn check(root: &Path) -> Result<()> {
    let mut found = Vec::new();
    for (name, text) in scripts(root)? {
        found.extend(findings(&name, &text));
    }
    for script in crate::profile_snapshot::command_line_dependent(root)? {
        found.push(format!(
            "{script}: the body changes with the chezmoi command line, so every such change reruns it"
        ));
    }
    ensure!(
        found.is_empty(),
        "setup scripts must hash only their own inputs:\n{}",
        found.join("\n")
    );
    println!("Every setup script hashes only its own inputs");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn own_data_and_rendered_partials_pass() {
        let text = "#!/bin/sh\n{{ includeTemplate \"inputs\" (list (includeTemplate \"profiles/mac/dot_config/mise/config.toml.tmpl\" .) .tools) }}\nexec x\n";
        assert_eq!(findings("run_x.sh.tmpl", text), Vec::<String>::new());
    }

    #[test]
    fn a_whole_context_hash_and_a_host_read_are_named() {
        let text = "# configuration: {{ . | toJson | sha256sum }}\n{{ includeTemplate \"inputs\" (list .chezmoi.args) -}}\n{{ includeTemplate \"inputs\" (list . .ocaml) }}\n";
        let found = findings("run_x.sh.tmpl", text);
        assert_eq!(found.len(), 3, "{found:?}");
        assert!(found[0].contains("hashes outside the inputs partial"));
        assert!(found[1].contains("read the host"));
        assert!(found[2].contains("whole context"));
    }
}
