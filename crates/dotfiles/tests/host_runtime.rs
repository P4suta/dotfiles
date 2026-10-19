use anyhow::{Context, Result, ensure};
use dotfiles::host_checks::Scratch;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

#[path = "../../dotguard/tests/support/fixture_git.rs"]
mod fixture_git;

const REFERENCE: &str = "refs/tags/fixture";
#[cfg(unix)]
const EXECUTABLE_MODE: u32 = 0o755;
#[cfg(unix)]
const READ_ONLY_DIRECTORY_MODE: u32 = 0o555;
#[cfg(unix)]
const READ_ONLY_FILE_MODE: u32 = 0o444;
#[cfg(unix)]
const MODULE_CACHE: &str = "go/pkg/mod/honnef.co/go/tools@v0.8.1";
const GO_ENVIRONMENT_FIXTURE: &str = "DOTFILES_HOSTS_GO_FIXTURE";
const GO_ENVIRONMENT_ROLE: &str = "DOTFILES_HOSTS_GO_ROLE";
const GO_ENVIRONMENT_HOME: &str = "DOTFILES_HOSTS_GO_HOME";
const GO_ENVIRONMENT_TEST: &str = "candidate_go_writes_and_settings_are_confined_to_scratch";
const NATIVE_PATH_TEST: &str =
    "candidate_paths_remove_parent_activation_without_reloading_native_configuration";
const NATIVE_PATH_ROLE: &str = "DOTFILES_NATIVE_PATH_FIXTURE_ROLE";
const NATIVE_PATH_SCOPE: &str = "DOTFILES_NATIVE_PATH_FIXTURE_SCOPE";
const NATIVE_CAPABILITY: &str = "native-path-capability";
const NATIVE_ORIGIN_TEST: &str = "native_path_origins_keep_path_and_metadata_paired";
const NATIVE_ORIGIN_ROLE: &str = "DOTFILES_NATIVE_PATH_ORIGIN_ROLE";
const NATIVE_ORIGIN_INHERITED: &str = "inherited-native-directory";
const NATIVE_ORIGIN_CAPTURED: &str = "captured-native-directory";
const GO_OVERRIDES: [&str; 12] = [
    "GOPATH",
    "GOMODCACHE",
    "GOCACHE",
    "GOBIN",
    "GOTMPDIR",
    "GOENV",
    "GOWORK",
    "GOFLAGS",
    "GOCACHEPROG",
    "GOROOT",
    "GOTOOLCHAIN",
    "GO111MODULE",
];

fn owned_scratch(directory: &Path) -> Result<Scratch> {
    let config = directory.join("chezmoi.toml");
    fs::write(
        &config,
        format!(
            "[data]\nprofile = {:?}\n",
            dotfiles::profiles::native_profile()?.name()
        ),
    )?;
    Scratch::new(&config)
}

fn candidate_home(scratch: &Scratch) -> Result<PathBuf> {
    Ok(scratch
        .command("fixture")
        .get_envs()
        .find_map(|(name, value)| (name == "HOME").then_some(value).flatten())
        .context("candidate home")?
        .into())
}

fn inspect_native_path_role(role: &std::ffi::OsStr) -> Result<()> {
    let scope = PathBuf::from(std::env::var_os(NATIVE_PATH_SCOPE).context("native path scope")?);
    let native = scope.join("native");
    let activated = scope.join("activated");
    if role == "capability" {
        return Ok(());
    }
    if role == "inspect" {
        let paths: Vec<_> =
            std::env::split_paths(&std::env::var_os("PATH").context("PATH")?).collect();
        assert!(paths.contains(&native));
        assert!(paths.iter().all(|path| !path.starts_with(&activated)));
        assert!(std::env::var_os("__MISE_DIFF").is_none());
        let mut capability = platform::process::command(NATIVE_CAPABILITY);
        capability.args(["--exact", NATIVE_PATH_TEST, "--ignored"]);
        capability.env(NATIVE_PATH_ROLE, "capability");
        ensure!(
            capability.status()?.success(),
            "the native capability remains available"
        );
        return Ok(());
    }
    let native_config = scope.join("home/.config/mise");
    fs::create_dir_all(&native_config)?;
    fs::write(native_config.join("miserc.toml"), "unclosed = [\n")?;
    fs::write(
        native_config.join("config.toml"),
        "[tools]\nnode = 'missing-private-fixture'\n",
    )?;
    if role == "invalid" {
        let error =
            owned_scratch(&scope).expect_err("malformed activation metadata must be refused");
        let diagnostic = format!("{error:#}");
        assert!(!diagnostic.contains("private-metadata-sentinel"));
        assert!(!diagnostic.contains("missing-private-fixture"));
        return Ok(());
    }
    assert_eq!(role, "activated");
    assert!(
        std::env::split_paths(&std::env::var_os("PATH").context("PATH")?)
            .any(|path| path.starts_with(&activated))
    );
    let scratch = owned_scratch(&scope)?;
    let output = scratch
        .command(std::env::current_exe()?)
        .args(["--exact", NATIVE_PATH_TEST, "--ignored"])
        .env(NATIVE_PATH_ROLE, "inspect")
        .output()?;
    ensure!(
        output.status.success(),
        "native PATH inspection failed: {}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    Ok(())
}

fn assert_pinned_mise(mise: &Path) -> Result<()> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let pins: toml::Table = toml::from_str(&fs::read_to_string(
        root.join("home/.chezmoidata/versions.toml"),
    )?)?;
    let version = platform::process::command(mise).arg("--version").output()?;
    ensure!(version.status.success(), "read the installed mise version");
    assert_eq!(
        std::str::from_utf8(&version.stdout)?
            .split_whitespace()
            .next(),
        pins["mise"]["version"].as_str()
    );
    Ok(())
}

fn native_path_fixture(scope: &Path, mise: &Path) -> Result<std::ffi::OsString> {
    let native = scope.join("native");
    let activated = scope.join("activated");
    for directory in [
        &native,
        &activated.join("bin"),
        &scope.join("home"),
        &scope.join("outer-config"),
    ] {
        fs::create_dir_all(directory)?;
    }
    fs::copy(
        std::env::current_exe()?,
        native.join(format!(
            "{NATIVE_CAPABILITY}{}",
            std::env::consts::EXE_SUFFIX
        )),
    )?;
    fs::copy(
        std::env::current_exe()?,
        if cfg!(windows) {
            activated.join("node.exe")
        } else {
            activated.join("bin/node")
        },
    )?;
    let mut table = toml::Table::new();
    let mut tools = toml::Table::new();
    tools.insert(
        "node".into(),
        format!("path:{}", activated.display()).into(),
    );
    table.insert("tools".into(), tools.into());
    fs::write(scope.join("mise.toml"), toml::to_string(&table)?)?;
    fs::write(scope.join("empty.toml"), "")?;
    Ok(std::env::join_paths([
        native.as_path(),
        mise.parent().context("mise directory")?,
    ])?)
}

#[test]
#[ignore = "uses the installed pinned mise in isolated native processes"]
fn candidate_paths_remove_parent_activation_without_reloading_native_configuration() -> Result<()> {
    if let Some(role) = std::env::var_os(NATIVE_PATH_ROLE) {
        return inspect_native_path_role(&role);
    }
    let scope = tempfile::tempdir()?;
    let installed = platform::code_environment::Installations::discover(
        &platform::home::home().context("native home")?,
    )?;
    assert_pinned_mise(&installed.mise)?;
    let path = native_path_fixture(scope.path(), &installed.mise)?;
    let native = scope.path().join("native");
    let home = scope.path().join("home");
    let config = scope.path().join("outer-config");
    let empty = scope.path().join("empty.toml");
    for role in ["activated", "invalid"] {
        let mut command = platform::process::command(&installed.mise);
        for (name, _) in std::env::vars_os() {
            let text = name.to_string_lossy();
            if text.starts_with("MISE_") || text.starts_with("__MISE_") {
                command.env_remove(name);
            }
        }
        command
            .current_dir(scope.path())
            .env("HOME", &home)
            .env("USERPROFILE", &home)
            .env("XDG_CONFIG_HOME", home.join(".config"))
            .env("MISE_CONFIG_DIR", &config)
            .env("MISE_GLOBAL_CONFIG_FILE", &empty)
            .env("MISE_SYSTEM_CONFIG_FILE", &empty)
            .env("MISE_SYSTEM_CONFIG_DIR", scope.path().join("system-config"))
            .env("MISE_DATA_DIR", scope.path().join("data"))
            .env("MISE_STATE_DIR", scope.path().join("state"))
            .env("MISE_CACHE_DIR", scope.path().join("cache"))
            .env(
                "MISE_CEILING_PATHS",
                scope.path().parent().context("scope parent")?,
            )
            .env("MISE_TRUSTED_CONFIG_PATHS", scope.path())
            .env("MISE_AUTO_INSTALL", "false")
            .env("PATH", &path)
            .env_remove("DOTFILES_HOST_PATH")
            .env_remove("DOTFILES_HOST_MISE_DIFF")
            .env_remove("DOTFILES_HOST_MISE_DIFF_PRESENT")
            .env(NATIVE_PATH_ROLE, role)
            .env(NATIVE_PATH_SCOPE, scope.path());
        if role == "activated" {
            command.args(["exec", "--"]).arg(std::env::current_exe()?);
        } else {
            command = platform::process::command(std::env::current_exe()?);
            command
                .env("HOME", &home)
                .env("USERPROFILE", &home)
                .env(
                    "PATH",
                    std::env::join_paths([
                        native.as_path(),
                        installed.mise.parent().context("mise directory")?,
                    ])?,
                )
                .env("__MISE_DIFF", "private-metadata-sentinel")
                .env_remove("DOTFILES_HOST_PATH")
                .env_remove("DOTFILES_HOST_MISE_DIFF")
                .env_remove("DOTFILES_HOST_MISE_DIFF_PRESENT")
                .env(NATIVE_PATH_ROLE, role)
                .env(NATIVE_PATH_SCOPE, scope.path());
        }
        command.args(["--exact", NATIVE_PATH_TEST, "--ignored"]);
        let output = command.output()?;
        ensure!(
            output.status.success(),
            "{role} native PATH regression failed: {}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }
    Ok(())
}

#[test]
fn native_path_origins_keep_path_and_metadata_paired() -> Result<()> {
    use platform::code_environment::{NATIVE_PATH_DIFF, NATIVE_PATH_DIFF_PRESENT, NativePath};

    if let Ok(role) = std::env::var(NATIVE_ORIGIN_ROLE) {
        let scope = tempfile::tempdir()?;
        let result = NativePath::capture(scope.path());
        let expected = match role.as_str() {
            "direct" => Some(NATIVE_ORIGIN_INHERITED),
            "explicit" | "absent-origin" | "empty-origin" => Some(NATIVE_ORIGIN_CAPTURED),
            _ => None,
        };
        if let Some(expected) = expected {
            assert_eq!(result?.as_os_str(), expected);
        } else {
            let error = result.expect_err("unpaired metadata must be refused before querying");
            assert!(!format!("{error:#}").contains("private-metadata-sentinel"));
        }
        return Ok(());
    }
    for (role, presence, diff, captured, current) in [
        ("direct", None, None, false, false),
        ("explicit", None, None, true, false),
        ("absent-origin", Some("0"), None, true, true),
        ("empty-origin", Some("0"), Some(""), true, true),
        ("mixed-origin", None, None, true, true),
        (
            "missing-marker",
            None,
            Some("private-metadata-sentinel"),
            true,
            false,
        ),
        (
            "missing-path",
            Some("1"),
            Some("private-metadata-sentinel"),
            false,
            false,
        ),
        ("missing-diff", Some("1"), None, true, false),
        (
            "invalid-presence",
            Some("2"),
            Some("private-metadata-sentinel"),
            true,
            false,
        ),
        (
            "false-presence",
            Some("0"),
            Some("private-metadata-sentinel"),
            true,
            false,
        ),
    ] {
        let mut command = platform::process::command(std::env::current_exe()?);
        command
            .args(["--exact", NATIVE_ORIGIN_TEST])
            .env(NATIVE_ORIGIN_ROLE, role)
            .env("PATH", NATIVE_ORIGIN_INHERITED)
            .env_remove("DOTFILES_HOST_PATH")
            .env_remove("__MISE_DIFF")
            .env_remove(NATIVE_PATH_DIFF)
            .env_remove(NATIVE_PATH_DIFF_PRESENT);
        if captured {
            command.env("DOTFILES_HOST_PATH", NATIVE_ORIGIN_CAPTURED);
        }
        if current {
            command.env("__MISE_DIFF", "private-metadata-sentinel");
        }
        if let Some(presence) = presence {
            command.env(NATIVE_PATH_DIFF_PRESENT, presence);
        }
        if let Some(diff) = diff {
            command.env(NATIVE_PATH_DIFF, diff);
        }
        let output = command.output()?;
        ensure!(
            output.status.success(),
            "{role} native origin regression failed: {}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }
    Ok(())
}

#[cfg(unix)]
fn readonly_module_cache(home: &Path, outside: &Path) -> Result<[PathBuf; 2]> {
    use std::os::unix::fs::{PermissionsExt as _, symlink};
    let module = home.join(MODULE_CACHE);
    let nested = module.join("cmd/staticcheck");
    fs::create_dir_all(&nested)?;
    fs::write(nested.join("main.go"), "package main\n")?;
    fs::set_permissions(
        nested.join("main.go"),
        fs::Permissions::from_mode(READ_ONLY_FILE_MODE),
    )?;
    fs::write(module.with_extension("partial"), "interrupted extraction")?;
    symlink(outside, module.join("outside"))?;
    symlink(outside.join("absent"), module.join("dangling"))?;
    fs::hard_link(outside.join("sentinel"), module.join("hardlink"))?;
    for directory in [&nested, &module] {
        fs::set_permissions(
            directory,
            fs::Permissions::from_mode(READ_ONLY_DIRECTORY_MODE),
        )?;
    }
    Ok([module, nested])
}

#[test]
#[cfg(unix)]
fn candidate_cleanup_removes_readonly_cache_without_changing_outside_targets() -> Result<()> {
    use std::os::unix::fs::PermissionsExt as _;
    let fixture = tempfile::tempdir()?;
    let outside = fixture.path().join("outside");
    fs::create_dir(&outside)?;
    let sentinel = outside.join("sentinel");
    fs::write(&sentinel, "outside bytes")?;
    fs::set_permissions(&sentinel, fs::Permissions::from_mode(READ_ONLY_FILE_MODE))?;
    fs::set_permissions(
        &outside,
        fs::Permissions::from_mode(READ_ONLY_DIRECTORY_MODE),
    )?;
    let directory_mode = fs::metadata(&outside)?.permissions().mode();
    let file_mode = fs::metadata(&sentinel)?.permissions().mode();
    let scratch = owned_scratch(fixture.path())?;
    let home = candidate_home(&scratch)?;
    let directory = home.parent().context("candidate directory")?.to_path_buf();
    let cache = readonly_module_cache(&home, &outside)?;
    assert!(cache.iter().all(|path| path.is_dir()));
    drop(scratch);
    let removed = !directory.exists();
    let preserved_directory = fs::metadata(&outside)?.permissions().mode();
    let preserved_file = fs::metadata(&sentinel)?.permissions().mode();
    let preserved_bytes = fs::read(&sentinel)?;
    fs::set_permissions(&outside, fs::Permissions::from_mode(EXECUTABLE_MODE))?;
    if !removed {
        for path in cache {
            if path.exists() {
                fs::set_permissions(path, fs::Permissions::from_mode(EXECUTABLE_MODE))?;
            }
        }
        platform::cleanup::report(
            fs::remove_dir_all(&directory),
            format_args!("remove failed cleanup fixture {}", directory.display()),
        );
    }
    assert!(
        removed,
        "candidate retained readonly cache at {}",
        directory.display()
    );
    assert_eq!(preserved_directory, directory_mode);
    assert_eq!(preserved_file, file_mode);
    assert_eq!(preserved_bytes, b"outside bytes");
    Ok(())
}

fn assert_go_environment(home: &Path) -> Result<()> {
    for (name, relative) in [
        ("GOPATH", "go"),
        ("GOMODCACHE", "go/pkg/mod"),
        ("GOCACHE", ".cache/go-build"),
        ("GOBIN", ".local/bin"),
    ] {
        assert_eq!(
            PathBuf::from(std::env::var_os(name).context("owned Go directory")?),
            home.join(relative)
        );
    }
    assert_eq!(
        platform::paths::canonical(&PathBuf::from(
            std::env::var_os("GOTMPDIR").context("owned Go temporary directory")?,
        ))?,
        home.parent().context("candidate directory")?
    );
    assert_eq!(std::env::var("GOENV")?, "off");
    assert_eq!(std::env::var("GOWORK")?, "off");
    for name in [
        "GOFLAGS",
        "GOCACHEPROG",
        "GOROOT",
        "GOTOOLCHAIN",
        "GO111MODULE",
    ] {
        assert!(std::env::var_os(name).is_none(), "inherited {name}");
    }
    Ok(())
}

#[test]
fn candidate_go_writes_and_settings_are_confined_to_scratch() -> Result<()> {
    if std::env::var(GO_ENVIRONMENT_ROLE).is_ok_and(|role| role == "inspect") {
        return assert_go_environment(&PathBuf::from(
            std::env::var_os(GO_ENVIRONMENT_HOME).context("candidate Go home")?,
        ));
    }
    if let Some(directory) = std::env::var_os(GO_ENVIRONMENT_FIXTURE) {
        let scratch = owned_scratch(Path::new(&directory))?;
        let home = candidate_home(&scratch)?;
        let output = scratch
            .command(std::env::current_exe()?)
            .args([GO_ENVIRONMENT_TEST, "--exact", "--nocapture"])
            .env(GO_ENVIRONMENT_ROLE, "inspect")
            .env(GO_ENVIRONMENT_HOME, home)
            .output()?;
        ensure!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stdout)
        );
        return Ok(());
    }
    let fixture = tempfile::tempdir()?;
    let outside = fixture.path().join("outside");
    fs::create_dir(&outside)?;
    let sentinel = outside.join("sentinel");
    fs::write(&sentinel, "outside bytes")?;
    let mut command = platform::process::command(std::env::current_exe()?);
    command
        .args([GO_ENVIRONMENT_TEST, "--exact", "--nocapture"])
        .env(GO_ENVIRONMENT_FIXTURE, fixture.path());
    for name in GO_OVERRIDES {
        command.env(name, &outside);
    }
    let output = command.output()?;
    ensure!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stdout)
    );
    assert_eq!(fs::read(sentinel)?, b"outside bytes");
    Ok(())
}

fn candidate_git(environment: &Command, repository: &Path) -> Result<Command> {
    let global = environment
        .get_envs()
        .find_map(|(name, value)| (name == "GIT_CONFIG_GLOBAL").then_some(value).flatten())
        .context("candidate Git configuration")?;
    let mut command = fixture_git::command("git", Path::new(global));
    for (name, value) in environment.get_envs() {
        if let Some(value) = value {
            command.env(name, value);
        } else {
            command.env_remove(name);
        }
    }
    command.current_dir(repository);
    Ok(command)
}

fn render_template(
    root: &Path,
    scope: &Path,
    config: &Path,
    home: &Path,
    environment: &Command,
    renderer: &Path,
    template: &str,
) -> Result<Vec<u8>> {
    let configured = dotfiles::profiles::chezmoi(root, scope, config, home);
    let mut render = platform::process::command(renderer);
    render.args(configured.get_args());
    for (name, value) in environment.get_envs() {
        if let Some(value) = value {
            render.env(name, value);
        } else {
            render.env_remove(name);
        }
    }
    let output = render
        .args(["execute-template", "--file"])
        .arg(root.join(template))
        .output()?;
    ensure!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    Ok(output.stdout)
}

#[cfg(unix)]
fn render_git_launcher(
    root: &Path,
    scope: &Path,
    config: &Path,
    home: &Path,
    environment: &Command,
    renderer: &Path,
) -> Result<PathBuf> {
    use std::os::unix::fs::PermissionsExt as _;
    let wrapper = home.join(".local/bin/git");
    fs::create_dir_all(wrapper.parent().context("Git launcher directory")?)?;
    fs::write(
        &wrapper,
        render_template(
            root,
            scope,
            config,
            home,
            environment,
            renderer,
            "home/dot_local/bin/executable_git.tmpl",
        )?,
    )?;
    fs::set_permissions(&wrapper, fs::Permissions::from_mode(EXECUTABLE_MODE))?;
    Ok(wrapper)
}

#[cfg(unix)]
fn assert_git_launcher(scratch: &Scratch, home: &Path) -> Result<()> {
    let output = scratch
        .command(home.join(".local/bin/git"))
        .arg("--version")
        .output()?;
    ensure!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(String::from_utf8(output.stdout)?.starts_with("git version "));
    Ok(())
}

#[test]
fn candidate_git_runs_the_rendered_hook_with_the_candidate_runtime() -> Result<()> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let scope = tempfile::tempdir()?;
    let config = scope.path().join("chezmoi.toml");
    fs::write(
        &config,
        format!(
            "[data]\nprofile = {:?}\n",
            dotfiles::profiles::native_profile()?.name()
        ),
    )?;
    let scratch = Scratch::new(&config)?;
    let native_home = platform::home::home().context("native tool storage home")?;
    let native = platform::code_environment::Installations::discover(&native_home)?;
    let renderer = scratch.renderer(&root, &native)?;
    let environment = scratch.command("git");
    let home = PathBuf::from(
        environment
            .get_envs()
            .find_map(|(name, value)| (name == "HOME").then_some(value).flatten())
            .context("candidate home")?,
    );
    let hooks = home.join(".config/git/hooks");
    fs::create_dir_all(&hooks)?;
    let hook = hooks.join("reference-transaction");
    fs::write(
        &hook,
        render_template(
            &root,
            scope.path(),
            &config,
            &home,
            &environment,
            &renderer,
            "home/dot_config/git/hooks/executable_reference-transaction.tmpl",
        )?,
    )?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        fs::set_permissions(&hook, fs::Permissions::from_mode(EXECUTABLE_MODE))?;
        let wrapper =
            render_git_launcher(&root, scope.path(), &config, &home, &environment, &renderer)?;
        assert!(
            !scratch
                .command(&wrapper)
                .arg("--version")
                .output()?
                .status
                .success()
        );
    }
    fs::write(home.join(".config/git/config"), "")?;
    let repository = scope.path().join("repository");
    fs::create_dir(&repository)?;
    ensure!(
        candidate_git(&environment, &repository)?
            .args(["init", "--bare", "--quiet"])
            .status()?
            .success(),
        "initialize the fixture repository"
    );
    fs::write(
        home.join(".config/git/config"),
        "[core]\nhooksPath = ~/.config/git/hooks\n[commit]\ngpgsign = false\n",
    )?;
    let object = candidate_git(&environment, &repository)?
        .args(["hash-object", "-w"])
        .arg(&hook)
        .output()?;
    ensure!(object.status.success(), "write the fixture object");
    let object = String::from_utf8(object.stdout)?;
    let run = || {
        candidate_git(&environment, &repository)?
            .args(["update-ref", REFERENCE, object.trim()])
            .output()
            .map_err(anyhow::Error::from)
    };
    let missing = run()?;
    assert!(!missing.status.success());
    assert!(String::from_utf8_lossy(&missing.stderr).contains("dotfiles"));
    let binary = Path::new(env!("CARGO_BIN_EXE_dotfiles"));
    scratch.provision_runtime(&root, binary, &native_home)?;
    let installed = home
        .join(".local/bin")
        .join(format!("dotfiles{}", std::env::consts::EXE_SUFFIX));
    assert_eq!(fs::read(&installed)?, fs::read(binary)?);
    assert_candidate_backend(&scratch, &home)?;
    #[cfg(unix)]
    assert_git_launcher(&scratch, &home)?;
    let output = run()?;
    ensure!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(!scope.path().join(".local/bin").exists());
    Ok(())
}

fn assert_candidate_backend(scratch: &Scratch, home: &Path) -> Result<()> {
    let guard = home
        .join(".local/bin")
        .join(format!("dotguard{}", std::env::consts::EXE_SUFFIX));
    let backend = guard.with_file_name(format!(
        "{}{}",
        dotguard::RENOVATE_BINARY,
        std::env::consts::EXE_SUFFIX
    ));
    assert!(backend.is_file(), "candidate Renovate backend is missing");
    let delegated = scratch
        .command(&guard)
        .args(["renovate", "unsupported"])
        .output()?;
    assert_eq!(delegated.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&delegated.stderr).contains("usage: dotguard renovate run"));
    let mismatched = scratch
        .command(&backend)
        .args(["different-source", "renovate", "run"])
        .output()?;
    assert_eq!(mismatched.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&mismatched.stderr).contains("\"rule\":\"tool.stale\""));
    Ok(())
}
