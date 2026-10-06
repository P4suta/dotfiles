use dotfiles_xtask::aside;
use std::fs;
use std::path::Path;
use std::process::Child;

/// A copy of a system program that keeps running for a few seconds, so its image stays mapped while the test replaces it.
#[expect(
    clippy::disallowed_methods,
    reason = "the test starts its own copy of a system program, which no host provision declares"
)]
fn run_copy(binary: &Path) -> Child {
    if cfg!(windows) {
        std::process::Command::new(binary)
            .args(["-n", "6", "127.0.0.1"])
            .stdout(std::process::Stdio::null())
            .spawn()
            .unwrap()
    } else {
        std::process::Command::new(binary).arg("5").spawn().unwrap()
    }
}

#[test]
fn a_running_binary_is_replaced_and_its_aside_copy_swept_later() {
    let scope = tempfile::tempdir().unwrap();
    let system = if cfg!(windows) {
        Path::new(r"C:\Windows\System32\PING.EXE")
    } else {
        Path::new("/bin/sleep")
    };
    let target = scope
        .path()
        .join(format!("tool{}", std::env::consts::EXE_SUFFIX));
    fs::copy(system, &target).unwrap();
    let mut running = run_copy(&target);
    let staged = scope.path().join("staged");
    fs::write(&staged, b"replacement").unwrap();
    if cfg!(windows) {
        assert!(
            fs::write(&target, b"in place").is_err(),
            "Windows let an installer overwrite a running image in place"
        );
    }
    aside::replace(&staged, &target).unwrap();
    assert_eq!(fs::read(&target).unwrap(), b"replacement");
    let asides = |directory: &Path| {
        fs::read_dir(directory)
            .unwrap()
            .flatten()
            .filter(|entry| entry.file_name().to_string_lossy().ends_with(".old"))
            .count()
    };
    assert_eq!(asides(scope.path()), 1);
    aside::sweep(&target);
    assert_eq!(
        asides(scope.path()),
        usize::from(cfg!(windows)),
        "a running aside copy stays until a later installation"
    );
    running.kill().ok();
    running.wait().unwrap();
    aside::sweep(&target);
    assert_eq!(asides(scope.path()), 0);
}

#[test]
fn a_failed_installation_restores_the_previous_binary() {
    let scope = tempfile::tempdir().unwrap();
    let target = scope.path().join("tool");
    fs::write(&target, b"previous").unwrap();
    let missing = scope.path().join("never-staged");
    assert!(aside::replace(&missing, &target).is_err());
    assert_eq!(fs::read(&target).unwrap(), b"previous");
}
