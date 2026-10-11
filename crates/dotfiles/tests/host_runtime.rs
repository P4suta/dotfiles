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
    template: &str,
) -> Result<Vec<u8>> {
    let mut render = dotfiles::profiles::chezmoi(root, scope, config, home);
    for (name, value) in environment.get_envs() {
        if let Some(value) = value {
            render.env(name, value);
        } else {
            render.env_remove(name);
        }
    }
    let rendered = render
        .args(["execute-template", "--file"])
        .arg(root.join(template))
        .output()?;
    ensure!(
        rendered.status.success(),
        "{}",
        String::from_utf8_lossy(&rendered.stderr)
    );
    Ok(rendered.stdout)
}

#[cfg(unix)]
fn render_git_launcher(
    root: &Path,
    scope: &Path,
    config: &Path,
    home: &Path,
    environment: &Command,
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
            "home/dot_config/git/hooks/executable_reference-transaction.tmpl",
        )?,
    )?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        fs::set_permissions(&hook, fs::Permissions::from_mode(EXECUTABLE_MODE))?;
        let wrapper = render_git_launcher(&root, scope.path(), &config, &home, &environment)?;
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
    scratch.provision_runtime(
        &root,
        binary,
        &platform::home::home().context("native tool storage home")?,
    )?;
    let installed = home
        .join(".local/bin")
        .join(format!("dotfiles{}", std::env::consts::EXE_SUFFIX));
    assert_eq!(fs::read(&installed)?, fs::read(binary)?);
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
