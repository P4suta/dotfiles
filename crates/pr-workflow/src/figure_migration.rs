use crate::{GITHUB, Github, Tool, figure, gh};
use anyhow::{Context, Result, ensure};
use base64::Engine as _;
use clap::Args;
use pulldown_cmark::{Event, Parser, Tag};
use rules::figure_migration_rules::{BranchIdentity, Evidence, Mode, Verification, may_retire};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::ffi::OsString;
use std::fs;
use std::io::Write as _;
use std::path::{Path, PathBuf};

const STATE: &str = ".local/state/pr-workflow/figure-migrations";
const ARCHIVE_TITLE: &str = "docs: preserve historical figure sources";
const PAGE_SIZE: usize = 100;
const BATCH_SIZE: usize = 20;
const MAX_BODY_CHARS: usize = 60_000;
const API: &str = "https://api.github.com/";
const RAW: &str = "https://raw.githubusercontent.com";
const SHA1_DIGITS: usize = 40;
const SHA256_DIGITS: usize = 64;
const REPOSITORIES: &str = "query($owner:String!,$cursor:String){user(login:$owner){repositories(first:100,after:$cursor,ownerAffiliations:OWNER){pageInfo{hasNextPage endCursor}nodes{nameWithOwner isFork isArchived diagram:ref(qualifiedName:\"refs/heads/diagrams\"){target{oid}} screenshot:ref(qualifiedName:\"refs/heads/screenshots\"){target{oid}}}}}}";
const REVIEWS: &str = "query($owner:String!,$name:String!,$cursor:String){repository(owner:$owner,name:$name){pullRequests(first:100,after:$cursor){pageInfo{hasNextPage endCursor}nodes{number reviews(first:100){pageInfo{hasNextPage endCursor}nodes{databaseId body url}}}}}}";

#[derive(Debug, Args)]
pub struct Action {
    #[arg(long)]
    owner: String,
    #[arg(long)]
    repo: Option<String>,
    #[arg(long)]
    apply: bool,
    #[arg(long, requires = "apply")]
    retire: bool,
}

#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
struct Branch {
    name: String,
    oid: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PageInfo {
    has_next_page: bool,
    end_cursor: Option<String>,
}

#[derive(Debug, Deserialize)]
struct Target {
    oid: String,
}

#[derive(Debug, Deserialize)]
struct Reference {
    target: Target,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Repository {
    name_with_owner: String,
    is_fork: bool,
    is_archived: bool,
    diagram: Option<Reference>,
    screenshot: Option<Reference>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RepositoryPage {
    nodes: Vec<Repository>,
    page_info: PageInfo,
}

#[derive(Debug, Deserialize)]
struct User {
    repositories: RepositoryPage,
}

#[derive(Debug, Deserialize)]
struct Data {
    user: Option<User>,
}

#[derive(Debug, Deserialize)]
struct Response {
    data: Option<Data>,
    #[serde(default)]
    errors: Vec<serde_json::Value>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct ArchivedFile {
    branch: String,
    name: String,
    oid: String,
    digest: String,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Manifest {
    version: u8,
    repository: String,
    repository_id: u64,
    branches: Vec<Branch>,
    files: Vec<ArchivedFile>,
    archive_issue: Option<u64>,
    mappings: BTreeMap<String, String>,
    verified_documents: BTreeMap<String, String>,
    retired: BTreeSet<String>,
    bundle_digest: Option<String>,
}

#[derive(Debug, Deserialize)]
struct Entry {
    path: String,
    mode: String,
    #[serde(rename = "type")]
    kind: String,
    sha: String,
}

#[derive(Debug, Deserialize)]
struct Tree {
    tree: Vec<Entry>,
    truncated: bool,
}

#[derive(Debug, Deserialize)]
struct Blob {
    sha: String,
    encoding: String,
    content: String,
}

#[derive(Clone, Debug, Deserialize)]
struct Document {
    id: u64,
    #[serde(default)]
    number: u64,
    #[serde(default)]
    title: String,
    body: Option<String>,
    #[serde(default)]
    url: String,
    html_url: String,
    #[serde(default)]
    pull_request: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Connection<T> {
    nodes: Vec<T>,
    page_info: PageInfo,
}

#[derive(Debug, Deserialize)]
struct ReviewedPull {
    number: u64,
    reviews: Connection<Review>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Review {
    database_id: u64,
    body: String,
    url: String,
}

#[derive(Debug, Deserialize)]
struct RestReview {
    id: u64,
    body: Option<String>,
    html_url: String,
}

fn digest(bytes: &[u8]) -> String {
    crate::build_inputs::hex(&Sha256::digest(bytes))
}

fn save(path: &Path, manifest: &Manifest) -> Result<()> {
    let mut file =
        tempfile::NamedTempFile::new_in(path.parent().context("manifest parent missing")?)?;
    serde_json::to_writer_pretty(file.as_file_mut(), manifest)?;
    file.as_file_mut().write_all(b"\n")?;
    file.as_file().sync_all()?;
    let (_, staged) = file.keep()?;
    crate::aside::replace(&staged, path)?;
    Ok(())
}

fn git(directory: &Path, arguments: &[OsString]) -> Result<String> {
    let output = Tool::Git
        .command()
        .current_dir(directory)
        .args(arguments)
        .output()?;
    ensure!(
        output.status.success(),
        "figure archive Git failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    Ok(String::from_utf8(output.stdout)?.trim().to_owned())
}

fn pages<T: serde::de::DeserializeOwned>(github: &Github, endpoint: &str) -> Result<Vec<T>> {
    let separator = if endpoint.contains('?') { '&' } else { '?' };
    let mut all = Vec::new();
    for page in 1_u64.. {
        let entries: Vec<T> = github.read(&format!(
            "{endpoint}{separator}per_page={PAGE_SIZE}&page={page}"
        ))?;
        let complete = entries.len() < PAGE_SIZE;
        all.extend(entries);
        if complete {
            return Ok(all);
        }
    }
    anyhow::bail!("document pagination did not complete")
}

fn repositories(github: &Github, owner: &str) -> Result<Vec<Repository>> {
    let mut cursor: Option<String> = None;
    let mut seen = BTreeSet::new();
    let mut found = Vec::new();
    loop {
        let response: Response = github.graphql(
            REPOSITORIES,
            &serde_json::json!({"owner": owner,"cursor":cursor}),
        )?;
        ensure!(
            response.errors.is_empty(),
            "figure repository inventory returned GraphQL errors"
        );
        let page = response
            .data
            .and_then(|data| data.user)
            .context("figure owner not found")?
            .repositories;
        found.extend(page.nodes.into_iter().filter(|repo| !repo.is_fork));
        if !page.page_info.has_next_page {
            return Ok(found);
        }
        cursor = page.page_info.end_cursor;
        ensure!(
            cursor
                .as_ref()
                .is_some_and(|cursor| seen.insert(cursor.clone())),
            "repository pagination did not advance"
        );
    }
}

fn branches(repository: &Repository) -> Vec<Branch> {
    [
        (&repository.diagram, "diagrams"),
        (&repository.screenshot, "screenshots"),
    ]
    .into_iter()
    .filter_map(|(reference, name)| {
        reference.as_ref().map(|reference| Branch {
            name: name.to_owned(),
            oid: reference.target.oid.clone(),
        })
    })
    .collect()
}

fn documents(github: &Github, repository: &str) -> Result<Vec<Document>> {
    let mut documents: Vec<Document> =
        pages(github, &format!("repos/{repository}/issues?state=all"))?;
    let has_pulls = documents
        .iter()
        .any(|document| document.pull_request.is_some());
    documents.extend(pages(
        github,
        &format!("repos/{repository}/issues/comments"),
    )?);
    documents.extend(pages(
        github,
        &format!("repos/{repository}/pulls/comments"),
    )?);
    documents.extend(pages(github, &format!("repos/{repository}/releases"))?);
    if has_pulls {
        documents.extend(reviews(github, repository)?);
    }
    Ok(documents)
}

fn reviews(github: &Github, repository: &str) -> Result<Vec<Document>> {
    let (owner, name) = repository
        .split_once('/')
        .context("review repository identity is missing")?;
    let mut cursor: Option<String> = None;
    let mut seen = BTreeSet::new();
    let mut documents = Vec::new();
    loop {
        let response: serde_json::Value = github.graphql(
            REVIEWS,
            &serde_json::json!({"owner":owner,"name":name,"cursor":cursor}),
        )?;
        ensure!(
            response.get("errors").is_none(),
            "review inventory returned GraphQL errors"
        );
        let page: Connection<ReviewedPull> =
            serde_json::from_value(response["data"]["repository"]["pullRequests"].clone())
                .context("review inventory is incomplete")?;
        for pull in page.nodes {
            if pull.reviews.page_info.has_next_page {
                let reviews: Vec<RestReview> = pages(
                    github,
                    &format!("repos/{repository}/pulls/{}/reviews", pull.number),
                )?;
                documents.extend(reviews.into_iter().map(|review| Document {
                    id: review.id,
                    number: 0,
                    title: String::new(),
                    body: review.body,
                    url: format!(
                        "{API}repos/{repository}/pulls/{}/reviews/{}",
                        pull.number, review.id
                    ),
                    html_url: review.html_url,
                    pull_request: None,
                }));
            } else {
                documents.extend(pull.reviews.nodes.into_iter().map(|review| Document {
                    id: review.database_id,
                    number: 0,
                    title: String::new(),
                    body: Some(review.body),
                    url: format!(
                        "{API}repos/{repository}/pulls/{}/reviews/{}",
                        pull.number, review.database_id
                    ),
                    html_url: review.url,
                    pull_request: None,
                }));
            }
        }
        if !page.page_info.has_next_page {
            return Ok(documents);
        }
        cursor = page.page_info.end_cursor;
        ensure!(
            cursor
                .as_ref()
                .is_some_and(|cursor| seen.insert(cursor.clone())),
            "review pagination did not advance"
        );
    }
}

fn addresses(repository: &str, branch: &str, name: &str) -> Vec<String> {
    let blob = format!("{GITHUB}/{repository}/blob/{branch}/{name}");
    vec![
        format!("{blob}?raw=true"),
        format!("{blob}?raw=1"),
        blob,
        format!("{GITHUB}/{repository}/raw/{branch}/{name}"),
        format!("{RAW}/{repository}/{branch}/{name}"),
    ]
}

fn refers(body: &str, repository: &str, branch: &str) -> bool {
    [
        format!("github.com/{repository}/blob/{branch}/"),
        format!("github.com/{repository}/raw/{branch}/"),
        format!("raw.githubusercontent.com/{repository}/{branch}/"),
    ]
    .iter()
    .any(|prefix| {
        body.to_ascii_lowercase()
            .contains(&prefix.to_ascii_lowercase())
    })
}

fn archive(github: &Github, directory: &Path, manifest: &mut Manifest) -> Result<()> {
    let bare = directory.join("history.git");
    fs::create_dir_all(&bare)?;
    if !bare.join("HEAD").is_file() {
        git(&bare, &["init".into(), "--bare".into()])?;
    }
    let remote = format!("{GITHUB}/{}.git", manifest.repository);
    for branch in &manifest.branches {
        if manifest.retired.contains(&branch.name) {
            continue;
        }
        git(
            &bare,
            &[
                "fetch".into(),
                "--no-tags".into(),
                remote.clone().into(),
                format!("refs/heads/{0}:refs/heads/{0}", branch.name).into(),
            ],
        )?;
        ensure!(
            git(
                &bare,
                &[
                    "rev-parse".into(),
                    format!("refs/heads/{}", branch.name).into()
                ]
            )? == branch.oid,
            "figure branch changed during archival"
        );
        let tree: Tree = github.read(&format!(
            "repos/{}/git/trees/{}?recursive=1",
            manifest.repository, branch.oid
        ))?;
        ensure!(!tree.truncated, "figure tree inventory is incomplete");
        for entry in tree.tree {
            let file = archive_file(github, directory, &manifest.repository, branch, entry)?;
            if let Some(existing) = manifest
                .files
                .iter()
                .find(|existing| existing.branch == file.branch && existing.name == file.name)
            {
                ensure!(
                    existing.oid == file.oid && existing.digest == file.digest,
                    "figure archive entry changed"
                );
            } else {
                manifest.files.push(file);
            }
        }
    }
    let bundle = directory.join("history.bundle");
    if !bundle.is_file() {
        git(
            &bare,
            &[
                "bundle".into(),
                "create".into(),
                bundle.as_os_str().to_owned(),
                "--all".into(),
            ],
        )?;
    }
    git(
        &bare,
        &[
            "bundle".into(),
            "verify".into(),
            bundle.as_os_str().to_owned(),
        ],
    )?;
    let checksum = digest(&fs::read(&bundle)?);
    if let Some(expected) = &manifest.bundle_digest {
        ensure!(
            expected == &checksum,
            "figure history bundle checksum differs"
        );
    }
    manifest.bundle_digest = Some(checksum);
    Ok(())
}

fn archive_file(
    github: &Github,
    directory: &Path,
    repository: &str,
    branch: &Branch,
    entry: Entry,
) -> Result<ArchivedFile> {
    ensure!(
        entry.kind == "blob" && entry.mode == "100644" && valid_filename(&entry.path),
        "unexpected figure tree entry {}",
        entry.path
    );
    let blob: Blob = github.read(&format!("repos/{repository}/git/blobs/{}", entry.sha))?;
    ensure!(
        blob.sha == entry.sha && blob.encoding == "base64",
        "figure blob identity differs"
    );
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(blob.content.split_whitespace().collect::<String>())?;
    let parent = directory.join(&branch.name);
    fs::create_dir_all(&parent)?;
    let path = parent.join(&entry.path);
    if path.exists() {
        ensure!(fs::read(&path)? == bytes, "archived figure content changed");
    } else {
        fs::write(&path, &bytes)?;
    }
    Ok(ArchivedFile {
        branch: branch.name.clone(),
        name: entry.path,
        oid: entry.sha,
        digest: digest(&bytes),
    })
}

fn valid_filename(name: &str) -> bool {
    !name.is_empty()
        && !name.starts_with('.')
        && !name.contains(['/', '\\'])
        && name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'_'))
}

fn d2_source(name: &str) -> bool {
    Path::new(name)
        .extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case("d2"))
}

fn media_file(name: &str) -> bool {
    Path::new(name).extension().is_some_and(|extension| {
        [
            "png", "jpg", "jpeg", "gif", "webp", "svg", "mp4", "mov", "webm",
        ]
        .iter()
        .any(|candidate| extension.eq_ignore_ascii_case(candidate))
    })
}

fn archive_details(name: &str, source: &str) -> String {
    if d2_source(name) {
        figure::source_details(source)
    } else {
        figure::text_details(source)
    }
}

fn valid_digest(value: &str) -> bool {
    matches!(value.len(), SHA1_DIGITS | SHA256_DIGITS)
        && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn validate_manifest(manifest: &Manifest) -> Result<()> {
    ensure!(
        manifest.version == 1 && manifest.repository_id != 0,
        "unsupported figure manifest identity"
    );
    for branch in &manifest.branches {
        ensure!(
            matches!(branch.name.as_str(), "diagrams" | "screenshots") && valid_digest(&branch.oid),
            "invalid figure branch identity"
        );
    }
    for file in &manifest.files {
        ensure!(
            valid_filename(&file.name)
                && manifest
                    .branches
                    .iter()
                    .any(|branch| branch.name == file.branch)
                && valid_digest(&file.oid)
                && file.digest.len() == SHA256_DIGITS
                && valid_digest(&file.digest),
            "invalid archived figure identity"
        );
    }
    ensure!(
        manifest
            .retired
            .iter()
            .all(|name| manifest.branches.iter().any(|branch| &branch.name == name)),
        "unknown retired figure branch"
    );
    Ok(())
}

fn endpoint(document: &Document, repository: &str) -> Result<String> {
    let endpoint = document
        .url
        .strip_prefix(API)
        .context("document API URL is foreign")?;
    ensure!(
        endpoint.starts_with(&format!("repos/{repository}/")),
        "document belongs to another repository"
    );
    Ok(endpoint.to_owned())
}

fn patch(github: &Github, endpoint: &str, value: &serde_json::Value) -> Result<Document> {
    let mut input = tempfile::NamedTempFile::new()?;
    serde_json::to_writer(input.as_file_mut(), value)?;
    input.as_file().sync_all()?;
    let response = gh(&[
        "api".into(),
        endpoint.into(),
        "--method".into(),
        "PATCH".into(),
        "--include".into(),
        "--input".into(),
        input.path().as_os_str().to_owned(),
    ])?;
    github.response(&response, "core")
}

fn image_addresses(body: &str) -> Vec<(String, String)> {
    let mut images = Vec::new();
    let mut current: Option<(String, String)> = None;
    for event in Parser::new(body) {
        match event {
            Event::Start(Tag::Image { dest_url, .. }) => {
                current = Some((String::new(), dest_url.into_string()));
            }
            Event::Text(text) | Event::Code(text) => {
                if let Some((alt, _)) = &mut current {
                    alt.push_str(&text);
                }
            }
            Event::End(pulldown_cmark::TagEnd::Image) => {
                if let Some(image) = current.take() {
                    images.push(image);
                }
            }
            _ => {}
        }
    }
    images
}

fn file_key(file: &ArchivedFile) -> String {
    format!("{}/{}", file.branch, file.name)
}

fn upload_archive(
    github: &Github,
    directory: &Path,
    state_path: &Path,
    manifest: &mut Manifest,
) -> Result<()> {
    let issue = archive_issue(github, state_path, manifest)?;
    recover_uploads(github, directory, manifest, issue)?;
    save(state_path, manifest)?;
    let missing: Vec<_> = manifest
        .files
        .iter()
        .filter(|file| !manifest.mappings.contains_key(&file_key(file)))
        .cloned()
        .collect();
    for batch in missing.chunks(BATCH_SIZE) {
        upload_batch(github, directory, manifest, issue, batch)?;
        save(state_path, manifest)?;
    }
    repair_archive_comments(github, manifest, issue)?;
    ensure!(
        manifest
            .files
            .iter()
            .all(|file| manifest.mappings.contains_key(&file_key(file))),
        "figure archive upload is incomplete"
    );
    Ok(())
}

fn repair_archive_body(body: &str, mappings: &BTreeMap<String, String>) -> Result<String> {
    let mut repaired = body.to_owned();
    let mut replacements = Vec::new();
    for (event, range) in Parser::new(body).into_offset_iter() {
        if let Event::Start(Tag::Image { dest_url, .. }) = event {
            if dest_url.starts_with(figure::ATTACHMENT_URL) {
                continue;
            }
            let image = &body[range.clone()];
            let images = image_addresses(image);
            let Some((key, _)) = images.first() else {
                continue;
            };
            if let Some(destination) = mappings.get(key) {
                let start = image
                    .find(dest_url.as_ref())
                    .context("pending archive image reference changed syntax")?;
                replacements.push((
                    range.start + start..range.start + start + dest_url.len(),
                    destination,
                ));
            }
        }
    }
    for (range, destination) in replacements.into_iter().rev() {
        repaired.replace_range(range, destination);
    }
    Ok(repaired)
}

fn repair_archive_comments(github: &Github, manifest: &Manifest, issue: u64) -> Result<()> {
    let comments: Vec<Document> = pages(
        github,
        &format!("repos/{}/issues/{issue}/comments", manifest.repository),
    )?;
    for comment in comments {
        let body = comment.body.as_deref().unwrap_or_default();
        let repaired = repair_archive_body(body, &manifest.mappings)?;
        if repaired == body {
            continue;
        }
        figure::verify_body(body, &repaired)?;
        let current: Document = github.read(&comment.url)?;
        ensure!(
            current.body == comment.body,
            "archive comment changed during repair"
        );
        let mut patch = tempfile::NamedTempFile::new()?;
        serde_json::to_writer(patch.as_file_mut(), &serde_json::json!({"body": repaired}))?;
        patch.flush()?;
        let response = gh(&[
            "api".into(),
            comment.url.clone().into(),
            "--method".into(),
            "PATCH".into(),
            "--input".into(),
            patch.path().as_os_str().to_owned(),
        ])?;
        let _: Document = github.response(&response, "core")?;
        let published: Document = github.read(&comment.url)?;
        ensure!(
            published.body.as_deref() == Some(repaired.as_str()),
            "archive repair readback differs"
        );
    }
    Ok(())
}

fn archive_issue(github: &Github, state_path: &Path, manifest: &mut Manifest) -> Result<u64> {
    if manifest.archive_issue.is_none() {
        let existing: Vec<Document> = pages(
            github,
            &format!("repos/{}/issues?state=all", manifest.repository),
        )?;
        let found: Vec<_> = existing
            .iter()
            .filter(|document| document.title == ARCHIVE_TITLE && document.pull_request.is_none())
            .collect();
        ensure!(
            found.len() <= 1,
            "multiple historical figure archives need inspection"
        );
        manifest.archive_issue = found.first().map(|document| document.number);
        if manifest.archive_issue.is_none() {
            let mut body = tempfile::NamedTempFile::new()?;
            body.write_all(b"GitHub attachments and editable D2 sources preserve historical figures from the retired storage branches.\nThe comments keep the original filenames and source content.\n")?;
            body.flush()?;
            let response = gh(&[
                "issue".into(),
                "create".into(),
                "--repo".into(),
                manifest.repository.clone().into(),
                "--title".into(),
                ARCHIVE_TITLE.into(),
                "--body-file".into(),
                body.path().as_os_str().to_owned(),
            ])?;
            manifest.archive_issue = Some(crate::created_number(&response)?);
        }
        save(state_path, manifest)?;
    }
    manifest.archive_issue.context("archive issue is missing")
}

fn recover_uploads(
    github: &Github,
    directory: &Path,
    manifest: &mut Manifest,
    issue: u64,
) -> Result<()> {
    let comments: Vec<Document> = pages(
        github,
        &format!("repos/{}/issues/{issue}/comments", manifest.repository),
    )?;
    manifest.mappings.clear();
    for comment in comments {
        let body = comment.body.as_deref().unwrap_or_default();
        let images = image_addresses(body);
        for file in &manifest.files {
            let key = file_key(file);
            if !media_file(&file.name) {
                let source = fs::read_to_string(directory.join(&key))?;
                if body.contains(&format!("## {key}\n"))
                    && body.contains(&archive_details(&file.name, &source))
                {
                    manifest.mappings.insert(key, comment.html_url.clone());
                }
            } else if let Some((_, address)) = images
                .iter()
                .find(|(alt, address)| alt == &key && address.starts_with(figure::ATTACHMENT_URL))
            {
                verify_attachment(directory, file, address)?;
                manifest.mappings.insert(key, address.clone());
            }
        }
    }
    Ok(())
}

fn upload_batch(
    github: &Github,
    directory: &Path,
    manifest: &mut Manifest,
    issue: u64,
    batch: &[ArchivedFile],
) -> Result<()> {
    let mut body = String::new();
    let mut attachments = Vec::new();
    for file in batch {
        let (fragment, attachment) = archive_fragment(directory, file)?;
        body.push_str(&fragment);
        if let Some(path) = attachment {
            attachments.push(path);
        }
    }
    ensure!(
        body.chars().count() <= MAX_BODY_CHARS,
        "figure archive batch exceeds the GitHub body limit"
    );
    let mut file = tempfile::NamedTempFile::new()?;
    file.write_all(body.as_bytes())?;
    file.flush()?;
    let mut args = vec![
        "issue".into(),
        "comment".into(),
        issue.to_string().into(),
        "--repo".into(),
        manifest.repository.clone().into(),
        "--body-file".into(),
        file.path().as_os_str().to_owned(),
    ];
    for attachment in &attachments {
        args.extend(["--attach".into(), attachment.as_os_str().to_owned()]);
    }
    let response = gh(&args)?;
    let comment_id = response
        .trim()
        .rsplit_once("#issuecomment-")
        .context("archive comment URL is missing")?
        .1
        .parse::<u64>()?;
    let published: Document = github.read(&format!(
        "repos/{}/issues/comments/{comment_id}",
        manifest.repository
    ))?;
    let published_body = published
        .body
        .as_deref()
        .context("archive comment has no body")?;
    figure::verify_body(&body, published_body)?;
    let images = image_addresses(published_body);
    for file in batch {
        let key = file_key(file);
        let destination = if media_file(&file.name) {
            images
                .iter()
                .find(|(alt, _)| alt == &key)
                .context("uploaded figure is missing")?
                .1
                .clone()
        } else {
            published.html_url.clone()
        };
        if media_file(&file.name) {
            verify_attachment(directory, file, &destination)?;
        }
        manifest.mappings.insert(key, destination);
    }
    Ok(())
}

fn archive_fragment(directory: &Path, file: &ArchivedFile) -> Result<(String, Option<PathBuf>)> {
    let key = file_key(file);
    let path = directory.join(&key);
    let bytes = fs::read(&path)?;
    ensure!(
        digest(&bytes) == file.digest,
        "archived figure checksum differs"
    );
    let (fragment, attachment) = if media_file(&file.name) {
        (figure::image_line(&path, &key)?, Some(path))
    } else {
        let source = std::str::from_utf8(&bytes).with_context(|| {
            format!("archived file {key} is neither supported media nor UTF-8 text")
        })?;
        (archive_details(&file.name, source), None)
    };
    Ok((format!("## {key}\n\n{fragment}\n\n"), attachment))
}

fn verify_attachment(directory: &Path, file: &ArchivedFile, address: &str) -> Result<()> {
    ensure!(
        address.starts_with(figure::ATTACHMENT_URL),
        "figure attachment belongs to an unsupported host"
    );
    let output = Tool::Gh
        .command()
        .env("GH_HOST", "github.com")
        .args([
            "api",
            address,
            "--method",
            "GET",
            "--header",
            "Accept: application/octet-stream",
        ])
        .output()?;
    ensure!(
        output.status.success(),
        "attachment download failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    ensure!(
        output.stdout.len() <= figure::MAX_IMAGE_BYTES && digest(&output.stdout) == file.digest,
        "uploaded attachment checksum differs for {}",
        file.name
    );
    ensure!(
        digest(&fs::read(directory.join(file_key(file)))?) == file.digest,
        "local figure archive checksum differs"
    );
    Ok(())
}

fn rewrite(body: &str, manifest: &Manifest) -> Result<String> {
    let mut replacements = Vec::new();
    for file in &manifest.files {
        let destination = manifest
            .mappings
            .get(&file_key(file))
            .context("figure has no uploaded destination")?;
        replacements.extend(
            addresses(&manifest.repository, &file.branch, &file.name)
                .into_iter()
                .map(|address| (address, destination)),
        );
    }
    replacements.sort_by_key(|(address, _)| std::cmp::Reverse(address.len()));
    let mut body = body.to_owned();
    for (address, destination) in replacements {
        body = body.replace(&address, destination);
    }
    Ok(body)
}

fn migrate_documents(
    github: &Github,
    directory: &Path,
    state_path: &Path,
    manifest: &mut Manifest,
) -> Result<usize> {
    let documents = documents(github, &manifest.repository)?;
    let backups = directory.join("documents");
    fs::create_dir_all(&backups)?;
    let mut changed = 0;
    for document in documents {
        let old = document.body.as_deref().unwrap_or_default();
        if !manifest
            .branches
            .iter()
            .any(|branch| refers(old, &manifest.repository, &branch.name))
        {
            continue;
        }
        let body = rewrite(old, manifest)?;
        ensure!(
            !manifest.branches.iter().any(|branch| refers(
                &body,
                &manifest.repository,
                &branch.name
            )),
            "unknown figure reference in {}",
            document.html_url
        );
        let endpoint = endpoint(&document, &manifest.repository)?;
        let current: Document = github.read(&endpoint)?;
        ensure!(
            current.body == document.body,
            "document changed before figure migration: {}",
            document.html_url
        );
        let backup = backups.join(format!("{}.json", digest(endpoint.as_bytes())));
        if !backup.exists() {
            fs::write(
                &backup,
                serde_json::to_vec_pretty(
                    &serde_json::json!({"endpoint":endpoint,"id":document.id,"body":old}),
                )?,
            )?;
        }
        let response = patch(github, &endpoint, &serde_json::json!({"body":body}))?;
        ensure!(
            response.body.as_deref() == Some(body.as_str()),
            "figure migration response differs"
        );
        let readback: Document = github.read(&endpoint)?;
        ensure!(
            readback.body == response.body,
            "figure migration readback differs"
        );
        manifest
            .verified_documents
            .insert(endpoint, digest(body.as_bytes()));
        save(state_path, manifest)?;
        changed += 1;
    }
    Ok(changed)
}

fn verified_archive(directory: &Path, manifest: &Manifest) -> Result<bool> {
    for file in &manifest.files {
        ensure!(
            digest(&fs::read(directory.join(file_key(file)))?) == file.digest,
            "preserved figure checksum differs"
        );
    }
    let bare = directory.join("history.git");
    let bundle = directory.join("history.bundle");
    ensure!(
        manifest.bundle_digest.as_deref() == Some(&digest(&fs::read(&bundle)?)),
        "preserved history bundle checksum differs"
    );
    git(
        &bare,
        &[
            "bundle".into(),
            "verify".into(),
            directory.join("history.bundle").into_os_string(),
        ],
    )?;
    let heads = git(
        &bare,
        &[
            "bundle".into(),
            "list-heads".into(),
            bundle.into_os_string(),
        ],
    )?;
    ensure!(
        manifest.branches.iter().all(|branch| heads
            .lines()
            .any(|head| head == format!("{} refs/heads/{}", branch.oid, branch.name))),
        "preserved bundle does not cover every figure branch"
    );
    Ok(manifest
        .files
        .iter()
        .all(|file| manifest.mappings.contains_key(&file_key(file))))
}

fn process(action: &Action, repository: &Repository, root: &Path) -> Result<PathBuf> {
    ensure!(
        !repository.is_fork && !repository.is_archived,
        "figure migration requires an active personal non-fork repository"
    );
    let github = Github::connect()?;
    let destination = crate::destination(&github, &repository.name_with_owner)?;
    ensure!(
        destination.personal && !destination.fork,
        "figure repository is outside the personal scope"
    );
    let repository_id = destination.repository_id;
    let directory = root.join(repository_id.to_string());
    fs::create_dir_all(&directory)?;
    let state_path = directory.join("manifest.json");
    let expected = branches(repository);
    let mut manifest = if state_path.is_file() {
        serde_json::from_slice::<Manifest>(&fs::read(&state_path)?)?
    } else {
        Manifest {
            version: 1,
            repository: repository.name_with_owner.clone(),
            repository_id,
            branches: expected.clone(),
            files: Vec::new(),
            archive_issue: None,
            mappings: BTreeMap::new(),
            verified_documents: BTreeMap::new(),
            retired: BTreeSet::new(),
            bundle_digest: None,
        }
    };
    ensure!(
        manifest.version == 1
            && manifest.repository_id == repository_id
            && manifest.repository == repository.name_with_owner,
        "figure migration manifest identity differs"
    );
    validate_manifest(&manifest)?;
    if manifest.files.is_empty() && manifest.archive_issue.is_none() {
        manifest.branches.clone_from(&expected);
    }
    ensure!(
        manifest
            .branches
            .iter()
            .filter(|branch| !manifest.retired.contains(&branch.name))
            .eq(expected.iter()),
        "figure branches changed since the migration plan"
    );
    let inventory = documents(&github, &manifest.repository)?;
    let references = inventory
        .iter()
        .filter(|document| {
            manifest.branches.iter().any(|branch| {
                refers(
                    document.body.as_deref().unwrap_or_default(),
                    &manifest.repository,
                    &branch.name,
                )
            })
        })
        .count();
    println!(
        "{}: {} branch(es), {} document reference(s), apply {}, retire {}",
        manifest.repository,
        expected.len(),
        references,
        action.apply,
        action.retire
    );
    save(&state_path, &manifest)?;
    if !action.apply {
        return Ok(directory);
    }
    archive(&github, &directory, &mut manifest)?;
    save(&state_path, &manifest)?;
    upload_archive(&github, &directory, &state_path, &mut manifest)?;
    let changed = migrate_documents(&github, &directory, &state_path, &mut manifest)?;
    let issue = manifest.archive_issue.context("archive issue missing")?;
    patch(
        &github,
        &format!("repos/{}/issues/{issue}", manifest.repository),
        &serde_json::json!({"state":"closed"}),
    )?;
    println!(
        "{}: migrated {changed} document(s), preserved {} file(s), retired {} branch(es); {}",
        manifest.repository,
        manifest.files.len(),
        manifest.retired.len(),
        state_path.display()
    );
    Ok(directory)
}

struct Migration {
    directory: PathBuf,
    manifest: Manifest,
}

fn branch_key(repository: &str, branch: &str) -> String {
    format!("{repository}/{branch}")
}

fn global_sweep(
    repositories: &[Repository],
    migrations: &mut [Migration],
    root: &Path,
) -> Result<BTreeMap<String, u64>> {
    let backups = root.join("document-backups");
    fs::create_dir_all(&backups)?;
    let mut remaining = BTreeMap::new();
    for migration in &*migrations {
        for branch in &migration.manifest.branches {
            remaining.insert(
                branch_key(&migration.manifest.repository, &branch.name),
                0_u64,
            );
        }
    }
    for repository in repositories {
        let github = Github::connect()?;
        let inventory = documents(&github, &repository.name_with_owner)?;
        for document in inventory {
            let old = document.body.as_deref().unwrap_or_default();
            let mut body = old.to_owned();
            for migration in &*migrations {
                if migration
                    .manifest
                    .branches
                    .iter()
                    .any(|branch| refers(&body, &migration.manifest.repository, &branch.name))
                {
                    body = rewrite(&body, &migration.manifest)?;
                }
            }
            if body != old {
                ensure!(
                    !repository.is_archived,
                    "an archived repository still references figure branches: {}",
                    document.html_url
                );
                let destination = crate::destination(&github, &repository.name_with_owner)?;
                ensure!(
                    destination.personal && !destination.fork,
                    "a referencing repository is outside personal scope"
                );
                let endpoint = endpoint(&document, &repository.name_with_owner)?;
                let current: Document = github.read(&endpoint)?;
                ensure!(
                    current.body == document.body,
                    "referencing document changed before migration"
                );
                let backup = backups.join(format!("{}.json", digest(endpoint.as_bytes())));
                if !backup.exists() {
                    fs::write(
                        backup,
                        serde_json::to_vec_pretty(
                            &serde_json::json!({"endpoint":endpoint,"body":old}),
                        )?,
                    )?;
                }
                let response = patch(&github, &endpoint, &serde_json::json!({"body":body}))?;
                ensure!(
                    response.body.as_deref() == Some(&body),
                    "cross-repository migration response differs"
                );
                let readback: Document = github.read(&endpoint)?;
                ensure!(
                    readback.body == response.body,
                    "cross-repository migration readback differs"
                );
            }
            for migration in &*migrations {
                for branch in &migration.manifest.branches {
                    if refers(&body, &migration.manifest.repository, &branch.name) {
                        *remaining
                            .get_mut(&branch_key(&migration.manifest.repository, &branch.name))
                            .context("branch inventory missing")? += 1;
                    }
                }
            }
        }
    }
    Ok(remaining)
}

struct RetirementPermit {
    path: PathBuf,
    token: String,
    permit: dotguard::figure_retirement::Permit,
}

impl RetirementPermit {
    fn acquire(
        directory: &Path,
        remote: &str,
        branch: &Branch,
        evidence: Evidence,
    ) -> Result<Self> {
        let token = crate::lease_token()?;
        let permit = dotguard::figure_retirement::Permit::verified(
            remote,
            &branch.name,
            &branch.oid,
            &token,
            evidence,
        )
        .context("figure retirement permit lacks verified evidence")?;
        let path = directory.join(dotguard::figure_retirement::FILE);
        let mut options = fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt as _;
            const PRIVATE_PERMIT_MODE: u32 = 0o600;
            options.mode(PRIVATE_PERMIT_MODE);
        }
        let mut file = options.open(&path).context("could not create the figure retirement permit; inspect the existing operation or filesystem error before recovery")?;
        let scoped = Self {
            path,
            token,
            permit,
        };
        let written = (|| -> Result<()> {
            file.write_all(&serde_json::to_vec(&scoped.permit)?)?;
            file.sync_all()?;
            Ok(())
        })();
        drop(file);
        written?;
        Ok(scoped)
    }
}

impl Drop for RetirementPermit {
    fn drop(&mut self) {
        platform::cleanup::remove_file(&self.path);
    }
}

fn retire(migration: &mut Migration, remaining: &BTreeMap<String, u64>) -> Result<()> {
    let github = Github::connect()?;
    let directory = &migration.directory;
    let manifest = &mut migration.manifest;
    for branch in manifest.branches.clone() {
        if manifest.retired.contains(&branch.name) {
            continue;
        }
        let current: serde_json::Value = github.read(&format!(
            "repos/{}/git/ref/heads/{}",
            manifest.repository, branch.name
        ))?;
        let evidence = Evidence {
            archive: if verified_archive(directory, manifest)? {
                Verification::Complete
            } else {
                Verification::Missing
            },
            inventory: Verification::Complete,
            documents: Verification::Complete,
            remaining_references: *remaining
                .get(&branch_key(&manifest.repository, &branch.name))
                .context("global reference inventory missing")?,
            branch: if current["object"]["sha"].as_str() == Some(&branch.oid) {
                BranchIdentity::Unchanged
            } else {
                BranchIdentity::Changed
            },
        };
        ensure!(
            may_retire(Mode::Apply, evidence),
            "figure branch retirement lacks complete evidence"
        );
        let remote = format!("{GITHUB}/{}.git", manifest.repository);
        let bare = directory.join("history.git");
        let permit = RetirementPermit::acquire(&bare, &remote, &branch, evidence)?;
        let output = Tool::Git
            .command()
            .current_dir(&bare)
            .env(dotguard::figure_retirement::TOKEN, &permit.token)
            .args(permit.permit.arguments())
            .output()?;
        ensure!(
            output.status.success(),
            "guarded figure retirement failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        drop(permit);
        ensure!(
            git(
                &directory.join("history.git"),
                &[
                    "ls-remote".into(),
                    remote.into(),
                    format!("refs/heads/{}", branch.name).into()
                ]
            )?
            .is_empty(),
            "retired figure branch still exists"
        );
        manifest.retired.insert(branch.name);
        save(&directory.join("manifest.json"), manifest)?;
    }
    println!(
        "{}: retired {} verified figure branch(es)",
        manifest.repository,
        manifest.retired.len()
    );
    Ok(())
}

pub fn run(action: &Action) -> Result<()> {
    let github = Github::connect()?;
    ensure!(
        github.login.eq_ignore_ascii_case(&action.owner),
        "figure migration must run as the repository owner"
    );
    let root = platform::home::home()
        .context("figure migration needs the user home")?
        .join(STATE);
    figure::private_directory(&root)?;
    let repositories = repositories(&github, &action.owner)?;
    let mut migrations = Vec::new();
    for repository in repositories.iter().filter(|repository| {
        action
            .repo
            .as_ref()
            .is_none_or(|selected| selected == &repository.name_with_owner)
            && (repository.diagram.is_some() || repository.screenshot.is_some())
    }) {
        let directory = process(action, repository, &root)?;
        let manifest = serde_json::from_slice(&fs::read(directory.join("manifest.json"))?)?;
        migrations.push(Migration {
            directory,
            manifest,
        });
    }
    if action.apply && action.retire {
        let remaining = global_sweep(&repositories, &mut migrations, &root)?;
        ensure!(
            remaining.values().all(|count| *count == 0),
            "figure branch references remain"
        );
        for migration in &mut migrations {
            retire(migration, &remaining)?;
        }
    }
    println!(
        "Inventoried {} owned repository/repositories; {} figure migration(s)",
        repositories.len(),
        migrations.len()
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn retirement_permission_requires_evidence_and_releases_its_repository_scope() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let branch = Branch {
            name: "diagrams".into(),
            oid: "a".repeat(SHA1_DIGITS),
        };
        let mut evidence = Evidence {
            archive: Verification::Complete,
            inventory: Verification::Complete,
            documents: Verification::Complete,
            remaining_references: 1,
            branch: BranchIdentity::Unchanged,
        };
        let path = directory.path().join(dotguard::figure_retirement::FILE);
        assert!(RetirementPermit::acquire(directory.path(), "origin", &branch, evidence).is_err());
        assert!(!path.exists());
        evidence.remaining_references = 0;
        let permit = RetirementPermit::acquire(directory.path(), "origin", &branch, evidence)?;
        let original = fs::read(&path)?;
        assert!(RetirementPermit::acquire(directory.path(), "origin", &branch, evidence).is_err());
        assert_eq!(fs::read(&path)?, original);
        drop(permit);
        assert!(!path.exists());
        Ok(())
    }

    fn fixture() -> Manifest {
        Manifest {
            version: 1,
            repository: "P4suta/project".into(),
            repository_id: 37,
            branches: vec![Branch {
                name: "diagrams".into(),
                oid: "a".repeat(40),
            }],
            files: vec![ArchivedFile {
                branch: "diagrams".into(),
                name: "figure.svg".into(),
                oid: "b".repeat(40),
                digest: "c".repeat(64),
            }],
            archive_issue: Some(24),
            mappings: [(
                "diagrams/figure.svg".into(),
                "https://github.com/user-attachments/assets/figure".into(),
            )]
            .into(),
            verified_documents: BTreeMap::new(),
            retired: BTreeSet::new(),
            bundle_digest: None,
        }
    }

    #[test]
    fn historical_image_rewriting_preserves_text_and_alternative_text() -> Result<()> {
        let body = "The parser preserves edits.\n\n![Original description](https://github.com/P4suta/project/blob/diagrams/figure.svg?raw=true)\n\nThe failure path stays visible.\n";
        let expected = "The parser preserves edits.\n\n![Original description](https://github.com/user-attachments/assets/figure)\n\nThe failure path stays visible.\n";
        assert_eq!(rewrite(body, &fixture())?, expected);
        assert!(!refers(expected, "P4suta/project", "diagrams"));
        Ok(())
    }

    #[test]
    fn historical_raw_urls_and_repeated_images_share_the_uploaded_asset() -> Result<()> {
        let body = "![First](https://raw.githubusercontent.com/P4suta/project/diagrams/figure.svg)\n![Second](https://github.com/P4suta/project/raw/diagrams/figure.svg)\n";
        let rewritten = rewrite(body, &fixture())?;
        assert_eq!(
            rewritten
                .matches("https://github.com/user-attachments/assets/figure")
                .count(),
            2
        );
        assert!(rewritten.contains("![First]"));
        assert!(rewritten.contains("![Second]"));
        Ok(())
    }

    #[test]
    fn unuploaded_files_and_path_traversal_cannot_authorize_migration() {
        let mut manifest = fixture();
        manifest.mappings.clear();
        assert!(rewrite("Original body", &manifest).is_err());
        manifest.files[0].name = "../outside.svg".into();
        assert!(validate_manifest(&manifest).is_err());
        manifest.files[0].name = "figure.svg".into();
        manifest.branches[0].name = "main".into();
        assert!(validate_manifest(&manifest).is_err());
    }

    #[test]
    fn archive_recovery_sees_uploaded_and_pending_images_without_reading_code_examples() {
        let body = "![diagrams/ready.svg](https://github.com/user-attachments/assets/ready)\n![diagrams/pending.svg](</local/pending.svg>)\n\n```md\n![example](example.svg)\n```\n";
        let images = image_addresses(body);
        assert_eq!(images.len(), 2);
        assert_eq!(images[0].0, "diagrams/ready.svg");
        assert_eq!(images[1].1, "/local/pending.svg");
    }

    #[test]
    fn resumed_archive_repairs_only_the_pending_image_reference() -> Result<()> {
        let body = "![diagrams/figure.svg](</local/figure.svg> \"Original title\")\n\n```md\n![diagrams/figure.svg](</local/figure.svg>)\n```\n";
        let expected = "![diagrams/figure.svg](<https://github.com/user-attachments/assets/figure> \"Original title\")\n\n```md\n![diagrams/figure.svg](</local/figure.svg>)\n```\n";
        assert_eq!(repair_archive_body(body, &fixture().mappings)?, expected);
        assert_eq!(
            repair_archive_body(expected, &fixture().mappings)?,
            expected
        );
        Ok(())
    }

    #[test]
    fn archived_readme_is_preserved_as_text_without_a_media_upload() -> Result<()> {
        let directory = tempfile::tempdir()?;
        fs::create_dir(directory.path().join("diagrams"))?;
        let source = "# Figure archive\n\nPreserve the original filenames.\n";
        fs::write(directory.path().join("diagrams/README.md"), source)?;
        let file = ArchivedFile {
            branch: "diagrams".into(),
            name: "README.md".into(),
            oid: "a".repeat(SHA1_DIGITS),
            digest: digest(source.as_bytes()),
        };
        let (body, attachment) = archive_fragment(directory.path(), &file)?;
        assert!(attachment.is_none());
        assert!(body.contains("## diagrams/README.md\n"));
        assert!(body.contains("<summary>File source</summary>"));
        assert!(body.contains(source));
        assert!(!body.contains("```d2"));
        Ok(())
    }
}
