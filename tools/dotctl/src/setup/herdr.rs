use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};

use super::download;
use crate::{env, proc};

#[derive(Debug)]
pub struct Options {
    pub manifest_url: String,
    pub channel: String,
    pub expected_build_id: String,
    pub retain: usize,
}

#[derive(Debug, PartialEq, Eq)]
struct Manifest {
    identity: String,
    build_id: String,
    protocol: String,
    url: String,
    sha256: String,
}

pub fn run(options: &Options) -> Result<i32> {
    println!(">>> herdr install ({} channel)", options.channel);
    if let Err(err) = install(options) {
        eprintln!("herdr install failed: {err:#}");
        eprintln!("Fall back to the official installer if this keeps failing:");
        eprintln!(
            "  powershell -ExecutionPolicy Bypass -c \"irm https://herdr.dev/install.ps1 | iex\""
        );
    }
    Ok(0)
}

fn manifest_url(options: &Options) -> String {
    const STABLE: &str = "latest.json";
    const PREVIEW: &str = "preview.json";

    if options.channel != "preview" {
        return options.manifest_url.clone();
    }
    match options.manifest_url.strip_suffix(STABLE) {
        Some(base) => format!("{base}{PREVIEW}"),
        None => options.manifest_url.clone(),
    }
}

fn install(options: &Options) -> Result<()> {
    let manifest = parse_manifest(&download::fetch(&manifest_url(options))?)?;

    if !options.expected_build_id.is_empty() && options.expected_build_id != manifest.build_id {
        eprintln!(
            "herdr: manifest advertises build {}, expected {}. Not installing.",
            manifest.build_id, options.expected_build_id
        );
        eprintln!(
            "Clear expected_build_id in .chezmoidata.toml to take the manifest's current build."
        );
        return Ok(());
    }

    let home = herdr_home();
    let standalone = home.join("packages").join("standalone");
    let releases = standalone.join("releases");
    let release_dir = releases.join(&manifest.identity);
    let exe = release_dir.join("herdr.exe");

    if is_current(&release_dir, &manifest.sha256) {
        println!("    release {} already installed", manifest.identity);
    } else {
        println!("    downloading release {}", manifest.identity);
        std::fs::create_dir_all(&releases)?;
        install_into(&releases, &manifest)?;
    }

    link(&standalone.join("current"), &release_dir)?;
    let visible = env::local_app_data()
        .context("no %LOCALAPPDATA%")?
        .join("Programs")
        .join("Herdr")
        .join("bin");
    link(&visible, &standalone.join("current"))?;

    prune(&releases, options.retain, &manifest.identity);

    let version = proc::capture(std::process::Command::new(&exe).arg("--version"))
        .map(|out| out.stdout_text().trim().to_owned())
        .unwrap_or_default();
    println!("    {version} (protocol {})", manifest.protocol);
    Ok(())
}

fn install_into(releases: &Path, manifest: &Manifest) -> Result<()> {
    let staging = releases.join(format!(".staging.{}", manifest.identity));
    let _ = std::fs::remove_dir_all(&staging);
    std::fs::create_dir_all(&staging)?;
    let staged = std::env::temp_dir().join(format!("herdr-{}.download", manifest.identity));
    download::download(&manifest.url, &staged)?;

    let got = download::sha256_of(&staged)?;
    if !got.eq_ignore_ascii_case(&manifest.sha256) {
        let _ = std::fs::remove_file(&staged);
        let _ = std::fs::remove_dir_all(&staging);
        bail!(
            "sha256 mismatch for {}: manifest says {}, got {got}",
            manifest.url,
            manifest.sha256
        );
    }

    place(&staged, &staging)?;
    let _ = std::fs::remove_file(&staged);
    let release_dir = releases.join(&manifest.identity);
    if !staging.join("herdr.exe").is_file() {
        bail!("the asset produced no herdr.exe in {}", staging.display());
    }

    let backup = if release_dir.exists() {
        let backup = releases.join(format!(
            ".backup.{}.{}",
            manifest.identity,
            std::process::id()
        ));
        std::fs::rename(&release_dir, &backup)
            .with_context(|| format!("moving {} aside", release_dir.display()))?;
        Some(backup)
    } else {
        None
    };
    std::fs::rename(&staging, &release_dir).inspect_err(|_| {
        if let Some(backup) = &backup {
            let _ = std::fs::rename(backup, &release_dir);
        }
    })?;
    if let Some(backup) = backup {
        let _ = std::fs::remove_dir_all(backup);
    }
    std::fs::write(stamp_path(&release_dir), &manifest.sha256)?;
    Ok(())
}

fn parse_manifest(json: &str) -> Result<Manifest> {
    let value: serde_json::Value = serde_json::from_str(json).context("manifest is not JSON")?;
    let asset = value
        .pointer("/assets/windows-x86_64")
        .context("manifest has no windows-x86_64 asset")?;
    let field = |parent: &serde_json::Value, name: &str| -> Result<String> {
        parent
            .get(name)
            .and_then(serde_json::Value::as_str)
            .map(ToOwned::to_owned)
            .with_context(|| format!("manifest asset has no {name}"))
    };
    let (url, sha256) = if let Some(url) = asset.as_str() {
        let digest = value
            .pointer("/sha256/windows-x86_64")
            .and_then(serde_json::Value::as_str)
            .map(ToOwned::to_owned)
            .context("stable manifest has no sha256 for windows-x86_64")?;
        (url.to_owned(), digest)
    } else {
        (field(asset, "url")?, field(asset, "sha256")?)
    };
    let build_id = value
        .get("build_id")
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default()
        .to_owned();
    let version = ["base_version", "version"]
        .iter()
        .find_map(|name| value.get(*name).and_then(serde_json::Value::as_str))
        .unwrap_or_default()
        .to_owned();
    Ok(Manifest {
        identity: release_identity(&version, &build_id)
            .context("manifest has neither a version nor a build_id")?,
        build_id,
        protocol: value
            .get("protocol")
            .map(scalar_to_string)
            .unwrap_or_default(),
        url,
        sha256,
    })
}

fn release_identity(version: &str, build_id: &str) -> Option<String> {
    let base = if build_id.is_empty() {
        version.to_owned()
    } else if version.is_empty() {
        build_id.to_owned()
    } else {
        format!("{version}-preview.{build_id}")
    };
    (!base.is_empty()).then(|| format!("{base}-x86_64-pc-windows-msvc"))
}

fn scalar_to_string(value: &serde_json::Value) -> String {
    match value {
        serde_json::Value::String(text) => text.clone(),
        other => other.to_string(),
    }
}

fn place(staged: &Path, release_dir: &Path) -> Result<()> {
    if is_zip(staged)? {
        let tar = std::path::Path::new(&std::env::var_os("SystemRoot").unwrap_or_default())
            .join("System32")
            .join("tar.exe");
        let mut cmd = env::command(tar.to_string_lossy().as_ref());
        cmd.arg("-xf").arg(staged).arg("-C").arg(release_dir);
        proc::capture_ok(&mut cmd).context("extracting the herdr archive")?;
        return Ok(());
    }

    let exe = release_dir.join("herdr.exe");
    std::fs::rename(staged, &exe).or_else(|_| std::fs::copy(staged, &exe).map(|_| ()))?;
    Ok(())
}

fn is_zip(path: &Path) -> Result<bool> {
    use std::io::Read;

    let mut magic = [0u8; 4];
    let read = std::fs::File::open(path)?.read(&mut magic)?;
    Ok(read == 4 && magic == [b'P', b'K', 0x03, 0x04])
}

fn stamp_path(release_dir: &Path) -> std::path::PathBuf {
    release_dir.join(".dotctl-sha256")
}

fn is_current(release_dir: &Path, want: &str) -> bool {
    if !release_dir.join("herdr.exe").is_file() {
        return false;
    }
    std::fs::read_to_string(stamp_path(release_dir))
        .is_ok_and(|stamp| stamp.trim().eq_ignore_ascii_case(want))
}

fn link(link_path: &Path, target: &Path) -> Result<()> {
    if let Some(parent) = link_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let is_junction = junction::exists(link_path).unwrap_or(false);

    if is_junction {
        if junction::get_target(link_path).is_ok_and(|current| current == target) {
            return Ok(());
        }
        junction::delete(link_path)?;
        std::fs::remove_dir(link_path)
            .with_context(|| format!("removing the emptied junction at {}", link_path.display()))?;
    } else if link_path.exists() {
        if !is_empty_dir(link_path) {
            bail!(
                "{} exists and is not a junction; refusing to replace it",
                link_path.display()
            );
        }
        std::fs::remove_dir(link_path)?;
    }

    junction::create(target, link_path)
        .with_context(|| format!("linking {} -> {}", link_path.display(), target.display()))
}

fn is_empty_dir(path: &Path) -> bool {
    std::fs::read_dir(path).is_ok_and(|mut entries| entries.next().is_none())
}

fn prune(releases: &Path, retain: usize, current: &str) {
    let Ok(entries) = std::fs::read_dir(releases) else {
        return;
    };
    let installed: Vec<(String, std::time::SystemTime)> = entries
        .filter_map(Result::ok)
        .filter(|entry| entry.path().is_dir())
        .filter_map(|entry| {
            let modified = entry
                .metadata()
                .and_then(|metadata| metadata.modified())
                .ok()?;
            Some((entry.file_name().into_string().ok()?, modified))
        })
        .collect();

    for name in stale_releases(&installed, retain, current) {
        let _ = std::fs::remove_dir_all(releases.join(name));
    }
}

/// Releases beyond the newest `retain` by installation time; version names do not sort numerically (`0.10` < `0.9` as text).
fn stale_releases(
    releases: &[(String, std::time::SystemTime)],
    retain: usize,
    current: &str,
) -> Vec<String> {
    let mut sorted: Vec<&(String, std::time::SystemTime)> = releases
        .iter()
        .filter(|(name, _)| !name.starts_with(".staging.") && !name.starts_with(".backup."))
        .collect();
    sorted.sort_by(|(a_name, a_time), (b_name, b_time)| {
        b_time.cmp(a_time).then_with(|| b_name.cmp(a_name))
    });
    sorted
        .into_iter()
        .skip(retain)
        .filter(|(name, _)| name.as_str() != current)
        .map(|(name, _)| name.clone())
        .collect()
}

fn herdr_home() -> PathBuf {
    if let Some(home) = std::env::var_os("HERDR_HOME").filter(|value| !value.is_empty()) {
        return PathBuf::from(home);
    }
    std::env::var_os("USERPROFILE")
        .or_else(|| std::env::var_os("HOME"))
        .map(PathBuf::from)
        .unwrap_or_default()
        .join(".herdr")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn timed(releases: &[(&str, u64)]) -> Vec<(String, std::time::SystemTime)> {
        releases
            .iter()
            .map(|(name, seconds)| {
                (
                    (*name).to_owned(),
                    std::time::UNIX_EPOCH + std::time::Duration::from_secs(*seconds),
                )
            })
            .collect()
    }

    const SAMPLE: &str = r#"{
        "base_version": "0.9.2",
        "build_id": "2026-07-28-abc1234",
        "protocol": "3",
        "assets": {
            "windows-x86_64": {
                "url": "https://herdr.dev/x.exe",
                "sha256": "AABBCC"
            }
        }
    }"#;

    fn options(manifest_url: &str, channel: &str) -> Options {
        Options {
            manifest_url: manifest_url.to_owned(),
            channel: channel.to_owned(),
            expected_build_id: String::new(),
            retain: 3,
        }
    }

    #[test]
    fn the_preview_channel_reads_the_preview_manifest() {
        const STABLE_URL: &str = "https://herdr.dev/latest.json";

        assert_eq!(
            manifest_url(&options(STABLE_URL, "preview")),
            "https://herdr.dev/preview.json"
        );
        assert_eq!(manifest_url(&options(STABLE_URL, "stable")), STABLE_URL);
    }

    #[test]
    fn a_manifest_url_naming_another_file_is_taken_as_given() {
        let url = "https://example.test/herdr.json";

        assert_eq!(manifest_url(&options(url, "preview")), url);
    }

    #[test]
    fn a_preview_manifest_yields_the_asset_and_the_official_name() {
        let manifest = parse_manifest(SAMPLE).expect("parses");
        assert_eq!(
            manifest,
            Manifest {
                identity: "0.9.2-preview.2026-07-28-abc1234-x86_64-pc-windows-msvc".to_owned(),
                build_id: "2026-07-28-abc1234".to_owned(),
                protocol: "3".to_owned(),
                url: "https://herdr.dev/x.exe".to_owned(),
                sha256: "AABBCC".to_owned(),
            }
        );
    }

    #[test]
    fn a_stable_manifest_reads_the_top_level_checksum() {
        let json = r#"{
            "version": "0.9.1",
            "protocol": 22,
            "assets": {"windows-x86_64": "https://herdr.dev/y.zip"},
            "sha256": {"windows-x86_64": "DDEEFF"}
        }"#;
        let manifest = parse_manifest(json).expect("parses");
        assert_eq!(
            manifest,
            Manifest {
                identity: "0.9.1-x86_64-pc-windows-msvc".to_owned(),
                build_id: String::new(),
                protocol: "22".to_owned(),
                url: "https://herdr.dev/y.zip".to_owned(),
                sha256: "DDEEFF".to_owned(),
            }
        );
    }

    #[test]
    fn a_preview_manifest_without_a_base_version_keeps_the_build_id_name() {
        let manifest = parse_manifest(SAMPLE.replace("\"base_version\": \"0.9.2\",", "").as_str())
            .expect("parses");
        assert_eq!(
            manifest.identity,
            "2026-07-28-abc1234-x86_64-pc-windows-msvc"
        );
    }

    #[test]
    fn a_manifest_with_neither_version_nor_build_is_an_error() {
        let json = r#"{"assets": {"windows-x86_64": {"url": "u", "sha256": "s"}}}"#;
        assert!(parse_manifest(json).is_err());
    }

    #[test]
    fn a_manifest_without_a_windows_asset_is_an_error() {
        let json = r#"{"build_id": "x", "assets": {"linux-x86_64": {}}}"#;
        assert!(parse_manifest(json).is_err());
    }

    #[test]
    fn a_manifest_without_a_sha_is_an_error() {
        let json = r#"{"build_id": "x", "assets": {"windows-x86_64": {"url": "u"}}}"#;
        assert!(parse_manifest(json).is_err());
    }

    #[test]
    fn retention_keeps_the_newest_builds() {
        let names = timed(&[
            ("0.9.2-preview.2026-07-26-a-x86_64-pc-windows-msvc", 26),
            ("0.9.2-preview.2026-07-28-c-x86_64-pc-windows-msvc", 28),
            ("0.9.2-preview.2026-07-27-b-x86_64-pc-windows-msvc", 27),
            ("0.9.2-preview.2026-07-25-z-x86_64-pc-windows-msvc", 25),
        ]);
        assert_eq!(
            stale_releases(
                &names,
                3,
                "0.9.2-preview.2026-07-28-c-x86_64-pc-windows-msvc"
            ),
            vec!["0.9.2-preview.2026-07-25-z-x86_64-pc-windows-msvc".to_owned()]
        );
    }

    #[test]
    fn retention_never_removes_the_current_build() {
        let names = timed(&[
            ("0.9.2-preview.2026-07-28-c-x86_64-pc-windows-msvc", 28),
            ("0.9.2-preview.2026-07-27-b-x86_64-pc-windows-msvc", 27),
            ("0.9.2-preview.2026-07-26-a-x86_64-pc-windows-msvc", 26),
        ]);
        let current = "0.9.2-preview.2026-07-26-a-x86_64-pc-windows-msvc";
        assert_eq!(stale_releases(&names, 1, current).len(), 1);
        assert!(!stale_releases(&names, 1, current).contains(&current.to_owned()));
    }

    #[test]
    fn retention_below_the_limit_removes_nothing() {
        let names = timed(&[("0.9.1-x86_64-pc-windows-msvc", 1)]);
        assert!(stale_releases(&names, 3, "0.9.1-x86_64-pc-windows-msvc").is_empty());
    }

    #[test]
    fn retention_orders_by_installation_rather_than_version_text() {
        let names = timed(&[
            ("0.9.9-x86_64-pc-windows-msvc", 1),
            ("0.10.0-x86_64-pc-windows-msvc", 2),
        ]);
        assert_eq!(
            stale_releases(&names, 1, "0.10.0-x86_64-pc-windows-msvc"),
            vec!["0.9.9-x86_64-pc-windows-msvc".to_owned()]
        );
    }

    #[test]
    fn retention_ignores_staging_and_backup_directories() {
        let names = timed(&[
            (".staging.0.9.1-x86_64-pc-windows-msvc", 3),
            (".backup.0.9.1-x86_64-pc-windows-msvc.1234", 2),
            ("0.9.1-x86_64-pc-windows-msvc", 1),
        ]);
        assert!(stale_releases(&names, 3, "0.9.1-x86_64-pc-windows-msvc").is_empty());
        assert_eq!(
            stale_releases(&names, 0, "0.9.1-x86_64-pc-windows-msvc").len(),
            0
        );
    }
}
