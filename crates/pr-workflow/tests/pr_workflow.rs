use anyhow::{Context, Result};
use sha2::{Digest, Sha256};
use std::fs;
use std::path::PathBuf;
use std::process::{Command, Output};
use std::sync::OnceLock;

#[path = "../../dotguard/tests/support/fixture_git.rs"]
mod fixture_git;

use fixture_git::fixture_process;

#[path = "../../dotfiles/tests/support/catalog.rs"]
mod catalog;

fn gh_directory() -> &'static tempfile::TempDir {
    static DIRECTORY: OnceLock<tempfile::TempDir> = OnceLock::new();
    DIRECTORY.get_or_init(|| {
        let directory = tempfile::tempdir().unwrap();
        for program in ["gh", "gh-stack", "git"] {
            let source = if program == "git" { "gh" } else { program };
            let result = platform::process::command("rustc")
                .args(["--edition", "2024"])
                .arg(format!(
                    "{}/tests/fixtures/{source}.rs",
                    env!("CARGO_MANIFEST_DIR")
                ))
                .arg("-o")
                .arg(
                    directory
                        .path()
                        .join(format!("{program}{}", std::env::consts::EXE_SUFFIX)),
                )
                .status()
                .unwrap();
            assert!(result.success());
        }
        directory
    })
}

fn real_git() -> Result<PathBuf> {
    let name = format!("git{}", std::env::consts::EXE_SUFFIX);
    std::env::split_paths(&std::env::var_os("PATH").context("fixture Git PATH")?)
        .map(|directory| directory.join(&name))
        .find(|path| path.is_file())
        .context("fixture real Git missing")
}

struct Fixture {
    directory: tempfile::TempDir,
    context_home: tempfile::TempDir,
    body: PathBuf,
    log: PathBuf,
    base: String,
    head: String,
    level: &'static str,
}

/// Run Git in a fixture checkout without the host's global hooks or identity.
fn git(directory: &std::path::Path, arguments: &[&str]) -> Result<String> {
    let output = fixture_git::command("git", &directory.join("gitconfig"))
        .current_dir(directory)
        .args(arguments)
        .output()?;
    anyhow::ensure!(
        output.status.success(),
        "git {arguments:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    Ok(String::from_utf8(output.stdout)?.trim().to_owned())
}

impl Fixture {
    /// A checkout whose head commit matches the PR head, with no CI matrix unless a test commits one.
    fn new(body: &str) -> Result<Self> {
        let directory = tempfile::tempdir()?;
        let context_home = tempfile::tempdir()?;
        let file = context_home.path().join("body.md");
        fs::write(&file, body)?;
        fs::write(directory.path().join("body.md"), body)?;
        let log = directory.path().join("gh.log");
        fs::write(
            directory.path().join("gitconfig"),
            "[user]\n\tname = Fixture\n\temail = fixture@example.com\n[commit]\n\tgpgsign = false\n",
        )?;
        git(
            directory.path(),
            &["init", "--quiet", "--initial-branch=feature"],
        )?;
        git(
            directory.path(),
            &["config", "--local", "user.name", "Fixture"],
        )?;
        git(
            directory.path(),
            &["config", "--local", "user.email", "fixture@example.com"],
        )?;
        fs::write(
            directory.path().join(".git/info/exclude"),
            "gitconfig\ngh.log\ngh-stack.log\n.config/\n.local/\nprose/\n",
        )?;
        git(
            directory.path(),
            &["commit", "--quiet", "--allow-empty", "-m", "base"],
        )?;
        let base_commit = git(directory.path(), &["rev-parse", "HEAD"])?;
        git(directory.path(), &["add", "body.md"])?;
        git(directory.path(), &["commit", "--quiet", "-m", "head"])?;
        git(
            directory.path(),
            &[
                "remote",
                "add",
                "origin",
                "https://github.com/owner/project.git",
            ],
        )?;
        let head = git(directory.path(), &["rev-parse", "HEAD"])?;
        // The verification gate of `ready` compares the head with its base branch and reads the installed shape catalog.
        for base in ["main", "parent", "bottom"] {
            git(
                directory.path(),
                &[
                    "update-ref",
                    &format!("refs/remotes/origin/{base}"),
                    &base_commit,
                ],
            )?;
        }
        catalog::install(directory.path())?;
        Ok(Self {
            directory,
            context_home,
            body: file,
            log,
            base: base_commit,
            head,
            level: "medium",
        })
    }

    fn command(&self, operation: &str) -> Result<Command> {
        self.command_for_repo(operation, "owner/project")
    }

    fn command_for_repo(&self, operation: &str, repo: &str) -> Result<Command> {
        let root = self.directory.path();
        if git(root, &["remote"])?
            .lines()
            .any(|remote| remote == "origin")
        {
            git(
                root,
                &[
                    "remote",
                    "set-url",
                    "origin",
                    &format!("https://github.com/{repo}.git"),
                ],
            )?;
        }
        self.command_at_repo(operation, repo, true)
    }

    fn command_at_repo(
        &self,
        operation: &str,
        repo: &str,
        legacy_assessment: bool,
    ) -> Result<Command> {
        let root = self.directory.path();
        let public = fs::read_to_string(self.signing_key()?.with_extension("pub"))?;
        // pr-workflow refuses a malformed repository before it reads an assessment, so none gets stored for one.
        let plain = repo.split_once('/').is_some_and(|(owner, name)| {
            !owner.is_empty()
                && !owner.starts_with('.')
                && !owner.contains(':')
                && !name.contains('/')
        });
        let assessment = if !legacy_assessment
            || !plain
            || operation == "start"
            || operation == "assess"
            || operation.starts_with("issue ")
        {
            None
        } else {
            Some(self.assessment(root, repo, &self.base, "HEAD")?.0)
        };
        if assessment.is_some() && self.body.exists() {
            fs::write(
                &self.body,
                self.assessed_body(&fs::read_to_string(&self.body)?),
            )?;
        }
        let live_body = fs::read_to_string(if self.body.exists() {
            self.body.clone()
        } else {
            root.join("body.md")
        })?;
        let mut paths = vec![gh_directory().path().to_path_buf()];
        paths.extend(std::env::split_paths(
            &std::env::var_os("PATH").context("PATH")?,
        ));
        let mut command =
            fixture_process::command(env!("CARGO_BIN_EXE_pr-workflow"), self.directory.path());
        command
            .current_dir(self.directory.path())
            .env("PATH", std::env::join_paths(paths)?)
            .env(
                "PROSE_CONFIG",
                concat!(env!("CARGO_MANIFEST_DIR"), "/../../home/dot_config/prose"),
            )
            .env("PROSE_RUNTIME", self.directory.path().join("prose"))
            .env("HOME", self.directory.path())
            .env("USERPROFILE", self.directory.path())
            .env("GH_FIXTURE_LOG", &self.log)
            .env("GH_FIXTURE_HEAD", &self.head)
            .env("PWF_FIXTURE_REAL_GIT", real_git()?)
            .env("GH_FIXTURE_BASE", &self.base)
            .env("GH_FIXTURE_VIEW", serde_json::json!({
                "title":"fix: preserve edits","body":live_body,
                "state":"OPEN","isDraft":true
            }).to_string())
            .env("GH_FIXTURE_LEVEL", self.level)
            .env("GH_FIXTURE_SIGNING_KEYS", serde_json::json!([{"id":17,"key":public.trim()}]).to_string())
            .env(
                "GH_STACK_FIXTURE_LOG",
                self.directory.path().join("gh-stack.log"),
            )
            .env_remove("DOTGUARD_STACK")
            .env("GH_FIXTURE_USER", r#"{"login":"P4suta","id":42543015}"#)
            .env("GH_FIXTURE_REPOSITORY", serde_json::json!({
                "id":37,"full_name":repo,"owner":{"login":repo.split('/').next().unwrap_or_default(),"id":1},"fork":false,"default_branch":"main"
            }).to_string())
            .args(operation.split_whitespace())
            .args(["--repo", repo]);
        if let Some(assessment) = assessment {
            command.env("GH_FIXTURE_ASSESSMENT", serde_json::to_string(&assessment)?);
        }
        if operation == "start" {
            command
                .env("HOME", self.context_home.path())
                .env("USERPROFILE", self.context_home.path());
        }
        Ok(command)
    }

    /// The body with the standing review exclusion of a noncritical route, which carries no assessment.
    fn assessed_body(&self, body: &str) -> String {
        let standing = if self.level != "critical"
            && !body
                .lines()
                .any(|line| line.trim() == "@coderabbitai ignore")
        {
            "\n@coderabbitai ignore\n"
        } else {
            "\n"
        };
        format!("{}\n{standing}", body.trim_end())
    }

    /// Signs the fixture assessment of `head` and stores it where pr-workflow reads it.
    /// Returns the signed object and the public signing key.
    fn assessment(
        &self,
        root: &std::path::Path,
        repo: &str,
        base: &str,
        head: &str,
    ) -> Result<(String, String)> {
        let key = self.signing_key()?;
        for (name, value) in [
            ("user.name", "Fixture"),
            ("user.email", "fixture@example.com"),
            ("gpg.format", "ssh"),
            ("gpg.ssh.program", "ssh-keygen"),
            (
                "user.signingkey",
                key.to_str().context("fixture signing key path")?,
            ),
        ] {
            git(root, &["config", "--local", name, value])?;
        }
        let head = git(root, &["rev-parse", head])?;
        let base = git(root, &["merge-base", base, &head])?;
        let tree = git(root, &["rev-parse", &format!("{head}^{{tree}}")])?;
        let changed = git(
            root,
            &["diff", "--name-only", "--no-renames", &base, &head, "--"],
        )?;
        let path = changed
            .lines()
            .next()
            .context("fixture assessment requires an actual changed path")?;
        let diff = fixture_git::command("git", &self.directory.path().join("gitconfig"))
            .current_dir(root)
            .args([
                "--no-pager",
                "--no-replace-objects",
                "diff",
                "--binary",
                "--full-index",
                "--no-ext-diff",
                "--no-textconv",
                "--no-renames",
                &base,
                &head,
                "--",
            ])
            .output()?;
        anyhow::ensure!(diff.status.success(), "fixture diff capture failed");
        let severe = (self.level == "critical").then(|| serde_json::json!({
            "ground":"authorization_boundary",
            "consequence":"A privileged operation could expose the complete fixture repository.", "consequence_kind":"catastrophic",
            "likelihood":"The changed interface is directly reachable by an untrusted caller.", "likelihood_kind":"exposed",
            "blast_radius":"Every fixture account would be affected by the privileged operation.", "blast_radius_kind":"systemic",
            "rollback":"An exposed credential cannot be recalled after publication.", "rollback_kind":"irreversible"
        }));
        let diff_sha256 = platform::build_inputs::hex(&Sha256::digest(&diff.stdout));
        let statement = serde_json::json!({
            "schema":1,"signer":"P4suta","binding":{"repository":repo,"base":base,"head":head,"diff_sha256":diff_sha256},
            "assessment":{"level":self.level,"model":"fixture model","rationale":"The actual committed fixture change has the selected consequence scope.","evidence":[{"path":path,"change":"The actual committed file changed at this exact head."}],"severe":severe,"finding_dispositions":[]},"history":null
        });
        let signed =
            platform::signed_git::create(root, &head, &tree, &serde_json::to_vec(&statement)?)?;
        for home in [self.directory.path(), self.context_home.path()] {
            pr_workflow::store_assessment(
                home,
                repo,
                &head,
                &diff_sha256,
                &signed.object,
                &signed.raw,
            )?;
        }
        Ok((
            signed.object,
            fs::read_to_string(key.with_extension("pub"))?,
        ))
    }

    fn signing_key(&self) -> Result<PathBuf> {
        let key = self.context_home.path().join("signing-key");
        if !key.exists() {
            let output = platform::tool::Tool::SshKeygen
                .command()
                .args(["-q", "-t", "ed25519", "-N", "", "-f"])
                .arg(&key)
                .output()?;
            anyhow::ensure!(
                output.status.success(),
                "fixture signing key: {}",
                String::from_utf8_lossy(&output.stderr)
            );
        }
        Ok(key)
    }

    fn assessment_records(
        &self,
        root: &std::path::Path,
        repo: &str,
        heads: &[(&str, &str)],
    ) -> Result<String> {
        let mut records = Vec::new();
        for (head, base) in heads {
            let head = git(root, &["rev-parse", head])?;
            let (packet, _) = self.assessment(root, repo, base, &head)?;
            records.push(format!("{head}={}", serde_json::to_string(&packet)?));
        }
        Ok(records.join("\n"))
    }

    fn sibling_head(&self, label: &str) -> Result<String> {
        let root = self.directory.path();
        let tree = git(root, &["rev-parse", "HEAD^{tree}"])?;
        let base = git(root, &["rev-parse", "origin/main"])?;
        git(root, &["commit-tree", &tree, "-p", &base, "-m", label])
    }

    fn personal_command(&self, operation: &str) -> Result<Command> {
        let mut command = self.command_for_repo(operation, "P4suta/project")?;
        command
            .env("GH_FIXTURE_REPOSITORY", serde_json::json!({
                "id":37,"full_name":"P4suta/project","owner":{"login":"P4suta","id":42_543_015},"fork":false,"default_branch":"main"
            }).to_string())
            .env("GH_FIXTURE_ISSUE", serde_json::json!({
                "number":23,"title":"Preserve edits","body":"The original input must survive edits.",
                "state":"open","html_url":"https://github.com/P4suta/project/issues/23"
            }).to_string());
        if operation == "start" {
            self.prepare_context_signer(&mut command)?;
            command.args(["--path", "body.md"]);
        }
        Ok(command)
    }

    fn prepare_context_signer(&self, command: &mut Command) -> Result<()> {
        let key = self.signing_key()?;
        let public = fs::read_to_string(key.with_extension("pub"))?;
        git(
            self.directory.path(),
            &["config", "--local", "gpg.format", "ssh"],
        )?;
        git(
            self.directory.path(),
            &[
                "config",
                "--local",
                "user.signingkey",
                key.to_str().context("fixture signing key path")?,
            ],
        )?;
        git(
            self.directory.path(),
            &[
                "remote",
                "set-url",
                "origin",
                "https://github.com/P4suta/project.git",
            ],
        )?;
        git(
            self.directory.path(),
            &[
                "config",
                "--local",
                "url.https://github.com/P4suta/project.git.insteadOf",
                "https://github.com/P4suta/project.git",
            ],
        )?;
        assert!(
            platform::issue_context::git(
                self.directory.path(),
                &["remote", "get-url", "origin"],
                None
            )?
            .trim()
                == "https://github.com/P4suta/project.git",
            "parent fixture origin must match the isolated child without ambient rewrites"
        );
        command
            .env("HOME", self.context_home.path())
            .env("USERPROFILE", self.context_home.path())
            .env(
                "GH_FIXTURE_SIGNING_KEYS",
                serde_json::json!([{"id":17,"key":public.trim()}]).to_string(),
            );
        Ok(())
    }

    fn document(&self, operation: &str, title: &str) -> Result<Output> {
        Ok(self
            .command(operation)?
            .args(["--title", title, "--body-file"])
            .arg(&self.body)
            .args(if operation == "create" {
                vec!["--head", "feature"]
            } else if operation == "edit" {
                vec!["--pr", "17"]
            } else {
                vec![]
            })
            .output()?)
    }

    fn pause(&self, marker: &str) -> Result<()> {
        let state = self.directory.path().join(".local/state/coderabbit-guard");
        fs::create_dir_all(&state)?;
        fs::write(state.join(marker), "owner pause")?;
        Ok(())
    }

    fn log(&self) -> Result<String> {
        Ok(fs::read_to_string(&self.log)?)
    }
}

const FORK_SOURCE: &str = "contributor/project";
const FORK_TARGET: &str = "P4suta/project";

struct ForkFixture {
    fixture: Fixture,
}

impl ForkFixture {
    fn new() -> Result<Self> {
        let fixture = Fixture::new(BODY)?;
        let root = fixture.directory.path();
        let key = fixture.signing_key()?;
        for (name, value) in [
            ("gpg.format", "ssh"),
            ("gpg.ssh.program", "ssh-keygen"),
            (
                "user.signingkey",
                key.to_str().context("fork signing key path")?,
            ),
        ] {
            git(root, &["config", "--local", name, value])?;
        }
        let origin = format!("https://github.com/{FORK_SOURCE}.git");
        git(root, &["remote", "set-url", "origin", &origin])?;
        git(
            root,
            &[
                "config",
                "--local",
                &format!("url.{origin}.insteadOf"),
                &origin,
            ],
        )?;
        Ok(Self { fixture })
    }

    fn repositories(source: &serde_json::Value) -> String {
        let target = serde_json::json!({
            "id":37,"full_name":FORK_TARGET,"owner":{"login":"P4suta","id":42_543_015},
            "fork":false,"default_branch":"main"
        });
        format!("repos/{FORK_TARGET}={target}\nrepos/{FORK_SOURCE}={source}")
    }

    fn source() -> serde_json::Value {
        serde_json::json!({
            "id":38,"full_name":FORK_SOURCE,"owner":{"login":"contributor","id":19},
            "fork":true,"source":{"id":37,"full_name":FORK_TARGET},"default_branch":"main"
        })
    }

    fn branches(&self, head: &str) -> String {
        format!(
            "repos/{FORK_TARGET}/branches/main={}\nrepos/{FORK_SOURCE}/branches/feature={}",
            serde_json::json!({"name":"main","commit":{"sha":self.fixture.base}}),
            serde_json::json!({"name":"feature","commit":{"sha":head}})
        )
    }

    fn command(&self, operation: &str) -> Result<Command> {
        let mut command = self
            .fixture
            .command_at_repo(operation, FORK_TARGET, false)?;
        let body = fs::read_to_string(&self.fixture.body)?;
        command
            .env("HOME", self.fixture.context_home.path())
            .env("USERPROFILE", self.fixture.context_home.path())
            .env(
                "GH_FIXTURE_REPOSITORIES",
                Self::repositories(&Self::source()),
            )
            .env("GH_FIXTURE_BRANCHES", self.branches(&self.fixture.head))
            .env(
                "GH_FIXTURE_ISSUE",
                serde_json::json!({
                    "number":23,"title":"Preserve edits","body":"Preserve the original input.",
                    "state":"open","html_url":format!("https://github.com/{FORK_TARGET}/issues/23")
                })
                .to_string(),
            )
            .env(
                "GH_FIXTURE_VIEW",
                serde_json::json!({
                    "title":"fix: preserve edits","body":body,"state":"OPEN","isDraft":true,
                    "headRefName":"feature","headRepository":{"name":"project"},
                    "headRepositoryOwner":{"login":"contributor"},"isCrossRepository":true
                })
                .to_string(),
            );
        Ok(command)
    }

    fn assess(&self) -> Result<()> {
        let input = self.fixture.context_home.path().join("assessment.json");
        fs::write(
            &input,
            serde_json::to_vec(&serde_json::json!({
                "level":"medium","model":"fixture model",
                "rationale":"The committed input changes within the reversible fixture scope.",
                "evidence":[{"path":"body.md","change":"The original committed input changed."}],
                "severe":null,"finding_dispositions":[]
            }))?,
        )?;
        let result = self
            .command("assess")?
            .args(["--base", &self.fixture.base, "--assessment-file"])
            .arg(&input)
            .output()?;
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        assert!(
            String::from_utf8_lossy(&result.stdout).contains("saved in owner-local state"),
            "{}",
            String::from_utf8_lossy(&result.stdout)
        );
        fs::write(&self.fixture.body, self.fixture.assessed_body(BODY))?;
        Ok(())
    }

    fn create(&self, head: &str) -> Result<Command> {
        let mut command = self.command("create")?;
        command
            .args(["--title", "fix: preserve edits", "--body-file"])
            .arg(&self.fixture.body)
            .args(["--head", head, "--base", "main", "--issue", "23"]);
        Ok(command)
    }

    fn clone_into(&self, parent: &std::path::Path) -> Result<PathBuf> {
        let root = parent.join("checkout");
        git(
            self.fixture.directory.path(),
            &[
                "clone",
                "--quiet",
                "--no-local",
                self.fixture
                    .directory
                    .path()
                    .to_str()
                    .context("fork checkout path")?,
                root.to_str().context("fork clone path")?,
            ],
        )?;
        git(
            &root,
            &[
                "remote",
                "set-url",
                "origin",
                &format!("https://github.com/{FORK_SOURCE}.git"),
            ],
        )?;
        Ok(root)
    }
}

#[test]
fn fork_sources_publish_assessed_heads_without_rewriting_origin() -> Result<()> {
    let fixture = ForkFixture::new()?;
    let root = fixture.fixture.directory.path();
    let origin = git(root, &["remote", "get-url", "origin"])?;
    fixture.assess()?;
    fs::write(
        &fixture.fixture.body,
        format!(
            "{}\nCloses #23.\n",
            fs::read_to_string(&fixture.fixture.body)?
        ),
    )?;
    for head in ["feature", "contributor:feature"] {
        let output = fixture.create(head)?.output()?;
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(git(root, &["remote", "get-url", "origin"])?, origin);
    }
    let live = fixture
        .command("check")?
        .args(["--pr", "17", "--issue", "23"])
        .output()?;
    assert!(
        live.status.success(),
        "{}",
        String::from_utf8_lossy(&live.stderr)
    );
    let log = fixture.fixture.log()?;
    assert_eq!(log.matches("\ncreate\n").count(), 2);
    assert_eq!(log.matches("\ncontributor:feature\n").count(), 2);
    assert!(log.contains(&format!("repos/{FORK_TARGET}")));
    assert!(log.contains(&format!("repos/{FORK_SOURCE}")));
    Ok(())
}

#[test]
fn fork_publication_refuses_mismatched_owner_head_and_unknown_source_metadata() -> Result<()> {
    let fixture = ForkFixture::new()?;
    fixture.assess()?;
    fs::write(
        &fixture.fixture.body,
        format!(
            "{}\nCloses #23.\n",
            fs::read_to_string(&fixture.fixture.body)?
        ),
    )?;
    let wrong_owner = fixture.create("other:feature")?.output()?;
    assert!(!wrong_owner.status.success());
    assert!(
        String::from_utf8_lossy(&wrong_owner.stderr).contains("qualified PR head owner differs")
    );
    let wrong_head = fixture
        .create("contributor:feature")?
        .env(
            "GH_FIXTURE_BRANCHES",
            fixture.branches(&fixture.fixture.base),
        )
        .output()?;
    assert!(!wrong_head.status.success());
    assert!(
        String::from_utf8_lossy(&wrong_head.stderr).contains("actual fork head branch differs")
    );
    let mut unknown = ForkFixture::source();
    unknown
        .as_object_mut()
        .context("fork source object")?
        .remove("source");
    let missing_network = fixture
        .create("contributor:feature")?
        .env(
            "GH_FIXTURE_REPOSITORIES",
            ForkFixture::repositories(&unknown),
        )
        .output()?;
    assert!(!missing_network.status.success());
    assert!(
        String::from_utf8_lossy(&missing_network.stderr)
            .contains("fork network metadata is missing")
    );
    for source in [
        serde_json::Value::Null,
        serde_json::json!({
            "id":38,"full_name":FORK_SOURCE,"owner":{"login":"other","id":19},
            "fork":true,"source":{"id":37,"full_name":FORK_TARGET}
        }),
    ] {
        let output = fixture
            .create("contributor:feature")?
            .env(
                "GH_FIXTURE_REPOSITORIES",
                ForkFixture::repositories(&source),
            )
            .output()?;
        assert!(!output.status.success());
        refusal(&output);
    }
    fixture.fixture.assessment(
        fixture.fixture.directory.path(),
        FORK_TARGET,
        &fixture.fixture.base,
        "HEAD",
    )?;
    fs::write(
        &fixture.fixture.body,
        fixture
            .fixture
            .assessed_body(&format!("{BODY}\nCloses #23.\n")),
    )?;
    let legacy = fixture.create("contributor:feature")?.output()?;
    assert!(!legacy.status.success());
    assert!(
        String::from_utf8_lossy(&legacy.stderr)
            .contains("assessment source and destination identities are stale or mismatched")
    );
    assert!(!fixture.fixture.log()?.contains("\ncreate\n"));
    Ok(())
}

#[test]
fn fork_live_documents_refuse_mismatched_source_metadata() -> Result<()> {
    let fixture = ForkFixture::new()?;
    fixture.assess()?;
    for fields in [
        serde_json::json!({"headRepositoryOwner":{"login":"other"}}),
        serde_json::json!({"headRepository":{"name":"other"}}),
        serde_json::json!({"headRefName":"missing"}),
        serde_json::json!({"isCrossRepository":false}),
        serde_json::json!({"headRepository":null}),
    ] {
        let mut view = serde_json::json!({
            "title":"fix: preserve edits","body":fs::read_to_string(&fixture.fixture.body)?,
            "state":"OPEN","isDraft":true,"headRefName":"feature", "headRepository":{"name":"project"},
            "headRepositoryOwner":{"login":"contributor"},"isCrossRepository":true
        });
        view.as_object_mut()
            .context("fork live view")?
            .extend(fields.as_object().context("fork live control")?.clone());
        let output = fixture
            .command("check")?
            .args(["--pr", "17"])
            .env("GH_FIXTURE_VIEW", view.to_string())
            .output()?;
        assert!(!output.status.success());
        refusal(&output);
    }
    assert!(!fixture.fixture.log()?.contains("\ncreate\n"));
    Ok(())
}

#[test]
fn copied_git_commits_do_not_authorize_an_unrelated_repository_network() -> Result<()> {
    let fixture = ForkFixture::new()?;
    fixture.assess()?;
    let parent = tempfile::tempdir()?;
    let copied = fixture.clone_into(parent.path())?;
    assert_eq!(git(&copied, &["rev-parse", "HEAD"])?, fixture.fixture.head);
    let unrelated = "other/project";
    git(
        &copied,
        &[
            "remote",
            "set-url",
            "origin",
            &format!("https://github.com/{unrelated}.git"),
        ],
    )?;
    let repositories = ForkFixture::repositories(&ForkFixture::source());
    let repositories = format!(
        "{repositories}\nrepos/{unrelated}={}",
        serde_json::json!({
            "id":99,"full_name":unrelated,"owner":{"login":"other","id":21},"fork":false
        })
    );
    let output = fixture
        .create("other:feature")?
        .current_dir(&copied)
        .env("GH_FIXTURE_REPOSITORIES", repositories)
        .output()?;
    assert!(!output.status.success());
    assert!(
        String::from_utf8_lossy(&output.stderr)
            .contains("do not share a verified repository network")
    );
    assert_eq!(git(&copied, &["rev-parse", "HEAD"])?, fixture.fixture.head);
    assert!(!fixture.fixture.log()?.contains("\ncreate\n"));
    Ok(())
}

#[test]
fn fork_sources_keep_personal_target_issue_authority() -> Result<()> {
    let fixture = ForkFixture::new()?;
    let missing = fixture
        .command("start")?
        .args(["--path", "body.md"])
        .output()?;
    assert!(!missing.status.success());
    assert_eq!(rule(&missing), "pr.issue");
    let accepted = fixture
        .command("start")?
        .args(["--issue", "23", "--path", "body.md"])
        .output()?;
    assert!(
        accepted.status.success(),
        "{}",
        String::from_utf8_lossy(&accepted.stderr)
    );
    let manifest = platform::issue_context::local_for(
        fixture.fixture.directory.path(),
        fixture.fixture.context_home.path(),
    )?;
    assert_eq!(manifest.receipts.len(), 1);
    assert!(manifest.receipts[0].context.destination.required);
    assert_eq!(
        manifest.receipts[0].context.destination.repository,
        FORK_TARGET
    );
    let closed = fixture.command("start")?.args(["--issue", "23", "--path", "body.md"])
        .env("GH_FIXTURE_ISSUE", serde_json::json!({
            "number":23,"title":"Preserve edits","body":"Preserve the original input.","state":"closed",
            "html_url":format!("https://github.com/{FORK_TARGET}/issues/23")
        }).to_string()).output()?;
    assert!(!closed.status.success());
    assert_eq!(rule(&closed), "pr.issue");
    assert_eq!(
        git(
            fixture.fixture.directory.path(),
            &["remote", "get-url", "origin"]
        )?,
        format!("https://github.com/{FORK_SOURCE}.git")
    );
    assert!(!fixture.fixture.log()?.contains("\ncreate\n"));
    Ok(())
}

#[test]
fn issue_contexts_stay_in_owner_local_state_and_never_reach_a_clone() -> Result<()> {
    let fixture = Fixture::new(BODY)?;
    let root = platform::issue_context::checkout(fixture.directory.path())?;
    let mut start = fixture.personal_command("start")?;
    start.args(["--issue", "23"]);
    let variables: Vec<_> = start
        .get_envs()
        .filter_map(|(key, value)| value.map(|value| (key.to_owned(), value.to_owned())))
        .collect();
    let started = start.output()?;
    assert!(
        started.status.success(),
        "{}",
        String::from_utf8_lossy(&started.stderr)
    );
    assert!(!root.join(".pr-workflow").exists());
    fs::write(root.join("body.md"), "Prior checked implementation.\n")?;
    git(&root, &["add", "body.md"])?;
    git(
        &root,
        &["commit", "--quiet", "-m", "checked implementation"],
    )?;
    let parent = tempfile::tempdir()?;
    let fresh = parent.path().join("checkout");
    git(
        &root,
        &[
            "clone",
            "--quiet",
            "--no-local",
            root.to_str().context("fixture checkout path")?,
            fresh.to_str().context("fresh clone path")?,
        ],
    )?;
    let origin = "https://github.com/P4suta/project.git";
    git(&fresh, &["remote", "set-url", "origin", origin])?;
    git(
        &fresh,
        &[
            "config",
            "--local",
            &format!("url.{origin}.insteadOf"),
            origin,
        ],
    )?;
    let home = tempfile::tempdir()?;
    assert!(platform::issue_context::local_for(&fresh, home.path()).is_err());
    fs::write(fresh.join("body.md"), "Fresh change.\n")?;
    let check = fixture_process::command(env!("CARGO_BIN_EXE_pr-workflow"), &fresh)
        .current_dir(&fresh)
        .envs(variables.iter().cloned())
        .env("HOME", home.path())
        .env("USERPROFILE", home.path())
        .arg("change-check")
        .output()?;
    assert!(!check.status.success());
    assert_eq!(rule(&check), "pr.issue-context");
    assert!(String::from_utf8_lossy(&check.stderr).contains("checked issue context missing"));
    Ok(())
}

/// The refusal record a failed run ends with, checked as plain JSON so the format stands on its own.
fn refusal(output: &Output) -> serde_json::Value {
    let stderr = String::from_utf8_lossy(&output.stderr);
    let line = stderr.lines().last().unwrap_or_default();
    let json = line
        .strip_prefix("dotfiles-refusal/2 ")
        .unwrap_or_else(|| panic!("no refusal record: {stderr}"));
    let record: serde_json::Value = serde_json::from_str(json).unwrap();
    let object = record.as_object().unwrap();
    assert_eq!(
        object.keys().map(String::as_str).collect::<Vec<_>>(),
        ["bypass", "cause", "evidence", "next", "rule"],
        "{line}"
    );
    for key in ["rule", "cause", "next"] {
        assert!(
            record[key]
                .as_str()
                .is_some_and(|text| !text.trim().is_empty()),
            "{key}: {line}"
        );
    }
    assert!(!record["next"].as_str().unwrap().contains('\n'), "{line}");
    let evidence = record["evidence"].as_array().unwrap();
    assert!(
        !evidence.is_empty()
            && evidence
                .iter()
                .all(|item| item.as_str().is_some_and(|text| !text.trim().is_empty())),
        "{line}"
    );
    assert!(
        record["bypass"].is_null(),
        "pr-workflow has no bypass: {line}"
    );
    assert!(
        platform::refusal::Refusal::find(&stderr).is_some_and(|parsed| parsed.is_complete()),
        "{line}"
    );
    record
}

#[test]
fn every_refusal_ends_with_one_complete_record_naming_its_rule() -> Result<()> {
    let closed = format!("{BODY}\nCloses #23.\n");

    let fixture = Fixture::new(BODY)?;
    assert_eq!(
        rule(&fixture.document("check", "Repair edits")?),
        "pr.title"
    );
    let fixture = Fixture::new(" \n")?;
    let output = fixture.document("check", "fix: preserve edits")?;
    let record = refusal(&output);
    assert_eq!(record["rule"], "pr.body");
    assert!(
        record["next"]
            .as_str()
            .unwrap()
            .starts_with("pr-workflow check --repo owner/project --title 'fix: preserve edits'"),
        "{record}"
    );

    let fixture = Fixture::new(&closed)?;
    assert_eq!(
        rule(&fixture.personal_command("start")?.output()?),
        "pr.issue"
    );
    let fixture = Fixture::new(BODY)?;
    let output = fixture
        .personal_command("create")?
        .args([
            "--issue",
            "23",
            "--title",
            "fix: preserve edits",
            "--body-file",
        ])
        .arg(&fixture.body)
        .args(["--head", "feature"])
        .output()?;
    assert_eq!(rule(&output), "pr.body");

    let fixture = Fixture::new(&closed)?;
    let output = fixture
        .personal_command("start")?
        .args(["--issue", "23"])
        .env(
            "GH_FIXTURE_HEADERS",
            "HTTP/2.0 200 OK\nX-Ratelimit-Remaining: 9\nX-Ratelimit-Reset: 1800000000\nX-Ratelimit-Resource: core",
        )
        .output()?;
    assert_eq!(rule(&output), "pr.quota");
    let output = fixture
        .personal_command("start")?
        .args(["--issue", "23"])
        .env("GH_FIXTURE_FAILURE", "user")
        .env_remove("GH_TOKEN")
        .env_remove("GITHUB_TOKEN")
        .env_remove("GH_ENTERPRISE_TOKEN")
        .env_remove("GITHUB_ENTERPRISE_TOKEN")
        .env_remove("DOPPLER_TOKEN")
        .output()?;
    assert_eq!(rule(&output), "pr.auth");
    Ok(())
}

fn rule(output: &Output) -> String {
    refusal(output)["rule"].as_str().unwrap().to_owned()
}

fn local_command(
    root: &std::path::Path,
    home: &std::path::Path,
    arguments: &[&str],
) -> Result<Command> {
    let (action, arguments) = arguments
        .split_first()
        .context("local fixture command missing")?;
    let mut paths = vec![gh_directory().path().to_path_buf()];
    paths.extend(std::env::split_paths(
        &std::env::var_os("PATH").context("PATH")?,
    ));
    let mut command = fixture_process::command(env!("CARGO_BIN_EXE_pr-workflow"), root);
    command
        .current_dir(root)
        .env("PATH", std::env::join_paths(paths)?)
        .env("HOME", home)
        .env("USERPROFILE", home)
        .env("PWF_FIXTURE_REAL_GIT", real_git()?)
        .env("GH_FIXTURE_LOG", home.join("offline-gh.log"))
        .env("GH_FIXTURE_FAILURE", "user")
        .env(
            "PROSE_CONFIG",
            concat!(env!("CARGO_MANIFEST_DIR"), "/../../home/dot_config/prose"),
        )
        .env("PROSE_RUNTIME", home.join("prose"))
        .args(["local", *action])
        .arg("--root")
        .arg(root)
        .args(arguments);
    Ok(command)
}

fn offline_context_check(root: &std::path::Path, home: &std::path::Path) -> Result<Output> {
    let configured = local_command(root, home, &["check"])?;
    let variables: Vec<_> = configured
        .get_envs()
        .filter_map(|(key, value)| value.map(|value| (key.to_owned(), value.to_owned())))
        .collect();
    Ok(
        fixture_process::command(env!("CARGO_BIN_EXE_pr-workflow"), root)
            .current_dir(root)
            .envs(variables)
            .arg("change-check")
            .output()?,
    )
}

fn commit_first_local_source(root: &std::path::Path) -> Result<()> {
    fs::write(
        root.join("gitconfig"),
        "[user]\n\tname = Fixture\n\temail = fixture@example.com\n[commit]\n\tgpgsign = false\n",
    )?;
    fs::write(root.join(".git/info/exclude"), "gitconfig\n")?;
    git(root, &["config", "--local", "user.name", "Fixture"])?;
    git(
        root,
        &["config", "--local", "user.email", "fixture@example.com"],
    )?;
    git(root, &["add", "main.rs"])?;
    git(root, &["commit", "--quiet", "-m", "first real source"])?;
    assert!(matches!(
        platform::local_issues::source(root)?,
        platform::local_issues::Source::Commit { .. }
    ));
    Ok(())
}

#[test]
fn a_new_local_project_prepares_unborn_scoped_work_without_github() -> Result<()> {
    let project = tempfile::tempdir()?;
    let home = tempfile::tempdir()?;
    let root = project.path();
    let init = local_command(root, home.path(), &["init"])?.output()?;
    assert!(
        init.status.success(),
        "{}",
        String::from_utf8_lossy(&init.stderr)
    );
    assert_eq!(
        platform::local_issues::source(root)?,
        platform::local_issues::Source::Unborn
    );
    assert!(
        platform::issue_context::guard_for(root, &[PathBuf::from("main.rs")], home.path()).is_err()
    );
    let body = home.path().join("local-issue.md");
    fs::write(
        &body,
        "Preserve the original input during local development.\n",
    )?;
    let created = local_command(
        root,
        home.path(),
        &["create", "--title", "Preserve local inputs", "--body-file"],
    )?
    .arg(&body)
    .output()?;
    assert!(
        created.status.success(),
        "{}",
        String::from_utf8_lossy(&created.stderr)
    );
    let selected = local_command(
        root,
        home.path(),
        &["select", "--issue", "1", "--path", "main.rs"],
    )?
    .output()?;
    assert!(
        selected.status.success(),
        "{}",
        String::from_utf8_lossy(&selected.stderr)
    );
    platform::issue_context::guard_for(root, &[PathBuf::from("main.rs")], home.path())?;
    fs::write(root.join("main.rs"), "fn main() {}\n")?;
    let checked = local_command(root, home.path(), &["check"])?.output()?;
    assert!(
        checked.status.success(),
        "{}",
        String::from_utf8_lossy(&checked.stderr)
    );
    let authoritative = offline_context_check(root, home.path())?;
    assert!(
        authoritative.status.success(),
        "{}",
        String::from_utf8_lossy(&authoritative.stderr)
    );
    assert!(
        platform::issue_context::guard_for(root, &[PathBuf::from("outside.rs")], home.path())
            .is_err()
    );
    let published = local_command(root, home.path(), &["status"])?.output()?;
    assert!(published.status.success());
    assert!(String::from_utf8_lossy(&published.stdout).contains("Preserve local inputs"));
    commit_first_local_source(root)?;
    assert!(
        platform::issue_context::guard_for(root, &[PathBuf::from("outside.rs")], home.path())
            .is_err()
    );
    let first_head = git(root, &["rev-parse", "HEAD"])?;
    let unrelated = git(
        root,
        &[
            "commit-tree",
            "HEAD^{tree}",
            "-m",
            "unrelated fixture source",
        ],
    )?;
    git(root, &["update-ref", "refs/heads/main", &unrelated])?;
    assert!(
        platform::issue_context::guard_for(root, &[PathBuf::from("main.rs")], home.path()).is_err()
    );
    git(root, &["update-ref", "refs/heads/main", &first_head])?;
    platform::issue_context::guard_for(root, &[PathBuf::from("main.rs")], home.path())?;
    assert!(!home.path().join("offline-gh.log").exists());
    assert!(!root.join(".pr-workflow").exists());
    Ok(())
}

#[test]
fn local_issues_keep_separate_scopes_and_refuse_closed_stale_or_late_work() -> Result<()> {
    let fixture = Fixture::new("Preserve local inputs.\n")?;
    let root = fixture.directory.path();
    let home = fixture.context_home.path();
    git(root, &["remote", "remove", "origin"])?;
    assert!(
        local_command(root, home, &["init"])?
            .output()?
            .status
            .success()
    );
    for title in ["Keep first input", "Keep second input"] {
        let created = local_command(root, home, &["create", "--title", title, "--body-file"])?
            .arg(&fixture.body)
            .output()?;
        assert!(
            created.status.success(),
            "{}",
            String::from_utf8_lossy(&created.stderr)
        );
    }
    for (issue, path) in [("1", "first.rs"), ("2", "second.rs")] {
        assert!(
            local_command(root, home, &["select", "--issue", issue, "--path", path])?
                .output()?
                .status
                .success()
        );
    }
    platform::issue_context::guard_for(
        root,
        &[PathBuf::from("first.rs"), PathBuf::from("second.rs")],
        home,
    )?;
    let closed =
        local_command(root, home, &["status", "--issue", "1", "--state", "closed"])?.output()?;
    assert!(closed.status.success());
    assert!(platform::issue_context::guard_for(root, &[PathBuf::from("first.rs")], home).is_err());
    platform::issue_context::guard_for(root, &[PathBuf::from("second.rs")], home)?;
    fs::create_dir_all(root.join(".pr-workflow"))?;
    fs::write(root.join(LEGACY_TRANSPORT), r#"{"schema":1,"receipts":[]}"#)?;
    assert!(platform::issue_context::guard_for(root, &[PathBuf::from("forged.rs")], home).is_err());
    fs::remove_dir_all(root.join(".pr-workflow"))?;
    fs::write(root.join("late.rs"), "fn late() {}\n")?;
    let late =
        local_command(root, home, &["select", "--issue", "2", "--path", "late.rs"])?.output()?;
    assert!(!late.status.success());
    assert!(String::from_utf8_lossy(&late.stderr).contains("late local selection"));
    fs::remove_file(root.join("late.rs"))?;
    let branch = git(root, &["symbolic-ref", "--short", "HEAD"])?;
    let unrelated = git(
        root,
        &[
            "commit-tree",
            "HEAD^{tree}",
            "-m",
            "unrelated fixture source",
        ],
    )?;
    git(
        root,
        &["update-ref", &format!("refs/heads/{branch}"), &unrelated],
    )?;
    assert!(platform::issue_context::guard_for(root, &[PathBuf::from("second.rs")], home).is_err());
    git(
        root,
        &["update-ref", &format!("refs/heads/{branch}"), &fixture.head],
    )?;
    let state = fs::read_dir(home.join(".local/state/pr-workflow/local-issues"))?
        .map(|entry| entry.map(|entry| entry.path()))
        .collect::<std::io::Result<Vec<_>>>()?
        .into_iter()
        .find(|path| {
            path.extension()
                .is_some_and(|extension| extension == "json")
        })
        .context("owned local issue state missing")?;
    let original = fs::read(&state)?;
    let mut tampered: serde_json::Value = serde_json::from_slice(&original)?;
    tampered["issues"]["1"]["status"] = serde_json::json!("open");
    fs::write(&state, serde_json::to_vec(&tampered)?)?;
    assert!(platform::issue_context::guard_for(root, &[PathBuf::from("first.rs")], home).is_err());
    fs::write(state, original)?;
    git(root, &["branch", "other"])?;
    git(root, &["symbolic-ref", "HEAD", "refs/heads/other"])?;
    assert!(platform::issue_context::guard_for(root, &[PathBuf::from("second.rs")], home).is_err());
    assert!(!home.join("offline-gh.log").exists());
    Ok(())
}

#[test]
fn remote_promotion_requires_checked_start_and_preserves_actual_local_history() -> Result<()> {
    let fixture = Fixture::new("Preserve local inputs.\n")?;
    let root = fixture.directory.path();
    let home = fixture.context_home.path();
    git(root, &["remote", "remove", "origin"])?;
    assert!(
        local_command(root, home, &["init"])?
            .output()?
            .status
            .success()
    );
    let created = local_command(
        root,
        home,
        &["create", "--title", "Keep local inputs", "--body-file"],
    )?
    .arg(&fixture.body)
    .output()?;
    assert!(
        created.status.success(),
        "{}",
        String::from_utf8_lossy(&created.stderr)
    );
    assert!(
        local_command(root, home, &["select", "--issue", "1", "--path", "body.md"])?
            .output()?
            .status
            .success()
    );
    fs::write(&fixture.body, format!("{BODY}\nCloses #23.\n"))?;
    let publication = fixture
        .personal_command("create")?
        .args([
            "--issue",
            "23",
            "--title",
            "fix: preserve inputs",
            "--head",
            "feature",
            "--body-file",
        ])
        .arg(&fixture.body)
        .env("GH_FIXTURE_FAILURE", "user")
        .output()?;
    assert!(!publication.status.success());
    assert_eq!(refusal(&publication)["rule"], "pr.auth");
    assert!(!fixture.log()?.contains("\ncreate\n"));
    fs::write(
        &fixture.body,
        "Changed previously registered local input.\n",
    )?;
    fs::write(
        root.join("body.md"),
        "Changed previously registered local input.\n",
    )?;
    git(
        root,
        &[
            "remote",
            "add",
            "origin",
            "https://github.com/P4suta/project.git",
        ],
    )?;
    let mut start = fixture.personal_command("start")?;
    start.args(["--issue", "23"]);
    assert!(platform::issue_context::guard_for(root, &[PathBuf::from("body.md")], home).is_err());
    let late_remote = start.output()?;
    assert!(!late_remote.status.success());
    assert!(
        String::from_utf8_lossy(&late_remote.stderr).contains("lacks an earlier checked context")
    );
    let promoted = start.arg("--promote-local").output()?;
    assert!(
        promoted.status.success(),
        "{}",
        String::from_utf8_lossy(&promoted.stderr)
    );
    assert!(
        String::from_utf8_lossy(&promoted.stdout)
            .contains("remote authorization begins at this checked start")
    );
    platform::issue_context::guard_for(root, &[PathBuf::from("body.md")], home)?;
    let manifest = platform::issue_context::local_for(root, home)?;
    assert_eq!(manifest.receipts.len(), 1);
    assert_eq!(manifest.receipts[0].context.issue, Some(23));
    assert!(!manifest.receipts[0].context.adopted);
    assert!(home.join(".local/state/pr-workflow/local-issues").exists());
    let local_check = local_command(root, home, &["check"])?.output()?;
    assert!(!local_check.status.success());
    Ok(())
}

#[test]
fn readying_refuses_a_closed_published_or_failing_pr() -> Result<()> {
    for (state, draft) in [("CLOSED", true), ("OPEN", false)] {
        let fixture = Fixture::new(BODY)?;
        let view = serde_json::json!({"title":"fix: preserve edits","body":BODY,"state":state,"isDraft":draft});
        let output = fixture
            .command("ready")?
            .args(["--pr", "17"])
            .env("GH_FIXTURE_VIEW", view.to_string())
            .output()?;
        assert_eq!(rule(&output), "pr.state");
    }
    let fixture = Fixture::new(BODY)?;
    let view = serde_json::json!({"title":"fix: preserve edits","body":BODY,"state":"OPEN","isDraft":true,
        "statusCheckRollup":[{"status":"COMPLETED","conclusion":"FAILURE"}]});
    let output = fixture
        .command("ready")?
        .args(["--pr", "17"])
        .env("GH_FIXTURE_VIEW", view.to_string())
        .output()?;
    let record = refusal(&output);
    assert_eq!(record["rule"], "pr.checks");
    assert_eq!(record["next"], "gh pr checks 17 --repo owner/project");
    Ok(())
}

#[test]
fn an_unusable_repository_policy_or_identity_is_refused_by_its_rule() -> Result<()> {
    let fixture = Fixture::new(BODY)?;
    let output = fixture.command_for_repo("start", "owner")?.output()?;
    assert_eq!(rule(&output), "pr.repository");
    let output = fixture
        .command("start")?
        .args(["--title-policy", "missing.json"])
        .output()?;
    assert_eq!(rule(&output), "pr.title-policy");
    let output = fixture
        .command("start")?
        .env("GH_FIXTURE_USER", r#"{"login":"P4suta","id":0}"#)
        .output()?;
    assert_eq!(rule(&output), "pr.auth");
    let output = fixture
        .personal_command("start")?
        .env(
            "GH_FIXTURE_REPOSITORY",
            serde_json::json!({
                "full_name":"P4suta/project","owner":{"login":"P4suta","id":1},"fork":false
            })
            .to_string(),
        )
        .output()?;
    assert_eq!(rule(&output), "pr.policy");
    Ok(())
}

#[test]
fn creation_refuses_an_invalid_branch_and_reports_a_failed_request() -> Result<()> {
    let fixture = Fixture::new(BODY)?;
    let output = fixture
        .command("create")?
        .args(["--title", "fix: preserve edits", "--body-file"])
        .arg(&fixture.body)
        .args(["--head", "a b"])
        .output()?;
    assert_eq!(rule(&output), "pr.branch");
    let output = fixture
        .command("create")?
        .args(["--title", "fix: preserve edits", "--body-file"])
        .arg(&fixture.body)
        .args(["--head", "feature"])
        .env("GH_FIXTURE_FAILURE", "create")
        .output()?;
    let record = refusal(&output);
    assert_eq!(record["rule"], "pr.failed");
    assert_eq!(record["next"], "pr-workflow create --help");
    Ok(())
}

const PAUSE_MARKER: &str = "<!-- coderabbit-pause -->";
/// The checkout file where an earlier release wrote issue contexts, which grants no authority.
const LEGACY_TRANSPORT: &str = ".pr-workflow/contexts.json";

const BODY: &str = "## Why\nAvoid losing edits.\n\n## Changes\nPreserve the original input.\n\n## Validation\nThe targeted regression test passed.\n";

/// Whether the command names `--issue 23`, the issue GitHub returns, what follows [`BODY`] in the document, and whether publication proceeds.
type Publication = (bool, Option<&'static str>, &'static str, bool);

const PUBLICATIONS: &[Publication] = &[
    (false, None, "\nCloses #23.\n", false),
    (true, None, "", false),
    (true, None, "\n<!-- Closes #23. -->\n", false),
    (true, None, "\n`Closes #23.`\n", false),
    (true, None, "\n```text\nCloses #23.\n```\n", false),
    (true, None, "\nCloses #230.\n", false),
    (
        true,
        None,
        "\n- Not run: the stopped engine.\n\nCloses #23.\n",
        true,
    ),
    (
        true,
        None,
        "\n| Check | Result |\n| --- | --- |\n| proofs | passed |\n\nCloses #23.\n",
        true,
    ),
    (
        true,
        None,
        "\nCloses https://github.com/other/project/issues/23.\n",
        false,
    ),
    (
        true,
        Some(
            r#"{"number":23,"title":"Preserve edits","body":"Input must survive.","state":"closed","html_url":"https://github.com/P4suta/project/issues/23"}"#,
        ),
        "\nCloses #23.\n",
        false,
    ),
    (
        true,
        Some(
            r#"{"number":24,"title":"Preserve edits","body":"Input must survive.","state":"open","html_url":"https://github.com/P4suta/project/issues/24"}"#,
        ),
        "\nCloses #23.\n",
        false,
    ),
    (
        true,
        Some(
            r#"{"number":23,"title":"Preserve edits","body":null,"state":"open","html_url":"https://github.com/P4suta/project/issues/23"}"#,
        ),
        "\nCloses #23.\n",
        false,
    ),
    (
        true,
        Some(
            r#"{"number":23,"title":"Preserve edits","body":"<!-- instructions -->","state":"open","html_url":"https://github.com/P4suta/project/issues/23"}"#,
        ),
        "\nCloses #23.\n",
        false,
    ),
    (
        true,
        Some(
            r#"{"number":23,"title":"Preserve edits","body":"TODO: describe intent","state":"open","html_url":"https://github.com/P4suta/project/issues/23"}"#,
        ),
        "\nCloses #23.\n",
        false,
    ),
    (
        true,
        Some(
            r#"{"number":23,"title":"Preserve edits","body":"Input must survive.","state":"open","html_url":"https://github.com/other/project/issues/23"}"#,
        ),
        "\nCloses #23.\n",
        false,
    ),
    (
        true,
        Some(
            r#"{"number":23,"title":"Preserve edits","body":"Input must survive.","state":"open","html_url":"https://github.com/P4suta/project/issues/23","pull_request":{}}"#,
        ),
        "\nCloses #23.\n",
        false,
    ),
    (true, Some("{truncated"), "\nCloses #23.\n", false),
    (true, None, "\nCloses #23.\n", true),
    (
        true,
        None,
        "\nFixes [the issue](https://github.com/P4suta/project/issues/23).\n",
        true,
    ),
];

#[test]
fn personal_publication_requires_a_real_open_issue_and_visible_reference() -> Result<()> {
    for operation in ["create", "edit", "ready"] {
        for &(issue_argument, issue, suffix, accepted) in PUBLICATIONS {
            let body = format!("{BODY}{suffix}");
            let fixture = Fixture::new(&body)?;
            let mut command = fixture.personal_command(operation)?;
            if operation == "ready" {
                command.args(["--pr", "17"]).env(
                    "GH_FIXTURE_VIEW",
                    serde_json::json!({
                        "title":"fix: preserve edits","body":body,"state":"OPEN","isDraft":true
                    })
                    .to_string(),
                );
            } else {
                command
                    .args(["--title", "fix: preserve edits", "--body-file"])
                    .arg(&fixture.body);
                command.args(if operation == "create" {
                    ["--head", "feature"]
                } else {
                    ["--pr", "17"]
                });
            }
            if issue_argument {
                command.args(["--issue", "23"]);
            }
            if let Some(issue) = issue {
                command.env("GH_FIXTURE_ISSUE", issue);
            }
            let output = command.output()?;
            assert_eq!(
                output.status.success(),
                accepted,
                "{operation}: {}",
                String::from_utf8_lossy(&output.stderr)
            );
            let log = if fixture.log.try_exists()? {
                fixture.log()?
            } else {
                String::new()
            };
            assert_eq!(log.contains(&format!("\n{operation}\n")), accepted, "{log}");
            if accepted && operation == "create" {
                assert!(log.contains("--draft"));
            }
        }
    }
    Ok(())
}

#[test]
fn global_owner_identity_and_forks_preserve_upstream_rules() -> Result<()> {
    for (repository, accepted) in [
        (serde_json::json!({"id":37,"full_name":"P4suta/project","owner":{"login":"P4suta","id":42_543_015},"fork":true}).to_string(), true),
        (serde_json::json!({"id":37,"full_name":"P4suta/project","owner":{"login":"P4suta","id":1},"fork":false}).to_string(), false),
        (serde_json::json!({"id":37,"full_name":"other/project","owner":{"login":"other","id":1},"fork":false}).to_string(), false),
        (serde_json::json!({"id":37,"full_name":"P4suta/project","owner":{"login":"other","id":42_543_015},"fork":false}).to_string(), false),
        (serde_json::json!({"id":37,"full_name":"P4suta/project","owner":{"login":"P4suta","id":42_543_015},"fork":null}).to_string(), false),
        ("{truncated".into(), false),
    ] {
        let fixture = Fixture::new(BODY)?;
        let output = fixture.personal_command("create")?
            .env("GH_FIXTURE_REPOSITORY", repository)
            .args(["--title","fix: preserve edits","--body-file"]).arg(&fixture.body)
            .args(["--head","feature"]).output()?;
        assert_eq!(output.status.success(), accepted, "{}", String::from_utf8_lossy(&output.stderr));
        assert_eq!(fixture.log()?.contains("\ncreate\n"), accepted);
    }
    Ok(())
}

#[test]
fn checked_preparation_does_not_publish_and_generation_cannot_skip_issues() -> Result<()> {
    let mut fixture = Fixture::new("Closes #23.\n@coderabbitai summary\n")?;
    fixture.level = "critical";
    for issue in [false, true] {
        let mut command = fixture.personal_command("start")?;
        if issue {
            command.args(["--issue", "23"]);
        }
        let output = command.output()?;
        assert_eq!(
            output.status.success(),
            issue,
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    assert!(!fixture.log()?.contains("\ncreate\n"));
    let output = fixture
        .personal_command("create")?
        .args(["--title", "@coderabbitai", "--body-file"])
        .arg(&fixture.body)
        .args([
            "--head",
            "feature",
            "--generation",
            "coderabbit",
            "--issue",
            "23",
        ])
        .output()?;
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(fixture.log()?.contains("--draft"));
    let mut fixture = Fixture::new("@coderabbitai summary\n")?;
    fixture.level = "critical";
    let output = fixture
        .personal_command("create")?
        .args(["--title", "@coderabbitai", "--body-file"])
        .arg(&fixture.body)
        .args([
            "--head",
            "feature",
            "--generation",
            "coderabbit",
            "--issue",
            "23",
        ])
        .output()?;
    assert!(!output.status.success());
    assert!(!fixture.log()?.contains("\ncreate\n"));
    Ok(())
}

fn assert_context_replay_is_refused(
    root: &std::path::Path,
    home: &std::path::Path,
    paths: &[PathBuf],
    branch: &str,
) -> Result<()> {
    assert!(platform::issue_context::guard_for(root, &[PathBuf::from("other.rs")], home).is_err());
    assert!(
        platform::issue_context::guard_for(root, &[PathBuf::from("../outside.rs")], home).is_err()
    );
    git(root, &["branch", "other"])?;
    git(root, &["symbolic-ref", "HEAD", "refs/heads/other"])?;
    assert!(platform::issue_context::guard_for(root, paths, home).is_err());
    git(
        root,
        &["symbolic-ref", "HEAD", &format!("refs/heads/{branch}")],
    )?;
    git(
        root,
        &[
            "remote",
            "set-url",
            "origin",
            "https://github.com/P4suta/other.git",
        ],
    )?;
    assert!(platform::issue_context::guard_for(root, paths, home).is_err());
    Ok(())
}

#[test]
fn checked_issue_context_guards_scoped_edits_and_rejects_repo_hints_and_replay() -> Result<()> {
    let fixture = Fixture::new(BODY)?;
    let root = platform::issue_context::checkout(fixture.directory.path())?;
    let paths = [PathBuf::from("body.md")];
    fs::create_dir_all(root.join(".pr-workflow"))?;
    fs::write(root.join(LEGACY_TRANSPORT), r#"{"schema":1,"receipts":[]}"#)?;
    assert!(
        platform::issue_context::guard_for(&root, &paths, fixture.context_home.path()).is_err()
    );
    fs::remove_dir_all(root.join(".pr-workflow"))?;
    fs::write(root.join("body.md"), "Prior authorized content.\n")?;
    let mut start = fixture.personal_command("start")?;
    start.args(["--issue", "23"]);
    let late = start.output()?;
    assert!(!late.status.success());
    assert!(String::from_utf8_lossy(&late.stderr).contains("lacks an earlier checked context"));
    assert!(!start.arg("--adopt-existing").output()?.status.success());
    let snapshot = platform::issue_context::snapshot(
        &root,
        platform::issue_context::Destination {
            repository: "P4suta/project".into(),
            repository_id: 37,
            owner_id: 42_543_015,
            required: true,
        },
        Some(23),
        &paths,
    )?;
    let (approval_path, approval) =
        platform::issue_context::adoption_request(&root, fixture.context_home.path(), &snapshot)?;
    fs::write(&approval_path, serde_json::to_vec(&approval)?)?;
    fs::write(root.join("body.md"), "Changed after approval.\n")?;
    assert!(!start.output()?.status.success());
    fs::write(root.join("body.md"), "Prior authorized content.\n")?;
    let output = start.output()?;
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(git(&root, &["rev-parse", "HEAD"])?, fixture.head);
    let stored = platform::issue_context::local_for(&root, fixture.context_home.path())?;
    assert_eq!(stored.receipts.len(), 1);
    let first = stored
        .receipts
        .first()
        .context("fixture checked context missing")?;
    let transport = serde_json::to_string(first)?;
    for private in [&root, fixture.context_home.path()] {
        let text = private.to_str().context("fixture private path")?;
        let encoded = serde_json::to_string(text)?;
        assert!(
            !transport.contains(text) && !first.signed_commit.contains(encoded.as_str()),
            "{transport}"
        );
    }
    let payload = serde_json::to_value(&first.context)?;
    for private in [
        "checkout",
        "common_git",
        "origin",
        "credentials",
        "topology",
    ] {
        assert!(payload.get(private).is_none(), "{payload}");
    }
    assert!(first.context.initial_dirty.contains_key("body.md"));
    assert!(first.context.adopted);
    assert!(!approval_path.exists());
    assert!(!start.output()?.status.success());
    platform::issue_context::guard_for(&root, &paths, fixture.context_home.path())?;
    fs::write(root.join("new-unregistered.rs"), "fn late_edit() {}\n")?;
    let mut later = fixture.personal_command("start")?;
    later.args(["--issue", "23", "--path", "new-unregistered.rs"]);
    let refused_late = later.output()?;
    assert!(!refused_late.status.success());
    assert!(String::from_utf8_lossy(&refused_late.stderr).contains("new-unregistered.rs"));
    fs::remove_file(root.join("new-unregistered.rs"))?;
    let restarted = fixture
        .personal_command("start")?
        .args(["--issue", "23"])
        .output()?;
    assert!(
        restarted.status.success(),
        "{}",
        String::from_utf8_lossy(&restarted.stderr)
    );
    assert_context_replay_is_refused(
        &root,
        fixture.context_home.path(),
        &paths,
        &first.context.branch,
    )?;
    Ok(())
}

#[test]
fn integration_checks_authenticate_contexts_and_keep_multiple_issue_scopes() -> Result<()> {
    let fixture = Fixture::new(BODY)?;
    let root = platform::issue_context::checkout(fixture.directory.path())?;
    let start = fixture
        .personal_command("start")?
        .args(["--issue", "23"])
        .output()?;
    assert!(
        start.status.success(),
        "{}",
        String::from_utf8_lossy(&start.stderr)
    );
    let mut second = fixture.personal_command("start")?;
    second.args(["--issue", "24", "--path", "other.rs"]);
    second.env("GH_FIXTURE_ISSUE", serde_json::json!({"number":24,"title":"Keep the second input","body":"Preserve the second path.","state":"open","html_url":"https://github.com/P4suta/project/issues/24"}).to_string());
    let second = second.output()?;
    assert!(
        second.status.success(),
        "{}",
        String::from_utf8_lossy(&second.stderr)
    );
    let stored = platform::issue_context::local_for(&root, fixture.context_home.path())?;
    assert_eq!(stored.receipts.len(), 2);
    platform::issue_context::guard_for(
        &root,
        &[PathBuf::from("body.md"), PathBuf::from("other.rs")],
        fixture.context_home.path(),
    )?;
    Ok(())
}

#[test]
fn native_fixture_checks_ignore_the_parent_github_event() -> Result<()> {
    let outer = Fixture::new("Outer CI repository.\n")?;
    let event = outer.context_home.path().join("event.json");
    fs::write(
        &event,
        serde_json::to_vec(&serde_json::json!({
            "repository":{"full_name":"P4suta/project"},
            "pull_request":{
                "base":{"repo":{"full_name":"P4suta/project"}},
                "head":{"ref":"feature","sha":outer.head}
            }
        }))?,
    )?;
    let output = fixture_process::command(std::env::current_exe()?, outer.directory.path())
        .env("GITHUB_ACTIONS", "true")
        .env("GITHUB_EVENT_PATH", &event)
        .env("GITHUB_REPOSITORY", "P4suta/project")
        .args([
            "--exact",
            "a_new_local_project_prepares_unborn_scoped_work_without_github",
            "change_gate_refuses_missing_forged_closed_and_unscoped_contexts",
            "every_worktree_of_a_repository_finds_the_context_of_its_branch",
            "issue_contexts_stay_in_owner_local_state_and_never_reach_a_clone",
            "issue_checks_use_the_actual_transaction_index_and_isolate_signature_objects",
            "working_change_checks_use_owner_context_and_cover_all_local_changes",
            "--test-threads=2",
            "--nocapture",
        ])
        .output()?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        output.status.success(),
        "{}{}",
        stdout,
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(stdout.contains("6 passed; 0 failed"), "{stdout}");
    Ok(())
}

#[test]
fn every_worktree_of_a_repository_finds_the_context_of_its_branch() -> Result<()> {
    let fixture = Fixture::new(BODY)?;
    let root = platform::issue_context::checkout(fixture.directory.path())?;
    let branch = git(&root, &["symbolic-ref", "--short", "HEAD"])?;
    let mut start = fixture.personal_command("start")?;
    start.args(["--issue", "23"]);
    let variables: Vec<_> = start
        .get_envs()
        .filter_map(|(key, value)| value.map(|value| (key.to_owned(), value.to_owned())))
        .collect();
    let started = start.output()?;
    assert!(
        started.status.success(),
        "{}",
        String::from_utf8_lossy(&started.stderr)
    );
    git(&root, &["switch", "--quiet", "-c", "parking"])?;
    let parent = tempfile::tempdir()?;
    let worktree = parent.path().join("second");
    git(
        &root,
        &[
            "worktree",
            "add",
            "--quiet",
            worktree.to_str().context("worktree path")?,
            &branch,
        ],
    )?;
    let worktree = platform::issue_context::checkout(&worktree)?;
    platform::issue_context::guard_for(
        &worktree,
        &[PathBuf::from("body.md")],
        fixture.context_home.path(),
    )?;
    fs::write(
        worktree.join("body.md"),
        "Changed in the second worktree.\n",
    )?;
    git(&worktree, &["add", "body.md"])?;
    let staged = fixture_process::command(env!("CARGO_BIN_EXE_pr-workflow"), &worktree)
        .current_dir(&worktree)
        .envs(variables.iter().cloned())
        .args(["change-check", "--staged"])
        .output()?;
    assert!(
        staged.status.success(),
        "{}",
        String::from_utf8_lossy(&staged.stderr)
    );
    assert!(String::from_utf8_lossy(&staged.stdout).contains("bound to live issues"));
    assert!(!worktree.join(".pr-workflow").exists() && !root.join(".pr-workflow").exists());
    assert!(
        platform::issue_context::guard_for(
            &root,
            &[PathBuf::from("body.md")],
            fixture.context_home.path()
        )
        .is_err(),
        "the parking branch of the first worktree has no context"
    );
    Ok(())
}

#[test]
fn working_change_checks_use_owner_context_and_cover_all_local_changes() -> Result<()> {
    let fixture = Fixture::new(BODY)?;
    let root = platform::issue_context::checkout(fixture.directory.path())?;
    fs::write(root.join("outside.rs"), "fn original() {}\n")?;
    git(&root, &["add", "outside.rs"])?;
    git(&root, &["commit", "--quiet", "-m", "tracked fixture"])?;
    let mut start = fixture.personal_command("start")?;
    start.args(["--issue", "23", "--path", "registered.rs"]);
    let variables: Vec<_> = start
        .get_envs()
        .filter_map(|(name, value)| value.map(|value| (name.to_owned(), value.to_owned())))
        .collect();
    let check = || -> Result<Output> {
        Ok(
            fixture_process::command(env!("CARGO_BIN_EXE_pr-workflow"), &root)
                .current_dir(&root)
                .envs(variables.iter().cloned())
                .arg("change-check")
                .output()?,
        )
    };
    let started = start.output()?;
    assert!(
        started.status.success(),
        "{}",
        String::from_utf8_lossy(&started.stderr)
    );
    assert!(!root.join(".pr-workflow").exists());
    fs::write(root.join("body.md"), "Changed registered input.\n")?;
    let unstaged = check()?;
    assert!(
        unstaged.status.success(),
        "{}",
        String::from_utf8_lossy(&unstaged.stderr)
    );
    git(&root, &["add", "body.md"])?;
    git(
        &root,
        &["restore", "--worktree", "--source=HEAD", "body.md"],
    )?;
    assert!(
        check()?.status.success(),
        "the actual staged change is included"
    );
    fs::write(root.join("registered.rs"), "fn registered() {}\n")?;
    assert!(
        check()?.status.success(),
        "registered untracked changes are included"
    );
    fs::write(root.join("unregistered.rs"), "fn unregistered() {}\n")?;
    let untracked = check()?;
    assert!(!untracked.status.success());
    assert_eq!(refusal(&untracked)["rule"], "pr.issue-context");
    assert!(String::from_utf8_lossy(&untracked.stderr).contains("unregistered.rs"));
    fs::remove_file(root.join("unregistered.rs"))?;
    fs::write(root.join("outside.rs"), "fn changed_outside() {}\n")?;
    let outside = check()?;
    assert!(!outside.status.success());
    assert!(String::from_utf8_lossy(&outside.stderr).contains("outside.rs"));
    git(&root, &["add", "outside.rs"])?;
    git(
        &root,
        &["restore", "--worktree", "--source=HEAD", "outside.rs"],
    )?;
    let staged = check()?;
    assert!(!staged.status.success());
    assert!(String::from_utf8_lossy(&staged.stderr).contains("outside.rs"));
    git(&root, &["restore", "--staged", "--worktree", "outside.rs"])?;
    assert!(check()?.status.success());
    git(&root, &["branch", "unregistered-branch"])?;
    git(
        &root,
        &["symbolic-ref", "HEAD", "refs/heads/unregistered-branch"],
    )?;
    let branch = check()?;
    assert!(!branch.status.success());
    assert!(String::from_utf8_lossy(&branch.stderr).contains("current branch"));
    Ok(())
}

#[test]
fn an_origin_change_during_signing_cannot_rebind_the_checked_destination() -> Result<()> {
    let fixture = Fixture::new(BODY)?;
    let root = platform::issue_context::checkout(fixture.directory.path())?;
    let mut start = fixture.personal_command("start")?;
    start.args(["--issue", "23"]);
    let config = root.join(".git/config");
    let original = fs::read_to_string(&config)?;
    let changed = original.replace(
        "url = https://github.com/P4suta/project.git",
        "url = https://github.com/P4suta/other.git",
    );
    assert_ne!(original, changed);
    start
        .env("GH_FIXTURE_REWRITE_FILE", &config)
        .env(
            "GH_FIXTURE_REWRITE_ENDPOINT",
            "users/P4suta/ssh_signing_keys?per_page=100",
        )
        .env("GH_FIXTURE_REWRITE_CONTENT", changed);
    let refused = start.output()?;
    assert!(!refused.status.success());
    assert_eq!(refusal(&refused)["rule"], "pr.stack");
    assert!(!root.join(".pr-workflow").exists());
    fs::write(config, original)?;
    assert!(platform::issue_context::local_for(&root, fixture.context_home.path()).is_err());
    Ok(())
}

#[test]
fn change_gate_refuses_missing_forged_closed_and_unscoped_contexts() -> Result<()> {
    let fixture = Fixture::new(BODY)?;
    let root = platform::issue_context::checkout(fixture.directory.path())?;
    let mut start = fixture.personal_command("start")?;
    start.args(["--issue", "23"]);
    let configured: Vec<_> = start
        .get_envs()
        .filter_map(|(key, value)| value.map(|value| (key.to_owned(), value.to_owned())))
        .collect();
    let check = || -> Result<Command> {
        let mut command =
            fixture_process::command(env!("CARGO_BIN_EXE_pr-workflow"), fixture.directory.path());
        command
            .current_dir(&root)
            .envs(configured.iter().cloned())
            .args(["change-check", "--staged"]);
        Ok(command)
    };
    fs::write(root.join("body.md"), "Changed input.\n")?;
    git(&root, &["add", "body.md"])?;
    let missing = check()?.output()?;
    assert!(!missing.status.success());
    assert_eq!(refusal(&missing)["rule"], "pr.issue-context");
    git(&root, &["restore", "--staged", "--worktree", "body.md"])?;
    let output = start.output()?;
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    fs::write(root.join("body.md"), "Changed input.\n")?;
    git(&root, &["add", "body.md"])?;
    let accepted = check()?.output()?;
    assert!(
        accepted.status.success(),
        "{}",
        String::from_utf8_lossy(&accepted.stderr)
    );
    let mut closed = check()?;
    closed.env("GH_FIXTURE_ISSUE", serde_json::json!({"number":23,"title":"Preserve edits","body":"Preserve the input.","state":"closed","html_url":"https://github.com/P4suta/project/issues/23"}).to_string());
    assert!(!closed.output()?.status.success());
    fs::write(root.join("unregistered.rs"), "fn unregistered() {}\n")?;
    git(&root, &["add", "unregistered.rs"])?;
    assert!(!check()?.output()?.status.success());
    let state = fixture
        .context_home
        .path()
        .join(".local/state/pr-workflow/contexts");
    let mut forged = 0;
    for entry in fs::read_dir(&state)? {
        let path = entry?.path();
        if path.extension().is_none_or(|extension| extension != "json")
            || path.to_string_lossy().contains(".adoption")
        {
            continue;
        }
        let mut local: serde_json::Value = serde_json::from_slice(&fs::read(&path)?)?;
        local["manifest"]["receipts"][0]["context"]["scopes"]
            .as_array_mut()
            .context("fixture checked scopes")?
            .push("unregistered.rs".into());
        fs::write(&path, serde_json::to_vec(&local)?)?;
        forged += 1;
    }
    assert_eq!(forged, 1, "one repository state file");
    let refused = check()?.output()?;
    assert!(
        !refused.status.success(),
        "forged owner-local scopes fail their signature"
    );
    assert_eq!(refusal(&refused)["rule"], "pr.issue-context");
    Ok(())
}

#[test]
fn a_new_personal_start_does_not_reuse_an_old_fork_permission() -> Result<()> {
    let fixture = Fixture::new(BODY)?;
    let root = platform::issue_context::checkout(fixture.directory.path())?;
    let mut fork = fixture.personal_command("start")?;
    fork.args(["--issue", "23"]).env("GH_FIXTURE_REPOSITORY", serde_json::json!({"id":37,"full_name":"P4suta/project","owner":{"login":"P4suta","id":42_543_015},"fork":true}).to_string());
    let output = fork.output()?;
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    platform::issue_context::guard_for(
        &root,
        &[PathBuf::from("other.rs")],
        fixture.context_home.path(),
    )?;
    let personal = fixture
        .personal_command("start")?
        .args(["--issue", "23"])
        .output()?;
    assert!(
        personal.status.success(),
        "{}",
        String::from_utf8_lossy(&personal.stderr)
    );
    let active = platform::issue_context::local_for(&root, fixture.context_home.path())?;
    assert_eq!(active.receipts.len(), 1);
    assert!(
        active
            .receipts
            .first()
            .context("personal context missing")?
            .context
            .destination
            .required
    );
    assert!(
        platform::issue_context::guard_for(
            &root,
            &[PathBuf::from("other.rs")],
            fixture.context_home.path()
        )
        .is_err()
    );
    Ok(())
}

#[test]
fn issue_checks_use_the_actual_transaction_index_and_isolate_signature_objects() -> Result<()> {
    let fixture = Fixture::new(BODY)?;
    let root = platform::issue_context::checkout(fixture.directory.path())?;
    let start = fixture
        .personal_command("start")?
        .args(["--issue", "23"])
        .output()?;
    assert!(
        start.status.success(),
        "{}",
        String::from_utf8_lossy(&start.stderr)
    );
    fs::write(root.join("body.md"), "Changed registered input.\n")?;
    git(&root, &["add", "body.md"])?;
    let configured = fixture.personal_command("start")?;
    let variables: Vec<_> = configured
        .get_envs()
        .filter_map(|(name, value)| value.map(|value| (name.to_owned(), value.to_owned())))
        .collect();
    let check = || -> Result<Command> {
        let mut command = fixture_process::command(env!("CARGO_BIN_EXE_pr-workflow"), &root);
        command
            .current_dir(&root)
            .envs(variables.iter().cloned())
            .args(["change-check", "--staged"]);
        Ok(command)
    };
    let owned = fixture
        .context_home
        .path()
        .join("redirected-object-database");
    fs::create_dir_all(&owned)?;
    let mut ordinary = check()?;
    ordinary
        .env("GIT_COMMON_DIR", &owned)
        .env("GIT_OBJECT_DIRECTORY", &owned)
        .env("GIT_ALTERNATE_OBJECT_DIRECTORIES", &owned);
    let accepted = ordinary.output()?;
    assert!(
        accepted.status.success(),
        "{}",
        String::from_utf8_lossy(&accepted.stderr)
    );
    assert_eq!(fs::read_dir(&owned)?.count(), 0);
    let index = fixture.context_home.path().join("transaction-index");
    fs::write(root.join("unregistered.rs"), "fn unregistered() {}\n")?;
    for arguments in [vec!["read-tree", "HEAD"], vec!["add", "unregistered.rs"]] {
        let output = fixture_git::command("git", &root.join("gitconfig"))
            .current_dir(&root)
            .env("GIT_INDEX_FILE", &index)
            .args(arguments)
            .output()?;
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    let refused = check()?.env("GIT_INDEX_FILE", &index).output()?;
    assert!(!refused.status.success());
    assert_eq!(refusal(&refused)["rule"], "pr.issue-context");
    assert!(String::from_utf8_lossy(&refused.stderr).contains("unregistered.rs"));
    Ok(())
}

#[test]
fn authentication_quota_and_api_failures_stop_without_retry_or_publication() -> Result<()> {
    for operation in ["start", "create", "edit", "ready", "check"] {
        for (variable, value) in [
            ("GH_FIXTURE_USER", "{truncated"),
            ("GH_FIXTURE_USER", r#"{"login":"","id":42543015}"#),
            ("GH_FIXTURE_USER", r#"{"login":"P4suta","id":0}"#),
            ("GH_FIXTURE_FAILURE", "user"),
            ("GH_FIXTURE_FAILURE", "repos/P4suta/project"),
            ("GH_FIXTURE_FAILURE", "repos/P4suta/project/issues/23"),
            (
                "GH_FIXTURE_HEADERS",
                "HTTP/2.0 200 OK\nX-Ratelimit-Remaining: 9\nX-Ratelimit-Reset: 1800000000\nX-Ratelimit-Resource: core",
            ),
            (
                "GH_FIXTURE_ISSUE_HEADERS",
                "HTTP/2.0 200 OK\nX-Ratelimit-Remaining: 0\nX-Ratelimit-Reset: 1800000000\nX-Ratelimit-Resource: core",
            ),
            ("GH_FIXTURE_HEADERS", "HTTP/2.0 200 OK"),
            (
                "GH_FIXTURE_HEADERS",
                "HTTP/2.0 200 OK\nX-Ratelimit-Remaining: nope\nX-Ratelimit-Reset: 1800000000\nX-Ratelimit-Resource: core",
            ),
            (
                "GH_FIXTURE_HEADERS",
                "HTTP/2.0 200 OK\nX-Ratelimit-Remaining: 4990\nX-Ratelimit-Remaining: 0\nX-Ratelimit-Reset: 1800000000\nX-Ratelimit-Resource: core",
            ),
        ] {
            let fixture = Fixture::new(&format!("{BODY}\nCloses #23.\n"))?;
            let mut command = fixture.personal_command(operation)?;
            command.args(["--issue", "23"]).env(variable, value);
            match operation {
                "start" => {}
                "ready" | "check" => {
                    command.args(["--pr", "17"]).env("GH_FIXTURE_VIEW", serde_json::json!({
                        "title":"fix: preserve edits","body":fs::read_to_string(&fixture.body)?,"state":"OPEN","isDraft":true
                    }).to_string());
                }
                _ => {
                    command
                        .args(["--title", "fix: preserve edits", "--body-file"])
                        .arg(&fixture.body);
                    command.args(if operation == "create" {
                        ["--head", "feature"]
                    } else {
                        ["--pr", "17"]
                    });
                }
            }
            let output = command.output()?;
            assert!(!output.status.success(), "{operation}: {variable}");
            refusal(&output);
            let log = fixture.log()?;
            for mutation in ["create", "edit", "ready"] {
                assert!(!log.contains(&format!("\n{mutation}\n")), "{log}");
            }
            assert_eq!(
                log.matches("\nuser\n").count(),
                1,
                "authentication must not retry: {log}"
            );
            assert_bounded_authentication_reads(&log);
        }
    }
    let fixture = Fixture::new(BODY)?;
    let output = fixture.document("check", "fix: preserve edits")?;
    assert!(output.status.success());
    assert!(!fixture.log.try_exists()?);
    let fixture = Fixture::new(&format!("{BODY}\nCloses #23.\n"))?;
    let output = fixture.personal_command("create")?
        .env("GH_HOST", "enterprise.example.org")
        .env("GH_FIXTURE_HEADERS", "HTTP/2.0 200 OK\r\nx-ratelimit-remaining: 10\r\nx-ratelimit-reset: 1800000000\r\nx-ratelimit-resource: core")
        .args(["--issue","23","--title","fix: preserve edits","--body-file"]).arg(&fixture.body)
        .args(["--head","feature"]).output()?;
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(fixture.log()?.matches("\nuser\n").count(), 1);
    Ok(())
}

fn assert_bounded_authentication_reads(log: &str) {
    const LEGACY_READ_LIMIT: usize = 5;
    const SIGNER_READ_LIMIT: usize = 2;
    const VIEW_READ_LIMIT: usize = 2;
    const REPOSITORY_READ_LIMIT: usize = 2;
    let calls: Vec<_> = log.split("invocation\n").skip(1).collect();
    let count = |prefix: &str| calls.iter().filter(|call| call.starts_with(prefix)).count();
    let signers = count("api\nusers/P4suta/ssh_signing_keys?per_page=100\n");
    let branch = count("api\nrepos/P4suta/project/branches/main\n");
    let threads = count("api\ngraphql\n");
    let repository = count("api\nrepos/P4suta/project\n");
    let views = count("pr\nview\n");
    assert!(
        signers <= SIGNER_READ_LIMIT && branch <= 1 && threads <= 1,
        "bounded assessment reads: {log}"
    );
    assert!(
        views <= VIEW_READ_LIMIT && repository <= REPOSITORY_READ_LIMIT,
        "bounded source readbacks: {log}"
    );
    let assessment_reads =
        signers + branch + threads + repository.saturating_sub(1) + views.saturating_sub(1);
    assert!(
        calls.len() - assessment_reads <= LEGACY_READ_LIMIT,
        "bounded original workflow reads: {log}"
    );
}

#[test]
fn invalid_documents_never_start_gh_and_check_has_no_effects() -> Result<()> {
    for (title, body) in [
        ("Repair edits", BODY),
        ("fix(): preserve edits", BODY),
        ("fix: preserve edits\nextra", BODY),
        ("fix: preserve edits", " \n"),
        ("fix: preserve edits", "<!-- Explain your change. -->"),
        ("fix: preserve edits", "## Changes\nTBD\n"),
        ("fix: TODO", BODY),
        ("fix: preserve edits", "Describe your changes here."),
        ("@coderabbitai", "@coderabbitai summary"),
        (
            "fix: preserve edits",
            "Real content.\n<!-- @coderabbitai summary -->",
        ),
        ("feat: <description>", BODY),
        ("feat: {{title}}", BODY),
        ("fix: preserve edits", "## Changes\n{{ description }}\n"),
        ("fix: preserve edits", "## Changes\n[TODO]\n"),
        (
            "fix: preserve edits",
            "## Changes\nTODO: describe the change\n",
        ),
    ] {
        let fixture = Fixture::new(body)?;
        for action in ["check", "create", "edit"] {
            let output = fixture.document(action, title)?;
            assert!(!output.status.success(), "{action}: {title}");
            refusal(&output);
            assert!(!fixture.log.try_exists()?);
        }
    }
    for title in [
        "fix: preserve edits",
        "feat(storage)!: replace the file format",
        "refactor(ui): simplify navigation",
        "custom: add a new task",
    ] {
        let fixture = Fixture::new(BODY)?;
        assert!(fixture.document("check", title)?.status.success());
        assert!(!fixture.log.try_exists()?);
    }
    Ok(())
}

#[test]
fn attribution_lines_are_refused_by_name_and_the_authors_own_lines_are_admitted() -> Result<()> {
    for form in [
        "Claude-Session: https://claude.ai/code/session_01Qesu",
        "Co-Authored-By: Claude Opus 5 <noreply@anthropic.com>",
        "🤖 Generated with [Claude Code](https://claude.com/claude-code)",
        "https://claude.ai/code/session_01Qesu",
    ] {
        let fixture = Fixture::new(&format!("{BODY}\n{form}\n"))?;
        for action in ["check", "create", "edit"] {
            let output = fixture.document(action, "fix: preserve edits")?;
            let stderr = String::from_utf8_lossy(&output.stderr);
            assert!(!output.status.success(), "{action}: {form}");
            assert!(
                stderr.contains("body line 10") && stderr.contains(form),
                "{action}: {stderr}"
            );
            assert!(
                stderr.contains("pr.attribution refused") && stderr.contains("Delete that line"),
                "{stderr}"
            );
            assert!(!fixture.log.try_exists()?);
        }
    }
    let admitted = format!(
        "{BODY}\nCo-authored-by: Example User <example@example.com>\nThe CLAUDE.md contract mentions Claude.\n"
    );
    let fixture = Fixture::new(&admitted)?;
    for action in ["check", "create"] {
        let output = fixture.document(action, "fix: preserve edits")?;
        assert!(output.status.success(), "{action}");
    }
    Ok(())
}

#[test]
fn personal_destinations_refuse_prose_violations_before_publication() -> Result<()> {
    let fixture = Fixture::new(
        "## Why
As the owner requested, the parser keeps edits.

Closes #23.
",
    )?;
    for action in ["check", "create", "edit"] {
        let output = fixture
            .personal_command(action)?
            .args(["--title", "fix: preserve edits", "--body-file"])
            .arg(&fixture.body)
            .args(match action {
                "create" => vec!["--issue", "23", "--head", "feature"],
                "edit" => vec!["--issue", "23", "--pr", "17"],
                _ => vec![],
            })
            .output()?;
        assert!(!output.status.success(), "{action}");
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(stderr.contains("Dotfiles.Attribution"), "{stderr}");
        assert!(stderr.contains("As the owner requested, the parser keeps edits."));
        assert!(stderr.contains("pr-workflow check"), "{stderr}");
        assert_eq!(refusal(&output)["rule"], "pr.failed");
        eprint!("{stderr}");
        let log = if fixture.log.try_exists()? {
            fixture.log()?
        } else {
            String::new()
        };
        assert!(
            !log.contains("\ncreate\n") && !log.contains("\nedit\n"),
            "{log}"
        );
        if action == "check" {
            assert!(log.is_empty(), "the local check stays offline");
        }
    }
    Ok(())
}

#[test]
fn external_destinations_and_forks_keep_their_own_writing_rules() -> Result<()> {
    let template = "## Checklist\nI have read the contributing guide, and we don't skip tests.\n";
    let fixture = Fixture::new(template)?;
    for action in ["check", "create", "edit"] {
        let output = fixture.document(action, "fix: preserve edits")?;
        assert!(
            output.status.success(),
            "{action}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    let fork = serde_json::json!({
        "id":37,"full_name":"P4suta/project","owner":{"login":"P4suta","id":42_543_015},"fork":true
    });
    let output = fixture
        .personal_command("create")?
        .env("GH_FIXTURE_REPOSITORY", fork.to_string())
        .args(["--title", "fix: preserve edits", "--body-file"])
        .arg(&fixture.body)
        .args(["--head", "feature"])
        .output()?;
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(fixture.log()?.contains("\ncreate\n"));
    Ok(())
}

#[test]
fn provenance_phrases_are_refused_in_pr_and_issue_documents() -> Result<()> {
    for phrase in ["AI-assisted", "generated by AI", "on behalf of the owner"] {
        let fixture = Fixture::new(&format!("{BODY}\nThis summary came {phrase}.\n"))?;
        for action in ["check", "create", "edit"] {
            let output = fixture.document(action, "fix: preserve edits")?;
            let stderr = String::from_utf8_lossy(&output.stderr);
            assert!(!output.status.success(), "{action}: {phrase}");
            assert!(
                stderr.contains("pr.provenance refused")
                    && stderr.contains("Rewrite that sentence"),
                "{stderr}"
            );
            assert!(!fixture.log.try_exists()?);
        }
        for (action, extra) in [
            ("check", vec![]),
            ("create", vec![]),
            ("edit", vec!["--number", "23"]),
        ] {
            let output = fixture
                .personal_command(&format!("issue {action}"))?
                .args(["--title", "Keep edits", "--body-file"])
                .arg(&fixture.body)
                .args(&extra)
                .output()?;
            assert!(!output.status.success(), "issue {action}: {phrase}");
            assert_eq!(refusal(&output)["rule"], "pr.provenance", "issue {action}");
        }
    }
    let quoted = Fixture::new(&format!(
        "{BODY}\nOld notes mention `AI-assisted` reviews.\n"
    ))?;
    let output = quoted.document("check", "fix: preserve edits")?;
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    Ok(())
}

#[test]
fn issue_bodies_pass_the_checker_before_gh_publishes_them() -> Result<()> {
    let refused = Fixture::new("As the owner requested, the parser keeps edits.\n")?;
    for (action, extra) in [
        ("check", vec![]),
        ("create", vec![]),
        ("edit", vec!["--number", "23"]),
    ] {
        let output = refused
            .personal_command(&format!("issue {action}"))?
            .args(["--title", "Keep edits", "--body-file"])
            .arg(&refused.body)
            .args(&extra)
            .output()?;
        assert!(!output.status.success(), "{action}");
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            stderr.contains("Dotfiles.Attribution")
                && stderr.contains("As the owner requested, the parser keeps edits.")
                && stderr.contains("pr-workflow issue check"),
            "{stderr}"
        );
        assert_eq!(refusal(&output)["rule"], "pr.failed");
        eprintln!("{}", stderr.lines().last().unwrap_or_default());
        let log = if refused.log.try_exists()? {
            refused.log()?
        } else {
            String::new()
        };
        assert!(!log.contains("\nissue\n"), "{log}");
    }
    let unfinished = Fixture::new("TODO\n")?;
    let output = unfinished
        .personal_command("issue create")?
        .args(["--title", "Keep edits", "--body-file"])
        .arg(&unfinished.body)
        .output()?;
    assert!(!output.status.success());
    assert!(!unfinished.log.try_exists()?);
    let accepted = Fixture::new("The parser keeps every edit.\n")?;
    for (action, extra) in [
        ("check", vec![]),
        ("create", vec![]),
        ("edit", vec!["--number", "23"]),
    ] {
        let output = accepted
            .personal_command(&format!("issue {action}"))?
            .args(["--title", "Keep edits", "--body-file"])
            .arg(&accepted.body)
            .args(&extra)
            .output()?;
        assert!(
            output.status.success(),
            "{action}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    let log = accepted.log()?;
    assert!(
        log.contains("issue\ncreate\n--repo\nP4suta/project"),
        "{log}"
    );
    assert!(
        log.contains("issue\nedit\n23\n--repo\nP4suta/project"),
        "{log}"
    );
    assert!(log.contains("body:\nThe parser keeps every edit."), "{log}");
    let external = Fixture::new("I have read the contributing guide.\n")?;
    let output = external
        .command("issue create")?
        .args(["--title", "Keep edits", "--body-file"])
        .arg(&external.body)
        .output()?;
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    Ok(())
}

#[test]
fn completed_markdown_and_literal_marker_examples_are_accepted() -> Result<()> {
    let body = "## Changes\nSort the todo list and remove obsolete `FIXME` comments.\n\n<details>\n<summary>Validation results</summary>\nThe regression passed.\n</details>\n\n```yaml\nvalue: ${{ secrets.X }}\nTODO\n{{description}}\n```\n\n~~~text\n[insert description]\n~~~\n\nAn inline example uses `{{title}}` and `TBD`.\nThe Actions expression ${{ secrets.token }} stays literal.\n";
    let fixture = Fixture::new(body)?;
    for operation in ["check", "create", "edit"] {
        let output = fixture.document(operation, "feat: add todo list sorting")?;
        assert!(
            output.status.success(),
            "{operation}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    let view = serde_json::json!({"title":"feat: add todo list sorting","body":body,"state":"OPEN","isDraft":true});
    let output = fixture
        .command("ready")?
        .args(["--pr", "17"])
        .env("GH_FIXTURE_VIEW", view.to_string())
        .output()?;
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(fixture.log()?.contains("\nready\n"));
    Ok(())
}

#[test]
fn review_exclusion_is_published_without_allowing_unfinished_generation() -> Result<()> {
    let body = format!("{BODY}\n@coderabbitai ignore\n");
    let fixture = Fixture::new(&body)?;
    for operation in ["check", "create", "edit"] {
        let output = fixture.document(operation, "fix: preserve the review pause")?;
        assert!(
            output.status.success(),
            "{operation}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    let view = serde_json::json!({"title":"fix: preserve the review pause","body":body,"state":"OPEN","isDraft":true});
    let output = fixture
        .command("ready")?
        .args(["--pr", "17"])
        .env("GH_FIXTURE_VIEW", view.to_string())
        .output()?;
    assert!(output.status.success());
    assert!(fixture.log()?.contains("@coderabbitai ignore"));
    for body in [
        "@coderabbitai ignore\n".to_owned(),
        "<!-- Explain the change. -->\n@coderabbitai ignore\n".to_owned(),
        format!("{BODY}\n@coderabbitai ignore\n@coderabbitai summary\n"),
        format!("{BODY}\n@coderabbitai ignore extra\n"),
        format!("{BODY}\n@coderabbitai review\n"),
    ] {
        let rejected = Fixture::new(&body)?;
        for operation in ["check", "create", "edit"] {
            assert!(
                !rejected
                    .document(operation, "fix: preserve the review pause")?
                    .status
                    .success()
            );
            assert!(!rejected.log.try_exists()?);
        }
    }
    Ok(())
}

/// Each pause marker the owner can set, and whether it pauses PR reviews.
const PAUSES: [(Option<&str>, bool); 4] = [
    (None, false),
    (Some("paused"), true),
    (Some("paused-pr"), true),
    (Some("paused-cli"), false),
];

fn excluded() -> String {
    format!("{BODY}\n@coderabbitai ignore\n")
}

fn pause_excluded() -> String {
    format!("{}{PAUSE_MARKER}\n", excluded())
}

#[test]
fn paused_pr_reviews_are_not_requested_by_generation() -> Result<()> {
    for (marker, paused) in PAUSES {
        let mut fixture =
            Fixture::new("@coderabbitai summary\n\n## Validation\nThe regression passed.\n")?;
        fixture.level = "critical";
        if let Some(marker) = marker {
            fixture.pause(marker)?;
        }
        for operation in ["create", "edit"] {
            let mut command = fixture.command(operation)?;
            command
                .args(["--title", "@coderabbitai", "--body-file"])
                .arg(&fixture.body)
                .args(["--generation", "coderabbit"]);
            if operation == "create" {
                command.args(["--head", "feature", "--draft"]);
            } else {
                command.args(["--pr", "17"]);
            }
            let output = command.output()?;
            assert_eq!(output.status.success(), !paused, "{marker:?} {operation}");
            if paused {
                let error = String::from_utf8_lossy(&output.stderr);
                assert!(
                    error.contains("PR reviews are paused") && error.contains("--generation local"),
                    "{error}"
                );
                assert_eq!(mutations(&fixture.log()?), [] as [String; 0]);
            }
        }
    }
    Ok(())
}

#[test]
fn a_draft_readied_during_the_pause_carries_the_pause_lines() -> Result<()> {
    for (marker, paused) in PAUSES {
        for (body, generation, accepted) in [
            (BODY.to_owned(), "local", true),
            (excluded(), "local", true),
            (pause_excluded(), "local", false),
            ("@coderabbitai summary\n".to_owned(), "coderabbit", !paused),
        ] {
            let mut fixture = Fixture::new(BODY)?;
            if generation == "coderabbit" {
                fixture.level = "critical";
            }
            if let Some(marker) = marker {
                fixture.pause(marker)?;
            }
            let title = if generation == "local" {
                "fix: preserve edits"
            } else {
                "@coderabbitai"
            };
            let view = serde_json::json!({"title":title,"body":body,"state":"OPEN","isDraft":true});
            let output = fixture
                .command("ready")?
                .args(["--pr", "17", "--generation", generation])
                .env("GH_FIXTURE_VIEW", view.to_string())
                .output()?;
            assert_eq!(
                output.status.success(),
                accepted,
                "{marker:?} {generation}: {}",
                String::from_utf8_lossy(&output.stderr)
            );
            assert_eq!(fixture.log()?.contains("\nready\n"), accepted);
            if !accepted {
                let error = String::from_utf8_lossy(&output.stderr);
                assert!(
                    if generation == "local" {
                        error.contains("can't carry local workflow evidence")
                    } else {
                        error.contains("paused")
                    },
                    "{error}"
                );
            }
        }
    }
    Ok(())
}

#[test]
fn paused_pr_reviews_are_not_awaited() -> Result<()> {
    for (marker, _) in PAUSES {
        for (body, expected) in [
            (BODY.to_owned(), "CodeRabbit review route: None"),
            (excluded(), "CodeRabbit review route: None"),
            (pause_excluded(), "can't carry local workflow evidence"),
        ] {
            let fixture = Fixture::new(BODY)?;
            if let Some(marker) = marker {
                fixture.pause(marker)?;
            }
            let view = serde_json::json!({"title":"fix: preserve edits","body":body,"state":"OPEN","isDraft":false});
            let output = fixture
                .command("check")?
                .args(["--pr", "17", "--final"])
                .env("GH_FIXTURE_VIEW", view.to_string())
                .output()?;
            assert_eq!(
                output.status.success(),
                !body.contains(PAUSE_MARKER),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
            let text = if output.status.success() {
                &output.stdout
            } else {
                &output.stderr
            };
            assert!(
                String::from_utf8_lossy(text).contains(expected),
                "{marker:?}"
            );
        }
    }
    Ok(())
}

#[test]
fn readying_without_the_pause_lines_waits_until_reviews_resume() -> Result<()> {
    let mut fixture = Fixture::new(BODY)?;
    fixture.level = "critical";
    fixture.pause("paused-pr")?;
    let view = serde_json::json!({"title":"fix: preserve edits","body":BODY,"state":"OPEN","isDraft":true});
    let ready = |fixture: &Fixture| -> Result<Output> {
        Ok(fixture
            .command("ready")?
            .args(["--pr", "17"])
            .env("GH_FIXTURE_VIEW", view.to_string())
            .output()?)
    };
    let output = ready(&fixture)?;
    assert!(!output.status.success());
    let error = String::from_utf8_lossy(&output.stderr);
    assert!(error.contains("critical PR review is paused"), "{error}");
    fs::remove_file(
        fixture
            .directory
            .path()
            .join(".local/state/coderabbit-guard/paused-pr"),
    )?;
    let output = ready(&fixture)?;
    assert!(output.status.success());
    assert!(String::from_utf8_lossy(&output.stdout).contains("CodeRabbit review route: Pr"));
    Ok(())
}

#[test]
fn a_pr_readied_during_the_pause_owes_its_review_after_resumption() -> Result<()> {
    let mut fixture = Fixture::new(BODY)?;
    fixture.level = "critical";
    fixture.pause("paused-pr")?;
    let body = format!("{BODY}\n@coderabbitai ignore\n");
    let view = |draft: bool| {
        serde_json::json!({"title":"fix: preserve edits","body":body,"state":"OPEN","isDraft":draft})
            .to_string()
    };
    let output = fixture
        .command("ready")?
        .args(["--pr", "17"])
        .env("GH_FIXTURE_VIEW", view(true))
        .output()?;
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("critical PR review is paused"));
    fs::remove_file(
        fixture
            .directory
            .path()
            .join(".local/state/coderabbit-guard/paused-pr"),
    )?;
    let output = fixture
        .command("check")?
        .args(["--pr", "17", "--final"])
        .env("GH_FIXTURE_VIEW", view(false))
        .output()?;
    assert!(!output.status.success());
    let stdout = String::from_utf8_lossy(&output.stderr);
    assert!(
        stdout.contains("review") || stdout.contains("CodeRabbit"),
        "{stdout}"
    );
    Ok(())
}

/// Edits critical PR 17 under the owner's pause `marker`, starting from `live` and an optional recorded pause exclusion.
/// Returns whether the edit passed, its standard error, whether gh received it, and whether the pause record remains.
fn edit_during_pause(
    marker: Option<&str>,
    draft: bool,
    live: &str,
    recorded: bool,
    edited: &str,
) -> Result<(bool, String, bool, bool)> {
    let mut fixture = Fixture::new(edited)?;
    fixture.level = "critical";
    if let Some(marker) = marker {
        fixture.pause(marker)?;
    }
    let record = fixture
        .directory
        .path()
        .join(".local/state/pr-workflow/pauses/owner__project/pr-17");
    if recorded {
        fs::create_dir_all(record.parent().context("pause record directory")?)?;
        fs::write(&record, "")?;
    }
    let view = serde_json::json!({"title":"fix: preserve edits","body":live,"state":"OPEN","isDraft":draft});
    let output = fixture
        .command("edit")?
        .args(["--title", "fix: preserve edits", "--body-file"])
        .arg(&fixture.body)
        .args(["--pr", "17"])
        .env("GH_FIXTURE_VIEW", view.to_string())
        .output()?;
    Ok((
        output.status.success(),
        String::from_utf8_lossy(&output.stderr).into_owned(),
        fs::read_to_string(&fixture.log)
            .unwrap_or_default()
            .contains("\nedit\n"),
        record.try_exists()?,
    ))
}

#[test]
fn an_edit_cannot_restore_review_of_a_ready_pr_during_the_pause() -> Result<()> {
    let excluded = format!("{BODY}\n@coderabbitai ignore\n");
    // Each row: the owner's pause, a draft, the live body, a recorded pause exclusion, the edited body, and acceptance.
    for (marker, draft, live, recorded, edited, accepted) in [
        (
            Some("paused-pr"),
            false,
            excluded.as_str(),
            true,
            excluded.as_str(),
            true,
        ),
        (
            Some("paused-pr"),
            true,
            excluded.as_str(),
            true,
            excluded.as_str(),
            true,
        ),
        (
            None,
            false,
            excluded.as_str(),
            true,
            excluded.as_str(),
            true,
        ),
        (
            Some("paused-pr"),
            false,
            excluded.as_str(),
            false,
            excluded.as_str(),
            true,
        ),
        (
            Some("paused-pr"),
            false,
            excluded.as_str(),
            true,
            BODY,
            false,
        ),
        (
            Some("paused-pr"),
            false,
            excluded.as_str(),
            false,
            BODY,
            false,
        ),
        (Some("paused"), false, excluded.as_str(), false, BODY, false),
        (Some("paused-pr"), true, excluded.as_str(), true, BODY, true),
        (Some("paused-pr"), false, BODY, false, BODY, true),
        (
            Some("paused-cli"),
            false,
            excluded.as_str(),
            false,
            BODY,
            true,
        ),
        (None, false, excluded.as_str(), true, BODY, true),
    ] {
        let (passed, error, published, remains) =
            edit_during_pause(marker, draft, live, recorded, edited)?;
        assert_eq!(passed, accepted, "{marker:?} {draft} {recorded} {error}");
        assert_eq!(published, accepted);
        if accepted {
            let paused_pr = matches!(marker, Some("paused" | "paused-pr"));
            assert_eq!(
                remains,
                edited.contains("@coderabbitai ignore") && (paused_pr || recorded),
                "{marker:?} {draft} {recorded}: the edit keeps a pause record only while the body excludes review"
            );
        } else {
            assert!(
                error.contains("PR reviews are paused") && error.contains("keep the standalone"),
                "{error}"
            );
        }
    }
    Ok(())
}

#[test]
fn a_body_carrying_local_workflow_evidence_never_reaches_github() -> Result<()> {
    for marker in [
        "<!-- pr-review-assessment: {\"object\":\"0\",\"raw\":\"r\"} -->",
        PAUSE_MARKER,
    ] {
        let fixture = Fixture::new(&format!("{BODY}\n{marker}\n"))?;
        let output = fixture.document("create", "fix: preserve edits")?;
        assert!(!output.status.success());
        assert_eq!(rule(&output), "pr.local-evidence");
        assert_eq!(
            mutations(&fs::read_to_string(&fixture.log).unwrap_or_default()),
            [] as [String; 0]
        );
    }
    Ok(())
}

#[test]
fn local_create_is_draft_and_passes_the_checked_body_by_file() -> Result<()> {
    let fixture = Fixture::new(BODY)?;
    let output = fixture.document("create", "fix: preserve `$(literal)` edits")?;
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let log = fixture.log()?;
    assert!(log.contains("\n--draft\n"));
    assert!(log.contains("fix: preserve `$(literal)` edits"));
    assert!(log.contains(BODY));
    assert!(!log.contains(fixture.body.to_str().unwrap()));
    Ok(())
}

/// A user at the default limit of three open PRs adds new work to the oldest instead of opening another.
/// PRs of other users and PRs from the same branch never count.
#[test]
fn create_refuses_a_second_open_pr_by_the_same_author_and_names_the_first() -> Result<()> {
    let fixture = Fixture::new(BODY)?;
    let pr = |number: u64, login: &str, head: &str| {
        serde_json::json!({
            "number": number,
            "html_url": format!("https://github.com/owner/project/pull/{number}"),
            "user": {"login": login},
            "head": {"ref": head}
        })
    };
    let create = |pulls: &serde_json::Value| -> Result<Output> {
        Ok(fixture
            .command("create")?
            .args(["--title", "fix: preserve edits", "--body-file"])
            .arg(&fixture.body)
            .args(["--head", "feature"])
            .env("GH_FIXTURE_PULLS", pulls.to_string())
            .output()?)
    };
    let output = create(&serde_json::json!([
        pr(9, "Other", "theirs"),
        pr(12, "P4suta", "feature"),
        pr(13, "P4suta", "second"),
        pr(15, "P4suta", "third")
    ]))?;
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    fs::remove_file(&fixture.log)?;
    let output = create(&serde_json::json!([
        pr(14, "P4suta", "later"),
        pr(11, "p4suta", "earlier"),
        pr(16, "P4suta", "latest")
    ]))?;
    let record = refusal(&output);
    assert_eq!(record["rule"], "pr.wip");
    assert_eq!(
        record["next"],
        "git switch earlier && git merge --no-edit feature && git push"
    );
    let evidence = record["evidence"].to_string();
    assert!(
        evidence.contains("#11 https://github.com/owner/project/pull/11 (earlier)")
            && evidence.contains("#14"),
        "{evidence}"
    );
    assert!(!fixture.log()?.contains("\ncreate\n"));
    Ok(())
}

#[test]
fn coderabbit_request_is_explicit_and_final_checks_refuse_pending_generation() -> Result<()> {
    let mut fixture =
        Fixture::new("@coderabbitai summary\n\n## Validation\nThe regression passed.\n")?;
    fixture.level = "critical";
    let output = fixture
        .command("create")?
        .args(["--title", "@coderabbitai", "--body-file"])
        .arg(&fixture.body)
        .args(["--head", "feature", "--generation", "coderabbit"])
        .output()?;
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(fixture.log()?.contains("--draft"));
    assert!(!fixture.document("check", "@coderabbitai")?.status.success());
    let final_check = fixture
        .command("check")?
        .args(["--title", "@coderabbitai", "--body-file"])
        .arg(&fixture.body)
        .args(["--generation", "coderabbit", "--final"])
        .output()?;
    assert!(!final_check.status.success());
    fs::write(&fixture.body, "@coderabbitai summary\nTODO\n")?;
    let output = fixture
        .command("create")?
        .args(["--title", "@coderabbitai", "--body-file"])
        .arg(&fixture.body)
        .args(["--head", "feature", "--generation", "coderabbit"])
        .output()?;
    assert!(!output.status.success());
    Ok(())
}

#[test]
fn external_title_policy_requires_matching_repository_and_source() -> Result<()> {
    let fixture = Fixture::new(BODY)?;
    let policy = fixture.directory.path().join("policy.json");
    for value in [
        r#"{"repository":"other/project","source":"https://example.org/contributing","reason":"Upstream requires a sentence."}"#,
        r#"{"repository":"owner/project","source":"","reason":"Upstream requires a sentence."}"#,
    ] {
        fs::write(&policy, value)?;
        let output = fixture
            .command("create")?
            .args(["--title", "Preserve edits", "--body-file"])
            .arg(&fixture.body)
            .args(["--head", "feature", "--title-policy"])
            .arg(&policy)
            .output()?;
        assert!(!output.status.success());
        assert!(!fixture.log.try_exists()?);
    }
    fs::write(
        &policy,
        r#"{"repository":"owner/project","source":"https://example.org/contributing","reason":"Upstream requires a sentence."}"#,
    )?;
    let output = fixture
        .command("create")?
        .args(["--title", "Preserve edits", "--body-file"])
        .arg(&fixture.body)
        .args(["--head", "feature", "--title-policy"])
        .arg(&policy)
        .output()?;
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(fixture.log()?.contains("--draft"));
    Ok(())
}

#[test]
fn ready_checks_live_contents_and_draft_state_before_mutation() -> Result<()> {
    for (title, body, state, draft, accepted) in [
        ("fix: preserve edits", BODY, "OPEN", true, true),
        ("Repair edits", BODY, "OPEN", true, false),
        (
            "fix: preserve edits",
            "<!-- instructions -->",
            "OPEN",
            true,
            false,
        ),
        (
            "@coderabbitai",
            "@coderabbitai summary",
            "OPEN",
            true,
            false,
        ),
        ("fix: preserve edits", BODY, "CLOSED", true, false),
        ("fix: preserve edits", BODY, "OPEN", false, false),
    ] {
        let fixture = Fixture::new(BODY)?;
        let view = serde_json::json!({"title":title,"body":body,"state":state,"isDraft":draft});
        let output = fixture
            .command("ready")?
            .args(["--pr", "17"])
            .env("GH_FIXTURE_VIEW", view.to_string())
            .output()?;
        assert_eq!(
            output.status.success(),
            accepted,
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(fixture.log()?.contains("\nready\n"), accepted);
    }
    for (checks, accepted) in [
        (
            r#"[{"status":"COMPLETED","conclusion":"SUCCESS"},{"state":"SUCCESS"}]"#,
            true,
        ),
        (r#"[{"status":"COMPLETED","conclusion":"NEUTRAL"}]"#, true),
        ("[]", false),
        (
            r#"[{"status":"COMPLETED","conclusion":"SUCCESS"},{"status":"IN_PROGRESS","conclusion":null}]"#,
            false,
        ),
        (r#"[{"status":"COMPLETED","conclusion":"FAILURE"}]"#, false),
        (
            r#"[{"status":"COMPLETED","conclusion":"CANCELLED"}]"#,
            false,
        ),
        (r#"[{"status":"COMPLETED","conclusion":"SKIPPED"}]"#, false),
        (r#"[{"state":"PENDING"}]"#, false),
        (r#"[{"state":"ERROR"}]"#, false),
    ] {
        let fixture = Fixture::new(BODY)?;
        let view = serde_json::json!({"title":"fix: preserve edits","body":BODY,"state":"OPEN","isDraft":true});
        let output = fixture
            .command("ready")?
            .args(["--pr", "17"])
            .env("GH_FIXTURE_VIEW", view.to_string())
            .env("GH_FIXTURE_CHECKS", checks)
            .output()?;
        assert_eq!(output.status.success(), accepted, "{checks}");
        assert_eq!(fixture.log()?.contains("\nready\n"), accepted, "{checks}");
    }
    let mut fixture = Fixture::new(BODY)?;
    fixture.level = "critical";
    let view = serde_json::json!({"title":"@coderabbitai","body":"@coderabbitai summary","state":"OPEN","isDraft":true});
    assert!(
        fixture
            .command("ready")?
            .args(["--pr", "17", "--generation", "coderabbit"])
            .env("GH_FIXTURE_VIEW", view.to_string())
            .output()?
            .status
            .success()
    );
    Ok(())
}

#[test]
fn ready_requires_every_required_check_of_the_base_to_pass() -> Result<()> {
    let rules = r#"[{"type":"required_status_checks","parameters":{"required_status_checks":[{"context":"required (ubuntu-latest)"},{"context":"lint"}]}},{"type":"deletion","parameters":null}]"#;
    for (checks, accepted) in [
        (
            r#"[{"name":"required (ubuntu-latest)","status":"COMPLETED","conclusion":"SUCCESS"},{"context":"lint","state":"SUCCESS"}]"#,
            true,
        ),
        (
            r#"[{"name":"required (ubuntu-latest)","status":"COMPLETED","conclusion":"SUCCESS"}]"#,
            false,
        ),
        (
            r#"[{"name":"other","status":"COMPLETED","conclusion":"SUCCESS"}]"#,
            false,
        ),
    ] {
        let fixture = Fixture::new(BODY)?;
        let view = serde_json::json!({"title":"fix: preserve edits","body":BODY,"state":"OPEN","isDraft":true});
        let output = fixture
            .command("ready")?
            .args(["--pr", "17"])
            .env("GH_FIXTURE_VIEW", view.to_string())
            .env("GH_FIXTURE_CHECKS", checks)
            .env("GH_FIXTURE_RULES", rules)
            .output()?;
        assert_eq!(output.status.success(), accepted, "{checks}");
        assert_eq!(fixture.log()?.contains("\nready\n"), accepted, "{checks}");
        assert!(
            fixture
                .log()?
                .contains("repos/owner/project/rules/branches/main")
        );
        if !accepted {
            assert_eq!(rule(&output), "pr.checks");
        }
    }
    Ok(())
}

#[test]
fn a_push_between_the_read_and_ready_returns_the_pr_to_draft() -> Result<()> {
    let fixture = Fixture::new(BODY)?;
    let view = serde_json::json!({"title":"fix: preserve edits","body":BODY,"state":"OPEN","isDraft":true});
    let output = fixture
        .command("ready")?
        .args(["--pr", "17"])
        .env("GH_FIXTURE_VIEW", view.to_string())
        .env(
            "GH_FIXTURE_PUSHED_HEAD",
            "0000000000000000000000000000000000000001",
        )
        .output()?;
    assert!(!output.status.success());
    let record = refusal(&output);
    assert_eq!(record["rule"], "pr.head");
    let log = fixture.log()?;
    let marked = log
        .find("\nready\n--repo\nowner/project\n17\n")
        .context("ready")?;
    let undone = log.find("\n17\n--undo\n").context("undo")?;
    assert!(marked < undone, "{log}");
    let fixture = Fixture::new(BODY)?;
    let output = fixture
        .command("ready")?
        .args(["--pr", "17"])
        .env("GH_FIXTURE_VIEW", view.to_string())
        .output()?;
    assert!(output.status.success());
    assert!(!fixture.log()?.contains("--undo"));
    Ok(())
}

#[test]
fn the_repository_owner_can_assess_a_bot_pull_request() -> Result<()> {
    let body = format!("{BODY}\nCloses #23.\n");
    let fixture = Fixture::new(&body)?;
    let branch = git(fixture.directory.path(), &["branch", "--show-current"])?;
    let mut command = fixture.personal_command("edit")?;
    let output = command
        .args(["--title", "fix: preserve edits", "--body-file"])
        .arg(&fixture.body)
        .args(["--pr", "17", "--issue", "23"])
        .env(
            "GH_FIXTURE_VIEW",
            serde_json::json!({
                "title":"fix: preserve edits", "body":body,
                "state":"OPEN", "isDraft":true,
                "author":{"login":"app/renovate","is_bot":true},
                "headRefName":branch, "headRepository":{"name":"project"},
                "headRepositoryOwner":{"login":"P4suta"},
                "isCrossRepository":false
            })
            .to_string(),
        )
        .output()?;
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let log = fixture.log()?;
    assert!(log.contains("users/P4suta/ssh_signing_keys"));
    assert!(!log.contains("users/app/renovate/ssh_signing_keys"));
    assert!(log.contains("\nedit\n17\n"));
    Ok(())
}

#[test]
fn bot_assessments_refuse_other_accounts_and_cross_repository_sources() -> Result<()> {
    for (repo, source) in [
        ("owner/project", Some(false)),
        ("P4suta/project", Some(true)),
        ("P4suta/project", None),
    ] {
        let fixture = Fixture::new(BODY)?;
        let mut command = fixture.command_for_repo("edit", repo)?;
        let output = command
            .args(["--title", "fix: preserve edits", "--body-file"])
            .arg(&fixture.body)
            .args(["--pr", "17"])
            .env(
                "GH_FIXTURE_VIEW",
                serde_json::json!({
                    "title":"fix: preserve edits", "body":BODY,
                    "state":"OPEN", "isDraft":true,
                    "author":{"login":"app/renovate","is_bot":true},
                    "isCrossRepository":source
                })
                .to_string(),
            )
            .output()?;
        assert!(!output.status.success());
        let error = String::from_utf8_lossy(&output.stderr);
        assert!(
            error.contains("bot assessments require the authenticated repository owner"),
            "{error}"
        );
        assert!(!fixture.log()?.contains("\nedit\n17\n"));
    }
    Ok(())
}

#[test]
fn a_human_pull_request_keeps_the_original_signer() -> Result<()> {
    let fixture = Fixture::new(BODY)?;
    let mut command = fixture.command_for_repo("edit", "P4suta/project")?;
    let output = command
        .args(["--title", "fix: preserve edits", "--body-file"])
        .arg(&fixture.body)
        .args(["--pr", "17"])
        .env(
            "GH_FIXTURE_VIEW",
            serde_json::json!({
                "title":"fix: preserve edits", "body":BODY,
                "state":"OPEN", "isDraft":true,
                "author":{"login":"other-user","is_bot":false},
                "isCrossRepository":false
            })
            .to_string(),
        )
        .output()?;
    assert!(!output.status.success());
    let log = fixture.log()?;
    assert!(log.contains("users/other-user/ssh_signing_keys"));
    assert!(!log.contains("\nedit\n17\n"));
    Ok(())
}

#[test]
fn edit_and_gh_failure_preserve_one_attempt_and_report_the_failure() -> Result<()> {
    for operation in ["create", "edit"] {
        let fixture = Fixture::new(BODY)?;
        let mut command = fixture.command(operation)?;
        command
            .args(["--title", "fix: preserve edits", "--body-file"])
            .arg(&fixture.body);
        if operation == "create" {
            command.args(["--head", "feature"]);
        } else {
            command.args(["--pr", "17"]);
        }
        let output = command.env("GH_FIXTURE_FAILURE", operation).output()?;
        assert!(!output.status.success());
        assert!(String::from_utf8_lossy(&output.stderr).contains("42"));
        // Creation also reads the open PRs of the user for the work-in-progress limit and lists the open PRs it could depend on.
        assert_eq!(fixture.log()?.matches("\nuser\n").count(), 1);
        assert_eq!(
            fixture.log()?.matches(&format!("\n{operation}\n")).count(),
            1
        );
    }
    let fixture = Fixture::new(BODY)?;
    assert!(
        fixture
            .document("edit", "fix: preserve edits")?
            .status
            .success()
    );
    assert!(fixture.log()?.contains("\nedit\n17\n"));
    Ok(())
}

#[test]
fn live_final_check_is_read_only_and_rejects_malformed_or_pending_documents() -> Result<()> {
    for (response, accepted) in [
        (serde_json::json!({"title":"fix: preserve edits", "body":BODY,"state":"OPEN","isDraft":false}).to_string(), true),
        (serde_json::json!({"title":"@coderabbitai", "body":"@coderabbitai summary","state":"OPEN","isDraft":false}).to_string(), false),
        ("{truncated".into(), false),
        (r#"{"title":"fix: preserve edits","body":null,"state":"OPEN","isDraft":true}"#.into(), false),
    ] {
        let fixture = Fixture::new(BODY)?;
        let output = fixture.command("check")?.args(["--pr", "17", "--generation", "local", "--final"]).env("GH_FIXTURE_VIEW", response).output()?;
        assert_eq!(output.status.success(), accepted);
        assert_eq!(fixture.log()?.matches("\nuser\n").count(), 1);
        assert!(fixture.log()?.contains("\nview\n"));
    }
    for failure in ["view", "ready"] {
        let fixture = Fixture::new(BODY)?;
        let view = serde_json::json!({"title":"fix: preserve edits","body":BODY,"state":"OPEN","isDraft":true});
        let output = fixture
            .command("ready")?
            .args(["--pr", "17"])
            .env("GH_FIXTURE_VIEW", view.to_string())
            .env("GH_FIXTURE_FAILURE", failure)
            .output()?;
        assert!(!output.status.success());
        assert!(String::from_utf8_lossy(&output.stderr).contains("42"));
        assert_eq!(fixture.log()?.matches(&format!("\n{failure}\n")).count(), 1);
    }
    Ok(())
}

#[test]
fn standing_suppression_never_hides_an_unresolved_outdated_bot_thread() -> Result<()> {
    let fixture = Fixture::new(BODY)?;
    let view = serde_json::json!({"title":"fix: preserve edits","body":BODY,"state":"OPEN","isDraft":true});
    for (resolved, accepted) in [(false, false), (true, true)] {
        let graph = serde_json::json!({"data":{"repository":{"pullRequest":{
            "headRefOid":fixture.head,"reviewThreads":{"pageInfo":{"hasNextPage":false,"endCursor":null},"nodes":[{
                "id":"thread-fixture-1","isResolved":resolved,"isOutdated":true,
                "comments":{"pageInfo":{"hasNextPage":false,"endCursor":null},"nodes":[{"author":{"login":"coderabbitai"},"body":"The supported invariant still needs its fix."}]}
            }]}
        }}}});
        let output = fixture
            .command("ready")?
            .args(["--pr", "17"])
            .env("GH_FIXTURE_VIEW", view.to_string())
            .env("GH_FIXTURE_GRAPHQL", graph.to_string())
            .output()?;
        assert_eq!(
            output.status.success(),
            accepted,
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        if !accepted {
            assert!(!fixture.log()?.contains("\nready\n"));
            assert!(String::from_utf8_lossy(&output.stderr).contains("unresolved"));
        }
    }
    Ok(())
}

#[test]
fn body_input_changes_and_cleanup_cannot_change_checked_publication_bytes() -> Result<()> {
    let fixture = Fixture::new(BODY)?;
    let output = fixture
        .command("create")?
        .args(["--title", "fix: preserve edits", "--body-file"])
        .arg(&fixture.body)
        .args(["--head", "feature"])
        .env("GH_FIXTURE_REWRITE_FILE", &fixture.body)
        .output()?;
    assert!(output.status.success());
    assert_eq!(
        fs::read_to_string(&fixture.body)?,
        "changed after validation"
    );
    let log = fixture.log()?;
    assert!(log.contains(BODY));
    let lines: Vec<_> = log.lines().collect();
    let index = lines
        .iter()
        .position(|line| *line == "--body-file")
        .unwrap();
    assert!(!std::path::Path::new(lines[index + 1]).try_exists()?);
    Ok(())
}

#[test]
fn invalid_repository_missing_files_and_invalid_cli_never_start_gh() -> Result<()> {
    for repo in [
        "project",
        "https://github.com/owner/project",
        "owner/project/other",
        "../project",
        "owner/--help",
    ] {
        let fixture = Fixture::new(BODY)?;
        let output = fixture
            .command_for_repo("create", repo)?
            .args(["--title", "fix: preserve edits", "--body-file"])
            .arg(&fixture.body)
            .args(["--head", "feature"])
            .output()?;
        assert!(!output.status.success());
        assert!(!fixture.log.try_exists()?);
    }
    let fixture = Fixture::new(BODY)?;
    fs::remove_file(&fixture.body)?;
    assert!(
        !fixture
            .document("create", "fix: preserve edits")?
            .status
            .success()
    );
    assert!(!fixture.log.try_exists()?);
    assert!(
        !fixture
            .command("ready")?
            .args(["--pr", "0"])
            .output()?
            .status
            .success()
    );
    assert!(!fixture.log.try_exists()?);
    Ok(())
}

/// Installs every shared skill except pull-request, and one skill another owner manages, into `skills`.
fn install_other_skills(
    source: &std::path::Path,
    skills: &std::path::Path,
) -> Result<skill_ops::Catalog> {
    use skill_ops as ops;
    fs::create_dir_all(skills)?;
    let mut catalog = ops::catalog(&source.join("home/dot_agents/skills"))?;
    catalog.skills.remove("pull-request");
    for skill in catalog.skills.values() {
        for relative in &skill.files {
            let destination = skills.join(relative);
            fs::create_dir_all(destination.parent().unwrap())?;
            fs::copy(
                source.join("home/dot_agents/skills").join(relative),
                destination,
            )?;
        }
    }
    fs::create_dir(skills.join("other-owned"))?;
    fs::write(
        skills.join("other-owned/SKILL.md"),
        "---\nname: other-owned\ndescription: Inspect an independent existing skill.\n---\n# Independent\nPreserve this skill.\n",
    )?;
    ops::catalog(skills)
}

fn apply_pr_skill(
    source: &std::path::Path,
    skills: &std::path::Path,
    paths: &[PathBuf],
) -> Result<()> {
    for path in paths {
        if let Ok(relative) = path.strip_prefix(source.join("home/dot_agents/skills/pull-request"))
        {
            let destination = skills.join("pull-request").join(relative);
            fs::create_dir_all(destination.parent().context("fixture skill parent")?)?;
            fs::write(destination, fs::read(path)?)?;
        } else {
            assert_eq!(
                *path,
                source.join("home/dot_claude/skills/symlink_pull-request")
            );
        }
    }
    Ok(())
}

#[test]
fn scoped_install_preserves_other_managed_skills_policy_and_history() -> Result<()> {
    use skill_ops as ops;
    let source = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .unwrap();
    let home = tempfile::tempdir()?;
    let skills = home.path().join(".agents/skills");
    let catalog = install_other_skills(source, &skills)?;
    let policy: ops::Policy =
        ops::read_json(&source.join("home/dot_config/skill-ops/policy.json"))?;
    ops::write_json(
        &home.path().join(".config/skill-ops/policy.json"),
        &policy,
        false,
    )?;
    let manifest = home.path().join(".config/skill-ops/approved.json");
    ops::write_json(
        &manifest,
        &ops::Approved {
            catalog_hash: ops::hash(&catalog)?,
            catalog,
            policy_hash: ops::hash(&policy)?,
            engine_hash: "existing-native-engine".into(),
            reviewed: std::collections::BTreeMap::default(),
        },
        false,
    )?;
    let history = home.path().join(".local/state/skill-ops/event.json");
    ops::write_json(&history, &serde_json::json!({"preserved":true}), false)?;
    let binary = home.path().join("built-binary");
    fs::write(&binary, "native executable fixture")?;
    pr_workflow::deploy(source, &binary, home.path(), |paths| {
        apply_pr_skill(source, &skills, paths)
    })?;
    let installed: ops::Approved = ops::read_json(&manifest)?;
    assert!(installed.catalog.skills.contains_key("other-owned"));
    assert!(installed.catalog.skills.contains_key("pull-request"));
    assert_eq!(installed.engine_hash, "existing-native-engine");
    assert!(installed.reviewed.is_empty());
    assert_eq!(installed.policy_hash, ops::hash(&policy)?);
    assert_eq!(
        ops::read_json::<serde_json::Value>(&history)?,
        serde_json::json!({"preserved":true})
    );
    assert_eq!(
        fs::read(
            home.path()
                .join(".local/bin")
                .join(format!("pr-workflow{}", std::env::consts::EXE_SUFFIX))
        )?,
        fs::read(&binary)?
    );
    pr_workflow::deploy(source, &binary, home.path(), |paths| {
        apply_pr_skill(source, &skills, paths)
    })?;
    assert_eq!(
        ops::read_json::<ops::Approved>(&manifest)?.catalog,
        installed.catalog
    );
    assert_eq!(
        ops::read_json::<serde_json::Value>(&history)?,
        serde_json::json!({"preserved":true})
    );
    let before = fs::read(&manifest)?;
    assert!(
        pr_workflow::deploy(source, &binary, home.path(), |_| anyhow::bail!(
            "injected apply failure"
        ))
        .is_err()
    );
    assert_eq!(fs::read(&manifest)?, before);
    fs::write(skills.join("other-owned/SKILL.md"), "changed locally")?;
    assert!(
        pr_workflow::deploy(source, &binary, home.path(), |_| panic!(
            "must not apply over unrelated changes"
        ))
        .is_err()
    );
    Ok(())
}

fn scoped_install_fixture() -> Result<(tempfile::TempDir, tempfile::TempDir, PathBuf)> {
    use skill_ops as ops;
    let source = tempfile::tempdir()?;
    let home = tempfile::tempdir()?;
    for name in ["pull-request", "base-skill"] {
        let skill = source.path().join("home/dot_agents/skills").join(name);
        fs::create_dir_all(&skill)?;
        fs::write(
            skill.join("SKILL.md"),
            format!(
                "---\nname: {name}\ndescription: Inspect the fixture.\n---\n# Fixture\nPreserve the source.\n"
            ),
        )?;
        let alias = source.path().join("home/dot_claude/skills");
        fs::create_dir_all(&alias)?;
        fs::write(
            alias.join(format!("symlink_{name}")),
            format!("../../.agents/skills/{name}"),
        )?;
    }
    let policy = ops::Policy {
        schema: 1,
        review_every: 50,
        minimum_pair_sessions: 5,
        pair_percent: 80,
        max_entry_words: 1200,
        requirements: Vec::new(),
        bundles: std::collections::BTreeMap::default(),
    };
    ops::write_json(
        &source.path().join("home/dot_config/skill-ops/policy.json"),
        &policy,
        false,
    )?;
    ops::write_json(
        &home.path().join(".config/skill-ops/policy.json"),
        &policy,
        false,
    )?;
    let catalog = install_other_skills(source.path(), &home.path().join(".agents/skills"))?;
    let approved = ops::Approved {
        catalog_hash: ops::hash(&catalog)?,
        catalog,
        policy_hash: ops::hash(&policy)?,
        engine_hash: "existing-native-engine".into(),
        reviewed: [(
            "pull-request".into(),
            ops::Reviewed {
                revision: "prior-reviewed-revision".into(),
                size: false,
            },
        )]
        .into(),
    };
    ops::write_json(
        &home.path().join(".config/skill-ops/approved.json"),
        &approved,
        false,
    )?;
    let binary = source.path().join("built-binary");
    fs::write(&binary, "native executable fixture")?;
    Ok((source, home, binary))
}

#[test]
fn scoped_install_ignores_experimental_records_without_opt_in() -> Result<()> {
    use skill_ops as ops;
    let (source, home, binary) = scoped_install_fixture()?;
    let decisions = source.path().join(ops::DECISIONS);
    fs::create_dir_all(&decisions)?;
    fs::write(
        decisions.join("pull-request.json"),
        "invalid experimental record",
    )?;
    let manifest = home.path().join(".config/skill-ops/approved.json");
    let before = ops::read_approved(&manifest)?;
    assert!(!platform::incidents::enabled_at(home.path()));
    pr_workflow::deploy(source.path(), &binary, home.path(), |paths| {
        apply_pr_skill(source.path(), &home.path().join(".agents/skills"), paths)
    })?;
    let after = ops::read_approved(&manifest)?;
    assert!(after.catalog.skills.contains_key("pull-request"));
    assert_eq!(after.reviewed, before.reviewed);
    Ok(())
}

#[test]
fn scoped_install_requires_experimental_decisions_only_after_opt_in() -> Result<()> {
    let (source, home, binary) = scoped_install_fixture()?;
    fs::write(
        home.path().join(platform::incidents::EXPERIMENTAL),
        rules::incident_rules::EXPERIMENTAL_OPT_IN,
    )?;
    assert!(platform::incidents::enabled_at(home.path()));
    let manifest = home.path().join(".config/skill-ops/approved.json");
    let before = fs::read(&manifest)?;
    let error = pr_workflow::deploy(source.path(), &binary, home.path(), |_| {
        panic!("must not apply before experimental decision checks")
    })
    .unwrap_err();
    assert!(
        error.to_string().contains("pull-request has no decision"),
        "{error:#}"
    );
    assert_eq!(fs::read(&manifest)?, before);
    fs::write(
        home.path().join(platform::incidents::DISABLED),
        "owner stop",
    )?;
    assert!(!platform::incidents::enabled_at(home.path()));
    pr_workflow::deploy(source.path(), &binary, home.path(), |paths| {
        apply_pr_skill(source.path(), &home.path().join(".agents/skills"), paths)
    })?;
    Ok(())
}

/// A body that doesn't close the issue gets a refusal naming the edit that closes it, never the unchanged command.
#[test]
fn a_body_without_the_closing_reference_names_the_edit_that_adds_it() -> Result<()> {
    let fixture = Fixture::new(BODY)?;
    let create = |fixture: &Fixture| -> Result<Output> {
        Ok(fixture
            .personal_command("create")?
            .args([
                "--issue",
                "23",
                "--title",
                "fix: preserve edits",
                "--body-file",
            ])
            .arg(&fixture.body)
            .args(["--head", "feature"])
            .output()?)
    };
    let record = refusal(&create(&fixture)?);
    assert_eq!(record["rule"], "pr.body");
    let body = fixture.body.to_str().context("UTF-8 path")?;
    let quoted = platform::refusal::command(&[body]);
    let next = record["next"].as_str().unwrap();
    assert_eq!(
        next,
        format!(
            "printf '\\n\\nCloses #23\\n' >> {quoted} && pr-workflow create --repo P4suta/project --issue 23 --title 'fix: preserve edits' --body-file {quoted} --head feature"
        )
    );
    let mut appended = fs::read_to_string(&fixture.body)?;
    appended.push_str("\n\nCloses #23\n");
    fs::write(&fixture.body, appended)?;
    let output = create(&fixture)?;
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );

    let fixture = Fixture::new(BODY)?;
    let view = serde_json::json!({"title":"fix: preserve edits","body":BODY,"state":"OPEN","isDraft":true});
    let output = fixture
        .personal_command("ready")?
        .args(["--issue", "23", "--pr", "17"])
        .env("GH_FIXTURE_VIEW", view.to_string())
        .output()?;
    let record = refusal(&output);
    assert_eq!(record["rule"], "pr.body");
    assert_eq!(
        record["next"],
        "gh pr edit 17 --repo P4suta/project --body-file <body-file>"
    );
    Ok(())
}

fn next_host_generation(root: &std::path::Path) -> Result<u64> {
    let count: u64 = git(root, &["rev-list", "--count", dotfiles::hosts::NOTES])?.parse()?;
    dotfiles::hosts_rules::next_generation(count).context("fixture generation")
}

#[test]
fn ready_requires_a_passing_note_for_every_os_family_in_the_ci_matrix() -> Result<()> {
    use dotfiles::hosts::Stage::{Code, Real};

    let mut fixture = Fixture::new(BODY)?;
    let root = fixture.directory.path().to_path_buf();
    fs::create_dir_all(root.join(".github/workflows"))?;
    fs::create_dir_all(root.join("crates/dotfiles"))?;
    fs::write(root.join(".chezmoiroot"), "home\n")?;
    fs::write(
        root.join("crates/dotfiles/Cargo.toml"),
        "[package]\nname = 'dotfiles'\n",
    )?;
    fs::write(
        root.join(".github/workflows/ci.yml"),
        "on: pull_request\njobs:\n  test:\n    strategy:\n      matrix:\n        os: [ubuntu-latest, windows-latest]\n    runs-on: ${{ matrix.os }}\n    steps: []\n",
    )?;
    git(&root, &["add", ".github", ".chezmoiroot", "crates"])?;
    git(&root, &["commit", "--quiet", "-m", "ci"])?;
    fixture.head = git(&root, &["rev-parse", "HEAD"])?;
    let tree = git(&root, &["rev-parse", "HEAD^{tree}"])?;
    let view = serde_json::json!({"title":"fix: preserve edits","body":BODY,"state":"OPEN","isDraft":true});
    let ready = || -> Result<Output> {
        Ok(fixture
            .command("ready")?
            .args(["--pr", "17"])
            .env("GH_FIXTURE_VIEW", view.to_string())
            .output()?)
    };
    let note = |os: &str, stage: Option<dotfiles::hosts::Stage>, status: i32| -> Result<()> {
        let record = dotfiles::hosts::Note {
            host: "fixture".to_owned(),
            os: os.to_owned(),
            tree: tree.clone(),
            status,
            gate: dotfiles::hosts::DEFAULT_GATE.to_owned(),
            stage,
            applied: None,
            inputs: stage.map(|stage| {
                let index = match stage {
                    Code => 0,
                    Real => 1,
                };
                dotfiles::hosts::input_hashes(&root, "HEAD", dotfiles::hosts::DEFAULT_GATE).unwrap()
                    [index]
                    .clone()
            }),
            generation: stage.map(|_| next_host_generation(&root)).transpose()?,
            reused: None,
            workspace: None,
            capture: None,
        };
        git(
            &root,
            &[
                "notes",
                "--ref",
                "refs/notes/hosts",
                "append",
                "-m",
                &serde_json::to_string(&record)?,
                "HEAD",
            ],
        )?;
        Ok(())
    };
    for (os, stage, status, refusal) in [
        (None, None, 0, "no linux gate result"),
        (Some("linux"), None, 0, "no linux gate result"),
        (Some("linux"), Some(Code), 0, "no linux gate result"),
        (Some("linux"), Some(Real), 0, "no windows gate result"),
        (Some("windows"), Some(Code), 0, "no windows gate result"),
        (Some("windows"), Some(Real), 1, "the windows gate failed"),
    ] {
        if let Some(os) = os {
            note(os, stage, status)?;
        }
        let output = ready()?;
        assert!(!output.status.success());
        let error = String::from_utf8_lossy(&output.stderr);
        assert!(
            error.contains(refusal) && error.contains("hosts check"),
            "{error}"
        );
        assert!(!fixture.log()?.contains("\nready\n"));
    }
    note("windows", Some(Real), 0)?;
    let output = ready()?;
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(fixture.log()?.contains("\nready\n"));
    Ok(())
}

#[test]
fn ready_refuses_a_pr_that_adds_an_unowned_parser_until_its_fuzz_target_exists() -> Result<()> {
    let mut fixture = Fixture::new(BODY)?;
    let root = fixture.directory.path().to_path_buf();
    fs::create_dir_all(root.join("src"))?;
    fs::write(
        root.join("src/lib.rs"),
        "pub fn parse(text: &str) -> bool {\n    text.is_empty()\n}\n",
    )?;
    git(&root, &["add", "src"])?;
    git(&root, &["commit", "--quiet", "-m", "parser"])?;
    let view = serde_json::json!({"title":"fix: preserve edits","body":BODY,"state":"OPEN","isDraft":true});
    let ready = |fixture: &Fixture| -> Result<Output> {
        Ok(fixture
            .command("ready")?
            .args(["--pr", "17"])
            .env("GH_FIXTURE_HEAD", &fixture.head)
            .env("GH_FIXTURE_VIEW", view.to_string())
            .output()?)
    };
    fixture.head = git(&root, &["rev-parse", "HEAD"])?;
    let output = ready(&fixture)?;
    let error = String::from_utf8_lossy(&output.stderr);
    assert!(!output.status.success());
    assert!(
        error.contains("assure.added") && error.contains("src/lib.rs:1: parser has no owner"),
        "{error}"
    );
    assert!(!fixture.log()?.contains("\nready\n"));
    fs::create_dir_all(root.join("fuzz/fuzz_targets"))?;
    fs::create_dir_all(root.join("fuzz/corpus/parse"))?;
    fs::write(
        root.join("fuzz/fuzz_targets/parse.rs"),
        "#![no_main]\nlibfuzzer_sys::fuzz_target!(|data: &[u8]| {});\n",
    )?;
    fs::write(root.join("fuzz/corpus/parse/seed"), "")?;
    fs::write(
        root.join("assure.toml"),
        "[[owner]]\nshape = \"parser\"\npath = \"src/lib.rs\"\nby = \"fuzz/fuzz_targets/parse.rs\"\n",
    )?;
    git(&root, &["add", "fuzz", "assure.toml"])?;
    git(&root, &["commit", "--quiet", "-m", "own the parser"])?;
    fixture.head = git(&root, &["rev-parse", "HEAD"])?;
    let output = ready(&fixture)?;
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(fixture.log()?.contains("\nready\n"));
    Ok(())
}

const PASSING: &str = r#"[{"status":"COMPLETED","conclusion":"SUCCESS"}]"#;
const FAILING: &str = r#"[{"status":"COMPLETED","conclusion":"FAILURE"}]"#;

#[test]
fn the_owner_can_ready_and_merge_a_bot_pull_request() -> Result<()> {
    let body = format!("{BODY}\nCloses #23.\n");
    let fixture = Fixture::new(&body)?;
    let views = [
        (true, "DRAFT"),
        (true, "DRAFT"),
        (false, "CLEAN"),
        (false, "CLEAN"),
        (false, "MERGED"),
    ]
    .into_iter()
    .map(|(draft, state)| -> Result<String> {
        let original = read(17, &fixture.head, draft, &body, PASSING, state);
        let (_, encoded) = original.split_once('=').context("fixture view")?;
        let mut view: serde_json::Value = serde_json::from_str(encoded)?;
        view.as_object_mut().context("fixture object")?.extend(
            serde_json::json!({
                "author":{"login":"app/renovate","is_bot":true},
                "headRefName":"feature","headRepository":{"name":"project"},
                "headRepositoryOwner":{"login":"P4suta"},"isCrossRepository":false
            })
            .as_object()
            .context("bot fixture")?
            .clone(),
        );
        Ok(format!("17={view}"))
    })
    .collect::<Result<Vec<_>>>()?
    .join("\n");
    let output = fixture
        .personal_command("merge")?
        .args(["--pr", "17", "--interval", "0"])
        .env("GH_FIXTURE_VIEWS", views)
        .output()?;
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let log = fixture.log()?;
    assert_eq!(mutations(&log), ["ready", "merge"]);
    assert!(!log.contains("users/app/renovate/ssh_signing_keys"));
    assert!(log.contains(&format!("\n--match-head-commit\n{}\n", fixture.head)));
    Ok(())
}

/// One read of a PR for `GH_FIXTURE_VIEWS`.
/// `merge` holds the merge state of GitHub, or `MERGED` for a merged PR.
fn read(number: u64, head: &str, draft: bool, body: &str, checks: &str, merge: &str) -> String {
    let checks: serde_json::Value = serde_json::from_str(checks).unwrap();
    format!(
        "{number}={}",
        serde_json::json!({
            "title":"fix: preserve edits","body":body,
            "state":if merge == "MERGED" {"MERGED"} else {"OPEN"},"isDraft":draft,
            "headRefOid":head,"baseRefName":if number == 17 {"parent"} else {"main"},
            "mergeable":if merge == "DIRTY" {"CONFLICTING"} else {"MERGEABLE"},
            "mergeStateStatus":merge,"statusCheckRollup":checks,
            "closingIssuesReferences":[{"number":23}]
        })
    )
}

/// The PR mutations the log records, in order.
fn mutations(log: &str) -> Vec<String> {
    log.split("invocation\n")
        .filter_map(|call| {
            let words: Vec<&str> = call.lines().collect();
            (words.first() == Some(&"pr")
                && words
                    .get(1)
                    .is_some_and(|word| ["update-branch", "edit", "ready", "merge"].contains(word)))
            .then(|| words[1].to_owned())
        })
        .collect()
}

#[test]
fn independent_prs_merge_in_order_through_update_checks_ready_and_the_verified_head() -> Result<()>
{
    let closed = format!("{BODY}\nCloses #23.\n");
    let excluded = format!("{closed}\n@coderabbitai ignore\n");
    let fixture = Fixture::new(BODY)?;
    fixture.pause("paused-pr")?;
    // The updated head sits in this checkout, so `ready` can read its OS family evidence.
    // Its tree runs no CI.
    let new = fixture.head.as_str();
    let old = fixture.sibling_head("prior fixture head")?;
    let other = fixture.sibling_head("second fixture PR head")?;
    let records = fixture.assessment_records(
        fixture.directory.path(),
        "P4suta/project",
        &[
            (new, "origin/main"),
            (&old, "origin/main"),
            (&other, "origin/main"),
        ],
    )?;
    let views = [
        read(17, &old, true, &closed, PASSING, "DRAFT"),
        read(17, new, true, &closed, "[]", "DRAFT"),
        read(17, new, true, &closed, PASSING, "DRAFT"),
        read(17, new, true, &excluded, PASSING, "DRAFT"),
        read(17, new, false, &excluded, PASSING, "UNKNOWN"),
        read(17, new, false, &excluded, PASSING, "CLEAN"),
        read(17, new, false, &excluded, PASSING, "MERGED"),
        read(18, &other, false, &excluded, PASSING, "CLEAN"),
        read(18, &other, false, &excluded, PASSING, "MERGED"),
    ]
    .join("\n");
    let output = fixture
        .personal_command("merge")?
        .args(["--pr", "17", "--pr", "18", "--interval", "0"])
        .env("GH_FIXTURE_VIEWS", views)
        .env("GH_FIXTURE_ASSESSMENTS", records)
        .env("GH_FIXTURE_BEHIND", &old)
        .output()?;
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let log = fixture.log()?;
    assert_eq!(
        mutations(&log),
        ["update-branch", "ready", "merge", "merge"]
    );
    assert!(log.contains("\nupdate-branch\n--repo\nP4suta/project\n17\n"));
    assert!(log.contains(&format!(
        "\nmerge\n--repo\nP4suta/project\n17\n--squash\n--match-head-commit\n{new}\n"
    )));
    assert!(log.contains(&format!(
        "\nmerge\n--repo\nP4suta/project\n18\n--squash\n--match-head-commit\n{other}\n"
    )));
    assert!(!log.contains(PAUSE_MARKER));
    Ok(())
}

#[test]
fn a_merge_stops_with_a_structured_refusal_and_merges_nothing() -> Result<()> {
    let closed = format!("{BODY}\nCloses #23.\n");
    let excluded = format!("{closed}\n@coderabbitai ignore\n");
    for (checks, merge, body, paused, timeout, rule) in [
        (
            FAILING,
            "BLOCKED",
            excluded.as_str(),
            true,
            "120",
            "pr.checks",
        ),
        (
            PASSING,
            "DIRTY",
            excluded.as_str(),
            true,
            "120",
            "pr.conflict",
        ),
        (PASSING, "CLEAN", closed.as_str(), false, "120", "pr.review"),
        ("[]", "BLOCKED", excluded.as_str(), true, "0", "pr.timeout"),
    ] {
        let mut fixture = Fixture::new(BODY)?;
        if rule == "pr.review" {
            fixture.level = "critical";
        }
        let view = read(17, &fixture.head, false, body, checks, merge);
        if paused {
            fixture.pause("paused-pr")?;
        }
        let output = fixture
            .personal_command("merge")?
            .args(["--pr", "17", "--interval", "0", "--timeout", timeout])
            .env("GH_FIXTURE_VIEWS", view)
            .output()?;
        assert_eq!(
            refusal(&output)["rule"],
            rule,
            "stdout={} stderr={}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(mutations(&fixture.log()?).is_empty(), "{rule}");
    }
    Ok(())
}

/// Runs Git in `directory` with an isolated configuration, so no hook or signing of the caller applies.
fn scoped_git(directory: &std::path::Path, scope: &std::path::Path, arguments: &[&str]) -> String {
    let output = fixture_git::command("git", &scope.join("gitconfig"))
        .current_dir(directory)
        .args(arguments)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "git {arguments:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap().trim().to_owned()
}

/// A checkout with shared commits, conflicting edits, clean overlapping edits, and disjoint paths.
fn branches(scope: &std::path::Path) -> Result<PathBuf> {
    fs::write(
        scope.join("gitconfig"),
        "[user]\n\tname = Fixture\n\temail = fixture@example.invalid\n",
    )?;
    scoped_git(scope, scope, &["init", "-q", "--bare", "remote.git"]);
    scoped_git(scope, scope, &["init", "-q", "-b", "main", "checkout"]);
    let checkout = scope.join("checkout");
    let commit = |branch: &str, from: &str, file: &str, contents: &str| -> Result<()> {
        scoped_git(&checkout, scope, &["checkout", "-q", "-B", branch, from]);
        fs::write(checkout.join(file), contents)?;
        scoped_git(&checkout, scope, &["add", file]);
        scoped_git(&checkout, scope, &["commit", "-q", "-m", branch]);
        scoped_git(&checkout, scope, &["push", "-q", "origin", branch]);
        Ok(())
    };
    scoped_git(
        &checkout,
        scope,
        &["remote", "add", "origin", "../remote.git"],
    );
    fs::write(checkout.join("README"), "base")?;
    fs::write(checkout.join("shared.txt"), "start\n1\n2\n3\n4\n5\nend\n")?;
    scoped_git(&checkout, scope, &["add", "README", "shared.txt"]);
    scoped_git(&checkout, scope, &["commit", "-q", "-m", "base"]);
    scoped_git(&checkout, scope, &["push", "-q", "origin", "main"]);
    commit("a", "main", "shared.txt", "a\n1\n2\n3\n4\n5\nend\n")?;
    commit("feature", "a", "feature.txt", "feature")?;
    commit("solo", "main", "shared.txt", "solo\n1\n2\n3\n4\n5\nend\n")?;
    commit(
        "compatible",
        "main",
        "shared.txt",
        "start\n1\n2\n3\n4\n5\ncompatible\n",
    )?;
    commit("identical", "main", "shared.txt", "a\n1\n2\n3\n4\n5\nend\n")?;
    commit("clean", "main", "clean.txt", "clean")?;
    Ok(checkout)
}

fn create_among_open_prs(
    fixture: &Fixture,
    checkout: &std::path::Path,
    head: &str,
    open: &str,
) -> Result<Output> {
    let mut command = fixture.personal_command("create")?;
    let (packet, _) = fixture.assessment(checkout, "P4suta/project", "origin/main", head)?;
    fs::write(
        &fixture.body,
        fixture.assessed_body(&fs::read_to_string(&fixture.body)?),
    )?;
    for (name, _) in std::env::vars_os() {
        if name.to_string_lossy().starts_with("GIT_") {
            command.env_remove(name);
        }
    }
    Ok(command
        .current_dir(checkout)
        .env(
            "GIT_CONFIG_GLOBAL",
            fixture.directory.path().join("gitconfig"),
        )
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("PWF_FIXTURE_FETCH_CHECKOUT", checkout)
        .env(
            "PWF_FIXTURE_FETCH_BARE",
            fixture.directory.path().join("remote.git"),
        )
        .env("GH_FIXTURE_HEAD", git(checkout, &["rev-parse", head])?)
        .env(
            "GH_FIXTURE_BASE",
            git(checkout, &["rev-parse", "origin/main"])?,
        )
        .env("GH_FIXTURE_ASSESSMENT", serde_json::to_string(&packet)?)
        .env("GH_FIXTURE_LIST", open)
        .args([
            "--issue",
            "23",
            "--title",
            "fix: preserve edits",
            "--body-file",
        ])
        .arg(&fixture.body)
        .args(["--head", head, "--base", "main"])
        .output()?)
}

#[test]
fn overlapping_prs_require_a_clean_merge_without_changing_the_checkout() -> Result<()> {
    let closed = format!("{BODY}\nCloses #23.\n");
    let fixture = Fixture::new(&closed)?;
    let checkout = branches(fixture.directory.path())?;
    git(
        &checkout,
        &[
            "remote",
            "set-url",
            "origin",
            "https://github.com/P4suta/project.git",
        ],
    )?;
    let create = |head: &str, open: &str| create_among_open_prs(&fixture, &checkout, head, open);
    let open = r#"[{"number":5,"headRefName":"a","baseRefName":"main"}]"#;
    let quoted = platform::refusal::command(&[fixture.body.display().to_string()]);
    let record = refusal(&create("feature", open)?);
    assert_eq!(record["rule"], "pr.dependency");
    assert_eq!(
        record["next"],
        format!(
            "pr-workflow stack create --repo P4suta/project --issue 23 --title 'fix: preserve edits' --body-file {quoted} --head feature --base a"
        )
    );
    let record = refusal(&create("solo", open)?);
    assert_eq!(record["rule"], "pr.dependency");
    assert!(
        record["cause"]
            .as_str()
            .unwrap()
            .contains("conflicts with #5")
    );
    assert!(
        record["next"]
            .as_str()
            .unwrap()
            .ends_with("--head solo --base a")
    );
    assert!(!fixture.log()?.contains("\ncreate\n"));
    fs::write(checkout.join("staged.txt"), "staged")?;
    scoped_git(&checkout, fixture.directory.path(), &["add", "staged.txt"]);
    fs::write(checkout.join("shared.txt"), "unstaged")?;
    fs::write(checkout.join("untracked.txt"), "untracked")?;
    let index = fs::read(checkout.join(".git/index"))?;
    let head_before = scoped_git(&checkout, fixture.directory.path(), &["rev-parse", "HEAD"]);
    for head in ["compatible", "identical", "clean"] {
        let output = create(head, open)?;
        assert!(
            output.status.success(),
            "{head}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    assert_eq!(fixture.log()?.matches("\ncreate\n").count(), 3);
    let two = r#"[{"number":6,"headRefName":"compatible","baseRefName":"main"},{"number":5,"headRefName":"a","baseRefName":"main"}]"#;
    let record = refusal(&create("solo", two)?);
    assert!(
        record["cause"]
            .as_str()
            .unwrap()
            .contains("conflicts with #5")
    );
    scoped_git(
        &checkout,
        fixture.directory.path(),
        &["config", "merge.renames", "invalid"],
    );
    let output = create("compatible", open)?;
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("merge-tree"));
    assert_eq!(fixture.log()?.matches("\ncreate\n").count(), 3);
    assert_eq!(fs::read(checkout.join(".git/index"))?, index);
    assert_eq!(
        scoped_git(&checkout, fixture.directory.path(), &["rev-parse", "HEAD"]),
        head_before
    );
    assert_eq!(fs::read_to_string(checkout.join("shared.txt"))?, "unstaged");
    assert_eq!(fs::read_to_string(checkout.join("staged.txt"))?, "staged");
    assert_eq!(
        fs::read_to_string(checkout.join("untracked.txt"))?,
        "untracked"
    );
    Ok(())
}

#[test]
fn create_has_no_independence_override() -> Result<()> {
    let fixture = Fixture::new(BODY)?;
    let output = fixture.command("create")?.arg("--independent").output()?;
    assert!(!output.status.success());
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("unexpected argument '--independent'")
    );
    assert!(!fixture.log.exists());
    Ok(())
}

impl Fixture {
    fn stack_log(&self) -> String {
        fs::read_to_string(self.directory.path().join("gh-stack.log")).unwrap_or_default()
    }

    /// A checkout of `P4suta/project` on `topic`, whose lease file the gh-stack fixture compares with the token it receives.
    fn stack_checkout(&self, origin: &str) -> Result<(PathBuf, PathBuf)> {
        let scope = self.directory.path();
        fs::write(scope.join("gitconfig"), "")?;
        scoped_git(scope, scope, &["init", "-q", "-b", "topic", "checkout"]);
        let checkout = scope.join("checkout");
        scoped_git(&checkout, scope, &["remote", "add", "origin", origin]);
        git(&checkout, &["config", "user.name", "Fixture"])?;
        git(&checkout, &["config", "user.email", "fixture@example.com"])?;
        fs::write(checkout.join("README"), "Stack base.\n")?;
        scoped_git(&checkout, scope, &["add", "README"]);
        scoped_git(&checkout, scope, &["commit", "-q", "-m", "stack base"]);
        let base = git(&checkout, &["rev-parse", "HEAD"])?;
        scoped_git(
            &checkout,
            scope,
            &["update-ref", "refs/remotes/origin/main", &base],
        );
        fs::write(checkout.join("parent.txt"), "Parent change.\n")?;
        scoped_git(&checkout, scope, &["add", "parent.txt"]);
        scoped_git(&checkout, scope, &["commit", "-q", "-m", "stack parent"]);
        let parent = git(&checkout, &["rev-parse", "HEAD"])?;
        scoped_git(
            &checkout,
            scope,
            &["update-ref", "refs/heads/parent", &parent],
        );
        scoped_git(
            &checkout,
            scope,
            &["update-ref", "refs/remotes/origin/parent", &parent],
        );
        fs::write(checkout.join("topic.txt"), "Prior topic change.\n")?;
        scoped_git(&checkout, scope, &["add", "topic.txt"]);
        scoped_git(
            &checkout,
            scope,
            &["commit", "-q", "-m", "prior stack topic"],
        );
        let prior = git(&checkout, &["rev-parse", "HEAD"])?;
        scoped_git(
            &checkout,
            scope,
            &["update-ref", "refs/heads/prior-topic", &prior],
        );
        fs::write(checkout.join("topic.txt"), "Topic change.\n")?;
        scoped_git(&checkout, scope, &["add", "topic.txt"]);
        scoped_git(&checkout, scope, &["commit", "-q", "-m", "stack topic"]);
        Ok((checkout.clone(), checkout.join(".git/dotguard-stack.lease")))
    }

    fn stack_command(
        &self,
        checkout: &std::path::Path,
        lease: &std::path::Path,
        arguments: &[&str],
    ) -> Result<Command> {
        let mut command = self.personal_command(&format!("stack {}", arguments[0]))?;
        for (name, _) in std::env::vars_os() {
            if name.to_string_lossy().starts_with("GIT_") {
                command.env_remove(name);
            }
        }
        command
            .current_dir(checkout)
            .env("GIT_CONFIG_GLOBAL", self.directory.path().join("gitconfig"))
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GH_STACK_FIXTURE_LEASE", lease)
            .args(&arguments[1..]);
        if arguments[0] == "create" || arguments[0] == "merge" {
            self.assessment(checkout, "P4suta/project", "origin/parent", "HEAD")?;
            fs::write(
                &self.body,
                self.assessed_body(&fs::read_to_string(&self.body)?),
            )?;
            command
                .env("GH_FIXTURE_HEAD", git(checkout, &["rev-parse", "HEAD"])?)
                .env(
                    "GH_FIXTURE_BASE",
                    git(checkout, &["rev-parse", "origin/parent"])?,
                );
            command.env(
                "GH_FIXTURE_ASSESSMENTS",
                self.assessment_records(
                    checkout,
                    "P4suta/project",
                    &[
                        ("parent", "origin/main"),
                        ("prior-topic", "origin/parent"),
                        ("HEAD", "origin/parent"),
                    ],
                )?,
            );
            command.env(
                "GH_FIXTURE_BASES",
                format!(
                    "{}={}\n{}={}\n{}={}",
                    git(checkout, &["rev-parse", "parent"])?,
                    git(checkout, &["rev-parse", "origin/main"])?,
                    git(checkout, &["rev-parse", "prior-topic"])?,
                    git(checkout, &["rev-parse", "origin/parent"])?,
                    git(checkout, &["rev-parse", "HEAD"])?,
                    git(checkout, &["rev-parse", "origin/parent"])?
                ),
            );
        }
        Ok(command)
    }
}

#[test]
fn a_stacked_pr_is_created_on_its_parent_and_linked_down_to_the_trunk() -> Result<()> {
    let closed = format!("{BODY}\nCloses #23.\n");
    let fixture = Fixture::new(&closed)?;
    let (checkout, lease) = fixture.stack_checkout("https://github.com/P4suta/project.git")?;
    let list = r#"[{"number":4,"headRefName":"bottom","baseRefName":"main"},{"number":5,"headRefName":"parent","baseRefName":"bottom"}]"#;
    let create = |base: &str| -> Result<Output> {
        Ok(fixture
            .stack_command(&checkout, &lease, &["create"])?
            .env("GH_FIXTURE_LIST", list)
            .args([
                "--issue",
                "23",
                "--title",
                "fix: preserve edits",
                "--body-file",
            ])
            .arg(&fixture.body)
            .args(["--head", "topic", "--base", base])
            .output()?)
    };
    let output = create("parent")?;
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        fixture
            .log()?
            .contains("\ncreate\n--repo\nP4suta/project\n--head\ntopic\n--base\nparent\n--draft\n")
    );
    assert_eq!(
        fixture.stack_log(),
        "gh-stack link --base main 4 5 17 repo=P4suta/project leased=false notifier=1\n"
    );
    let record = refusal(&create("orphan")?);
    assert_eq!(record["rule"], "pr.stack");
    assert_eq!(fixture.log()?.matches("\ncreate\n").count(), 1);
    Ok(())
}

/// A squash-merged branch and one the default branch already holds go, and an unmerged or checked-out branch stays.
#[test]
fn prune_merged_deletes_only_branches_that_merged() -> Result<()> {
    let fixture = Fixture::new(BODY)?;
    let (checkout, lease) = fixture.stack_checkout("git@github.com:P4suta/project.git")?;
    let scope = fixture.directory.path();
    let base = git(&checkout, &["rev-parse", "origin/main"])?;
    scoped_git(
        &checkout,
        scope,
        &["update-ref", "refs/heads/landed", &base],
    );
    let prior = git(&checkout, &["rev-parse", "prior-topic"])?;
    let parent = git(&checkout, &["rev-parse", "parent"])?;
    // The fixture's Git forwards to the binary in Git's own exec path, so the run never meets whichever guard the host installed.
    let real = PathBuf::from(git(&checkout, &["--exec-path"])?)
        .join(format!("git{}", std::env::consts::EXE_SUFFIX));
    let pulls = serde_json::json!([
        {"merged_at":"2026-10-20T00:00:00Z","head":{"ref":"prior-topic","sha":prior}},
        {"merged_at":null,"head":{"ref":"parent","sha":parent}}
    ]);
    let prune = |extra: &[&str]| -> Result<Output> {
        let mut command = fixture.personal_command("prune-merged")?;
        for (name, _) in std::env::vars_os() {
            if name.to_string_lossy().starts_with("GIT_") {
                command.env_remove(name);
            }
        }
        Ok(command
            .current_dir(&checkout)
            .env("PWF_FIXTURE_REAL_GIT", &real)
            .env("GIT_CONFIG_GLOBAL", scope.join("gitconfig"))
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GH_FIXTURE_PULLS", pulls.to_string())
            .args(extra)
            .output()?)
    };
    let preview = prune(&["--dry-run"])?;
    let printed = String::from_utf8_lossy(&preview.stdout);
    assert!(
        preview.status.success(),
        "{}",
        String::from_utf8_lossy(&preview.stderr)
    );
    assert!(
        printed.contains("Would delete prior-topic") && printed.contains("Would delete landed")
    );
    assert!(!printed.contains("parent") && !printed.contains("Would delete topic"));
    assert!(git(&checkout, &["branch", "--list", "prior-topic"])?.contains("prior-topic"));

    let output = prune(&[])?;
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let left = git(
        &checkout,
        &["for-each-ref", "--format=%(refname:short)", "refs/heads"],
    )?;
    let left: Vec<&str> = left.lines().collect();
    assert_eq!(left, ["parent", "topic"]);
    assert!(!lease.exists());
    Ok(())
}

#[test]
fn stack_sync_and_rebase_run_gh_stack_under_the_lease_and_release_it() -> Result<()> {
    let fixture = Fixture::new(BODY)?;
    let (checkout, lease) = fixture.stack_checkout("git@github.com:P4suta/project.git")?;
    let output = fixture
        .stack_command(&checkout, &lease, &["sync"])?
        .output()?;
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        fixture.stack_log(),
        "gh-stack checkout topic repo=P4suta/project leased=true notifier=1\n\
         gh-stack sync repo=P4suta/project leased=true notifier=1\n"
    );
    assert!(!lease.exists());

    let output = fixture
        .stack_command(&checkout, &lease, &["rebase"])?
        .env("GH_STACK_FIXTURE_EXIT", "3")
        .output()?;
    let record = refusal(&output);
    assert_eq!(record["rule"], "pr.conflict");
    assert_eq!(
        record["next"],
        "pr-workflow stack rebase --repo P4suta/project --continue"
    );
    assert!(
        fixture
            .stack_log()
            .ends_with("gh-stack rebase repo=P4suta/project leased=true notifier=1\n")
    );
    assert!(!lease.exists());

    fs::write(&lease, "held")?;
    let record = refusal(
        &fixture
            .stack_command(&checkout, &lease, &["sync"])?
            .output()?,
    );
    assert_eq!(record["rule"], "pr.stack");
    assert_eq!(fs::read_to_string(&lease)?, "held");

    let other = Fixture::new(BODY)?;
    let (checkout, lease) = other.stack_checkout("https://github.com/other/project.git")?;
    let record = refusal(
        &other
            .stack_command(&checkout, &lease, &["sync"])?
            .output()?,
    );
    assert_eq!(record["rule"], "pr.stack");
    assert_eq!(other.stack_log(), "");
    Ok(())
}

#[test]
fn a_stack_merges_together_after_restacking_and_verifies_each_head() -> Result<()> {
    let closed = format!("{BODY}\nCloses #23.\n");
    let excluded = format!("{closed}\n@coderabbitai ignore\n");
    let fixture = Fixture::new(BODY)?;
    fixture.pause("paused-pr")?;
    let (checkout, lease) = fixture.stack_checkout("https://github.com/P4suta/project.git")?;
    let parent = git(&checkout, &["rev-parse", "parent"])?;
    let top = git(&checkout, &["rev-parse", "HEAD"])?;
    let stale = git(&checkout, &["rev-parse", "prior-topic"])?;
    let views = [
        read(5, &parent, false, &excluded, PASSING, "BLOCKED"),
        read(5, &parent, false, &excluded, PASSING, "BLOCKED"),
        read(5, &parent, false, &excluded, PASSING, "MERGED"),
        read(17, &stale, false, &excluded, PASSING, "BLOCKED"),
        read(17, &top, false, &excluded, PASSING, "BLOCKED"),
        read(17, &top, false, &excluded, PASSING, "MERGED"),
    ]
    .join("\n");
    let output = fixture
        .stack_command(&checkout, &lease, &["merge", "--pr", "17", "--interval", "0"])?
        .env(
            "GH_FIXTURE_LIST",
            r#"[{"number":5,"headRefName":"parent","baseRefName":"main"},{"number":17,"headRefName":"topic","baseRefName":"parent"}]"#,
        )
        .env("GH_FIXTURE_VIEWS", views)
        .env("GH_FIXTURE_BEHIND", &stale)
        .output()?;
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        fixture.stack_log(),
        "gh-stack checkout topic repo=P4suta/project leased=true notifier=1\n\
         gh-stack sync repo=P4suta/project leased=true notifier=1\n\
         gh-stack merge 17 --yes --squash repo=P4suta/project leased=false notifier=1\n"
    );
    assert_eq!(mutations(&fixture.log()?), Vec::<String>::new());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains(&format!("#5 merged at {parent}"))
            && stdout.contains(&format!("#17 merged at {top}")),
        "{stdout}"
    );
    Ok(())
}

/// pr-workflow and dotguard agree on the variable and file of the stack lease.
#[test]
fn the_stack_lease_names_match_dotguard() -> Result<()> {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let guard = fs::read_to_string(root.join("crates/dotguard/src/stack.rs"))?;
    let workflow = fs::read_to_string(root.join("crates/pr-workflow/src/lib.rs"))?;
    for (guard_line, workflow_line) in [
        (
            "pub const TOKEN: &str = \"DOTGUARD_STACK\";",
            "const STACK_TOKEN: &str = \"DOTGUARD_STACK\";",
        ),
        (
            "pub const LEASE: &str = \"dotguard-stack.lease\";",
            "const STACK_LEASE: &str = \"dotguard-stack.lease\";",
        ),
    ] {
        assert!(guard.contains(guard_line), "{guard_line}");
        assert!(workflow.contains(workflow_line), "{workflow_line}");
    }
    Ok(())
}
