//! Images that personal PR and issue bodies show from a branch of their own repository, such as `diagrams` or `screenshots`.

use crate::{GITHUB, gh};
use anyhow::{Context, Result, ensure};
use base64::Engine as _;
use std::ffi::OsString;
use std::io::Write as _;

/// The mode of a regular file in a Git tree.
const FILE_MODE: &str = "100644";
/// The suffix of an image address that makes GitHub serve the file itself.
const RAW: &str = "?raw=true";

/// The bytes of one file that a figure branch stores.
pub(crate) enum Content<'a> {
    Text(&'a str),
    Binary(&'a [u8]),
}

/// One file to add to a figure branch.
pub(crate) struct File<'a> {
    pub(crate) name: String,
    pub(crate) content: Content<'a>,
}

/// The address prefix of every file that a repository's branch stores.
pub(crate) fn prefix(repository: &str, branch: &str) -> String {
    format!("{GITHUB}/{repository}/blob/{branch}/")
}

/// The Markdown image line that shows a stored file.
pub(crate) fn image_line(repository: &str, branch: &str, file: &str, alt: &str) -> String {
    format!(
        "![{}]({}{file}{RAW})",
        alt.trim(),
        prefix(repository, branch)
    )
}

/// The stored filename that an image address names on a repository's branch, if it names one.
pub(crate) fn stored_name<'a>(repository: &str, branch: &str, address: &'a str) -> Option<&'a str> {
    let prefix = prefix(repository, branch);
    address
        .get(..prefix.len())
        .filter(|start| start.eq_ignore_ascii_case(&prefix))?;
    address.get(prefix.len()..)?.strip_suffix(RAW)
}

/// Refuses an image description that a Markdown image can't carry on one line.
///
/// # Errors
/// Returns an error for an empty description or one with a line break or a bracket.
pub(crate) fn check_alt(alt: &str) -> Result<()> {
    ensure!(
        !alt.trim().is_empty() && !alt.contains(['\n', '[', ']']),
        "--alt must describe the image on one line without brackets"
    );
    Ok(())
}

fn json_file(value: &serde_json::Value) -> Result<tempfile::NamedTempFile> {
    let mut file = tempfile::NamedTempFile::new()?;
    serde_json::to_writer(file.as_file_mut(), value)?;
    file.as_file_mut().flush()?;
    Ok(file)
}

/// Sends one GitHub API request with a JSON body and returns the response object.
fn api(method: &str, endpoint: &str, body: &serde_json::Value) -> Result<serde_json::Value> {
    let input = json_file(body)?;
    let args: Vec<OsString> = vec![
        "api".into(),
        "--hostname".into(),
        "github.com".into(),
        "--method".into(),
        method.into(),
        endpoint.into(),
        "--input".into(),
        input.path().as_os_str().to_owned(),
    ];
    serde_json::from_str(&gh(&args)?).with_context(|| format!("read the {endpoint} response"))
}

fn sha(response: &serde_json::Value) -> Result<String> {
    response["sha"]
        .as_str()
        .map(str::to_owned)
        .context("GitHub returned no object id")
}

/// The tip of a figure branch.
struct Tip {
    commit: String,
    tree: String,
}

/// The tip of a branch, if the branch exists, and whether it already holds the named file.
fn branch_state(repository: &str, branch: &str, file: &str) -> Result<(Option<Tip>, bool)> {
    let (owner, repository_name) = repository
        .split_once('/')
        .context("repository must be OWNER/REPO")?;
    let query = "query($owner:String!,$name:String!,$ref:String!,$expression:String!){repository(owner:$owner,name:$name){ref(qualifiedName:$ref){target{oid ... on Commit{tree{oid}}}} object(expression:$expression){oid}}}";
    let response = api(
        "POST",
        "graphql",
        &serde_json::json!({
            "query": query,
            "variables": {
                "owner": owner,
                "name": repository_name,
                "ref": format!("refs/heads/{branch}"),
                "expression": format!("{branch}:{file}"),
            }
        }),
    )?;
    let repository = &response["data"]["repository"];
    ensure!(
        repository.is_object(),
        "GitHub returned no repository for the image: {response}"
    );
    let target = &repository["ref"]["target"];
    let tip = match (target["oid"].as_str(), target["tree"]["oid"].as_str()) {
        (Some(commit), Some(tree)) => Some(Tip {
            commit: commit.to_owned(),
            tree: tree.to_owned(),
        }),
        (None, None) if repository["ref"].is_null() => None,
        _ => anyhow::bail!("the {branch} branch doesn't point at a commit: {response}"),
    };
    Ok((tip, repository["object"]["oid"].is_string()))
}

/// Adds files to a branch in one commit, and starts the branch as an orphan when it's absent.
/// The first file names the image, and a branch that already holds it keeps its tip.
///
/// # Errors
/// Returns an error when GitHub refuses a request or another writer moved the branch first.
pub(crate) fn store(repository: &str, branch: &str, files: &[File<'_>]) -> Result<()> {
    let image = files.first().context("a figure needs its image file")?;
    let (tip, stored) = branch_state(repository, branch, &image.name)?;
    if stored {
        return Ok(());
    }
    let endpoint = |path: &str| format!("repos/{repository}/git/{path}");
    let mut entries = Vec::new();
    for file in files {
        let body = match file.content {
            Content::Text(text) => serde_json::json!({"content": text, "encoding": "utf-8"}),
            Content::Binary(bytes) => serde_json::json!({
                "content": base64::engine::general_purpose::STANDARD.encode(bytes),
                "encoding": "base64",
            }),
        };
        let blob = api("POST", &endpoint("blobs"), &body)?;
        entries.push(serde_json::json!({
            "path": file.name,
            "mode": FILE_MODE,
            "type": "blob",
            "sha": sha(&blob)?,
        }));
    }
    let mut tree = serde_json::json!({"tree": entries});
    if let Some(tip) = &tip {
        tree["base_tree"] = serde_json::json!(tip.tree);
    }
    let tree = sha(&api("POST", &endpoint("trees"), &tree)?)?;
    let commit = sha(&api(
        "POST",
        &endpoint("commits"),
        &serde_json::json!({
            "message": format!("docs: add {}", image.name),
            "tree": tree,
            "parents": tip.iter().map(|tip| &tip.commit).collect::<Vec<_>>(),
        }),
    )?)?;
    if tip.is_some() {
        api(
            "PATCH",
            &endpoint(&format!("refs/heads/{branch}")),
            &serde_json::json!({"sha": commit, "force": false}),
        )?;
    } else {
        api(
            "POST",
            &endpoint("refs"),
            &serde_json::json!({"ref": format!("refs/heads/{branch}"), "sha": commit}),
        )?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{check_alt, image_line, stored_name};

    #[test]
    fn an_image_line_names_its_file_on_the_branch() {
        let line = image_line("P4suta/project", "screenshots", "a.png", " Screen ");
        assert_eq!(
            line,
            "![Screen](https://github.com/P4suta/project/blob/screenshots/a.png?raw=true)"
        );
        let address = line.trim_start_matches("![Screen](").trim_end_matches(')');
        assert_eq!(
            stored_name("p4suta/PROJECT", "screenshots", address),
            Some("a.png")
        );
        assert_eq!(stored_name("P4suta/project", "diagrams", address), None);
        assert_eq!(stored_name("P4suta/other", "screenshots", address), None);
    }

    #[test]
    fn a_description_stays_on_one_line_without_brackets() {
        assert!(check_alt("Settings screen").is_ok());
        for alt in ["", "  ", "a\nb", "a [b]"] {
            assert!(check_alt(alt).is_err(), "{alt:?}");
        }
    }
}
