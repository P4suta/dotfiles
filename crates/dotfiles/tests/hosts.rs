use anyhow::{Context, Result, ensure};
use dotfiles::hosts::{
    Managed, Peer, Tailnet, evidence, hub_user, label_family, managed, outcome, pinned, push_gate,
    remote_gate, render, require_admission, required, scanned, select, tailnet, wrap,
};
use dotfiles::hosts_rules::{Evidence, Family};
use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};

#[path = "../../dotguard/tests/support/fixture_git.rs"]
mod fixture_git;

#[path = "../../../xtask/tests/support/incident_fixture.rs"]
mod incident_fixture;

use fixture_git::fixture_process;

struct CodeFixture {
    scope: tempfile::TempDir,
    root: PathBuf,
    host: PathBuf,
    old: PathBuf,
}

fn code_fixture() -> Result<CodeFixture> {
    let scope = tempfile::tempdir()?;
    let root = scope.path().join("project");
    let host = scope.path().join("host");
    let installed = host.join("data/installs/python/3.14.8");
    let old = scope.path().join("old");
    for directory in [&root, &host, &installed, &old] {
        fs::create_dir_all(directory)?;
    }
    let mise = host.join(format!("mise{}", std::env::consts::EXE_SUFFIX));
    let compiled = platform::process::command("rustc")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/fixtures/code_environment.rs"
        ))
        .args(["--edition", "2024"])
        .arg("-o")
        .arg(&mise)
        .status()?;
    ensure!(compiled.success(), "compile the code environment fixture");
    let native_path = std::env::join_paths(std::iter::once(host.clone()).chain(
        std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default()),
    ))?;
    fs::write(
        mise.with_extension("native-path.json"),
        serde_json::to_vec(&serde_json::json!({
            "PATH": native_path.to_str().context("fixture native PATH")?
        }))?,
    )?;
    for directory in [&installed, &old] {
        fs::copy(
            &mise,
            directory.join(format!("python{}", std::env::consts::EXE_SUFFIX)),
        )?;
    }
    fs::copy(
        &mise,
        old.join(format!(
            "unrelated-application{}",
            std::env::consts::EXE_SUFFIX
        )),
    )?;
    fs::write(root.join("mise.toml"), "[tools]\npython = '3.14.8'\n")?;
    fs::create_dir_all(root.join("home/.chezmoidata"))?;
    fs::write(
        installed.parent().unwrap().join(".mise.backend.toml"),
        "short = 'python'\nfull = 'core:python'\n",
    )?;
    fs::write(
        root.join("home/.chezmoidata/versions.toml"),
        "[mise]\nversion = '2026.10.0'\n",
    )?;
    fs::write(root.join("gitconfig"), "")?;
    fs::write(
        host.join(".gitconfig"),
        "[url \"git@github.com:\"]\n insteadOf = https://github.com/\n[credential]\n helper = broken-fixture\n",
    )?;
    git(&root, &["init", "--quiet"])?;
    git(
        &root,
        &[
            "remote",
            "add",
            "origin",
            "https://github.com/third-party/project.git",
        ],
    )?;
    let source = platform::paths::canonical(&root.join("mise.toml"))?;
    let tools = serde_json::json!({"python": [{"version": "3.14.8", "requested_version": "3.14.8", "install_path": installed, "source": {"type": "mise.toml", "path": source}, "installed": true, "active": true}]});
    fs::write(root.join("tools.json"), serde_json::to_vec(&tools)?)?;
    let mut paths = vec![installed];
    if cfg!(windows) {
        let pwsh = platform::tool::Tool::Mise
            .command()
            .args(["which", "pwsh"])
            .output()?;
        ensure!(
            pwsh.status.success(),
            "resolve the pinned PowerShell fixture shell"
        );
        paths.push(
            Path::new(String::from_utf8(pwsh.stdout)?.trim())
                .parent()
                .unwrap()
                .to_path_buf(),
        );
    }
    let mut resolved = String::new();
    for path in paths {
        writeln!(resolved, "{}", path.display())?;
    }
    fs::write(root.join("paths"), resolved)?;
    Ok(CodeFixture {
        scope,
        root,
        host,
        old,
    })
}

fn seed_renderer(fixture: &CodeFixture) -> Result<()> {
    let output = platform::tool::Tool::Mise
        .command()
        .args(["which", "chezmoi"])
        .output()?;
    ensure!(
        output.status.success(),
        "resolve the pinned renderer fixture"
    );
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let table: toml::Table = toml::from_str(&fs::read_to_string(root.join("mise.toml"))?)?;
    let pin = table["tools"]["chezmoi"]
        .as_str()
        .context("the fixture needs a chezmoi pin")?;
    fs::write(
        fixture.root.join("mise.toml"),
        format!("[tools]\npython = '3.14.8'\nchezmoi = {pin:?}\n"),
    )?;
    let directory = fixture.host.join("data/installs/chezmoi").join(pin);
    fs::create_dir_all(&directory)?;
    fs::copy(
        String::from_utf8(output.stdout)?.trim(),
        directory.join(format!("chezmoi{}", std::env::consts::EXE_SUFFIX)),
    )?;
    fs::copy(
        fixture
            .host
            .join(format!("mise{}", std::env::consts::EXE_SUFFIX)),
        fixture
            .old
            .join(format!("chezmoi{}", std::env::consts::EXE_SUFFIX)),
    )?;
    Ok(())
}

#[test]
fn the_code_gate_and_its_children_ignore_the_fixture_host_environment() -> Result<()> {
    let CodeFixture {
        scope,
        root,
        host,
        old,
    } = code_fixture()?;
    let run = |polluted: bool| -> Result<std::process::Output> {
        let mut path = vec![host.clone()];
        if polluted {
            path.insert(0, old.clone());
        }
        path.extend(std::env::split_paths(
            &std::env::var_os("PATH").unwrap_or_default(),
        ));
        let mut command = fixture_process::command(env!("CARGO_BIN_EXE_dotfiles"), scope.path());
        command
            .args(["hosts", "code", "--gate", "python"])
            .current_dir(&root)
            .env("HOME", &host)
            .env("USERPROFILE", &host)
            .env("MISE_DATA_DIR", host.join("data"))
            .env("PATH", std::env::join_paths(path)?);
        for name in [
            "GH_TOKEN",
            "GITHUB_TOKEN",
            "SSH_AUTH_SOCK",
            "UNRELATED_SETTING",
            "ProgramData",
            "ALLUSERSPROFILE",
        ] {
            if polluted {
                command.env(name, "fixture-host-value");
            } else {
                command.env_remove(name);
            }
        }
        command.env(
            "GIT_CONFIG_GLOBAL",
            if polluted {
                host.join(".gitconfig")
            } else {
                root.join("gitconfig")
            },
        );
        Ok(command.output()?)
    };
    let clean = run(false)?;
    let polluted = run(true)?;
    for output in [&clean, &polluted] {
        ensure!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    assert_eq!(clean.stdout, polluted.stdout);
    assert_eq!(
        String::from_utf8(clean.stdout)?.replace("\r\n", "\n"),
        "pinned parent\npinned child\nhttps://github.com/third-party/project.git\n"
    );
    assert!(!host.join("data/gate-state").exists());
    Ok(())
}

fn rust_code_fixture() -> Result<(CodeFixture, platform::code_environment::Installations)> {
    const RUST_PIN: &str = "1.99.0";
    let fixture = code_fixture()?;
    let cargo = fixture.host.join(".cargo");
    let bin = cargo.join("bin");
    let proxy = fixture.host.join("data/installs/rust").join(RUST_PIN);
    let sysroot = fixture.host.join("sysroot");
    for directory in [&bin, &proxy, &sysroot.join("bin")] {
        fs::create_dir_all(directory)?;
    }
    let mise = fixture
        .host
        .join(format!("mise{}", std::env::consts::EXE_SUFFIX));
    for name in ["rustc", "rustup"] {
        let program = format!("{name}{}", std::env::consts::EXE_SUFFIX);
        fs::copy(&mise, bin.join(&program))?;
        fs::copy(&mise, proxy.join(program))?;
    }
    for (directory, name) in [
        (&bin, "unrelated-application"),
        (&sysroot.join("bin"), "cargo"),
        (&sysroot.join("bin"), "rustc"),
    ] {
        fs::copy(
            &mise,
            directory.join(format!("{name}{}", std::env::consts::EXE_SUFFIX)),
        )?;
    }
    fs::write(
        fixture.root.join("rust-sysroot"),
        sysroot.to_string_lossy().as_bytes(),
    )?;
    fs::write(
        proxy
            .parent()
            .context("Rust installation parent")?
            .join(".mise.backend.toml"),
        "short = 'rust'\nfull = 'core:rust'\n",
    )?;
    let inventory = fixture.root.join("tools.json");
    let mut tools: serde_json::Value = serde_json::from_slice(&fs::read(&inventory)?)?;
    let mut rust = tools["python"][0].clone();
    rust["version"] = RUST_PIN.into();
    rust["requested_version"] = RUST_PIN.into();
    rust["install_path"] = serde_json::json!(proxy);
    tools["rust"] = serde_json::json!([rust]);
    fs::write(inventory, serde_json::to_vec(&tools)?)?;
    fs::write(
        fixture.root.join("mise.toml"),
        format!("[tools]\npython = '3.14.8'\nrust = {RUST_PIN:?}\n"),
    )?;
    let installed = platform::code_environment::Installations {
        mise,
        data: fixture.host.join("data"),
        cargo,
        rustup: fixture.host.join(".rustup"),
        kani: fixture.host.join(".kani"),
        cache: fixture.host.join(".cache/dotfiles/hosts"),
    };
    Ok((fixture, installed))
}

#[test]
fn a_code_gate_exposes_only_its_captured_rustup_manager() -> Result<()> {
    const RUST_PIN: &str = "1.99.0";
    const NIGHTLY: &str = "nightly-2026-08-21";
    let (fixture, installed) = rust_code_fixture()?;
    let environment =
        platform::code_environment::CodeEnvironment::build(&fixture.root, &installed)?;
    for (tool, expected) in [
        (platform::tool::Tool::Rustup, "captured rustup\n"),
        (platform::tool::Tool::Cargo, "cargo 1.99.0 fixture\n"),
        (platform::tool::Tool::Rustc, "rustc 1.99.0 fixture\n"),
    ] {
        let output = environment
            .command(tool.program())
            .arg("--version")
            .output()?;
        ensure!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(
            String::from_utf8(output.stdout)?.replace("\r\n", "\n"),
            expected
        );
    }
    let miri = environment.command("cargo-miri");
    let path = miri
        .get_envs()
        .find_map(|(name, value)| (name == "PATH").then_some(value).flatten())
        .context("owned bootstrap PATH")?;
    let bootstrap = std::env::split_paths(path)
        .next()
        .context("the owned bootstrap precedes every tool")?;
    assert_ne!(bootstrap, installed.cargo.join("bin"));
    let captured = installed
        .cargo
        .join("bin")
        .join(format!("rustup{}", std::env::consts::EXE_SUFFIX));
    for name in ["rustup", "cargo-miri"] {
        let proxy = bootstrap.join(format!("{name}{}", std::env::consts::EXE_SUFFIX));
        assert!(proxy.is_file(), "the owned bootstrap must supply {name}");
        assert_eq!(
            platform::paths::canonical(&proxy)?,
            platform::paths::canonical(&captured)?
        );
        assert_eq!(fs::read(&proxy)?, fs::read(&captured)?);
    }
    let mut nightly = environment.command(platform::tool::Tool::Rustup.program());
    assert_eq!(
        nightly
            .get_envs()
            .find_map(|(name, value)| { (name == "RUSTUP_TOOLCHAIN").then_some(value).flatten() }),
        Some(std::ffi::OsStr::new(RUST_PIN))
    );
    let output = nightly
        .args(["run", NIGHTLY, "cargo", "--rustc-version"])
        .output()?;
    ensure!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        String::from_utf8(output.stdout)?.replace("\r\n", "\n"),
        format!("rustc {NIGHTLY} fixture\n")
    );
    assert!(
        environment
            .command("unrelated-application")
            .output()
            .is_err()
    );
    assert_eq!(
        fs::read(
            installed
                .cargo
                .join("bin")
                .join(format!("rustup{}", std::env::consts::EXE_SUFFIX))
        )?,
        fs::read(&installed.mise)?
    );
    Ok(())
}

#[test]
fn concurrent_code_gates_reuse_installed_tools_with_independent_writable_state() -> Result<()> {
    const CONCURRENT_BUILDS: usize = 8;
    let fixture = code_fixture()?;
    let installed = platform::code_environment::Installations {
        mise: fixture
            .host
            .join(format!("mise{}", std::env::consts::EXE_SUFFIX)),
        data: fixture.host.join("data"),
        cargo: fixture.host.join(".cargo"),
        rustup: fixture.host.join(".rustup"),
        kani: fixture.host.join(".kani"),
        cache: fixture.host.join(".cache/dotfiles/hosts"),
    };
    let barrier = std::sync::Barrier::new(CONCURRENT_BUILDS);
    std::thread::scope(|scope| -> Result<()> {
        let threads: Vec<_> = (0..CONCURRENT_BUILDS)
            .map(|_| {
                scope.spawn(|| -> Result<_> {
                    barrier.wait();
                    let environment = platform::code_environment::CodeEnvironment::build(
                        &fixture.root,
                        &installed,
                    )?;
                    let output = environment.command("python").output()?;
                    ensure!(
                        output.status.success(),
                        "{}",
                        String::from_utf8_lossy(&output.stderr)
                    );
                    let home = environment
                        .command("python")
                        .get_envs()
                        .find_map(|(name, value)| (name == "HOME").then_some(value).flatten())
                        .context("code home")?
                        .to_owned();
                    Ok((environment, home, output.stdout))
                })
            })
            .collect();
        let environments: Vec<_> = threads
            .into_iter()
            .map(|thread| thread.join().expect("the code builder completed"))
            .collect::<Result<_>>()?;
        let homes: std::collections::BTreeSet<_> =
            environments.iter().map(|(_, home, _)| home).collect();
        assert_eq!(homes.len(), CONCURRENT_BUILDS);
        for (_, _, output) in &environments {
            assert_eq!(
                String::from_utf8_lossy(output).replace("\r\n", "\n"),
                "pinned parent\npinned child\nhttps://github.com/third-party/project.git\n"
            );
        }
        assert!(!fixture.host.join("data/gate-state").exists());
        Ok(())
    })
}

#[test]
fn code_builds_survive_scratch_cleanup_and_stay_host_local() -> Result<()> {
    let fixture = code_fixture()?;
    let mut installed = platform::code_environment::Installations {
        mise: fixture
            .host
            .join(format!("mise{}", std::env::consts::EXE_SUFFIX)),
        data: fixture.host.join("data"),
        cargo: fixture.host.join(".cargo"),
        rustup: fixture.host.join(".rustup"),
        kani: fixture.host.join(".kani"),
        cache: fixture.host.join(".cache/dotfiles/hosts"),
    };
    let target = |environment: &platform::code_environment::CodeEnvironment| -> Result<PathBuf> {
        Ok(environment
            .command("cargo")
            .get_envs()
            .find_map(|(name, value)| (name == "CARGO_TARGET_DIR").then_some(value).flatten())
            .context("code target")?
            .into())
    };
    let first = platform::code_environment::CodeEnvironment::build(&fixture.root, &installed)?;
    let build = target(&first)?;
    assert!(
        build.starts_with(fixture.host.join(".cache")),
        "{}",
        build.display()
    );
    fs::create_dir_all(&build)?;
    fs::write(build.join("cached-output"), "built")?;
    drop(first);
    let second = platform::code_environment::CodeEnvironment::build(&fixture.root, &installed)?;
    assert_eq!(build, target(&second)?);
    assert_eq!(fs::read(build.join("cached-output"))?, b"built");
    installed.cargo = fixture.scope.path().join("other-host/.cargo");
    installed.cache = fixture
        .scope
        .path()
        .join("other-host/.cache/dotfiles/hosts");
    let other = platform::code_environment::CodeEnvironment::build(&fixture.root, &installed)?;
    assert_ne!(build, target(&other)?);
    Ok(())
}

#[test]
fn the_code_environment_owns_mise_after_its_installation_is_removed() -> Result<()> {
    const LOCATIONS: [&str; 5] = [
        ".local/bin",
        "opt/homebrew/Cellar/mise/pinned/bin",
        "usr/local/Cellar/mise/pinned/bin",
        "opt/local/bin",
        "nix/store/pinned-mise/bin",
    ];
    let fixture = code_fixture()?;
    let executable = format!("mise{}", std::env::consts::EXE_SUFFIX);
    for location in LOCATIONS {
        let installation = tempfile::tempdir_in(fixture.scope.path())?;
        let directory = installation.path().join(location);
        fs::create_dir_all(&directory)?;
        let mise = directory.join(&executable);
        fs::copy(fixture.host.join(&executable), &mise)?;
        let installed = platform::code_environment::Installations {
            mise,
            data: fixture.host.join("data"),
            cargo: fixture.host.join(".cargo"),
            rustup: fixture.host.join(".rustup"),
            kani: fixture.host.join(".kani"),
            cache: fixture.host.join(".cache/dotfiles/hosts"),
        };
        let environment =
            platform::code_environment::CodeEnvironment::build(&fixture.root, &installed)?;
        installation.close()?;
        let output = environment.command("mise").arg("--version").output()?;
        ensure!(
            output.status.success(),
            "{location}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(
            String::from_utf8(output.stdout)?.replace("\r\n", "\n"),
            "2026.10.0 fixture-platform (2026-10-01)\n",
            "{location}"
        );
    }
    Ok(())
}

#[test]
fn code_proofs_reuse_only_the_pinned_kani_bundle_without_host_configuration() -> Result<()> {
    let fixture = code_fixture()?;
    let kani = fixture.host.join(".kani");
    let selected = kani.join("kani-0.68.0");
    let other = kani.join("kani-0.67.0");
    for bundle in [&selected, &other] {
        fs::create_dir_all(bundle)?;
        fs::write(bundle.join("verifier"), "fixed release")?;
    }
    fs::write(kani.join("kani.toml"), "private host settings")?;
    let tools = fixture.root.join("tools.json");
    let mut inventory: serde_json::Value = serde_json::from_slice(&fs::read(&tools)?)?;
    let mut entry = inventory["python"][0].clone();
    let verifier = fixture
        .host
        .join("data/installs/cargo-kani-verifier/0.68.0");
    fs::create_dir_all(&verifier)?;
    fs::write(
        verifier.parent().unwrap().join(".mise.backend.toml"),
        "short = 'kani-verifier'\nfull = 'cargo:kani-verifier'\n",
    )?;
    entry["version"] = "0.68.0".into();
    entry["requested_version"] = "0.68.0".into();
    entry["install_path"] = serde_json::json!(verifier);
    inventory["cargo:kani-verifier"] = serde_json::json!([entry]);
    fs::write(tools, serde_json::to_vec(&inventory)?)?;
    let installed = platform::code_environment::Installations {
        mise: fixture
            .host
            .join(format!("mise{}", std::env::consts::EXE_SUFFIX)),
        data: fixture.host.join("data"),
        cargo: fixture.host.join(".cargo"),
        rustup: fixture.host.join(".rustup"),
        kani,
        cache: fixture.host.join(".cache/dotfiles/hosts"),
    };
    let environment =
        platform::code_environment::CodeEnvironment::build(&fixture.root, &installed)?;
    let command = environment.command("kani");
    let home = command
        .get_envs()
        .find_map(|(name, value)| (name == "HOME").then_some(value).flatten())
        .unwrap();
    let borrowed = Path::new(home).join(".kani");
    assert_eq!(
        fs::read(borrowed.join("kani-0.68.0/verifier"))?,
        b"fixed release"
    );
    assert!(!borrowed.join("kani-0.67.0").exists());
    assert!(!borrowed.join("kani.toml").exists());
    drop(environment);
    assert_eq!(fs::read(selected.join("verifier"))?, b"fixed release");
    assert_eq!(
        fs::read(installed.kani.join("kani.toml"))?,
        b"private host settings"
    );
    Ok(())
}

#[test]
fn a_code_gate_can_construct_another_isolated_code_gate() -> Result<()> {
    let fixture = code_fixture()?;
    let installed = platform::code_environment::Installations {
        mise: fixture
            .host
            .join(format!("mise{}", std::env::consts::EXE_SUFFIX)),
        data: fixture.host.join("data"),
        cargo: fixture.host.join(".cargo"),
        rustup: fixture.host.join(".rustup"),
        kani: fixture.host.join(".kani"),
        cache: fixture.host.join(".cache/dotfiles/hosts"),
    };
    let environment =
        platform::code_environment::CodeEnvironment::build(&fixture.root, &installed)?;
    let output = environment
        .command(env!("CARGO_BIN_EXE_dotfiles"))
        .args(["hosts", "code", "--gate", "python"])
        .output()?;
    ensure!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        String::from_utf8(output.stdout)?.replace("\r\n", "\n"),
        "pinned parent\npinned child\nhttps://github.com/third-party/project.git\n"
    );
    assert!(!fixture.host.join("data/gate-state").exists());
    Ok(())
}

#[test]
fn a_mise_bootstrap_with_a_different_release_pin_stops_before_the_gate() -> Result<()> {
    let fixture = code_fixture()?;
    fs::write(
        fixture.root.join("home/.chezmoidata/versions.toml"),
        "[mise]\nversion = '2026.9.0'\n",
    )?;
    let mut path = vec![fixture.host.clone()];
    path.extend(std::env::split_paths(
        &std::env::var_os("PATH").unwrap_or_default(),
    ));
    let output = fixture_process::command(env!("CARGO_BIN_EXE_dotfiles"), fixture.scope.path())
        .args(["hosts", "code", "--gate", "python"])
        .current_dir(&fixture.root)
        .env("HOME", &fixture.host)
        .env("USERPROFILE", &fixture.host)
        .env("PATH", std::env::join_paths(path)?)
        .output()?;
    assert!(!output.status.success());
    assert!(String::from_utf8(output.stderr)?.contains("release pin 2026.9.0"));
    assert_eq!(output.stdout, [] as [u8; 0]);
    Ok(())
}

#[test]
fn a_different_native_mise_uses_the_source_launcher_only_in_scratch() -> Result<()> {
    let fixture = code_fixture()?;
    seed_renderer(&fixture)?;
    let native = fixture
        .host
        .join(format!("mise{}", std::env::consts::EXE_SUFFIX));
    let pinned = fixture
        .root
        .join(format!("pinned-mise{}", std::env::consts::EXE_SUFFIX));
    fs::copy(&native, &pinned)?;
    fs::write(native.with_extension("version"), "2026.10.2")?;
    let partials = fixture.root.join("home/.chezmoitemplates/partials");
    fs::create_dir_all(&partials)?;
    let (partial, script) = if cfg!(windows) {
        (
            "install-mise-ps1",
            format!(
                "$bin = Join-Path $env:USERPROFILE '.local/bin'; New-Item -ItemType Directory -Force $bin | Out-Null; Copy-Item -LiteralPath '{}' -Destination (Join-Path $bin mise.exe)",
                pinned.to_string_lossy().replace('\'', "''")
            ),
        )
    } else {
        (
            "install-mise-sh",
            format!(
                "mkdir -p \"$HOME/.local/bin\" && cp '{}' \"$HOME/.local/bin/mise\"",
                pinned.to_string_lossy().replace('\'', "'\"'\"'")
            ),
        )
    };
    fs::write(partials.join(partial), script)?;
    let mut paths = vec![fixture.old.clone(), fixture.host.clone()];
    paths.extend(std::env::split_paths(
        &std::env::var_os("PATH").unwrap_or_default(),
    ));
    let output = fixture_process::command(env!("CARGO_BIN_EXE_dotfiles"), fixture.scope.path())
        .args(["hosts", "code", "--gate", "mise --version && python"])
        .current_dir(&fixture.root)
        .env("HOME", &fixture.host)
        .env("USERPROFILE", &fixture.host)
        .env("MISE_DATA_DIR", fixture.host.join("data"))
        .env("__MISE_DIFF", "inherited-host-activation")
        .env("__MISE_ORIG_PATH", &fixture.old)
        .env("PATH", std::env::join_paths(paths)?)
        .output()?;
    ensure!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        String::from_utf8(output.stdout)?.replace("\r\n", "\n"),
        "2026.10.0 fixture-platform (2026-10-01)\npinned parent\npinned child\nhttps://github.com/third-party/project.git\n"
    );
    assert_eq!(fs::read(native.with_extension("version"))?, b"2026.10.2");
    assert_eq!(fs::read(native)?, fs::read(pinned)?);
    assert!(!fixture.host.join(".local/bin/mise").exists());
    assert!(!fixture.host.join(".local/bin/mise.exe").exists());
    Ok(())
}

#[test]
fn a_missing_pinned_renderer_installs_only_in_the_candidate_home() -> Result<()> {
    let fixture = code_fixture()?;
    seed_renderer(&fixture)?;
    let table: toml::Table = toml::from_str(&fs::read_to_string(fixture.root.join("mise.toml"))?)?;
    let pin = table["tools"]["chezmoi"].as_str().context("renderer pin")?;
    let native = fixture
        .host
        .join(format!("mise{}", std::env::consts::EXE_SUFFIX));
    let directory = fixture.host.join("data/installs/chezmoi").join(pin);
    let installed = directory.join(format!("chezmoi{}", std::env::consts::EXE_SUFFIX));
    let stored = fixture
        .root
        .join(format!("renderer{}", std::env::consts::EXE_SUFFIX));
    fs::rename(&installed, &stored)?;
    fs::write(
        native.with_extension("renderer"),
        stored.to_string_lossy().as_bytes(),
    )?;
    let config = fixture.scope.path().join("chezmoi.toml");
    fs::write(
        &config,
        format!(
            "[data]\nprofile = {:?}\n",
            dotfiles::profiles::native_profile()?.name()
        ),
    )?;
    let scratch = dotfiles::host_checks::Scratch::new(&config)?;
    let storage = platform::code_environment::Installations {
        mise: native,
        data: fixture.host.join("data"),
        cargo: fixture.host.join(".cargo"),
        rustup: fixture.host.join(".rustup"),
        kani: fixture.host.join(".kani"),
        cache: fixture.host.join(".cache/dotfiles/hosts"),
    };
    let renderer = scratch.renderer(&fixture.root, &storage)?;
    assert_eq!(fs::read(&renderer)?, fs::read(stored)?);
    assert!(!renderer.starts_with(&fixture.host));
    assert!(!installed.exists());
    Ok(())
}

#[cfg(windows)]
#[test]
fn invalid_registered_compiler_data_stops_before_the_gate() -> Result<()> {
    let fixture = code_fixture()?;
    let mut path = vec![fixture.host.clone()];
    path.extend(std::env::split_paths(
        &std::env::var_os("PATH").unwrap_or_default(),
    ));
    for data in [
        "{}".to_owned(),
        serde_json::json!({
            "executable": "cl.exe", "directories": [], "libraries": [],
            "includes": [], "library_paths": []
        })
        .to_string(),
    ] {
        let output = fixture_process::command(env!("CARGO_BIN_EXE_dotfiles"), fixture.scope.path())
            .args(["hosts", "code", "--gate", "python"])
            .current_dir(&fixture.root)
            .env("HOME", &fixture.host)
            .env("USERPROFILE", &fixture.host)
            .env("PATH", std::env::join_paths(&path)?)
            .env("DOTFILES_HOST_COMPILER", data)
            .output()?;
        assert!(!output.status.success());
        assert!(String::from_utf8(output.stderr)?.contains("registered compiler"));
        assert_eq!(output.stdout, [] as [u8; 0]);
    }
    Ok(())
}

#[cfg(windows)]
#[test]
fn the_code_environment_runs_batch_commands_from_powershell() -> Result<()> {
    let fixture = code_fixture()?;
    let installed = platform::code_environment::Installations {
        mise: fixture.host.join("mise.exe"),
        data: fixture.host.join("data"),
        cargo: fixture.host.join(".cargo"),
        rustup: fixture.host.join(".rustup"),
        kani: fixture.host.join(".kani"),
        cache: fixture.host.join(".cache/dotfiles/hosts"),
    };
    let environment =
        platform::code_environment::CodeEnvironment::build(&fixture.root, &installed)?;
    fs::write(fixture.root.join("native.cmd"), "@echo batch gate\r\n")?;
    let output = environment
        .command(platform::tool::Tool::Pwsh.program())
        .args(["-NoProfile", "-Command", "& ./native.cmd"])
        .output()?;
    ensure!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(String::from_utf8(output.stdout)?.trim(), "batch gate");
    Ok(())
}

#[cfg(windows)]
#[test]
fn the_code_environment_links_with_the_registered_windows_toolchain() -> Result<()> {
    let fixture = code_fixture()?;
    let sysroot = platform::tool::Tool::Rustc
        .command()
        .args(["--print", "sysroot"])
        .output()?;
    ensure!(sysroot.status.success(), "resolve the pinned Rust compiler");
    let compiler = Path::new(String::from_utf8(sysroot.stdout)?.trim()).join("bin/rustc.exe");
    let installed = platform::code_environment::Installations {
        mise: fixture.host.join("mise.exe"),
        data: fixture.host.join("data"),
        cargo: fixture.host.join(".cargo"),
        rustup: fixture.host.join(".rustup"),
        kani: fixture.host.join(".kani"),
        cache: fixture.host.join(".cache/dotfiles/hosts"),
    };
    let environment =
        platform::code_environment::CodeEnvironment::build(&fixture.root, &installed)?;
    let source = fixture.root.join("native.rs");
    let binary = fixture.root.join("native.exe");
    fs::write(&source, "fn main() {}\n")?;
    let linked = environment
        .command(&compiler)
        .arg(&source)
        .arg("-o")
        .arg(&binary)
        .output()?;
    ensure!(
        linked.status.success(),
        "{}",
        String::from_utf8_lossy(&linked.stderr)
    );
    assert!(environment.command(&binary).status()?.success());
    Ok(())
}

#[test]
fn a_missing_exact_pin_installs_only_in_the_code_scratch_home() -> Result<()> {
    let fixture = code_fixture()?;
    fs::write(fixture.root.join("missing"), "")?;
    let listing = fixture.root.join("tools.json");
    let mut tools: serde_json::Value = serde_json::from_slice(&fs::read(&listing)?)?;
    tools["python"][0]["installed"] = false.into();
    tools["python"][0]["active"] = false.into();
    let existing = fixture.host.join("data/installs/uv/0.12.22");
    fs::create_dir_all(&existing)?;
    fs::write(existing.join("kept"), "read-only installation")?;
    tools["uv"] = tools["python"].clone();
    tools["uv"][0]["installed"] = true.into();
    tools["uv"][0]["active"] = true.into();
    tools["uv"][0]["version"] = "0.12.22".into();
    tools["uv"][0]["requested_version"] = "0.12.22".into();
    tools["uv"][0]["install_path"] = existing.to_string_lossy().into_owned().into();
    fs::write(&listing, serde_json::to_vec(&tools)?)?;
    let original = fs::read(&listing)?;
    let paths = fs::read(fixture.root.join("paths"))?;
    let mut path = vec![fixture.host.clone()];
    path.extend(std::env::split_paths(
        &std::env::var_os("PATH").unwrap_or_default(),
    ));
    let run = || -> Result<std::process::Output> {
        Ok(
            fixture_process::command(env!("CARGO_BIN_EXE_dotfiles"), fixture.scope.path())
                .args(["hosts", "code", "--gate", "python"])
                .current_dir(&fixture.root)
                .env("HOME", &fixture.host)
                .env("USERPROFILE", &fixture.host)
                .env("MISE_DATA_DIR", fixture.host.join("data"))
                .env("PATH", std::env::join_paths(&path)?)
                .output()?,
        )
    };
    let output = run()?;
    ensure!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(String::from_utf8(output.stdout)?.contains("pinned child"));
    let installed = PathBuf::from(fs::read_to_string(fixture.root.join("installed-at"))?);
    assert!(!installed.starts_with(&fixture.host));
    assert!(!installed.exists());
    assert_eq!(
        fs::read_to_string(existing.join("kept"))?,
        "read-only installation"
    );
    assert!(fixture.host.join("data/installs/python/3.14.8").is_dir());
    assert_eq!(fs::read(&listing)?, original);
    assert_eq!(fs::read(fixture.root.join("paths"))?, paths);
    fs::remove_file(fixture.root.join("installed-at"))?;
    tools["python"][0]["source"]["type"] = "global".into();
    fs::write(&listing, serde_json::to_vec(&tools)?)?;
    assert!(!run()?.status.success());
    assert!(!fixture.root.join("installed-at").exists());
    Ok(())
}

#[test]
fn an_unusable_installation_link_is_replaced_only_in_scratch() -> Result<()> {
    let fixture = code_fixture()?;
    fs::write(fixture.root.join("relocate"), "")?;
    let binary = fixture
        .host
        .join("data/installs/python/3.14.8")
        .join(format!("python{}", std::env::consts::EXE_SUFFIX));
    let original = fs::read(&binary)?;
    let mut path = vec![fixture.host.clone()];
    path.extend(std::env::split_paths(
        &std::env::var_os("PATH").unwrap_or_default(),
    ));
    let output = fixture_process::command(env!("CARGO_BIN_EXE_dotfiles"), fixture.scope.path())
        .args(["hosts", "code", "--gate", "python"])
        .current_dir(&fixture.root)
        .env("HOME", &fixture.host)
        .env("USERPROFILE", &fixture.host)
        .env("MISE_DATA_DIR", fixture.host.join("data"))
        .env("PATH", std::env::join_paths(path)?)
        .output()?;
    ensure!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(String::from_utf8(output.stdout)?.contains("pinned child"));
    let installed = PathBuf::from(fs::read_to_string(fixture.root.join("installed-at"))?);
    assert!(!installed.starts_with(&fixture.host));
    assert!(!installed.exists());
    assert!(!fixture.host.join("data/gate-state").exists());
    assert_eq!(fs::read(binary)?, original);
    Ok(())
}

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
    ["code", "real"].map(|stage| format!(r#"{{"host":"h","os":"{os}","tree":"{tree}","status":{status},"gate":"just check","stage":"{stage}"}}"#)).join("\n")
}

fn candidate_record(root: &Path, os: &str, tree: &str, status: i32) -> Result<String> {
    let gate = dotfiles::hosts::DEFAULT_GATE;
    let inputs = dotfiles::hosts::input_hashes(root, "HEAD", gate)?;
    let count = if git(root, &["rev-parse", "--verify", dotfiles::hosts::NOTES]).is_ok() {
        git(root, &["rev-list", "--count", dotfiles::hosts::NOTES])?.parse::<u64>()?
    } else {
        0
    };
    let generation = count
        .checked_add(1)
        .context("fixture generation overflow")?;
    ["code", "real"]
        .into_iter()
        .zip(inputs)
        .map(|(stage, inputs)| {
            serde_json::to_string(&serde_json::json!({
                "host": "fixture", "os": os, "tree": tree, "status": status,
                "gate": gate, "stage": stage, "inputs": inputs, "generation": generation
            }))
            .map_err(Into::into)
        })
        .collect::<Result<Vec<_>>>()
        .map(|notes| notes.join("\n"))
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
    assert_eq!(
        evidence(
            r#"{"host":"h","os":"linux","tree":"t","status":0,"gate":"just check"}"#,
            "t"
        )?,
        [Evidence::Missing; 3]
    );
    let code =
        r#"{"host":"h","os":"linux","tree":"t","status":0,"gate":"just check","stage":"code"}"#;
    assert_eq!(evidence(code, "t")?, [Evidence::Missing; 3]);
    let real =
        r#"{"host":"h","os":"linux","tree":"t","status":1,"gate":"just check","stage":"real"}"#;
    assert_eq!(
        evidence(&format!("{code}\n{real}"), "t")?,
        [Evidence::Failed, Evidence::Missing, Evidence::Missing]
    );
    Ok(())
}

fn git(directory: &Path, arguments: &[&str]) -> Result<String> {
    let output = fixture_git::command("git", &directory.join("gitconfig"))
        .current_dir(directory)
        .args(arguments)
        .output()?;
    ensure!(
        output.status.success(),
        "git {arguments:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    Ok(String::from_utf8(output.stdout)?.trim().to_owned())
}

fn link_directory(source: &Path, destination: &Path) -> Result<()> {
    #[cfg(unix)]
    std::os::unix::fs::symlink(source, destination)?;
    #[cfg(windows)]
    std::os::windows::fs::symlink_dir(source, destination)?;
    Ok(())
}

struct BootstrapFixture {
    scope: tempfile::TempDir,
    cargo: PathBuf,
    rustup: PathBuf,
    mise: PathBuf,
    pin: String,
}

fn bootstrap_fixture() -> Result<BootstrapFixture> {
    let scope = tempfile::tempdir()?;
    let directory = platform::paths::canonical(scope.path())?;
    let root = directory.join("project");
    let host = directory.join("host");
    let job = directory.join("job");
    for directory in [&root, &host, &job] {
        fs::create_dir_all(directory)?;
    }
    let home = platform::home::home().context("native tool home")?;
    let installed = platform::code_environment::Installations::discover(&home)?;
    let cargo = installed.cargo.clone();
    let rustup = platform::paths::canonical(
        &cargo
            .join("bin")
            .join(format!("rustup{}", std::env::consts::EXE_SUFFIX)),
    )?;
    link_directory(&installed.rustup, &host.join(".rustup"))?;
    let data = if cfg!(windows) {
        host.join("AppData/Local/mise")
    } else {
        host.join(".local/share/mise")
    };
    let versions: toml::Value = toml::from_str(&fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../mise.toml"
    ))?)?;
    let pin = versions["tools"]["rust"]["version"]
        .as_str()
        .context("project Rust pin")?;
    let installs = data.join("installs/rust");
    fs::create_dir_all(&installs)?;
    fs::write(
        installs.join(".mise.backend.toml"),
        "short = 'rust'\nfull = 'core:rust'\n",
    )?;
    link_directory(cargo.join("bin").as_path(), &installs.join(pin))?;
    fs::write(root.join("mise.toml"), format!("[tools]\nrust = {pin:?}\n"))?;
    fs::write(
        root.join("Cargo.toml"),
        "[package]\nname = 'dotfiles'\nversion = '0.0.0'\nedition = '2024'\n",
    )?;
    fs::write(
        root.join("Cargo.lock"),
        "version = 4\n[[package]]\nname = 'dotfiles'\nversion = '0.0.0'\n",
    )?;
    fs::create_dir(root.join("src"))?;
    fs::copy(
        concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/bootstrap.rs"),
        root.join("src/main.rs"),
    )?;
    fs::write(
        root.join("gitconfig"),
        "[user]\nname = Fixture\nemail = fixture@example.com\n[commit]\ngpgsign = false\n",
    )?;
    git(&root, &["init", "--quiet"])?;
    git(
        &root,
        &["add", "Cargo.toml", "Cargo.lock", "mise.toml", "src"],
    )?;
    git(&root, &["commit", "--quiet", "-m", "bootstrap"])?;
    git(
        &root,
        &[
            "bundle",
            "create",
            job.join("head.bundle").to_str().context("bundle path")?,
            "HEAD",
        ],
    )?;
    Ok(BootstrapFixture {
        scope,
        cargo,
        rustup,
        mise: installed.mise,
        pin: pin.to_owned(),
    })
}

fn assert_bootstrap_cargo(path: &str, job: &Path, cargo: &Path) -> Result<()> {
    let program = format!("cargo{}", std::env::consts::EXE_SUFFIX);
    let proxy = std::env::split_paths(path)
        .map(|directory| directory.join(&program))
        .find(|executable| executable.is_file())
        .context("bootstrap Cargo executable")?;
    assert_eq!(
        proxy.parent(),
        Some(job.join("bootstrap/.cargo/bin").as_path())
    );
    assert_eq!(
        platform::paths::canonical(&proxy)?,
        platform::paths::canonical(&cargo.join("bin").join(program))?
    );
    Ok(())
}

#[test]
fn the_remote_bootstrap_uses_native_rust_tools_with_owned_writable_state() -> Result<()> {
    let BootstrapFixture {
        scope,
        cargo,
        rustup,
        mise,
        pin,
    } = bootstrap_fixture()?;
    let directory = platform::paths::canonical(scope.path())?;
    let root = directory.join("project");
    let host = directory.join("host");
    let job = directory.join("job");
    let global = host.join("untrusted.toml");
    fs::write(&global, "[tools]\nrust = 'invalid-native-version'\n")?;
    let family = if cfg!(windows) {
        Family::Windows
    } else if cfg!(target_os = "macos") {
        Family::Mac
    } else {
        Family::Linux
    };
    let argv = wrap(family, &remote_gate(family, "bootstrap fixture"));
    let mut paths = vec![mise.parent().context("mise directory")?.to_path_buf()];
    paths.extend(std::env::split_paths(
        &std::env::var_os("PATH").unwrap_or_default(),
    ));
    let output = fixture_git::command(&argv[0], &root.join("gitconfig"))
        .args(&argv[1..])
        .current_dir(&job)
        .env("HOME", &host)
        .env("USERPROFILE", &host)
        .env("LOCALAPPDATA", host.join("AppData/Local"))
        .env("CARGO_HOME", &cargo)
        .env("MISE_GLOBAL_CONFIG_FILE", &global)
        .env("PATH", std::env::join_paths(paths)?)
        .output()?;
    ensure!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8(output.stdout)?;
    let values: std::collections::BTreeMap<_, _> = stdout
        .lines()
        .filter_map(|line| line.split_once('='))
        .collect();
    for (name, expected) in [
        ("HOME", job.join("bootstrap")),
        ("CARGO_HOME", host.join(".cache/dotfiles/hosts/cargo")),
        ("MISE_CARGO_HOME", host.join(".cache/dotfiles/hosts/cargo")),
        ("MISE_GLOBAL_CONFIG_FILE", job.join("bootstrap/empty.toml")),
        ("MISE_SYSTEM_CONFIG_FILE", job.join("bootstrap/empty.toml")),
    ] {
        assert_eq!(Path::new(values[name]), expected);
    }
    assert_bootstrap_cargo(values["PATH"], &job, &cargo)?;
    assert!(!Path::new(values["CARGO_HOME"]).join("bin").exists());
    let compiler = platform::tool::Tool::Rustc.command().arg("-vV").output()?;
    ensure!(compiler.status.success(), "resolve the Rust host triple");
    let compiler = String::from_utf8(compiler.stdout)?;
    let triple = compiler
        .lines()
        .find_map(|line| line.strip_prefix("host: "))
        .context("Rust host triple")?;
    let active = fixture_git::command(&rustup, &root.join("gitconfig"))
        .env("RUSTUP_HOME", host.join(".rustup"))
        .env("RUSTUP_TOOLCHAIN", values["RUSTUP_TOOLCHAIN"])
        .args(["show", "active-toolchain"])
        .output()?;
    ensure!(active.status.success(), "resolve the bootstrap toolchain");
    let active = String::from_utf8(active.stdout)?;
    assert_eq!(
        active.split_whitespace().next(),
        Some(format!("{pin}-{triple}").as_str())
    );
    assert_eq!(
        platform::paths::canonical(
            &job.join("bootstrap/.cargo/bin")
                .join(format!("rustup{}", std::env::consts::EXE_SUFFIX))
        )?,
        rustup
    );
    assert_eq!(
        fs::read_to_string(&global)?,
        "[tools]\nrust = 'invalid-native-version'\n"
    );
    assert!(!host.join(".cargo").exists());
    Ok(())
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
    fs::create_dir_all(root.join("crates/dotfiles"))?;
    fs::write(root.join(".chezmoiroot"), "home\n")?;
    fs::write(
        root.join("crates/dotfiles/Cargo.toml"),
        "[package]\nname = 'dotfiles'\n",
    )?;
    fs::write(
        root.join(".github/workflows/ci.yml"),
        "jobs:\n  test:\n    strategy:\n      matrix:\n        os: [ubuntu-latest, macos-latest]\n    runs-on: ${{ matrix.os }}\n",
    )?;
    git(root, &["init", "--quiet"])?;
    git(root, &["add", ".github", ".chezmoiroot", "crates"])?;
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
                &candidate_record(root, os, tree, status)?,
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
    assert_unsequenced_candidates_are_refused(root, &tree, refused)?;
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
    let wrong_gate = candidate_record(root, "linux", &tree, 0)?
        .replace(dotfiles::hosts::DEFAULT_GATE, "just check");
    git(
        root,
        &[
            "notes",
            "--ref",
            "refs/notes/hosts",
            "append",
            "-m",
            &wrong_gate,
            "HEAD",
        ],
    )?;
    refused("no linux gate result");
    note("linux", &tree, 0)?;
    refused("no macos gate result");
    note("macos", "another-tree", 0)?;
    refused("no macos gate result");
    note("macos", &tree, 1)?;
    refused("the macos gate failed");
    note("macos", &tree, 0)?;
    push_gate(root, github, update.as_bytes())?;
    assert!(push_gate(root, github, b"malformed\n").is_err());
    assert_newer_failure_invalidates_candidate(root, &tree, &head, note, refused)
}

fn assert_unsequenced_candidates_are_refused(
    root: &Path,
    tree: &str,
    refused: impl Fn(&str),
) -> Result<()> {
    refused("no linux gate result");
    git(
        root,
        &[
            "notes",
            "--ref",
            "refs/notes/hosts",
            "append",
            "-m",
            &record("linux", tree, 0),
            "HEAD",
        ],
    )?;
    refused("no linux gate result");
    let legacy = candidate_record(root, "linux", tree, 0)?
        .lines()
        .map(|line| -> Result<String> {
            let mut note: serde_json::Value = serde_json::from_str(line)?;
            note.as_object_mut()
                .context("legacy fixture")?
                .remove("generation");
            Ok(serde_json::to_string(&note)?)
        })
        .collect::<Result<Vec<_>>>()?
        .join("\n");
    git(
        root,
        &[
            "notes",
            "--ref",
            dotfiles::hosts::NOTES,
            "append",
            "-m",
            &legacy,
            "HEAD",
        ],
    )?;
    refused("no linux gate result");
    Ok(())
}

fn assert_newer_failure_invalidates_candidate(
    root: &Path,
    tree: &str,
    head: &str,
    note: impl Fn(&str, &str, i32) -> Result<()>,
    refused: impl Fn(&str),
) -> Result<()> {
    let previous = git(
        root,
        &["notes", "--ref", dotfiles::hosts::NOTES, "show", head],
    )?;
    git(
        root,
        &["commit", "--quiet", "--allow-empty", "-m", "failed run"],
    )?;
    note("linux", tree, 1)?;
    refused("the linux gate failed");
    git(
        root,
        &["commit", "--quiet", "--allow-empty", "-m", "new candidate"],
    )?;
    let candidate = git(root, &["rev-parse", "HEAD"])?;
    assert!(require_admission(root, &candidate).is_err());
    let mut copies = Vec::new();
    for (line, text) in previous
        .lines()
        .enumerate()
        .filter(|(_, text)| !text.is_empty())
    {
        let mut record: serde_json::Value = serde_json::from_str(text)?;
        if record["status"] == 0 && record["generation"].is_u64() {
            record["reused"] = serde_json::json!({
                "commit": head, "line": line + 1, "host": record["host"]
            });
            copies.push(serde_json::to_string(&record)?);
        }
    }
    git(
        root,
        &[
            "notes",
            "--ref",
            dotfiles::hosts::NOTES,
            "append",
            "-m",
            &copies.join("\n"),
            "HEAD",
        ],
    )?;
    assert!(require_admission(root, &candidate).is_err());
    note("linux", tree, 0)?;
    require_admission(root, &candidate)?;
    assert!(require_admission(root, head).is_err());
    Ok(())
}

#[test]
fn profile_admission_uses_the_commit_and_leaves_other_repositories_to_their_gates() -> Result<()> {
    let scope = tempfile::tempdir()?;
    let root = scope.path();
    fs::write(
        root.join("gitconfig"),
        "[user]\n\tname = Fixture\n\temail = fixture@example.com\n[commit]\n\tgpgsign = false\n",
    )?;
    fs::create_dir_all(root.join(".github/workflows"))?;
    fs::write(
        root.join(".github/workflows/ci.yml"),
        "jobs:\n  test:\n    strategy:\n      matrix:\n        os: ${{ fromJSON(needs.changes.outputs.test_os) }}\n    runs-on: ${{ matrix.os }}\n",
    )?;
    git(root, &["init", "--quiet"])?;
    git(root, &["add", ".github"])?;
    git(root, &["commit", "--quiet", "-m", "ci"])?;
    let head = git(root, &["rev-parse", "HEAD"])?;
    let update = format!(
        "refs/heads/feature {head} refs/heads/feature {}\n",
        "0".repeat(40)
    );
    let github = "git@github.com:owner/project.git";
    push_gate(root, github, update.as_bytes())?;
    require_admission(root, &head)?;
    assert!(require_admission(root, "missing-commit").is_err());
    fs::create_dir_all(root.join("crates/dotfiles"))?;
    fs::write(root.join(".chezmoiroot"), "home\n")?;
    fs::write(
        root.join("crates/dotfiles/Cargo.toml"),
        "[package]\nname = 'dotfiles'\n",
    )?;
    require_admission(root, &head)?;
    git(root, &["add", ".chezmoiroot", "crates"])?;
    git(root, &["commit", "--quiet", "-m", "profile"])?;
    let profile = git(root, &["rev-parse", "HEAD"])?;
    assert!(require_admission(root, &profile).is_err());
    fs::remove_file(root.join(".chezmoiroot"))?;
    fs::remove_file(root.join("crates/dotfiles/Cargo.toml"))?;
    assert!(require_admission(root, &profile).is_err());
    require_admission(root, &head)?;
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
        let status = platform::process::command("rustc")
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

fn prepare_host_check_project(root: &Path, home: &Path) -> Result<()> {
    fs::create_dir_all(root.join(".github/workflows"))?;
    fs::create_dir_all(root.join("crates/dotfiles"))?;
    fs::write(root.join(".chezmoiroot"), "home\n")?;
    fs::write(
        root.join("crates/dotfiles/Cargo.toml"),
        "[package]\nname = 'dotfiles'\n",
    )?;
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
    git(root, &["init", "--quiet"])?;
    git(root, &["add", "."])?;
    git(root, &["commit", "--quiet", "-m", "ci"])?;
    Ok(())
}

#[test]
fn an_unreachable_host_stops_the_check_with_its_next_action() -> Result<()> {
    let tools = tools()?;
    let scope = tempfile::tempdir()?;
    let root = scope.path().join("project");
    let home = scope.path().join("home");
    prepare_host_check_project(&root, &home)?;
    let mut paths = vec![tools.path().to_path_buf()];
    paths.extend(std::env::split_paths(
        &std::env::var_os("PATH").unwrap_or_default(),
    ));
    let status = |online: bool| {
        format!(
            r#"{{"Self":{{"DNSName":"phone.example.ts.net.","OS":"android","Online":true}},"Peer":{{"a":{{"DNSName":"box.example.ts.net.","OS":"linux","TailscaleIPs":["100.64.0.2"],"Online":{online}}}}}}}"#
        )
    };
    let stages = std::cell::RefCell::new(r#"{"code":0,"real":0,"applied":{"drifted_targets":[".first",".nested/target with spaces"],"verification_error":"fixture verification error"}}"#.to_owned());
    let check = |online: bool, doctor: &str| -> Result<std::process::Output> {
        Ok(
            fixture_process::command(env!("CARGO_BIN_EXE_dotfiles"), scope.path())
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
                .env(
                    "HOSTS_FIXTURE_SPACE",
                    if doctor == "full" {
                        "1"
                    } else {
                        "100000000000"
                    },
                )
                .env("HOSTS_FIXTURE_STAGES", stages.borrow().clone())
                .output()?,
        )
    };
    let head = git(&root, &["rev-parse", "HEAD"])?;
    let update = format!(
        "refs/heads/main {head} refs/heads/main {}\n",
        "0".repeat(40)
    );
    for (online, doctor, action) in [
        (false, "ok", "box is offline on the tailnet"),
        (true, "fail", "run `ssh -o BatchMode=yes box true`"),
        (
            true,
            "full",
            "box has 1024 bytes free; one more workspace requires",
        ),
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
    for stage in ["code", "real"] {
        assert!(
            String::from_utf8_lossy(&output.stdout).contains(&format!("linux box {stage}: passed"))
        );
    }
    assert_applied_evidence(&root, &output.stdout)?;
    push_gate(&root, "https://github.com/owner/project", update.as_bytes())?;
    assert_reuse(&root, &head, check)?;
    assert_stale_workspace_recovery(&root, &stages, check)?;
    Ok(())
}

const RECOVERY_WORKSPACE: &str = "/owned/jobs/job/workspace";

fn record_stale_completed_workspace(root: &Path) -> Result<()> {
    let previous = git(
        root,
        &["notes", "--ref", dotfiles::hosts::NOTES, "show", "HEAD"],
    )?;
    let notes: Vec<serde_json::Value> = previous
        .lines()
        .filter(|line| !line.is_empty())
        .map(serde_json::from_str)
        .collect::<std::result::Result<_, _>>()?;
    let mut completed = notes
        .iter()
        .find(|note| note["stage"] == "code" && note["reused"].is_null())
        .context("completed code fixture")?
        .clone();
    let reused = notes
        .iter()
        .find(|note| note["stage"] == "real" && note["reused"].is_object())
        .context("reused real fixture")?;
    completed["workspace"] =
        serde_json::json!({"path":RECOVERY_WORKSPACE,"job":"job","reused":[null,reused["reused"]]});
    git(
        root,
        &[
            "notes",
            "--ref",
            dotfiles::hosts::NOTES,
            "append",
            "-m",
            &serde_json::to_string(&completed)?,
            "HEAD",
        ],
    )?;
    let mut failure = reused.clone();
    failure
        .as_object_mut()
        .context("failed actual fixture")?
        .remove("reused");
    failure["status"] = 1.into();
    failure["generation"] = git(root, &["rev-list", "--count", dotfiles::hosts::NOTES])?
        .parse::<u64>()?
        .checked_add(1)
        .context("fixture generation overflow")?
        .into();
    git(
        root,
        &[
            "notes",
            "--ref",
            dotfiles::hosts::NOTES,
            "append",
            "-m",
            &serde_json::to_string(&failure)?,
            "HEAD",
        ],
    )?;
    Ok(())
}

fn assert_stale_workspace_recovery(
    root: &Path,
    stages: &std::cell::RefCell<String>,
    check: impl Fn(bool, &str) -> Result<std::process::Output>,
) -> Result<()> {
    #[derive(serde::Serialize)]
    struct Completed<'a> {
        code: i32,
        real: i32,
        applied: &'a serde_json::Value,
        workspace: &'a str,
    }
    record_stale_completed_workspace(root)?;
    let normal = stages.borrow().clone();
    let report: serde_json::Value = serde_json::from_str(&normal)?;
    *stages.borrow_mut() = serde_json::to_string(&Completed {
        code: 0,
        real: 0,
        applied: &report["applied"],
        workspace: RECOVERY_WORKSPACE,
    })?;
    let recovery = check(false, "ok")?;
    let diagnostic = String::from_utf8_lossy(&recovery.stderr);
    assert!(!recovery.status.success());
    assert!(
        diagnostic.contains("offline on the tailnet"),
        "{diagnostic}"
    );
    let recovered = git(
        root,
        &["notes", "--ref", dotfiles::hosts::NOTES, "show", "HEAD"],
    )?;
    let recovered: Vec<serde_json::Value> = recovered
        .lines()
        .filter(|line| !line.is_empty())
        .map(serde_json::from_str)
        .collect::<std::result::Result<_, _>>()?;
    assert_eq!(
        recovered
            .iter()
            .filter(|note| note["workspace"]["job"] == "job"
                && note["stage"].is_null()
                && note["status"] == 0)
            .count(),
        1
    );
    assert_eq!(
        recovered
            .iter()
            .filter(|note| note["workspace"]["job"] == "job" && note["stage"] == "code")
            .count(),
        1
    );
    assert_eq!(
        recovered
            .iter()
            .filter(|note| note["workspace"]["job"] == "job" && note["stage"] == "real")
            .count(),
        0
    );
    assert!(require_admission(root, "HEAD").is_err());
    *stages.borrow_mut() = normal;
    let retried = check(true, "ok")?;
    ensure!(
        retried.status.success(),
        "{}",
        String::from_utf8_lossy(&retried.stderr)
    );
    assert!(String::from_utf8_lossy(&retried.stdout).contains("linux box real: passed"));
    require_admission(root, "HEAD")?;
    Ok(())
}

fn assert_reuse(
    root: &Path,
    head: &str,
    check: impl Fn(bool, &str) -> Result<std::process::Output>,
) -> Result<()> {
    let reused = check(false, "fail")?;
    ensure!(
        reused.status.success(),
        "{}",
        String::from_utf8_lossy(&reused.stderr)
    );
    let stdout = String::from_utf8(reused.stdout)?;
    for stage in ["code", "real"] {
        assert!(
            stdout.contains(&format!("linux box {stage}: reused {head}:")),
            "{stdout}"
        );
    }
    assert!(!stdout.contains("submitted"));
    let notes = git(
        root,
        &["notes", "--ref", "refs/notes/hosts", "show", "HEAD"],
    )?;
    let records: Vec<serde_json::Value> = notes
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(serde_json::from_str)
        .collect::<std::result::Result<_, _>>()?;
    assert_eq!(records.len(), 4);
    for (first, reused) in records[..2].iter().zip(&records[2..]) {
        assert_eq!(reused["inputs"], first["inputs"]);
        assert_eq!(reused["reused"]["commit"], head);
        assert_eq!(reused["status"], 0);
        assert!(first["generation"].is_u64());
        assert_eq!(reused["generation"], first["generation"]);
    }
    fs::create_dir(root.join("home"))?;
    fs::write(root.join("home/dot_profile"), "candidate profile")?;
    git(root, &["add", "."])?;
    git(root, &["commit", "--quiet", "-m", "profile"])?;
    let profile = check(true, "ok")?;
    ensure!(
        profile.status.success(),
        "{}",
        String::from_utf8_lossy(&profile.stderr)
    );
    let stdout = String::from_utf8(profile.stdout)?;
    assert!(stdout.contains("linux box code: passed"), "{stdout}");
    assert!(stdout.contains("linux box real: passed"), "{stdout}");
    fs::write(root.join("lib.rs"), "candidate code")?;
    git(root, &["add", "."])?;
    git(root, &["commit", "--quiet", "-m", "code"])?;
    let code = check(true, "ok")?;
    ensure!(
        code.status.success(),
        "{}",
        String::from_utf8_lossy(&code.stderr)
    );
    let stdout = String::from_utf8(code.stdout)?;
    assert!(stdout.contains("linux box code: passed"), "{stdout}");
    assert!(stdout.contains("linux box real: reused"), "{stdout}");
    Ok(())
}

fn assert_applied_evidence(root: &Path, stdout: &[u8]) -> Result<()> {
    let stdout = String::from_utf8_lossy(stdout);
    for target in [".first", ".nested/target with spaces"] {
        assert!(
            stdout.contains(&format!("linux box applied drift: {target}")),
            "{stdout}"
        );
    }
    assert!(
        stdout.contains("linux box applied verification error: fixture verification error"),
        "{stdout}"
    );
    let notes = git(
        root,
        &["notes", "--ref", "refs/notes/hosts", "show", "HEAD"],
    )?;
    let notes: Vec<serde_json::Value> = notes
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(serde_json::from_str)
        .collect::<std::result::Result<_, _>>()?;
    let real = notes
        .iter()
        .find(|note| note["stage"] == "real")
        .context("real note")?;
    assert_eq!(
        real["applied"]["drifted_targets"],
        serde_json::json!([".first", ".nested/target with spaces"])
    );
    assert_eq!(
        real["applied"]["verification_error"],
        "fixture verification error"
    );
    Ok(())
}

#[test]
fn one_head_reports_every_failing_host_and_stage() -> Result<()> {
    if incident_fixture::run("one_head_reports_every_failing_host_and_stage")? {
        return Ok(());
    }
    let tools = tools()?;
    let scope = incident_fixture::scope()?;
    let root = scope.path().join("project");
    let home = scope.path().join("home");
    fs::create_dir_all(root.join(".github/workflows"))?;
    fs::create_dir_all(home.join(".local/state/hosts"))?;
    fs::write(
        home.join(".local/state/hosts/ssh_config"),
        render(
            &["box".into(), "mac".into(), "pc".into()],
            "owner",
            &home.join("known_hosts"),
        ),
    )?;
    fs::write(
        root.join("gitconfig"),
        "[user]\nname = Fixture\nemail = fixture@example.com\n[commit]\ngpgsign = false\n",
    )?;
    fs::write(root.join(".gitignore"), "gitconfig\n")?;
    fs::write(
        root.join(".github/workflows/ci.yml"),
        "jobs:\n  test:\n    strategy:\n      matrix:\n        os: [ubuntu-latest, macos-latest, windows-latest]\n    runs-on: ${{ matrix.os }}\n",
    )?;
    git(&root, &["init", "--quiet"])?;
    git(&root, &["add", "."])?;
    git(&root, &["commit", "--quiet", "-m", "ci"])?;
    let mut paths = vec![tools.path().to_path_buf()];
    paths.extend(std::env::split_paths(
        &std::env::var_os("PATH").unwrap_or_default(),
    ));
    let status = r#"{"Self":{"DNSName":"phone.example.ts.net.","OS":"android","Online":true},"Peer":{"a":{"DNSName":"box.example.ts.net.","OS":"linux","TailscaleIPs":["100.64.0.2"],"Online":true},"b":{"DNSName":"mac.example.ts.net.","OS":"macOS","TailscaleIPs":["100.64.0.3"],"Online":true},"c":{"DNSName":"pc.example.ts.net.","OS":"windows","TailscaleIPs":["100.64.0.4"],"Online":true}}}"#;
    let output = fixture_process::command(env!("CARGO_BIN_EXE_dotfiles"), scope.path())
        .args(["hosts", "check"])
        .current_dir(&root)
        .env("PATH", std::env::join_paths(paths)?)
        .env("HOME", &home)
        .env("USERPROFILE", &home)
        .env("GIT_CONFIG_GLOBAL", root.join("gitconfig"))
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_AUTHOR_NAME", "Fixture")
        .env("GIT_AUTHOR_EMAIL", "fixture@example.com")
        .env("GIT_COMMITTER_NAME", "Fixture")
        .env("GIT_COMMITTER_EMAIL", "fixture@example.com")
        .env("HOSTS_FIXTURE_STATUS", status)
        .env("HOSTS_FIXTURE_DOCTOR", "ok")
        .env("HOSTS_FIXTURE_STAGES", r#"{"code":7,"real":9,"applied":{"drifted_targets":[".drift"],"verification_error":null}}"#)
        .env("DOTFILES_EXPECTED_FAILURE", "")
        .output()?;
    assert!(!output.status.success());
    let stderr = String::from_utf8(output.stderr)?;
    print!("{}", String::from_utf8(output.stdout)?);
    eprint!("{stderr}");
    for (os, host) in [("linux", "box"), ("macos", "mac"), ("windows", "pc")] {
        for (stage, status) in [("code", 7), ("real", 9)] {
            assert!(
                stderr.contains(&format!("{os} {host} {stage}: failed with {status}")),
                "{stderr}"
            );
        }
    }
    let notes = git(
        &root,
        &["notes", "--ref", "refs/notes/hosts", "show", "HEAD"],
    )?;
    assert_eq!(
        evidence(&notes, &git(&root, &["rev-parse", "HEAD^{tree}"])?)?,
        [Evidence::Failed; 3]
    );
    let recorded = skill_ops::incidents(&scope.path().join("state"))?;
    assert_eq!(recorded.len(), 1);
    assert_eq!(recorded[0].rule, "tool.failure");
    incident_fixture::preserve(scope);
    Ok(())
}
