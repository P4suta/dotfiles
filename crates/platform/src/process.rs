//! The one place any dotfiles crate creates a process.
//! Every crate starts a program through [`command`], so `clippy.toml` refuses `Command::new` everywhere else.

use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::{io, process};

const SHIMS: &str = "shims";
const MISE: &str = "mise";

fn variable(command: &Command, name: &str) -> Option<OsString> {
    command
        .get_envs()
        .find(|(key, _)| named(key, name))
        .map_or_else(
            || std::env::var_os(name),
            |(_, value)| value.map(OsStr::to_owned),
        )
}

/// The configured shim locations, including the native default.
#[must_use]
pub fn mise_shims(environment: impl Fn(&'static str) -> Option<OsString>) -> Vec<PathBuf> {
    let mut directories = Vec::new();
    if let Some(home) = environment(if cfg!(windows) { "USERPROFILE" } else { "HOME" }) {
        directories.push(crate::home::mise_shims(Path::new(&home)));
    }
    for (name, suffix) in [
        ("MISE_SHIMS_DIR", ""),
        ("MISE_DATA_DIR", SHIMS),
        ("XDG_DATA_HOME", "mise/shims"),
        ("LOCALAPPDATA", "mise/shims"),
    ] {
        if let Some(directory) = environment(name).filter(|value| !value.is_empty()) {
            directories.push(PathBuf::from(directory).join(suffix));
        }
    }
    directories
}

fn canonical(directory: &Path, working: &Path) -> io::Result<PathBuf> {
    let directory = if directory.is_absolute() {
        directory.to_path_buf()
    } else {
        working.join(directory)
    };
    match crate::paths::canonical(&directory) {
        Ok(canonical) => Ok(canonical),
        Err(error) if unresolved(&error) => Ok(directory),
        Err(error) => Err(error),
    }
}

/// The error Windows returns when it won't let this process follow a junction that an installer created, such as the Herdr and Codex `bin` links on `PATH`.
/// Win32 names it `ERROR_UNTRUSTED_MOUNT_POINT`.
const UNTRUSTED_MOUNT_POINT: i32 = 448;

/// Whether canonicalizing failed only because the path doesn't resolve, so its written form stands.
fn unresolved(error: &io::Error) -> bool {
    matches!(
        error.kind(),
        io::ErrorKind::NotFound | io::ErrorKind::NotADirectory
    ) || cfg!(windows) && error.raw_os_error() == Some(UNTRUSTED_MOUNT_POINT)
}

fn named(component: &OsStr, name: &str) -> bool {
    if cfg!(windows) {
        component
            .as_encoded_bytes()
            .eq_ignore_ascii_case(name.as_bytes())
    } else {
        component == name
    }
}

fn shim(directory: &Path, configured: &[PathBuf]) -> bool {
    crate::process_rules::install_path([
        configured.iter().any(|path| path == directory),
        directory
            .parent()
            .and_then(Path::file_name)
            .is_some_and(|name| named(name, MISE)),
        directory.file_name().is_some_and(|name| named(name, SHIMS)),
    ]) == crate::process_rules::InstallPath::Shim
}

/// An installation command with no environment setters.
///
/// ```compile_fail
/// let mut install = platform::process::mise_install(platform::process::command("mise")).unwrap();
/// install.env("PATH", "mise/shims");
/// ```
#[derive(Debug)]
pub struct MiseInstall(Command);

impl MiseInstall {
    /// Start the installation and wait for its exit status.
    ///
    /// # Errors
    ///
    /// Returns an error if starting or waiting for the child fails.
    pub fn status(&mut self) -> io::Result<process::ExitStatus> {
        self.0.status()
    }

    /// Start the installation and capture its output.
    ///
    /// # Errors
    ///
    /// Returns an error if starting the child or reading its output fails.
    pub fn output(&mut self) -> io::Result<process::Output> {
        self.0.output()
    }

    /// Start the prepared installation.
    ///
    /// # Errors
    ///
    /// Returns an error if starting the child fails.
    pub fn spawn(&mut self) -> io::Result<process::Child> {
        self.0.spawn()
    }

    /// Read the prepared child environment.
    pub fn get_envs(&self) -> impl Iterator<Item = (&OsStr, Option<&OsStr>)> {
        self.0.get_envs()
    }
}

/// Prepare a fully configured command before it can hold a mise installation lock.
/// The child retains direct tool directories and excludes configured shim directories and their filesystem aliases.
/// The child ignores parent activation metadata before applying its configured environment.
///
/// # Errors
///
/// Returns an error if preparing the installation environment fails.
pub fn mise_install(mut command: Command) -> io::Result<MiseInstall> {
    let working = command
        .get_current_dir()
        .map_or_else(std::env::current_dir, |directory| {
            Ok(directory.to_path_buf())
        })?;
    let configured = mise_shims(|name| variable(&command, name))
        .into_iter()
        .chain(mise_shims(std::env::var_os))
        .collect::<Vec<_>>();
    let configured = configured
        .iter()
        .map(|directory| canonical(directory, &working))
        .collect::<io::Result<Vec<_>>>()?;
    let path = variable(&command, "PATH").unwrap_or_default();
    let mut directories = Vec::new();
    for directory in std::env::split_paths(&path) {
        if !directories.contains(&directory)
            && !shim(&directory, &configured)
            && !shim(&canonical(&directory, &working)?, &configured)
        {
            directories.push(directory);
        }
    }
    let path = std::env::join_paths(directories)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidInput, error))?;
    #[cfg(windows)]
    validate_shell_path(&path)?;
    command.env_remove("__MISE_DIFF").env("PATH", path);
    Ok(MiseInstall(command))
}

#[cfg(windows)]
fn validate_shell_path(path: &OsStr) -> io::Result<()> {
    use std::os::windows::ffi::OsStrExt;

    let code_units = path.encode_wide().count();
    match crate::process_rules::shell_path(code_units) {
        crate::process_rules::ShellPath::Usable => Ok(()),
        crate::process_rules::ShellPath::Discarded => Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!(
                "installation PATH has {code_units} UTF-16 code units after removing shims and duplicates; cmd discards PATH above {}; shorten the configured tool directories before installing",
                crate::process_rules::CMD_PATH_LIMIT
            ),
        )),
    }
}

#[expect(
    clippy::disallowed_methods,
    reason = "the workspace's one process constructor"
)]
fn construct<const ISOLATE_REPOSITORY: bool>(program: impl AsRef<OsStr>) -> Command {
    let mut command = Command::new(program);
    if ISOLATE_REPOSITORY {
        for name in GIT_LOCAL_ENVIRONMENT_TEXT.split(' ') {
            command.env_remove(name);
        }
    }
    command
}

/// A process for `program`, which callers name through a registry or a path they verified.
#[must_use]
pub fn command(program: impl AsRef<OsStr>) -> Command {
    construct::<true>(program)
}

macro_rules! git_local_environment {
    ($first:literal $(, $name:literal)* $(,)?) => {
        /// The repository-local variables listed by `git rev-parse --local-env-vars`.
        pub const GIT_LOCAL_ENVIRONMENT: [&str; [$first $(, $name)*].len()] = [$first $(, $name)*];
        const GIT_LOCAL_ENVIRONMENT_TEXT: &str = concat!($first $(, " ", $name)*);
    };
}

git_local_environment![
    "GIT_ALTERNATE_OBJECT_DIRECTORIES",
    "GIT_CONFIG",
    "GIT_CONFIG_PARAMETERS",
    "GIT_CONFIG_COUNT",
    "GIT_OBJECT_DIRECTORY",
    "GIT_DIR",
    "GIT_WORK_TREE",
    "GIT_IMPLICIT_WORK_TREE",
    "GIT_GRAFT_FILE",
    "GIT_INDEX_FILE",
    "GIT_NO_REPLACE_OBJECTS",
    "GIT_REPLACE_REF_BASE",
    "GIT_PREFIX",
    "GIT_SHALLOW_FILE",
    "GIT_COMMON_DIR",
];

/// A repository operation that explicitly receives the invoking Git context.
#[must_use]
pub fn repository_command(program: impl AsRef<OsStr>) -> Command {
    construct::<false>(program)
}

/// A process that inherits only the explicitly constructed environment.
#[must_use]
pub fn isolated_command<K, V>(
    program: impl AsRef<OsStr>,
    environment: impl IntoIterator<Item = (K, V)>,
) -> Command
where
    K: AsRef<OsStr>,
    V: AsRef<OsStr>,
{
    let mut command = construct::<false>(program);
    command.env_clear().envs(environment);
    command
}

/// An owned Unix PowerShell cache, retained until its child exits.
/// Concurrent PowerShell processes can corrupt a shared startup profile.
#[cfg(feature = "tools")]
#[derive(Debug)]
pub struct PowerShellCache {
    directory: Option<tempfile::TempDir>,
    path: PathBuf,
}

#[cfg(feature = "tools")]
impl PowerShellCache {
    /// Create a cache that no other invocation shares.
    ///
    /// # Errors
    ///
    /// Returns an error when creating the cache fails.
    pub fn new() -> io::Result<Self> {
        let directory = tempfile::tempdir()?;
        let path = directory.path().to_path_buf();
        Ok(Self {
            directory: Some(directory),
            path,
        })
    }

    /// Keep native Windows configuration and direct Unix cache writes to this scope.
    pub fn apply<'a>(&self, command: &'a mut Command) -> &'a mut Command {
        #[cfg(unix)]
        command.env("XDG_CACHE_HOME", &self.path);
        command
    }
}

#[cfg(feature = "tools")]
impl Drop for PowerShellCache {
    fn drop(&mut self) {
        if let Some(directory) = self.directory.take() {
            crate::cleanup::report(
                directory.close(),
                format_args!("remove PowerShell cache {}", self.path.display()),
            );
        }
    }
}

/// The same registered command and child environment on a terminal.
#[cfg(feature = "pty")]
#[must_use]
pub fn pty_command(command: &Command) -> portable_pty::CommandBuilder {
    let mut terminal = portable_pty::CommandBuilder::new(command.get_program());
    terminal.args(command.get_args());
    if let Some(directory) = command.get_current_dir() {
        terminal.cwd(directory);
    }
    for (name, value) in command.get_envs() {
        if let Some(value) = value {
            terminal.env(name, value);
        } else {
            terminal.env_remove(name);
        }
    }
    terminal
}

#[cfg(test)]
mod tests {
    use super::{UNTRUSTED_MOUNT_POINT, unresolved};
    use std::io;

    #[test]
    fn a_junction_windows_refuses_keeps_its_written_path_like_a_missing_one() {
        assert!(unresolved(&io::Error::from(io::ErrorKind::NotFound)));
        assert!(unresolved(&io::Error::from(io::ErrorKind::NotADirectory)));
        assert_eq!(
            unresolved(&io::Error::from_raw_os_error(UNTRUSTED_MOUNT_POINT)),
            cfg!(windows)
        );
        assert!(!unresolved(&io::Error::from(
            io::ErrorKind::PermissionDenied
        )));
    }
}
