use anyhow::{Context, Result, ensure};
use std::ffi::{OsStr, OsString};
use std::process::{Command, ExitStatus};

pub fn translate(
    arguments: &[OsString],
    mut convert: impl FnMut(&OsStr) -> Result<OsString>,
) -> Result<Vec<OsString>> {
    ensure!(
        arguments.len() >= 2
            && arguments[0] == "-Y"
            && matches!(
                arguments[1].to_str(),
                Some("sign" | "verify" | "find-principals" | "check-novalidate")
            ),
        "the WSL bridge supports SSH signing and verification only"
    );
    let mut result = arguments[..2].to_vec();
    let mut index = 2;
    while index < arguments.len() {
        let argument = &arguments[index];
        // Git passes `-U` for a literal signing key held by the agent and `-Overify-time=...` as one attached option.
        if argument == "-q"
            || argument == "-U"
            || argument
                .to_str()
                .is_some_and(|value| value.len() > 2 && value.starts_with("-O"))
        {
            result.push(argument.clone());
            index += 1;
            continue;
        }
        if matches!(argument.to_str(), Some("-f" | "-s" | "-n" | "-I" | "-O")) {
            let value = arguments
                .get(index + 1)
                .context("missing ssh-keygen option value")?;
            result.push(argument.clone());
            let file = matches!(argument.to_str(), Some("-f" | "-s"));
            let inline = value
                .to_str()
                .is_some_and(|value| value.starts_with("key::"));
            result.push(if crate::profile_rules::translate_file(file, inline) {
                convert(value)?
            } else {
                value.clone()
            });
            index += 2;
        } else {
            ensure!(
                !argument
                    .to_str()
                    .is_some_and(|value| value.starts_with('-')),
                "unsupported ssh-keygen option"
            );
            result.push(convert(argument)?);
            index += 1;
        }
    }
    Ok(result)
}

pub fn run(arguments: &[OsString]) -> Result<ExitStatus> {
    ensure!(
        crate::profiles::native_profile()? == crate::profile_rules::Profile::Wsl,
        "WSL signing requires a native WSL host"
    );
    let translated = translate(arguments, |path| {
        let output = Command::new("wslpath").arg("-w").arg(path).output()?;
        ensure!(output.status.success(), "Windows path conversion failed");
        let path = String::from_utf8(output.stdout)?
            .trim_end_matches(['\r', '\n'])
            .to_owned();
        ensure!(
            !path.is_empty(),
            "Windows path conversion returned an empty path"
        );
        Ok(path.into())
    })?;
    Command::new("ssh-keygen.exe")
        .args(translated)
        .status()
        .context("start Windows OpenSSH signing")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn signing_and_verification_paths_cross_the_windows_boundary() {
        let sign: Vec<OsString> = [
            "-Y",
            "sign",
            "-n",
            "git",
            "-f",
            "key::ssh-ed25519 AAAA",
            "/tmp/buffer with spaces",
        ]
        .map(Into::into)
        .into();
        let output = translate(&sign, |path| {
            Ok(format!("windows:{}", path.to_string_lossy()).into())
        })
        .unwrap();
        assert_eq!(output[5], "key::ssh-ed25519 AAAA");
        assert_eq!(output[6], "windows:/tmp/buffer with spaces");
        let verify: Vec<OsString> = [
            "-Y",
            "verify",
            "-n",
            "git",
            "-f",
            "/home/fixture/signers",
            "-s",
            "/tmp/signature",
            "-I",
            "user@example.invalid",
        ]
        .map(Into::into)
        .into();
        let output = translate(&verify, |path| {
            Ok(format!("windows:{}", path.to_string_lossy()).into())
        })
        .unwrap();
        assert_eq!(output[5], "windows:/home/fixture/signers");
        assert_eq!(output[7], "windows:/tmp/signature");
        assert_eq!(output[9], "user@example.invalid");
        assert!(
            translate(
                &["-Y".into(), "sign".into(), "-f".into()],
                |_| unreachable!()
            )
            .is_err()
        );
        assert!(translate(&sign, |_| anyhow::bail!("conversion unavailable")).is_err());
    }

    #[test]
    fn git_literal_key_and_verify_time_arguments_are_preserved() {
        let windows = |path: &OsStr| -> Result<OsString> {
            Ok(format!("windows:{}", path.to_string_lossy()).into())
        };
        let sign: Vec<OsString> = [
            "-Y",
            "sign",
            "-n",
            "git",
            "-f",
            "/tmp/.git_signing_key_tmpAbCdEf",
            "-U",
            "/tmp/.git_signing_buffer_tmpAbCdEf",
        ]
        .map(Into::into)
        .into();
        assert_eq!(
            translate(&sign, windows).unwrap(),
            [
                "-Y",
                "sign",
                "-n",
                "git",
                "-f",
                "windows:/tmp/.git_signing_key_tmpAbCdEf",
                "-U",
                "windows:/tmp/.git_signing_buffer_tmpAbCdEf",
            ]
        );
        let verify: Vec<OsString> = [
            "-Y",
            "verify",
            "-n",
            "git",
            "-f",
            "/home/fixture/signers",
            "-I",
            "user@example.invalid",
            "-s",
            "/tmp/signature",
            "-Overify-time=20261004093000",
        ]
        .map(Into::into)
        .into();
        let output = translate(&verify, windows).unwrap();
        assert_eq!(output[10], "-Overify-time=20261004093000");
        let unsupported: Vec<OsString> = ["-Y", "sign", "-Z", "x"].map(Into::into).into();
        assert!(translate(&unsupported, windows).is_err());
    }
}
