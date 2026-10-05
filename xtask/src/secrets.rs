use crate::tool::Tool;
use anyhow::{Context, Result, ensure};
use std::ffi::OsString;
use std::path::Path;
use std::process::{ExitStatus, Stdio};

/// The variable OpenCode's GitHub Model Context Protocol server reads its bearer token from.
const GITHUB_MCP_TOKEN: &str = "GH_MCP_TOKEN";

#[derive(Clone, Copy, Debug, clap::ValueEnum)]
pub enum Agent {
    Codex,
    Claude,
    Opencode,
}
impl Agent {
    fn name(self) -> &'static str {
        match self {
            Self::Codex => "codex",
            Self::Claude => "claude",
            Self::Opencode => "opencode",
        }
    }
}

pub fn agent(
    root: &Path,
    config: &Path,
    agent: Agent,
    arguments: &[OsString],
) -> Result<ExitStatus> {
    ensure!(
        config.is_absolute() && !crate::canonical(config)?.starts_with(root),
        "agent configuration must be machine-local"
    );
    let home = std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" })
        .context("native home unavailable")?;
    let context = crate::setup::load(root, config, Path::new(&home))?;
    let selected = context
        .data
        .pointer(&format!("/doppler/secrets/{}", agent.name()));
    let selected = if let Some(value) = selected {
        value
            .as_array()
            .context("secret selection must be an array")?
            .iter()
            .map(|name| name.as_str().context("secret selection must contain names"))
            .collect::<Result<Vec<_>>>()?
    } else {
        Vec::new()
    };
    ensure!(
        !conflicting_origin(agent, &selected),
        "{GITHUB_MCP_TOKEN} comes from the GitHub CLI login; remove it from doppler.secrets.{}",
        agent.name()
    );
    let arguments = if matches!(agent, Agent::Opencode) {
        opencode_arguments(arguments)
    } else {
        arguments.to_vec()
    };
    let command: Vec<_> = std::iter::once(agent.name().into())
        .chain(arguments)
        .collect();
    let environment = agent_environment(agent);
    if selected.is_empty() {
        return crate::tool::external(&command[0])
            .args(&command[1..])
            .envs(environment)
            .status()
            .context("start agent without API secrets");
    }
    run_with(
        context
            .data
            .pointer("/doppler/project")
            .and_then(serde_json::Value::as_str)
            .context("Doppler project missing")?,
        context
            .data
            .pointer("/doppler/config")
            .and_then(serde_json::Value::as_str)
            .context("Doppler config missing")?,
        &selected.join(","),
        &command,
        &environment,
    )
}

/// OpenCode's GitHub Model Context Protocol server reuses the login of `gh`, the GitHub command-line tool, so the credential has one origin that `gh` keeps current.
/// The server's read-only header limits what the assistant can do through it.
/// The token travels only in the assistant's environment, and without a `gh` login the assistant starts without it.
fn agent_environment(agent: Agent) -> Vec<(&'static str, OsString)> {
    if !matches!(agent, Agent::Opencode) {
        return Vec::new();
    }
    let token = Tool::Gh
        .command()
        .args(["auth", "token"])
        .stderr(Stdio::null())
        .output()
        .ok()
        .filter(|output| output.status.success())
        .map(|output| String::from_utf8_lossy(&output.stdout).trim().to_owned())
        .filter(|token| !token.is_empty());
    if let Some(token) = token {
        vec![(GITHUB_MCP_TOKEN, token.into())]
    } else {
        eprintln!("Warning: the GitHub MCP server is unavailable until `gh auth login` succeeds");
        Vec::new()
    }
}

/// OpenCode runs Model Context Protocol servers in a shared background service that keeps the environment it started with.
/// The interface and `run` thus start their own server, which inherits the credentials this launch supplies.
/// A subcommand comes first on the command line, so any other first word besides a directory passes through unchanged, as does a launch that already chooses a server.
fn opencode_arguments(arguments: &[OsString]) -> Vec<OsString> {
    let chooses_server = arguments.iter().any(|argument| {
        argument == "--standalone"
            || argument == "--server"
            || argument.to_string_lossy().starts_with("--server=")
    });
    let standalone = OsString::from("--standalone");
    match arguments.first() {
        _ if chooses_server => arguments.to_vec(),
        Some(word) if word == "run" => std::iter::once(word.clone())
            .chain(std::iter::once(standalone))
            .chain(arguments[1..].iter().cloned())
            .collect(),
        Some(word) if !word.to_string_lossy().starts_with('-') && !Path::new(word).is_dir() => {
            arguments.to_vec()
        }
        _ => std::iter::once(standalone)
            .chain(arguments.iter().cloned())
            .collect(),
    }
}

/// A configuration must not also select from Doppler a secret the launcher already supplies from another origin, so each credential keeps one origin.
fn conflicting_origin(agent: Agent, selected: &[&str]) -> bool {
    matches!(agent, Agent::Opencode) && selected.contains(&GITHUB_MCP_TOKEN)
}

pub fn names(input: &str) -> Result<Vec<&str>> {
    let result: Vec<_> = input.split(',').collect();
    ensure!(!result.is_empty(), "at least one secret name is required");
    for (index, name) in result.iter().enumerate() {
        ensure!(
            !name.is_empty()
                && !name.starts_with(|character: char| character.is_ascii_digit())
                && name
                    .bytes()
                    .all(|byte| byte.is_ascii_uppercase() || byte.is_ascii_digit() || byte == b'_'),
            "invalid secret name"
        );
        ensure!(!result[..index].contains(name), "duplicate secret name");
    }
    Ok(result)
}

pub fn validate(input: &str, environment: impl Fn(&str) -> Option<OsString>) -> Result<()> {
    for name in names(input)? {
        let value = environment(name);
        let text = value.as_ref().and_then(|value| value.to_str());
        ensure!(
            crate::profile_rules::secret_ready(
                value.is_some(),
                text.is_some_and(|value| !value.trim().is_empty()),
                text.is_some_and(|value| value.starts_with("${") && value.ends_with('}'))
            ),
            "required secret is missing, empty, invalid, or unresolved: {name}"
        );
    }
    Ok(())
}

pub fn consume(input: &str, command: &[OsString]) -> Result<ExitStatus> {
    validate(input, |name| std::env::var_os(name))?;
    let executable = command.first().context("consuming command is required")?;
    let mut child = crate::tool::external(executable);
    child.args(&command[1..]);
    if !names(input)?.contains(&"DOPPLER_TOKEN") {
        child.env_remove("DOPPLER_TOKEN");
    }
    child.status().context("start secret consumer")
}

pub fn run(project: &str, config: &str, input: &str, command: &[OsString]) -> Result<ExitStatus> {
    run_with(project, config, input, command, &[])
}

fn run_with(
    project: &str,
    config: &str,
    input: &str,
    command: &[OsString],
    environment: &[(&str, OsString)],
) -> Result<ExitStatus> {
    names(input)?;
    ensure!(
        !project.trim().is_empty() && !config.trim().is_empty(),
        "Doppler project and config must be selected explicitly"
    );
    ensure!(!command.is_empty(), "consuming command is required");
    let executable = std::env::current_exe()?;
    Tool::Doppler
        .command()
        .args([
            "run",
            "--project",
            project,
            "--config",
            config,
            "--only-secrets",
            input,
            "--no-fallback",
            "--",
        ])
        .arg(executable)
        .args(["require-secrets", "--names", input, "--"])
        .args(command)
        .envs(environment.iter().map(|(name, value)| (name, value)))
        .status()
        .context("start scoped Doppler consumer")
}

pub fn exit(status: ExitStatus) -> ! {
    #[cfg(unix)]
    let code = {
        use std::os::unix::process::ExitStatusExt;
        status
            .code()
            .unwrap_or_else(|| 128 + status.signal().unwrap_or(1))
    };
    #[cfg(not(unix))]
    let code = status.code().unwrap_or(1);
    std::process::exit(code)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn opencode_sessions_run_their_own_server_so_they_see_the_credentials() {
        let shaped = |arguments: &[&str]| -> Vec<String> {
            opencode_arguments(&arguments.iter().map(OsString::from).collect::<Vec<_>>())
                .into_iter()
                .map(|argument| argument.to_string_lossy().into_owned())
                .collect()
        };
        assert_eq!(shaped(&[]), ["--standalone"]);
        assert_eq!(shaped(&["--continue"]), ["--standalone", "--continue"]);
        assert_eq!(
            shaped(&["--log-level", "info"]),
            ["--standalone", "--log-level", "info"]
        );
        let directory = std::env::temp_dir().to_string_lossy().into_owned();
        assert_eq!(shaped(&[&directory]), ["--standalone", directory.as_str()]);
        assert_eq!(shaped(&["run", "hello"]), ["run", "--standalone", "hello"]);
        assert_eq!(shaped(&["mcp", "list"]), ["mcp", "list"]);
        assert_eq!(
            shaped(&["--server", "http://127.0.0.1:1"]),
            ["--server", "http://127.0.0.1:1"]
        );
        assert_eq!(shaped(&["--standalone"]), ["--standalone"]);
    }

    #[test]
    fn the_github_mcp_token_has_one_origin() {
        assert!(conflicting_origin(Agent::Opencode, &["GH_MCP_TOKEN"]));
        assert!(!conflicting_origin(Agent::Opencode, &["OTHER_KEY"]));
        assert!(!conflicting_origin(Agent::Codex, &["GH_MCP_TOKEN"]));
    }

    #[test]
    fn only_selected_resolved_nonempty_values_are_admitted() {
        assert!(validate("API_KEY", |_| Some("fixture".into())).is_ok());
        for value in [
            None,
            Some(OsString::from("")),
            Some(OsString::from("${fixture.config.API_KEY}")),
        ] {
            assert!(validate("API_KEY", |_| value.clone()).is_err());
        }
        for invalid in [
            "",
            "API_KEY,API_KEY",
            "API-KEY",
            "lowercase",
            "1KEY",
            "API_KEY,",
        ] {
            assert!(names(invalid).is_err());
        }
    }
}
