#![allow(
    clippy::disallowed_methods,
    reason = "integration tests spawn the real Git they verify"
)]

use anyhow::{Result, ensure};
use dotfiles_xtask::hosts::{
    Managed, Peer, Tailnet, evidence, hub_user, label_family, managed, outcome, pinned, push_gate,
    remote_gate, render, required, scanned, select, tailnet, wrap,
};
use dotfiles_xtask::hosts_rules::{Evidence, Family};
use std::fs;
use std::path::Path;
use std::process::Command;

fn workflow(text: &str) -> Vec<(String, String)> {
    vec![("ci.yml".to_owned(), text.to_owned())]
}

#[test]
fn the_ci_matrix_names_the_required_os_families() -> Result<()> {
    assert_eq!(
        required(&workflow(
            "jobs:\n  a:\n    strategy:\n      matrix:\n        os: [ubuntu-latest, macos-14]\n        include:\n          - os: windows-2022\n    runs-on: ${{ matrix.os }}\n  b:\n    uses: owner/repo/.github/workflows/x.yml@main\n"
        ))?,
        [true, true, true]
    );
    assert_eq!(
        required(&workflow(
            "jobs:\n  a:\n    runs-on: [self-hosted, linux]\n  b:\n    runs-on:\n      labels: macos-latest\n"
        ))?,
        [true, true, false]
    );
    assert_eq!(required(&workflow("on: push\n"))?, [false; 3]);
    assert!(required(&workflow("jobs:\n  a:\n    runs-on: ${{ inputs.os }}\n")).is_err());
    assert!(required(&workflow("jobs:\n  a:\n    runs-on: my-runner\n")).is_err());
    assert_eq!(label_family("Windows-latest"), Some(Family::Windows));
    Ok(())
}

fn record(os: &str, tree: &str, status: i32) -> String {
    format!(r#"{{"host":"h","os":"{os}","tree":"{tree}","status":{status},"gate":"just check"}}"#)
}

#[test]
fn the_latest_note_for_the_commit_tree_decides_each_family() -> Result<()> {
    let notes = [
        record("linux", "t", 0),
        record("macos", "t", 0),
        record("macos", "t", 2),
        record("windows", "other", 0),
    ]
    .join("\n");
    assert_eq!(
        evidence(&notes, "t")?,
        [Evidence::Passed, Evidence::Failed, Evidence::Missing]
    );
    assert!(evidence("not json", "t").is_err());
    assert!(evidence(&record("plan9", "t", 0), "t").is_err());
    Ok(())
}

fn git(directory: &Path, arguments: &[&str]) -> Result<String> {
    let output = dotfiles_xtask::tool::Tool::Git
        .command()
        .current_dir(directory)
        .env("GIT_CONFIG_GLOBAL", directory.join("gitconfig"))
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .args(arguments)
        .output()?;
    ensure!(
        output.status.success(),
        "git {arguments:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    Ok(String::from_utf8(output.stdout)?.trim().to_owned())
}

#[test]
fn a_push_is_refused_until_every_os_family_note_passes() -> Result<()> {
    let scope = tempfile::tempdir()?;
    let root = scope.path();
    fs::write(
        root.join("gitconfig"),
        "[user]\n\tname = Fixture\n\temail = fixture@example.com\n[commit]\n\tgpgsign = false\n",
    )?;
    fs::create_dir_all(root.join(".github/workflows"))?;
    fs::write(
        root.join(".github/workflows/ci.yml"),
        "jobs:\n  test:\n    strategy:\n      matrix:\n        os: [ubuntu-latest, macos-latest]\n    runs-on: ${{ matrix.os }}\n",
    )?;
    git(root, &["init", "--quiet"])?;
    git(root, &["add", ".github"])?;
    git(root, &["commit", "--quiet", "-m", "ci"])?;
    let head = git(root, &["rev-parse", "HEAD"])?;
    let tree = git(root, &["rev-parse", "HEAD^{tree}"])?;
    let zero = "0".repeat(40);
    let update = format!("refs/heads/feature {head} refs/heads/feature {zero}\n");
    let github = "git@github.com:owner/project.git";
    let note = |os: &str, tree: &str, status: i32| -> Result<()> {
        git(
            root,
            &[
                "notes",
                "--ref",
                "refs/notes/hosts",
                "append",
                "-m",
                &record(os, tree, status),
                "HEAD",
            ],
        )?;
        Ok(())
    };
    let refused = |expected: &str| {
        let error = push_gate(root, github, update.as_bytes())
            .unwrap_err()
            .to_string();
        assert!(
            error.contains(expected) && error.contains("hosts check"),
            "{error}"
        );
    };
    refused("no linux gate result");
    push_gate(root, "ssh://hub/~/git/project.git", update.as_bytes())?;
    push_gate(
        root,
        github,
        format!("(delete) {zero} refs/heads/feature {head}\n").as_bytes(),
    )?;
    push_gate(
        root,
        github,
        format!("refs/tags/v1 {head} refs/tags/v1 {zero}\n").as_bytes(),
    )?;
    note("linux", &tree, 0)?;
    refused("no macos gate result");
    note("macos", "another-tree", 0)?;
    refused("no macos gate result");
    note("macos", &tree, 1)?;
    refused("the macos gate failed");
    note("macos", &tree, 0)?;
    push_gate(root, github, update.as_bytes())?;
    assert!(push_gate(root, github, b"malformed\n").is_err());
    Ok(())
}

fn peer(alias: &str, family: Family, online: bool) -> Peer {
    Peer {
        alias: alias.to_owned(),
        dns: format!("{alias}.example.ts.net"),
        address: "100.64.0.1".to_owned(),
        family,
        online,
    }
}

#[test]
fn the_tailnet_supplies_one_host_per_other_family() -> Result<()> {
    let status = br#"{"Self":{"DNSName":"laptop.example.ts.net.","OS":"windows","TailscaleIPs":["100.64.0.9"],"Online":true},
        "Peer":{"a":{"DNSName":"box.example.ts.net.","OS":"linux","TailscaleIPs":["fd7a::1","100.64.0.2"],"Online":true},
                "b":{"DNSName":"phone.example.ts.net.","OS":"android","TailscaleIPs":["100.64.0.3"],"Online":true}}}"#;
    let parsed = tailnet(status)?;
    assert_eq!(parsed.this, Some(("laptop".to_owned(), Family::Windows)));
    assert_eq!(parsed.peers.len(), 1);
    assert_eq!(parsed.peers[0].alias, "box");
    assert_eq!(parsed.peers[0].dns, "box.example.ts.net");
    assert_eq!(parsed.peers[0].address, "100.64.0.2");
    let net = Tailnet {
        this: Some(("laptop".to_owned(), Family::Windows)),
        peers: vec![
            peer("nas", Family::Linux, true),
            peer("home", Family::Linux, true),
            peer("mac", Family::Mac, false),
        ],
    };
    let names = |peers: Vec<Peer>| peers.into_iter().map(|peer| peer.alias).collect::<Vec<_>>();
    assert!(select(&net, &[], &[]).is_err());
    assert_eq!(
        names(select(&net, &["home".into(), "mac".into()], &[])?),
        ["home", "mac"]
    );
    assert_eq!(
        names(select(&net, &["nas".into()], &["home".into()])?),
        ["home"]
    );
    assert!(select(&net, &[], &["phone".into()]).is_err());
    Ok(())
}

#[test]
fn managed_files_round_trip_and_keys_come_only_from_key_lines() {
    let text = render(
        &["home".into(), "mac".into()],
        "owner",
        Path::new("C:\\state\\known_hosts"),
    );
    assert!(text.contains("UserKnownHostsFile C:/state/known_hosts"));
    assert!(text.contains(
        "StrictHostKeyChecking yes
  ForwardAgent no
"
    ));
    assert_eq!(
        managed(&text),
        Managed {
            aliases: vec!["home".into(), "mac".into()],
            user: Some("owner".into())
        }
    );
    let known = "home,home.ts.net,100.64.0.2 100.64.0.2:22 SSH-2.0-Tailscale\nhome,home.ts.net,100.64.0.2 ssh-ed25519 AAAA\n";
    assert_eq!(
        pinned(known).get("home").map(String::as_str),
        Some("ssh-ed25519 AAAA")
    );
    assert_eq!(
        scanned("# 100.64.0.2:22 SSH-2.0-OpenSSH\n100.64.0.2 ssh-ed25519 BBBB\n").as_deref(),
        Some("ssh-ed25519 BBBB")
    );
    assert_eq!(scanned("100.64.0.2:22 SSH-2.0-Tailscale\n"), None);
    let remotes = "origin\tgit@github.com:owner/project.git (fetch)\nhub\tssh://owner@mac/~/git/project.git (push)\n";
    assert_eq!(hub_user(remotes, &["mac".into()]).as_deref(), Some("owner"));
    assert_eq!(hub_user(remotes, &["home".into()]), None);
}

#[test]
fn only_a_finished_job_reports_a_gate_status() {
    assert_eq!(
        outcome(
            "home",
            "submission x\nhome:x\nhome:x finished succeeded\nhome:y finished failed (9)\n"
        ),
        Some(0)
    );
    assert_eq!(outcome("home", "home:x finished failed (3)\n"), Some(3));
    assert_eq!(
        outcome(
            "mac",
            "mac:x finished launch failed: starting the job command\n"
        ),
        None
    );
    assert_eq!(outcome("mac", "ssh: connection reset\n"), None);
    assert_eq!(
        wrap(Family::Mac, "just check"),
        ["zsh", "-lc", "just check"]
    );
    assert_eq!(wrap(Family::Windows, "just check")[0], "powershell.exe");
    for family in Family::ALL {
        let gate = remote_gate(family, "just check");
        let trust = gate.find("MISE_TRUSTED_CONFIG_PATHS").expect("trust");
        let clone = gate
            .find("clone --quiet head.bundle checkout")
            .expect("clone");
        assert!(trust < clone, "checkout hooks run mise during the clone");
        assert!(gate.contains("just check"));
    }
}

fn tools() -> Result<tempfile::TempDir> {
    let directory = tempfile::tempdir()?;
    for name in ["tailscale", "domyjob"] {
        let status = Command::new("rustc")
            .arg(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/tests/fixtures/hosts_tools.rs"
            ))
            .arg("-o")
            .arg(
                directory
                    .path()
                    .join(format!("{name}{}", std::env::consts::EXE_SUFFIX)),
            )
            .status()?;
        ensure!(status.success(), "compile the {name} fixture");
    }
    Ok(directory)
}

#[test]
fn an_unreachable_host_stops_the_check_with_its_next_action() -> Result<()> {
    let tools = tools()?;
    let scope = tempfile::tempdir()?;
    let root = scope.path().join("project");
    let home = scope.path().join("home");
    fs::create_dir_all(root.join(".github/workflows"))?;
    fs::create_dir_all(home.join(".local/state/hosts"))?;
    fs::write(
        home.join(".local/state/hosts/ssh_config"),
        render(&["box".into()], "owner", &home.join("known_hosts")),
    )?;
    fs::write(
        root.join("gitconfig"),
        "[user]\n\tname = Fixture\n\temail = fixture@example.com\n[commit]\n\tgpgsign = false\n",
    )?;
    fs::write(root.join(".gitignore"), "gitconfig\n")?;
    fs::write(
        root.join(".github/workflows/ci.yml"),
        "jobs:\n  test:\n    runs-on: ubuntu-latest\n",
    )?;
    git(&root, &["init", "--quiet"])?;
    git(&root, &["add", "."])?;
    git(&root, &["commit", "--quiet", "-m", "ci"])?;
    let mut paths = vec![tools.path().to_path_buf()];
    paths.extend(std::env::split_paths(
        &std::env::var_os("PATH").unwrap_or_default(),
    ));
    let status = |online: bool| {
        format!(
            r#"{{"Self":{{"DNSName":"phone.example.ts.net.","OS":"android","Online":true}},"Peer":{{"a":{{"DNSName":"box.example.ts.net.","OS":"linux","TailscaleIPs":["100.64.0.2"],"Online":{online}}}}}}}"#
        )
    };
    let check = |online: bool, doctor: &str| -> Result<std::process::Output> {
        Ok(Command::new(env!("CARGO_BIN_EXE_dotfiles-xtask"))
            .args(["hosts", "check"])
            .current_dir(&root)
            .env("PATH", std::env::join_paths(&paths)?)
            .env("HOME", &home)
            .env("USERPROFILE", &home)
            .env("GIT_CONFIG_GLOBAL", root.join("gitconfig"))
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_AUTHOR_NAME", "Fixture")
            .env("GIT_AUTHOR_EMAIL", "fixture@example.com")
            .env("GIT_COMMITTER_NAME", "Fixture")
            .env("GIT_COMMITTER_EMAIL", "fixture@example.com")
            .env("HOSTS_FIXTURE_STATUS", status(online))
            .env("HOSTS_FIXTURE_DOCTOR", doctor)
            .output()?)
    };
    let head = git(&root, &["rev-parse", "HEAD"])?;
    let update = format!(
        "refs/heads/main {head} refs/heads/main {}\n",
        "0".repeat(40)
    );
    for (online, doctor, action) in [
        (false, "ok", "box is offline on the tailnet"),
        (true, "fail", "run `ssh -o BatchMode=yes box true`"),
    ] {
        let output = check(online, doctor)?;
        assert!(!output.status.success());
        let error = String::from_utf8_lossy(&output.stderr);
        assert!(
            error.contains("linux: ") && error.contains(action),
            "{error}"
        );
        assert!(push_gate(&root, "https://github.com/owner/project", update.as_bytes()).is_err());
    }
    fs::write(root.join("untracked"), "")?;
    let output = check(true, "ok")?;
    assert!(String::from_utf8_lossy(&output.stderr).contains("commit first"));
    fs::remove_file(root.join("untracked"))?;
    let output = check(true, "ok")?;
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(String::from_utf8_lossy(&output.stdout).contains("linux box: passed"));
    push_gate(&root, "https://github.com/owner/project", update.as_bytes())?;
    Ok(())
}
