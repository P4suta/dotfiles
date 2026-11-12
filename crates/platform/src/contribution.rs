use anyhow::{Context, Result, ensure};
use serde::Deserialize;
use std::{borrow::Cow, fs, io::Read, path::Path};

const SCHEMA: u32 = 1;
const MAX_NOTICE_BYTES: u64 = 16 * 1024;
const NOTICE_DIRECTORY: &str = ".config/dotfiles/contribution-notices";
const GITHUB_PREFIX: &str = "https://github.com/";

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Notice {
    schema: u32,
    repository: String,
    #[serde(rename = "notice")]
    text: String,
    source: String,
    reason: String,
    remove_when: String,
}

fn single_line(value: &str) -> bool {
    !value.trim().is_empty() && value.trim() == value && !value.chars().any(char::is_control)
}

impl Notice {
    /// # Errors
    /// Rejects malformed registrations, hidden notices, and mismatched destinations.
    pub fn parse(bytes: &[u8], repository: &str) -> Result<Self> {
        ensure!(
            u64::try_from(bytes.len())? <= MAX_NOTICE_BYTES,
            "contribution notice exceeds its byte limit"
        );
        let notice: Self = serde_json::from_slice(bytes)?;
        ensure!(
            notice.schema == SCHEMA
                && crate::source_identity::github_slug(&format!("{GITHUB_PREFIX}{repository}"))
                    .as_deref()
                    == Some(repository)
                && notice.repository.eq_ignore_ascii_case(repository),
            "contribution notice belongs to another destination or format"
        );
        let source = url::Url::parse(&notice.source)?;
        ensure!(
            source.scheme() == "https"
                && source.host_str().is_some()
                && source.username().is_empty()
                && source.password().is_none()
                && single_line(&notice.reason)
                && single_line(&notice.remove_when)
                && single_line(&notice.text)
                && !notice.text.contains(['`', '<', '>', '*', '#', '[', ']']),
            "contribution notice requires visible text, a primary HTTPS source, a reason, and a removal condition"
        );
        Ok(notice)
    }

    /// # Errors
    /// Rejects missing, repeated, embedded, or nonfinal required notices.
    pub fn project(&self, body: &str) -> Result<String> {
        ensure!(
            body.lines().filter(|line| *line == self.text).count() == 1,
            "the exact required contribution notice must occur once"
        );
        let remaining = body
            .trim_end()
            .strip_suffix(&self.text)
            .context("the required contribution notice must be the final line")?;
        ensure!(
            remaining.is_empty() || remaining.ends_with('\n'),
            "the required contribution notice must occupy its own line"
        );
        Ok(remaining.to_owned())
    }
}

/// # Errors
/// Rejects malformed local registrations and unreadable notice files.
pub fn project<'a>(repository: Option<&str>, body: &'a str) -> Result<Cow<'a, str>> {
    let Some(repository) = repository else {
        return Ok(Cow::Borrowed(body));
    };
    let Some(home) = crate::home::home() else {
        return Ok(Cow::Borrowed(body));
    };
    project_in(&home, repository, body)
}

/// # Errors
/// Rejects malformed local registrations and unreadable notice files.
pub fn project_in<'a>(home: &Path, repository: &str, body: &'a str) -> Result<Cow<'a, str>> {
    ensure!(
        crate::source_identity::github_slug(&format!("{GITHUB_PREFIX}{repository}")).as_deref()
            == Some(repository),
        "contribution notice requires an exact GitHub repository"
    );
    let filename = format!(
        "{}.json",
        repository.to_ascii_lowercase().replace('/', "__")
    );
    let path = home.join(NOTICE_DIRECTORY).join(filename);
    let metadata = match fs::symlink_metadata(&path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(Cow::Borrowed(body));
        }
        Err(error) => return Err(error.into()),
    };
    ensure!(
        metadata.is_file() && metadata.len() <= MAX_NOTICE_BYTES,
        "contribution notice must be a bounded regular file"
    );
    let mut bytes = Vec::new();
    fs::File::open(path)?
        .take(MAX_NOTICE_BYTES + 1)
        .read_to_end(&mut bytes)?;
    Ok(Cow::Owned(
        Notice::parse(&bytes, repository)?.project(body)?,
    ))
}
