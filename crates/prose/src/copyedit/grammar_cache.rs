use crate::{Bundle, build_inputs::hex, tool::Tool};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs::{self, File, OpenOptions};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

pub const PIN: &str = "nlp/languagetool.toml";
pub const ADAPTER: &str = "nlp/GrammarAnnotations.java";
const RECEIPT: &str = "verified.json";
const ARCHIVE: &str = "grammar.zip";
const DOWNLOAD_ORIGIN: &str = "https://languagetool.org/download/";
const HASH_BUFFER: usize = 64 * 1024;

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Pin {
    pub version: String,
    pub url: String,
    pub sha256: String,
    pub java: String,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Receipt {
    archive_sha256: String,
    tree_sha256: String,
}

#[derive(Debug)]
pub struct Installation {
    pub root: PathBuf,
    pub pin: Pin,
}

impl Installation {
    pub fn command(&self, program: &str) -> Command {
        let mut command = Tool::Mise.command();
        command
            .args(["exec", &format!("java@{}", self.pin.java), "--", program])
            .env_remove("JAVA_TOOL_OPTIONS")
            .env_remove("JDK_JAVA_OPTIONS")
            .env_remove("JDK_JAVAC_OPTIONS")
            .env_remove("CLASSPATH");
        command
    }

    pub fn classpath(&self, classes: Option<&Path>) -> Result<std::ffi::OsString> {
        let mut paths = vec![
            self.root.clone(),
            self.root.join("languagetool.jar"),
            self.root.join("libs/*"),
        ];
        if let Some(classes) = classes {
            paths.push(classes.to_path_buf());
        }
        std::env::join_paths(paths).context("join the local grammar engine classpath")
    }

    pub fn compiler_classpath(&self) -> Result<std::ffi::OsString> {
        const LIBRARIES: [&str; 4] = [
            "languagetool-core.jar",
            "jackson-annotations.jar",
            "jackson-core.jar",
            "jackson-databind.jar",
        ];
        let paths: Vec<_> = LIBRARIES
            .into_iter()
            .map(|name| self.root.join("libs").join(name))
            .collect();
        ensure!(
            paths.iter().all(|path| path.is_file()),
            "the grammar compiler classpath is incomplete"
        );
        std::env::join_paths(paths).context("join the typed grammar adapter's compile dependencies")
    }
}

fn file_hash(path: &Path) -> Result<String> {
    let mut file = File::open(path)?;
    let mut buffer = vec![0; HASH_BUFFER];
    let mut hasher = Sha256::new();
    loop {
        let count = file.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        hasher.update(&buffer[..count]);
    }
    Ok(hex(&hasher.finalize()))
}

fn verify(target: &Path, pin: &Pin) -> Result<()> {
    let receipt: Receipt = serde_json::from_slice(&fs::read(target.join(RECEIPT))?)?;
    ensure!(
        receipt.archive_sha256 == pin.sha256,
        "grammar cache belongs to another archive"
    );
    let root = target.join(format!("LanguageTool-{}", pin.version));
    ensure!(
        crate::tree_digest(&root)? == receipt.tree_sha256,
        "grammar engine files differ from their verified archive"
    );
    Ok(())
}

fn extract(bundle: &Bundle, target: &Path, pin: &Pin) -> Result<()> {
    let temporary = tempfile::tempdir_in(target.parent().context("grammar cache needs a parent")?)?;
    let archive = temporary.path().join(ARCHIVE);
    if let Some(source) = std::env::var_os("PROSE_LANGUAGE_TOOL_ARCHIVE") {
        fs::copy(PathBuf::from(source), &archive)?;
    } else {
        let output = bundle
            .xh()
            .args([
                "--quiet",
                "--ignore-stdin",
                "--ignore-netrc",
                "--follow",
                "--download",
                "--output",
            ])
            .arg(&archive)
            .args(["GET", &pin.url])
            .stdin(Stdio::null())
            .output()
            .context("download the pinned grammar archive")?;
        ensure!(
            output.status.success(),
            "grammar archive download failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    ensure!(
        file_hash(&archive)? == pin.sha256,
        "grammar archive differs from its pinned SHA-256"
    );
    let installation = Installation {
        root: temporary.path().to_path_buf(),
        pin: pin.clone(),
    };
    let output = installation
        .command("java")
        .current_dir(temporary.path())
        .args([
            "-m",
            "jdk.jartool/sun.tools.jar.Main",
            "--extract",
            "--file",
        ])
        .arg(&archive)
        .stdin(Stdio::null())
        .output()
        .context("extract the verified grammar archive with the pinned Java runtime")?;
    ensure!(
        output.status.success(),
        "grammar archive extraction failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    fs::remove_file(&archive)?;
    let root = temporary
        .path()
        .join(format!("LanguageTool-{}", pin.version));
    let receipt = Receipt {
        archive_sha256: pin.sha256.clone(),
        tree_sha256: crate::tree_digest(&root)?,
    };
    fs::write(
        temporary.path().join(RECEIPT),
        serde_json::to_vec(&receipt)?,
    )?;
    fs::rename(temporary.keep(), target)?;
    Ok(())
}

pub fn prepare(bundle: &Bundle) -> Result<Installation> {
    let text = fs::read_to_string(bundle.config.join(PIN))?;
    let pin: Pin = toml::from_str(&text).context("read the grammar engine pin")?;
    ensure!(
        !pin.version.is_empty()
            && pin
                .version
                .chars()
                .all(|character| character.is_ascii_digit() || character == '.'),
        "invalid grammar version"
    );
    ensure!(
        pin.url == format!("{DOWNLOAD_ORIGIN}LanguageTool-{}.zip", pin.version),
        "grammar archive must come from its pinned upstream origin"
    );
    ensure!(
        pin.sha256.len() == rules::skill_rules::DIGEST_LENGTH
            && pin.sha256.bytes().all(|byte| byte.is_ascii_hexdigit()),
        "invalid grammar archive digest"
    );
    ensure!(
        pin.java.starts_with("temurin-")
            && pin
                .java
                .chars()
                .all(|character| character.is_ascii_alphanumeric()
                    || matches!(character, '-' | '.' | '+')),
        "invalid pinned Java distribution"
    );
    let runtime = crate::runtime_directory()?;
    fs::create_dir_all(&runtime)?;
    let target = runtime.join(format!("grammar-{}", pin.sha256));
    let lock = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(runtime.join("grammar.lock"))?;
    lock.lock()?;
    if !target.exists() {
        extract(bundle, &target, &pin)?;
    }
    verify(&target, &pin)?;
    let root = target.join(format!("LanguageTool-{}", pin.version));
    Ok(Installation { root, pin })
}
