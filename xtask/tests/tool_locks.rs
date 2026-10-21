use std::fs;
use std::path::Path;
use xtask::tool_locks;

#[path = "../../crates/platform/tests/support/fixture_process.rs"]
mod fixture_process;

const TOOL: &str = "pypi:ansible-core";
const DIRECTORY: &str = ".mise/locks/pypi-ansible-core/2.21.5";
const GRAPH: &str = "version = 1\nrevision = 5\n";
const PROJECT: &str = "[project]\ndependencies = [\"ansible-core==2.21.5\"]\n";

fn write(root: &Path, path: &str, content: &str) {
    let path = root.join(path);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, content).unwrap();
}

fn fixture() -> tempfile::TempDir {
    let root = tempfile::tempdir().unwrap();
    write(
        root.path(),
        "mise.toml",
        "[tools]\n\"pypi:ansible-core\" = { version = \"2.21.5\", os = [\"linux\"] }\n",
    );
    write(root.path(), &format!("{DIRECTORY}/uv.lock"), GRAPH);
    write(root.path(), &format!("{DIRECTORY}/pyproject.toml"), PROJECT);
    lock(
        root.path(),
        &format!("path = {DIRECTORY:?}, digest = {:?}", digest(GRAPH)),
    );
    root
}

fn digest(text: &str) -> String {
    use sha2::{Digest, Sha256};
    format!(
        "sha256:{}",
        xtask::build_inputs::hex(&Sha256::digest(text.as_bytes()))
    )
}

fn lock(root: &Path, metadata: &str) {
    write(
        root,
        "mise.lock",
        &format!(
            "[[tools.\"{TOOL}\"]]\nversion = \"2.21.5\"\nbackend = \"{TOOL}\"\nuv = {{ {metadata} }}\n"
        ),
    );
}

fn refuses(root: &Path, reason: &str) {
    let error = format!("{:#}", tool_locks::check(root).unwrap_err());
    assert!(error.contains(TOOL), "{error}");
    assert!(error.contains(reason), "{error}");
    assert!(error.contains("Next action:"), "{error}");
    assert!(error.contains("mise lock --platform linux-x64"), "{error}");
}

#[test]
fn linux_only_locks_are_checked_on_every_host() {
    let root = fixture();
    tool_locks::check(root.path()).unwrap();
    write(
        root.path(),
        "mise.lock",
        &format!(
            "lockfile_version = 3\n[[tools.\"{TOOL}\"]]\nversion = \"2.21.5\"\nbackend = \"{TOOL}\"\n"
        ),
    );
    refuses(root.path(), "missing uv");
    let output = fixture_process::command(env!("CARGO_BIN_EXE_xtask"), root.path())
        .arg("--root")
        .arg(root.path())
        .arg("tool-locks")
        .output()
        .unwrap();
    assert!(!output.status.success());
    let error = String::from_utf8_lossy(&output.stderr);
    assert!(
        error.contains(TOOL) && error.contains("missing uv") && error.contains("Next action:"),
        "{error}"
    );
}

#[test]
fn a_pypi_request_cannot_disappear_from_the_lock_or_keep_an_old_version() {
    for text in [
        "[tools]\n".to_owned(),
        format!("[[tools.\"{TOOL}\"]]\nversion = \"2.21.4\"\nbackend = \"{TOOL}\"\n"),
    ] {
        let root = fixture();
        write(root.path(), "mise.lock", &text);
        refuses(root.path(), "missing lock entry");
    }
}

#[test]
fn uv_paths_and_digests_must_be_present_and_well_formed() {
    for (metadata, reason) in [
        (format!("digest = {:?}", digest(GRAPH)), "uv path"),
        (format!("path = {DIRECTORY:?}"), "uv digest"),
        (
            format!("path = {DIRECTORY:?}, digest = \"sha256:bad\""),
            "uv digest",
        ),
        (
            format!("path = \"../outside\", digest = {:?}", digest(GRAPH)),
            "uv path",
        ),
        (
            format!(
                "path = \".mise\\\\locks\\\\tool\", digest = {:?}",
                digest(GRAPH)
            ),
            "uv path",
        ),
    ] {
        let root = fixture();
        lock(root.path(), &metadata);
        refuses(root.path(), reason);
    }
}

#[test]
fn the_directory_and_both_sidecars_are_required() {
    for (path, reason) in [
        (DIRECTORY.to_owned(), "directory"),
        (format!("{DIRECTORY}/uv.lock"), "uv.lock"),
        (format!("{DIRECTORY}/pyproject.toml"), "pyproject.toml"),
    ] {
        let root = fixture();
        let path = root.path().join(path);
        if path.is_dir() {
            fs::remove_dir_all(path).unwrap();
        } else {
            fs::remove_file(path).unwrap();
        }
        refuses(root.path(), reason);
    }
}

#[test]
fn the_digest_covers_uv_lock_and_accepts_windows_line_endings() {
    let root = fixture();
    write(
        root.path(),
        &format!("{DIRECTORY}/uv.lock"),
        &GRAPH.replace('\n', "\r\n"),
    );
    tool_locks::check(root.path()).unwrap();
    write(
        root.path(),
        &format!("{DIRECTORY}/uv.lock"),
        "version = 2\n",
    );
    refuses(root.path(), "SHA-256");
}

#[test]
fn the_project_must_pin_the_requested_package_and_version() {
    for dependencies in [
        "ansible-core==2.21.4",
        "other==2.21.5",
        "ansible-core>=2.21.5",
        "ansible-core==2.21.50",
    ] {
        let root = fixture();
        write(
            root.path(),
            &format!("{DIRECTORY}/pyproject.toml"),
            &format!("[project]\ndependencies = [{dependencies:?}]\n"),
        );
        refuses(root.path(), "pinned version");
    }
}

#[test]
fn every_backend_with_uv_and_registry_aliases_are_checked() {
    for (name, backend) in [
        ("yamllint", "pypi:ansible-core"),
        ("custom", "custom:ansible-core"),
    ] {
        let root = fixture();
        let config = fs::read_to_string(root.path().join("mise.toml"))
            .unwrap()
            .replace(TOOL, name);
        write(root.path(), "mise.toml", &config);
        let text = fs::read_to_string(root.path().join("mise.lock"))
            .unwrap()
            .replace(&format!("tools.\"{TOOL}\""), &format!("tools.\"{name}\""))
            .replace(
                &format!("backend = \"{TOOL}\""),
                &format!("backend = \"{backend}\""),
            );
        write(root.path(), "mise.lock", &text);
        tool_locks::check(root.path()).unwrap();
        fs::remove_file(root.path().join(format!("{DIRECTORY}/uv.lock"))).unwrap();
        let error = format!("{:#}", tool_locks::check(root.path()).unwrap_err());
        assert!(error.contains(name) && error.contains("uv.lock"), "{error}");
    }
}

#[test]
fn unreferenced_version_directories_are_refused() {
    let root = fixture();
    write(
        root.path(),
        ".mise/locks/pypi-ansible-core/2.21.4/uv.lock",
        GRAPH,
    );
    refuses(root.path(), "unreferenced version directory");
}

#[test]
fn tools_without_uv_do_not_need_sidecars() {
    let root = fixture();
    write(root.path(), "mise.toml", "[tools]\nrust = \"1.99.1\"\n");
    write(
        root.path(),
        "mise.lock",
        "[[tools.rust]]\nversion = \"1.99.1\"\nbackend = \"core:rust\"\n",
    );
    fs::remove_dir_all(root.path().join(".mise")).unwrap();
    tool_locks::check(root.path()).unwrap();
}

#[test]
#[ignore = "resolves the published PyPI dependency graph with the installed pinned mise and uv"]
fn mise_lock_repairs_an_os_filtered_pypi_tool_on_this_host() {
    let root = fixture();
    write(
        root.path(),
        "mise.lock",
        &format!(
            "lockfile_version = 3\n[[tools.\"{TOOL}\"]]\nversion = \"2.21.5\"\nbackend = \"{TOOL}\"\n"
        ),
    );
    fs::remove_dir_all(root.path().join(SIDECAR_ROOT)).unwrap();
    let config = fs::read_to_string(root.path().join("mise.toml")).unwrap();
    write(
        root.path(),
        "mise.toml",
        &format!("{config}uv = \"0.13.1\"\npython = \"3.14.8\"\n[settings]\nlockfile = true\n"),
    );
    let output = platform::tool::Tool::Mise
        .command()
        .current_dir(root.path())
        .env("MISE_CACHE_DIR", root.path().join("cache"))
        .env("MISE_TRUSTED_CONFIG_PATHS", root.path())
        .args(["lock", "--platform", "linux-x64", TOOL])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    tool_locks::check(root.path()).unwrap_or_else(|error| {
        panic!(
            "{error:#}\n{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        )
    });
}

const SIDECAR_ROOT: &str = ".mise/locks";
