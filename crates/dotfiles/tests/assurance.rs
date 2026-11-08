use anyhow::{Context, Result, ensure};
use serde_json::json;
use std::{fs, path::Path, process::Command};

#[path = "support/mutation_inventory.rs"]
mod mutation_inventory;

#[path = "../../dotguard/tests/support/fixture_git.rs"]
mod fixture_git;

const FUZZ_INPUTS: u64 = 10_000;
const FUZZ_SEED: u64 = 657;
const MAX_INPUT_BYTES: usize = 8192;
const FUZZ_TOOL_VERSION: &str = "cargo-fuzz 0.13.2";
const OUTPUT: &str = "target/verification-assurance";
const ADAPTER_SHARDS: usize = 6;

fn run(command: &mut Command) -> Result<String> {
    let output = command.output()?;
    let text = format!(
        "{}\n{}",
        String::from_utf8(output.stdout)?,
        String::from_utf8(output.stderr)?
    );
    eprintln!("{text}");
    ensure!(
        output.status.success(),
        "assurance command failed: {}",
        output.status
    );
    Ok(text)
}

fn bounded(image: &str) -> Command {
    let mut command = platform::process::command("dotfiles");
    command.args([
        "bounded",
        "run",
        "--image",
        image,
        "--project",
        ".",
        "--",
        "env",
        "CARGO_BUILD_JOBS=1",
        "RUST_TEST_THREADS=1",
        "CARGO_PROFILE_TEST_DEBUG=0",
        "CARGO_PROFILE_TEST_DEBUG_ASSERTIONS=true",
        "CARGO_PROFILE_TEST_OVERFLOW_CHECKS=true",
    ]);
    command
        .env_remove("CARGO_TARGET_DIR")
        .env_remove("CARGO_BUILD_TARGET_DIR");
    command
}

fn git_output(arguments: &[&str]) -> Result<Vec<u8>> {
    let scope = tempfile::tempdir()?;
    let global = scope.path().join("gitconfig");
    fs::write(&global, [])?;
    let output = fixture_git::command("git", &global)
        .args(arguments)
        .output()?;
    ensure!(
        output.status.success(),
        "Git source capture failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    Ok(output.stdout)
}

fn source_baseline() -> Result<String> {
    let selected = match std::env::var("DOTFILES_MUTATION_BASE") {
        Ok(value) => value,
        Err(std::env::VarError::NotPresent) => "HEAD".to_owned(),
        Err(error) => return Err(error).context("mutation base must be UTF-8"),
    };
    let revision = String::from_utf8(git_output(&[
        "rev-parse",
        "--verify",
        "--end-of-options",
        &format!("{selected}^{{commit}}"),
    ])?)?;
    let revision = revision.trim();
    let paths = git_output(&[
        "ls-tree",
        "--name-only",
        revision,
        "--",
        mutation_inventory::SOURCE,
    ])?;
    let bytes = if paths.is_empty() {
        Vec::new()
    } else {
        git_output(&[
            "show",
            "--no-ext-diff",
            "--no-textconv",
            &format!("{revision}:{}", mutation_inventory::SOURCE),
        ])?
    };
    fs::write(Path::new(OUTPUT).join("source-before.rs"), &bytes)?;
    fs::write(
        Path::new(OUTPUT).join("source-before-commit.txt"),
        format!("{revision}\n"),
    )?;
    eprintln!("Mutation source baseline: {revision}");
    Ok(String::from_utf8(bytes)?)
}

fn capture_mutation_inventory(image: &str) -> Result<serde_json::Value> {
    let inventory = format!("{OUTPUT}/mutations-inventory.json");
    let mut listing = bounded(image);
    listing.args(["sh", "-c", "cargo mutants \"$@\" --list --json > \"$0\""]);
    listing.arg(format!("/work/{inventory}"));
    mutation_selection(&mut listing, false);
    run(&mut listing)?;
    let inventory = serde_json::from_slice(&fs::read(inventory)?)?;
    let source = fs::read_to_string(mutation_inventory::SOURCE)?;
    mutation_inventory::validate(&source, &inventory)?;
    mutation_inventory::validate_changed(&source_baseline()?, &source, &inventory)?;
    Ok(inventory)
}

fn mutations(image: &str, rules: bool) -> Result<()> {
    let name = if rules { "rule-mutations" } else { "mutations" };
    let expected = if rules {
        None
    } else {
        Some(capture_mutation_inventory(image)?)
    };
    let inventories =
        mutation_inventory::collect_shards(if rules { 1 } else { ADAPTER_SHARDS }, |shard| {
            let name = if rules {
                name.to_owned()
            } else {
                format!("{name}-shard-{shard}")
            };
            mutation_shard(image, rules, shard, &name)?;
            let inventory = Path::new(OUTPUT)
                .join(name)
                .join("mutants.out/mutants.json");
            Ok(serde_json::from_slice(&fs::read(inventory)?)?)
        })?;
    if let Some(expected) = expected {
        mutation_inventory::complete(&expected, &inventories)?;
    }
    Ok(())
}

fn mutation_shard(image: &str, rules: bool, shard: usize, name: &str) -> Result<()> {
    let output = format!("/work/{OUTPUT}/{name}");
    let mut command = bounded(image);
    command.args([
        "cargo",
        "mutants",
        "--no-shuffle",
        "--jobs",
        "1",
        "--timeout",
        "120",
        "--build-timeout",
        "600",
        "--output",
        &output,
    ]);
    if !rules {
        command.args([
            "--shard",
            &format!("{shard}/{ADAPTER_SHARDS}"),
            "--sharding",
            "round-robin",
        ]);
    }
    mutation_selection(&mut command, rules);
    command.args([
        "--",
        if rules {
            "verification_rules::"
        } else {
            "verification::"
        },
    ]);
    let result = run(&mut command);
    validate_mutation_tests(Path::new(OUTPUT).join(name).as_path(), rules)?;
    result?;
    Ok(())
}

fn mutation_selection(command: &mut Command, rules: bool) {
    command.arg("--cargo-arg=--lib");
    if rules {
        command.args([
            "--package",
            "rules",
            "--test-package",
            "rules",
            "--file",
            "crates/rules/src/verification_rules.rs",
        ]);
    } else {
        command.args([
            "--package",
            "dotfiles",
            "--test-package",
            "dotfiles",
            "--file",
            "crates/dotfiles/src/verification/{source,plan,proof,reports,mod}.rs",
            "--re",
            mutation_inventory::ADAPTER_SELECTION,
        ]);
    }
}

fn validate_mutation_tests(output: &Path, rules: bool) -> Result<()> {
    let directory = output.join("mutants.out");
    let report: serde_json::Value =
        serde_json::from_slice(&fs::read(directory.join("outcomes.json"))?)?;
    let package = if rules { "rules" } else { "dotfiles" };
    let expected = format!("--package={package}@{}", env!("CARGO_PKG_VERSION"));
    let prefix = if rules {
        "test verification_rules::"
    } else {
        "test verification::"
    };
    let outcomes = report["outcomes"]
        .as_array()
        .context("missing mutation outcomes")?;
    ensure!(!outcomes.is_empty(), "empty mutation outcomes");
    for outcome in outcomes {
        let phases = outcome["phase_results"]
            .as_array()
            .context("missing mutation phases")?;
        for phase in phases.iter().filter(|phase| phase["phase"] == "Test") {
            let argv = phase["argv"]
                .as_array()
                .context("missing mutation test arguments")?;
            ensure!(
                argv.iter().any(|argument| argument == &expected),
                "mutation tested another package"
            );
            let log = outcome["log_path"]
                .as_str()
                .context("missing mutation test log")?;
            let log = fs::read_to_string(directory.join(log))?;
            ensure!(
                log.lines().any(|line| line.starts_with(prefix)),
                "mutation executed no selected tests"
            );
        }
    }
    Ok(())
}

fn copy_corpus(source: &Path, destination: &Path) -> Result<()> {
    fs::create_dir_all(destination)?;
    for entry in fs::read_dir(source)? {
        let entry = entry?;
        ensure!(
            entry.file_type()?.is_file(),
            "corpus entry must be a regular file"
        );
        fs::copy(entry.path(), destination.join(entry.file_name()))?;
    }
    Ok(())
}

fn fuzz(image: &str) -> Result<()> {
    let version = run(bounded(image).args(["cargo", "fuzz", "--version"]))?;
    ensure!(
        version.lines().any(|line| line == FUZZ_TOOL_VERSION),
        "unexpected cargo-fuzz version"
    );
    let root = std::env::current_dir()?;
    let directory = tempfile::Builder::new()
        .prefix("verification-corpus-")
        .tempdir_in(root.join("target"))?;
    copy_corpus(
        Path::new("fuzz/corpus/verification_input"),
        directory.path(),
    )?;
    let corpus = Path::new("/work").join(directory.path().strip_prefix(&root)?);
    let corpus = corpus.to_str().context("corpus path is not UTF-8")?;
    run(bounded(image).args(["test", "-d", corpus]))
        .context("copied corpus is not visible inside the container")?;
    let text = run(bounded(image).args([
        "CARGO_TARGET_DIR=/work/target/verification-assurance/fuzz-build",
        "cargo",
        "+nightly-2026-08-21",
        "fuzz",
        "run",
        "--codegen-units",
        "16",
        "--sanitizer",
        "address",
        "--debug-assertions",
        "verification_input",
        corpus,
        "--",
        &format!("-runs={FUZZ_INPUTS}"),
        &format!("-seed={FUZZ_SEED}"),
        &format!("-max_len={MAX_INPUT_BYTES}"),
    ]))?;
    ensure!(
        text.contains(&format!("Done {FUZZ_INPUTS} runs")),
        "fuzz campaign did not complete its input count"
    );
    println!(
        "{}",
        json!({"version":1,"source":std::env::var("DOTFILES_VERIFICATION_SOURCE")?,"plan":std::env::var("DOTFILES_VERIFICATION_PLAN")?,"method":"fuzz","scope":"affected","tool_version":FUZZ_TOOL_VERSION,"target":"verification_input","bounds":format!("AddressSanitizer; {FUZZ_INPUTS} inputs; seed {FUZZ_SEED}; max_len {MAX_INPUT_BYTES}"),"results":[{"name":"verification-input-boundaries","status":"passed","executed":FUZZ_INPUTS,"failed":0,"skipped":0,"inconclusive":0}]})
    );
    Ok(())
}

fn main() -> Result<()> {
    std::env::set_current_dir(Path::new(env!("CARGO_MANIFEST_DIR")).join("../.."))?;
    let image = std::env::var("DOTFILES_VERIFICATION_IMAGE")
        .context("select an immutable verification image")?;
    fs::create_dir_all(OUTPUT)?;
    match std::env::args().nth(1).as_deref() {
        Some("mutation-inventory") => capture_mutation_inventory(&image).map(drop),
        Some("mutations") => mutations(&image, false),
        Some("rule-mutations") => mutations(&image, true),
        Some("fuzz") => fuzz(&image),
        _ => anyhow::bail!("expected mutation-inventory, mutations, rule-mutations, or fuzz"),
    }
}
