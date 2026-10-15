use super::cache::ARTIFACT_BYTE_LIMIT;
use super::{Fetch, FetchFailure};
use anyhow::{Context, Result, ensure};
use platform::tool::Tool;
use std::path::Path;
use std::process::{Command, Stdio};

const CONNECT_TIMEOUT_SECONDS: u64 = 10;
const FETCH_TIMEOUT_SECONDS: u64 = 60;
const RETRYABLE_CURL_EXITS: [i32; 7] = [6, 7, 18, 28, 52, 55, 56];

#[derive(Debug)]
pub struct CurlRelease([u64; 3]);

impl CurlRelease {
    /// Read the native transport release without granting fetch permission to a process.
    ///
    /// # Errors
    ///
    /// Refuses malformed releases and transports without an enforced unknown-length byte limit.
    pub fn parse(output: &str) -> Result<Self> {
        let mut fields = output
            .lines()
            .next()
            .context("curl release output is empty")?
            .split_ascii_whitespace();
        ensure!(
            fields.next() == Some("curl"),
            "unrecognized curl release output"
        );
        let text = fields
            .next()
            .context("curl release output has no version")?;
        ensure!(
            rules::artifact_rules::pinned(text.as_bytes()),
            "curl release must be a numeric stable version"
        );
        let parts = text
            .split('.')
            .map(str::parse::<u64>)
            .collect::<Result<Vec<_>, _>>()
            .context("curl release requires numeric components")?;
        let version: [u64; 3] = parts
            .try_into()
            .map_err(|_: Vec<_>| anyhow::anyhow!("curl release requires three components"))?;
        let [minimum_major, minimum_minor, minimum_patch] =
            rules::artifact_rules::CURL_BYTE_LIMIT_VERSION;
        ensure!(
            rules::artifact_rules::bounded_curl(version),
            "curl release must enforce unknown-length download limits (minimum {minimum_major}.{minimum_minor}.{minimum_patch})"
        );
        Ok(Self(version))
    }

    #[must_use]
    pub const fn version(&self) -> [u64; 3] {
        self.0
    }
}

#[derive(Debug)]
pub(super) struct Curl;

fn download(tool: Tool, source: &str, destination: &Path) -> Command {
    let mut command = tool.command();
    command
        .args([
            "--disable",
            "--globoff",
            "--proto",
            "=https",
            "--proto-redir",
            "=https",
            "--tlsv1.2",
            "--fail",
            "--silent",
            "--show-error",
            "--location",
            "--connect-timeout",
        ])
        .arg(CONNECT_TIMEOUT_SECONDS.to_string())
        .arg("--max-time")
        .arg(FETCH_TIMEOUT_SECONDS.to_string())
        .arg("--max-filesize")
        .arg(ARTIFACT_BYTE_LIMIT.to_string())
        .arg("--output")
        .arg(destination)
        .arg("--url")
        .arg(source)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    command
}

impl Fetch for Curl {
    fn fetch(&self, source: &str, destination: &Path) -> std::result::Result<(), FetchFailure> {
        let tool = if cfg!(windows) {
            Tool::CurlExe
        } else {
            Tool::Curl
        };
        let release = tool
            .command()
            .args(["--disable", "--version"])
            .stdin(Stdio::null())
            .stderr(Stdio::null())
            .output()
            .map_err(|error| {
                FetchFailure::Refused(format!("curl release probe failed: {error}"))
            })?;
        if !release.status.success() {
            return Err(FetchFailure::Refused("curl release probe refused".into()));
        }
        let output = String::from_utf8(release.stdout)
            .map_err(|_| FetchFailure::Refused("curl release output is not UTF-8".into()))?;
        CurlRelease::parse(&output).map_err(|error| {
            FetchFailure::Refused(format!("curl capability refused: {error:#}"))
        })?;
        let status = download(tool, source, destination)
            .status()
            .map_err(|error| {
                FetchFailure::Refused(format!(
                    "curl could not start: {:?}, OS error {:?}",
                    error.kind(),
                    error.raw_os_error()
                ))
            })?;
        if status.success() {
            return Ok(());
        }
        let Some(code) = status.code() else {
            return Err(FetchFailure::Refused(
                "curl terminated without an exit code".into(),
            ));
        };
        let cause = format!("curl acquisition exited with code {code}");
        if RETRYABLE_CURL_EXITS.contains(&code) {
            Err(FetchFailure::Retryable(cause))
        } else {
            Err(FetchFailure::Refused(cause))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn one_pinned_url_cannot_expand_to_multiple_transfers() {
        let source = "https://example.com/{one,two}/[1-3]";
        let command = download(Tool::Curl, source, Path::new("owned-destination"));
        let arguments: Vec<_> = command.get_args().collect();
        assert_eq!(arguments.first(), Some(&std::ffi::OsStr::new("--disable")));
        assert!(arguments.contains(&std::ffi::OsStr::new("--globoff")));
        assert_eq!(arguments.iter().filter(|arg| **arg == "--url").count(), 1);
        assert_eq!(arguments.last(), Some(&std::ffi::OsStr::new(source)));
        for (flag, value) in [
            ("--proto", "=https".to_owned()),
            ("--proto-redir", "=https".to_owned()),
            ("--connect-timeout", CONNECT_TIMEOUT_SECONDS.to_string()),
            ("--max-time", FETCH_TIMEOUT_SECONDS.to_string()),
            ("--max-filesize", ARTIFACT_BYTE_LIMIT.to_string()),
        ] {
            assert!(
                arguments
                    .windows(2)
                    .any(|pair| pair == [flag, value.as_str()])
            );
        }
        assert!(!arguments.contains(&std::ffi::OsStr::new("--retry")));
    }

    #[test]
    fn release_output_requires_the_bounded_transport_capability() {
        for (text, expected) in [
            ("curl 8.4.0 (native) libcurl/8.4.0", [8, 4, 0]),
            (
                "curl 8.20.1 (native) libcurl/8.20.1\nFeatures: SSL",
                [8, 20, 1],
            ),
            ("curl 9.0.0 (native) libcurl/9.0.0", [9, 0, 0]),
        ] {
            assert_eq!(CurlRelease::parse(text).unwrap().version(), expected);
        }
        for text in [
            "curl 8.3.99 (native) libcurl/8.3.99",
            "curl 7.88.1 (native) libcurl/7.88.1",
            "curl 8.4",
            "curl 8.4.0-DEV",
            "curl +8.4.0",
            "curl 18446744073709551616.4.0",
            "wget 9.0.0",
            "",
        ] {
            assert!(CurlRelease::parse(text).is_err(), "{text}");
        }
    }
}
