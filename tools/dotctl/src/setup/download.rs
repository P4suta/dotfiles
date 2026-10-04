use std::path::Path;

use anyhow::{Context, Result, bail};
use sha2::{Digest, Sha256};

use crate::{env, proc};

pub fn fetch(url: &str) -> Result<String> {
    let mut cmd = env::command("curl.exe");
    cmd.args(["--fail", "--silent", "--show-error", "--location", url]);
    proc::capture_ok(&mut cmd).with_context(|| format!("fetching {url}"))
}

pub fn download(url: &str, to: &Path) -> Result<()> {
    let mut cmd = env::command("curl.exe");
    cmd.args([
        "--fail",
        "--silent",
        "--show-error",
        "--location",
        url,
        "--output",
    ])
    .arg(to);
    proc::capture_ok(&mut cmd).with_context(|| format!("downloading {url}"))?;
    Ok(())
}

pub fn download_verified(url: &str, sha256: &str, to: &Path) -> Result<()> {
    download(url, to)?;
    let got = sha256_of(to)?;
    if !got.eq_ignore_ascii_case(sha256) {
        let _ = std::fs::remove_file(to);
        bail!("sha256 mismatch for {url}: expected {sha256}, got {got}");
    }
    Ok(())
}

pub fn sha256_of(path: &Path) -> Result<String> {
    let mut file = std::fs::File::open(path)?;
    let mut hasher = Sha256::new();
    std::io::copy(&mut file, &mut hasher)?;
    Ok(format!("{:x}", hasher.finalize()))
}
