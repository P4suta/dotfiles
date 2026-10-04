use anyhow::{Context, Result, ensure};
use std::ffi::OsString;
use std::path::Path;
use std::process::{Command, ExitStatus};

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
    let defaults = if context.profile == crate::profile_rules::Profile::Mac
        && matches!(agent, Agent::Opencode)
    {
        vec!["GH_MCP_TOKEN"]
    } else {
        Vec::new()
    };
    let selected = if let Some(value) = selected {
        value
            .as_array()
            .context("secret selection must be an array")?
            .iter()
            .map(|name| name.as_str().context("secret selection must contain names"))
            .collect::<Result<Vec<_>>>()?
    } else {
        defaults
    };
    let command: Vec<_> = std::iter::once(agent.name().into())
        .chain(arguments.iter().cloned())
        .collect();
    if selected.is_empty() {
        return Command::new(&command[0])
            .args(&command[1..])
            .status()
            .context("start agent without API secrets");
    }
    run(
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
    )
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
    let mut child = Command::new(executable);
    child.args(&command[1..]);
    if !names(input)?.contains(&"DOPPLER_TOKEN") {
        child.env_remove("DOPPLER_TOKEN");
    }
    child.status().context("start secret consumer")
}

pub fn run(project: &str, config: &str, input: &str, command: &[OsString]) -> Result<ExitStatus> {
    names(input)?;
    ensure!(
        !project.trim().is_empty() && !config.trim().is_empty(),
        "Doppler project and config must be selected explicitly"
    );
    ensure!(!command.is_empty(), "consuming command is required");
    let executable = std::env::current_exe()?;
    Command::new("doppler")
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
