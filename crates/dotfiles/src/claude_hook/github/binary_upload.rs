use anyhow::{Context, Result, ensure};
use std::collections::BTreeMap;
use std::path::{Component, Path};

const UPLOAD_HOST: &str = "uploads.github.com";
const UPLOAD_PATH: &str = "/user-attachments/assets";
const REQUEST_CONTENT_TYPE: &str = "application/octet-stream";
const PARAMETER_COUNT: usize = 3;
const MEDIA_TYPES: [&str; 8] = [
    "image/png",
    "image/jpeg",
    "image/gif",
    "image/webp",
    "image/svg+xml",
    "video/mp4",
    "video/quicktime",
    "video/webm",
];

#[derive(Debug)]
pub(super) struct Upload {
    input: String,
}

impl Upload {
    pub(super) fn verify(&self, directory: &Path) -> Result<()> {
        let path = super::super::native_path(&self.input);
        ensure!(
            path.is_absolute() || !matches!(path.components().next(), Some(Component::Prefix(_))),
            "GitHub binary attachment has an unresolved drive-relative path"
        );
        let path = if path.is_absolute() {
            path
        } else {
            directory.join(path)
        };
        ensure!(
            path.metadata()
                .with_context(|| format!("inspect GitHub binary attachment {}", path.display()))?
                .is_file(),
            "GitHub binary attachment is not a regular file"
        );
        Ok(())
    }
}

fn endpoint(value: &str) -> Option<()> {
    let url = url::Url::parse(value).ok()?;
    if url.scheme() != "https"
        || url.host_str() != Some(UPLOAD_HOST)
        || url.path() != UPLOAD_PATH
        || url.port().is_some()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.fragment().is_some()
    {
        return None;
    }
    let mut parameters = BTreeMap::new();
    for (name, value) in url.query_pairs() {
        if parameters.insert(name, value).is_some() {
            return None;
        }
    }
    let name = parameters.get("name")?;
    if parameters.len() != PARAMETER_COUNT
        || name.is_empty()
        || name.contains(['\0', '\r', '\n'])
        || !MEDIA_TYPES.contains(&parameters.get("content_type")?.as_ref())
        || parameters.get("repository_id")?.parse::<u64>().ok()? == 0
    {
        return None;
    }
    Some(())
}

#[derive(Default)]
struct Request {
    endpoint: Option<String>,
    method: Option<String>,
    content_type: Option<String>,
    accept: Option<String>,
    hostname: Option<String>,
    input: Option<String>,
}

impl Request {
    fn header(&mut self, value: &str) -> Option<()> {
        let (name, value) = value.split_once(':')?;
        let slot = match name.trim().to_ascii_lowercase().as_str() {
            "content-type" => &mut self.content_type,
            "accept" => &mut self.accept,
            _ => return None,
        };
        if slot.replace(value.trim().to_owned()).is_some() {
            return None;
        }
        Some(())
    }

    fn finish(self) -> Option<Upload> {
        endpoint(self.endpoint.as_deref()?)?;
        let input = self.input?;
        if self
            .method
            .as_deref()
            .is_some_and(|method| method != "POST")
            || self
                .hostname
                .as_deref()
                .is_some_and(|host| host != "github.com")
            || !self
                .content_type?
                .eq_ignore_ascii_case(REQUEST_CONTENT_TYPE)
            || input.is_empty()
            || input == "-"
        {
            return None;
        }
        Some(Upload { input })
    }
}

pub(super) fn parse(arguments: &[String]) -> Option<Upload> {
    let mut request = Request::default();
    let mut index = 0;
    while index < arguments.len() {
        if super::option(arguments, &mut index, "--jq", Some("-q"))
            .ok()?
            .is_some()
            || super::option(arguments, &mut index, "--template", Some("-t"))
                .ok()?
                .is_some()
        {
            continue;
        }
        if let Some(value) = super::option(arguments, &mut index, "--method", Some("-X")).ok()? {
            if request.method.replace(value).is_some() {
                return None;
            }
        } else if let Some(value) =
            super::option(arguments, &mut index, "--header", Some("-H")).ok()?
        {
            request.header(&value)?;
        } else if let Some(value) = super::option(arguments, &mut index, "--hostname", None).ok()? {
            if request.hostname.replace(value).is_some() {
                return None;
            }
        } else if let Some(value) = super::option(arguments, &mut index, "--input", None).ok()? {
            if request.input.replace(value).is_some() {
                return None;
            }
        } else if matches!(
            arguments[index].as_str(),
            "--silent" | "--include" | "-i" | "--verbose"
        ) {
            index += 1;
        } else if !arguments[index].starts_with('-') {
            if request.endpoint.replace(arguments[index].clone()).is_some() {
                return None;
            }
            index += 1;
        } else {
            return None;
        }
    }
    request.finish()
}
