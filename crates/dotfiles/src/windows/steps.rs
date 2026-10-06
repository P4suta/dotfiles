//! The Windows setup steps, which read their options from the machine's chezmoi data.

use super::setup;
use crate::setup::{ContextData, Step};
use anyhow::{Context, Result, bail, ensure};
use serde_json::Value;

fn value<'a>(context: &'a ContextData, pointer: &str) -> Result<&'a Value> {
    context
        .data
        .pointer(&context.platform(pointer))
        .with_context(|| format!("configuration requires {}", context.platform(pointer)))
}

fn text(context: &ContextData, pointer: &str) -> Result<String> {
    Ok(value(context, pointer)?
        .as_str()
        .with_context(|| format!("{} requires a string", context.platform(pointer)))?
        .to_owned())
}

fn texts(context: &ContextData, pointer: &str) -> Result<Vec<String>> {
    value(context, pointer)?
        .as_array()
        .with_context(|| format!("{} requires a list", context.platform(pointer)))?
        .iter()
        .map(|item| {
            item.as_str()
                .map(str::to_owned)
                .context("configuration list requires strings")
        })
        .collect()
}

fn flag(context: &ContextData, pointer: &str) -> Result<bool> {
    value(context, pointer)?
        .as_bool()
        .with_context(|| format!("{} requires a boolean", context.platform(pointer)))
}

fn number(context: &ContextData, pointer: &str) -> Result<u64> {
    value(context, pointer)?
        .as_u64()
        .with_context(|| format!("{} requires a number", context.platform(pointer)))
}

fn finished(step: Step, code: i32) -> Result<()> {
    ensure!(code == 0, "the {} step exited with {code}", step.name());
    Ok(())
}

/// The key that sshd authorizes: the signing key when the data asks for it and the profile signs.
fn authorized_key(context: &ContextData) -> Result<String> {
    let signing = context.data["signing"]["enabled"].as_bool() == Some(true);
    if !(flag(context, "/sshd/authorize_signing_key")? && signing) {
        return Ok(String::new());
    }
    let key = context.data["signing"]["public_key"]
        .as_str()
        .unwrap_or_default()
        .trim()
        .to_owned();
    ensure!(
        !key.is_empty(),
        "Signing requires a machine-local public key"
    );
    Ok(key)
}

/// Runs one Windows setup step.
///
/// # Errors
///
/// Returns an error when the data lacks an option the step reads, or the step fails.
pub(crate) fn run(context: &ContextData, step: Step) -> Result<()> {
    match step {
        Step::Scoop => finished(
            step,
            setup::scoop::run(&setup::scoop::Options {
                installer_commit: text(context, "/scoop/installer_commit")?,
                installer_sha256: text(context, "/scoop/installer_sha256")?,
                buckets: texts(context, "/scoop/buckets")?,
                apps: texts(context, "/scoop/apps")?,
                winget_duplicates: texts(context, "/scoop/winget_duplicates")?,
            })?,
        ),
        Step::Winget => {
            let apps = texts(context, "/winget/apps")?;
            if apps.is_empty() {
                return Ok(());
            }
            finished(step, setup::winget::run(&apps)?)
        }
        Step::Tools => finished(step, setup::tools::run(&texts(context, "/scoop/apps")?)?),
        Step::Desktop | Step::Integrations => finished(step, setup::shell::run()?),
        Step::PythonStub => {
            setup::python_stub::run();
            Ok(())
        }
        Step::Herdr => {
            if flag(context, "/herdr/enabled")? {
                setup::herdr::run(&setup::herdr::Options {
                    manifest_url: text(context, "/herdr/manifest_url")?,
                    channel: text(context, "/herdr/channel")?,
                    expected_build_id: text(context, "/herdr/expected_build_id")?,
                    retain: usize::try_from(number(context, "/herdr/retain")?)?,
                });
            }
            Ok(())
        }
        Step::Sshd => {
            if !flag(context, "/sshd/enabled")? {
                return Ok(());
            }
            finished(
                step,
                setup::sshd::run(&setup::sshd::Options {
                    port: u16::try_from(number(context, "/sshd/port")?)?,
                    default_shell: text(context, "/sshd/default_shell")?,
                    password_auth: flag(context, "/sshd/password_auth")?,
                    firewall_profile: text(context, "/sshd/firewall_profile")?,
                    authorized_key: authorized_key(context)?,
                    elevated: false,
                })?,
            )
        }
        Step::Keyboard => finished(
            step,
            setup::keyboard::run(&setup::keyboard::Options {
                keyboard_delay: u32::try_from(number(context, "/keyboard/keyboard_delay")?)?,
                keyboard_speed: u32::try_from(number(context, "/keyboard/keyboard_speed")?)?,
            })?,
        ),
        Step::StorageScout => finished(step, setup::storage_scout::run()?),
        Step::Ocomment => {
            if flag(context, "/ocomment/enabled")? {
                setup::ocomment::run(&setup::ocomment::Options {
                    repo_url: text(context, "/ocomment/repo_url")?,
                    destination: text(context, "/ocomment/destination")?,
                });
            }
            Ok(())
        }
        other => bail!("{} is no Windows setup step", other.name()),
    }
}
