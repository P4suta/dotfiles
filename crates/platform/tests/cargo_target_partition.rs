#![cfg(feature = "tools")]

use anyhow::{Context, Result};
use serde_json::Value;
use std::fs::{self, File, FileTimes};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

const SUBJECT: &str = "cache_subject";
const CONSUMER: &str = "cache_consumer";
const PRESERVED_MTIME: Duration = Duration::from_secs(1_700_000_000);
const NEWER_MTIME: Duration = Duration::from_secs(2);
#[cfg(target_os = "macos")]
const DARWIN_EILSEQ: i32 = 92;
const WORKSPACE: &str = "[workspace]\nmembers=['subject','consumer']\nresolver='3'\n";
const LOCK: &str = "version=4\n[[package]]\nname='cache_consumer'\nversion='0.1.0'\ndependencies=['cache_subject']\n[[package]]\nname='cache_subject'\nversion='0.1.0'\n";

struct Snapshot {
    root: PathBuf,
    subject: PathBuf,
}

impl Snapshot {
    fn capture(parent: &Path, name: &str, symbol: &str) -> Result<Self> {
        let root = parent.join(name);
        let subject = root.join("subject/src/lib.rs");
        fs::create_dir_all(subject.parent().context("subject source parent")?)?;
        fs::create_dir_all(root.join("consumer/src"))?;
        for (path, contents) in [
            ("Cargo.toml", WORKSPACE),
            ("Cargo.lock", LOCK),
            (
                "subject/Cargo.toml",
                "[package]\nname='cache_subject'\nversion='0.1.0'\nedition='2024'\n",
            ),
            (
                "consumer/Cargo.toml",
                "[package]\nname='cache_consumer'\nversion='0.1.0'\nedition='2024'\n[dependencies]\ncache_subject={path='../subject'}\n",
            ),
        ] {
            fs::write(root.join(path), contents)?;
        }
        fs::write(&subject, format!("pub fn {symbol}() -> u32 {{ 37 }}\n"))?;
        fs::write(
            root.join("consumer/src/main.rs"),
            format!("fn main() {{ println!(\"{{}}\", cache_subject::{symbol}()); }}\n"),
        )?;
        let preserved = SystemTime::UNIX_EPOCH + PRESERVED_MTIME;
        File::options()
            .write(true)
            .open(&subject)?
            .set_times(FileTimes::new().set_modified(preserved))?;
        Ok(Self { root, subject })
    }

    fn check(&self, package: &str, cache: &Path, home: &Path) -> Result<Vec<Value>> {
        let target = platform::cargo_target::code(&self.root, cache)?;
        let output = platform::tool::Tool::Cargo
            .command()
            .current_dir(&self.root)
            .args([
                "check",
                "--locked",
                "--offline",
                "--jobs",
                "2",
                "--package",
                package,
                "--message-format=json-render-diagnostics",
            ])
            .env("CARGO_HOME", home)
            .env("CARGO_TARGET_DIR", &target)
            .env("CARGO_INCREMENTAL", "0")
            .env("CARGO_PROFILE_DEV_DEBUG", "0")
            .output()?;
        assert!(
            output.status.success(),
            "actual {package} check from {} failed under {}: {:?}\nstdout={}\nstderr={}",
            self.root.display(),
            target.display(),
            output.status.code(),
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        std::str::from_utf8(&output.stdout)?
            .lines()
            .map(serde_json::from_str)
            .collect::<serde_json::Result<_>>()
            .map_err(Into::into)
    }
}

fn subject_artifact(reports: &[Value]) -> Result<&Value> {
    reports
        .iter()
        .find(|report| {
            report["reason"] == "compiler-artifact" && report["target"]["name"] == SUBJECT
        })
        .context("actual Cargo returned no subject artifact")
}

#[test]
fn alternating_captured_sources_keep_their_correct_cached_symbols() -> Result<()> {
    let owned = tempfile::tempdir()?;
    let directory = platform::paths::canonical(owned.path())?;
    let cache = directory.join("targets");
    let home = directory.join("cargo-home");
    fs::create_dir(&home)?;
    let current = Snapshot::capture(&directory, "current", "current_symbol")?;
    let previous = Snapshot::capture(&directory, "diagnosis", "previous_symbol")?;
    let original_mtime = fs::metadata(&current.subject)?.modified()?;
    let first = current.check(SUBJECT, &cache, &home)?;
    assert_eq!(subject_artifact(&first)?["fresh"], false);
    let later = SystemTime::now()
        .checked_add(NEWER_MTIME)
        .context("newer diagnosis timestamp overflows")?;
    File::options()
        .write(true)
        .open(&previous.subject)?
        .set_times(FileTimes::new().set_modified(later))?;
    let second = previous.check(SUBJECT, &cache, &home)?;
    assert_eq!(subject_artifact(&second)?["fresh"], false);
    assert_eq!(fs::metadata(&current.subject)?.modified()?, original_mtime);
    let returned = current.check(CONSUMER, &cache, &home)?;
    assert_eq!(subject_artifact(&returned)?["fresh"], true);
    previous.check(CONSUMER, &cache, &home)?;
    let repeated = current.check(CONSUMER, &cache, &home)?;
    assert_eq!(subject_artifact(&repeated)?["fresh"], true);
    assert_ne!(
        platform::cargo_target::code(&current.root, &cache)?,
        platform::cargo_target::code(&previous.root, &cache)?
    );
    assert!(!home.join("registry/index").exists());
    platform::cleanup::report(owned.close(), "remove the alternating Cargo Source fixture");
    Ok(())
}

#[test]
fn cargo_target_refuses_missing_roots_files_and_relative_cache_bases() -> Result<()> {
    let owned = tempfile::tempdir()?;
    let root = platform::paths::canonical(owned.path())?;
    let cache = root.join("cache");
    assert!(platform::cargo_target::code(&root.join("missing"), &cache).is_err());
    let file = root.join("source-file");
    fs::write(&file, b"not a Source directory")?;
    assert!(platform::cargo_target::code(&file, &cache).is_err());
    assert!(platform::cargo_target::code(&root, Path::new("relative-cache")).is_err());
    assert_eq!(
        platform::cargo_target::code(&root, &cache)?,
        platform::cargo_target::code(&root.join("."), &cache)?
    );
    Ok(())
}

#[cfg(unix)]
#[test]
fn cargo_target_preserves_or_refuses_non_unicode_source_identity() -> Result<()> {
    use std::os::unix::ffi::OsStringExt as _;

    let owned = tempfile::tempdir()?;
    let root = platform::paths::canonical(owned.path())?;
    let first = root.join(std::ffi::OsString::from_vec(vec![b'r', 0xfe]));
    let second = root.join(std::ffi::OsString::from_vec(vec![b'r', 0xff]));
    assert_eq!(first.to_string_lossy(), second.to_string_lossy());
    let cache = root.join("cache");
    let first_creation = fs::create_dir(&first);
    #[cfg(target_os = "macos")]
    if matches!(&first_creation, Err(error) if error.raw_os_error() == Some(DARWIN_EILSEQ)) {
        assert!(platform::cargo_target::code(&first, &cache).is_err());
        assert!(platform::cargo_target::code(&second, &cache).is_err());
        return Ok(());
    }
    first_creation?;
    fs::create_dir(&second)?;
    assert_ne!(
        platform::cargo_target::code(&first, &cache)?,
        platform::cargo_target::code(&second, &cache)?
    );
    Ok(())
}
