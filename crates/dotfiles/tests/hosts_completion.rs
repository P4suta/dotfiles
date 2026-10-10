use anyhow::{Result, ensure};
use dotfiles::hosts::{NOTES, evidence, render};
use dotfiles::hosts_rules::Evidence;
use std::fs;
use std::path::Path;
use std::process::Stdio;
use std::time::{Duration, Instant};

#[path = "../../dotguard/tests/support/fixture_git.rs"]
mod fixture_git;

const COMPLETION_LIMIT: Duration = Duration::from_secs(10);
const POLL_INTERVAL: Duration = Duration::from_millis(10);

fn git(root: &Path, arguments: &[&str]) -> Result<String> {
    let output = fixture_git::command("git", &root.join("gitconfig"))
        .arg("-C")
        .arg(root)
        .args(arguments)
        .output()?;
    ensure!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    Ok(String::from_utf8(output.stdout)?.trim().to_owned())
}

#[test]
fn completed_jobs_record_every_result_and_exit_without_a_waiting_session() -> Result<()> {
    let scope = tempfile::tempdir()?;
    let root = scope.path().join("project");
    let home = scope.path().join("home");
    let bin = scope.path().join("bin");
    fs::create_dir_all(root.join(".github/workflows"))?;
    fs::create_dir_all(home.join(".local/state/hosts"))?;
    fs::create_dir(&bin)?;
    let executable = bin.join(format!("domyjob{}", std::env::consts::EXE_SUFFIX));
    let status = platform::process::command("rustc")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/fixtures/hosts_job.rs"
        ))
        .args(["--edition", "2024", "-o"])
        .arg(&executable)
        .status()?;
    ensure!(status.success(), "compile the completed-job fixture");
    fs::copy(
        &executable,
        bin.join(format!("tailscale{}", std::env::consts::EXE_SUFFIX)),
    )?;
    fs::write(
        home.join(".local/state/hosts/ssh_config"),
        render(
            &["box".into(), "mac".into(), "pc".into()],
            "fixture",
            &home.join("known_hosts"),
        ),
    )?;
    fs::write(
        root.join("gitconfig"),
        "[user]\nname = Fixture\nemail = fixture@example.com\n[commit]\ngpgsign = false\n[core]\nhooksPath = absent-hooks\n",
    )?;
    fs::write(root.join(".gitignore"), "gitconfig\n")?;
    fs::write(
        root.join(".github/workflows/ci.yml"),
        "jobs:\n  test:\n    strategy:\n      matrix:\n        os: [ubuntu-latest, macos-latest, windows-latest]\n    runs-on: ${{ matrix.os }}\n",
    )?;
    git(&root, &["init", "--quiet"])?;
    git(&root, &["add", "."])?;
    git(&root, &["commit", "--quiet", "-m", "ci"])?;
    let mut paths = vec![bin];
    paths.extend(std::env::split_paths(
        &std::env::var_os("PATH").unwrap_or_default(),
    ));
    let path = std::env::join_paths(paths)?;
    let tailnet = r#"{"Self":{"DNSName":"phone.example.ts.net.","OS":"android","Online":true},"Peer":{"a":{"DNSName":"box.example.ts.net.","OS":"linux","TailscaleIPs":["100.64.0.2"],"Online":true},"b":{"DNSName":"mac.example.ts.net.","OS":"macOS","TailscaleIPs":["100.64.0.3"],"Online":true},"c":{"DNSName":"pc.example.ts.net.","OS":"windows","TailscaleIPs":["100.64.0.4"],"Online":true}}}"#;
    for (failed, code, real, expected) in [
        ("no", 0, 0, Evidence::Passed),
        ("yes", 7, 9, Evidence::Failed),
    ] {
        let state = scope.path().join(failed);
        fs::create_dir(&state)?;
        let release = state.join("release");
        let log = state.join("output.log");
        let output = fs::File::create(&log)?;
        let mut child = fixture_git::command(env!("CARGO_BIN_EXE_dotfiles"), &root.join("gitconfig"))
            .args(["hosts", "check"]).current_dir(&root)
            .env("PATH", &path).env("HOME", &home).env("USERPROFILE", &home)
            .env("HOSTS_JOB_TAILNET", tailnet).env("HOSTS_JOB_FAILED", failed)
            .env("HOSTS_JOB_RELEASE", &release).env("HOSTS_JOB_STATE", &state)
            .env("HOSTS_JOB_STAGES", format!(r#"{{"code":{code},"real":{real},"applied":{{"drifted_targets":[],"verification_error":null}}}}"#))
            .stdin(Stdio::null()).stdout(output.try_clone()?).stderr(output).spawn()?;
        let deadline = Instant::now() + COMPLETION_LIMIT;
        let completed = loop {
            if child.try_wait()?.is_some() {
                break true;
            }
            if Instant::now() >= deadline {
                break false;
            }
            std::thread::sleep(POLL_INTERVAL);
        };
        fs::write(release, "release fixture waiters")?;
        let status = child.wait()?;
        assert!(
            completed,
            "completed remote jobs retained their waiting sessions: {}",
            fs::read_to_string(&log)?
        );
        assert_eq!(
            status.success(),
            failed == "no",
            "{}",
            fs::read_to_string(&log)?
        );
        let notes = git(&root, &["notes", "--ref", NOTES, "show", "HEAD"])?;
        assert_eq!(
            notes.lines().filter(|line| !line.is_empty()).count(),
            if failed == "no" { 6 } else { 12 }
        );
        assert_eq!(
            evidence(&notes, &git(&root, &["rev-parse", "HEAD^{tree}"])?)?,
            [expected; 3]
        );
    }
    Ok(())
}
