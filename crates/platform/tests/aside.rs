use platform::aside;
use platform::process::command;
use std::fs;
use std::io::{BufRead, Read, Write};
use std::path::Path;
use std::process::Child;

const CHILD: &str = "DOTFILES_ASIDE_TEST_CHILD";
const READY: &str = "installed binary is running";

#[test]
fn hold_running_binary() {
    if std::env::var_os(CHILD).is_some() {
        println!("{READY}");
        std::io::stdout().flush().unwrap();
        std::io::stdin().read_to_end(&mut Vec::new()).unwrap();
    }
}

fn run_copy(binary: &Path) -> Child {
    let mut child = command(binary)
        .args(["--exact", "hold_running_binary", "--nocapture"])
        .env(CHILD, "1")
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    let mut output = std::io::BufReader::new(child.stdout.take().unwrap());
    loop {
        let mut line = String::new();
        assert!(output.read_line(&mut line).unwrap() > 0);
        if line.contains(READY) {
            break;
        }
    }
    child.stdout = Some(output.into_inner());
    child
}

#[test]
fn install_replaces_a_running_binary() {
    let scope = tempfile::tempdir().unwrap();
    let target = scope
        .path()
        .join(format!("tool{}", std::env::consts::EXE_SUFFIX));
    fs::copy(std::env::current_exe().unwrap(), &target).unwrap();
    let mut running = run_copy(&target);
    let staged = scope.path().join("staged");
    fs::write(&staged, b"replacement").unwrap();
    if cfg!(windows) {
        assert!(
            fs::write(&target, b"in place").is_err(),
            "Windows let an installer overwrite a running image in place"
        );
    }
    aside::install(&staged, &target).unwrap();
    assert_eq!(fs::read(&target).unwrap(), b"replacement");
    let asides = |directory: &Path| {
        fs::read_dir(directory)
            .unwrap()
            .flatten()
            .filter(|entry| entry.file_name().to_string_lossy().ends_with(".old"))
            .count()
    };
    assert_eq!(asides(scope.path()), usize::from(cfg!(windows)));
    aside::sweep(&target);
    assert_eq!(asides(scope.path()), usize::from(cfg!(windows)));
    assert!(running.try_wait().unwrap().is_none());
    running.stdin.take();
    assert!(running.wait().unwrap().success());
    aside::sweep(&target);
    assert_eq!(asides(scope.path()), 0);
}

#[test]
fn replacement_sweeps_old_asides_without_removing_the_installed_binary() {
    let scope = tempfile::tempdir().unwrap();
    let target = scope.path().join("tool");
    let old = scope.path().join("tool.42.old");
    let unrelated = scope.path().join("another.42.old");
    fs::write(&target, b"previous").unwrap();
    fs::write(&old, b"old").unwrap();
    fs::write(&unrelated, b"unrelated").unwrap();
    let staged = scope.path().join("staged");
    fs::write(&staged, b"replacement").unwrap();
    aside::replace(&staged, &target).unwrap();
    assert_eq!(fs::read(&target).unwrap(), b"replacement");
    assert!(!old.exists());
    assert!(unrelated.exists());
}

#[test]
fn a_failed_rename_leaves_the_previous_binary_in_place() {
    let scope = tempfile::tempdir().unwrap();
    let target = scope.path().join("tool");
    fs::write(&target, b"previous").unwrap();
    let missing = scope.path().join("never-staged");
    let error = aside::replace(&missing, &target).unwrap_err();
    assert_eq!(
        error.downcast_ref::<std::io::Error>().unwrap().kind(),
        std::io::ErrorKind::NotFound
    );
    assert_eq!(fs::read(&target).unwrap(), b"previous");
    assert_eq!(fs::read_dir(scope.path()).unwrap().count(), 1);
}

#[test]
fn a_failed_staging_leaves_the_previous_binary_in_place() {
    let scope = tempfile::tempdir().unwrap();
    let target = scope.path().join("tool");
    fs::write(&target, b"previous").unwrap();
    let missing = scope.path().join("never-built");
    assert!(aside::install(&missing, &target).is_err());
    assert_eq!(fs::read(&target).unwrap(), b"previous");
}

#[cfg(not(windows))]
#[test]
fn readers_always_find_a_complete_binary_during_replacement() {
    use std::sync::{
        Barrier,
        atomic::{AtomicBool, Ordering},
    };

    const CONTENT: usize = 4096;
    const REPLACEMENTS: usize = 256;
    let scope = tempfile::tempdir().unwrap();
    let target = scope.path().join("tool");
    fs::write(&target, [0; CONTENT]).unwrap();
    let ready = Barrier::new(2);
    let finished = AtomicBool::new(false);
    std::thread::scope(|threads| {
        let reader = threads.spawn(|| {
            ready.wait();
            let mut observations = 0;
            loop {
                let bytes = fs::read(&target)?;
                assert!(bytes == [0; CONTENT] || bytes == [1; CONTENT]);
                observations += 1;
                if finished.load(Ordering::Acquire) {
                    break;
                }
            }
            Ok::<_, std::io::Error>(observations)
        });
        ready.wait();
        let result = (|| {
            for index in 0..REPLACEMENTS {
                let staged = scope.path().join("staged");
                fs::write(&staged, [u8::from(index % 2 == 0); CONTENT])?;
                if index % 2 == 0 {
                    aside::replace(&staged, &target)?;
                } else {
                    aside::install(&staged, &target)?;
                }
            }
            Ok::<_, anyhow::Error>(())
        })();
        finished.store(true, Ordering::Release);
        assert!(reader.join().unwrap().unwrap() > 0);
        result.unwrap();
    });
}
