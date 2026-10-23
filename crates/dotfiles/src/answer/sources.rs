use anyhow::{Context, Result, ensure};
use platform::tool::Tool;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::io::Read as _;
use std::process::Stdio;
use url::Url;

pub(super) const MAX_SOURCE_BYTES: usize = 1_048_576;
const MAX_EXCERPT_BYTES: usize = 4096;
const RESPONSE_METADATA_BYTES: usize = 64;
const PUBLIC_HOSTS: &[&str] = &[
    "ninjal.ac.jp",
    "ndl.go.jp",
    "bunka.go.jp",
    "jisc.go.jp",
    "jis.go.jp",
    "w3.org",
    "unicode.org",
    "iso.org",
    "nhk.or.jp",
];
const DICTIONARY_HOSTS: &[&str] = &[
    "japanknowledge.com",
    "dictionary.sanseido-publ.co.jp",
    "daijisen.jp",
    "kotobank.jp",
];
const TECHNICAL_HOSTS: &[&str] = &[
    "lean-lang.org",
    "docs.rs",
    "doc.rust-lang.org",
    "rust-lang.github.io",
    "learn.chatgpt.com",
    "developers.openai.com",
    "platform.openai.com",
    "code.claude.com",
    "plato.stanford.edu",
    "writingcenter.fas.harvard.edu",
    "amstat.org",
    "cochrane.org",
];

#[derive(Clone, Copy, Debug, Deserialize, Serialize, Eq, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub enum Authority {
    Public,
    Dictionary,
    Technical,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Source {
    pub url: String,
    pub authority: Authority,
    pub title: String,
    pub publisher: String,
    pub locator: String,
    pub excerpt: String,
}

fn at_host(host: &str, domains: &[&str]) -> bool {
    domains.iter().any(|domain| {
        host == *domain
            || host
                .strip_suffix(domain)
                .is_some_and(|prefix| prefix.ends_with('.'))
    })
}

pub fn validate(source: &Source) -> Result<Url> {
    let url = Url::parse(&source.url).context("source URL is invalid")?;
    ensure!(
        url.scheme() == "https"
            && url.username().is_empty()
            && url.password().is_none()
            && url.port().is_none(),
        "sources require HTTPS without credentials or a custom port"
    );
    let host = url.host_str().context("source URL has no host")?;
    let domains = match source.authority {
        Authority::Public => PUBLIC_HOSTS,
        Authority::Dictionary => DICTIONARY_HOSTS,
        Authority::Technical => TECHNICAL_HOSTS,
    };
    ensure!(
        at_host(host, domains),
        "source host {host} does not belong to its registered authority class"
    );
    ensure!(
        !source.title.trim().is_empty()
            && !source.publisher.trim().is_empty()
            && !source.locator.trim().is_empty(),
        "source needs its title, publisher, and entry locator"
    );
    ensure!(
        !source.excerpt.trim().is_empty() && source.excerpt.len() <= MAX_EXCERPT_BYTES,
        "source excerpt is empty or exceeds its byte bound"
    );
    Ok(url)
}

fn response_body(bytes: Vec<u8>) -> Result<String> {
    let response =
        String::from_utf8(bytes).context("source is not UTF-8 text; use a textual entry")?;
    let body = response.strip_suffix("\nHTTP_STATUS:200").context(
        "source did not return HTTP 200; redirects and unavailable entries supply no evidence",
    )?;
    ensure!(
        body.len() <= MAX_SOURCE_BYTES,
        "source exceeds its byte bound"
    );
    Ok(body.to_owned())
}

fn fetch(url: &Url) -> Result<String> {
    let diagnostics = tempfile::NamedTempFile::new()?;
    let mut child = Tool::Curl
        .command()
        .args([
            "--disable",
            "--globoff",
            "--proto",
            "=https",
            "--fail",
            "--silent",
            "--show-error",
            "--max-time",
            "20",
            "--max-filesize",
            "1048576",
            "--write-out",
            "\nHTTP_STATUS:%{http_code}",
            "--url",
            url.as_str(),
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::from(diagnostics.reopen()?))
        .spawn()
        .context("retrieve the identified primary source")?;
    let mut bytes = Vec::new();
    let read = child
        .stdout
        .take()
        .context("source transport has no output")?
        .take((MAX_SOURCE_BYTES + RESPONSE_METADATA_BYTES + 1) as u64)
        .read_to_end(&mut bytes);
    if read.is_err() || bytes.len() > MAX_SOURCE_BYTES + RESPONSE_METADATA_BYTES {
        platform::cleanup::report(child.kill(), "stop the owned oversized source response");
        platform::cleanup::report(child.wait(), "wait for the owned source response");
        read.context("read the bounded source response")?;
        anyhow::bail!("source exceeds its byte bound");
    }
    let status = child.wait()?;
    ensure!(status.success(), "source retrieval is incomplete: {status}");
    response_body(bytes)
}

pub fn check(source: &Source, fetched: &mut BTreeMap<String, String>) -> Result<()> {
    let url = validate(source)?;
    if !fetched.contains_key(url.as_str()) {
        fetched.insert(url.to_string(), fetch(&url)?);
    }
    check_saved(source, fetched)?;
    Ok(())
}

pub fn check_saved(source: &Source, fetched: &BTreeMap<String, String>) -> Result<()> {
    let url = validate(source)?;
    ensure!(
        fetched
            .get(url.as_str())
            .is_some_and(|body| body.contains(&source.excerpt)),
        "source does not contain the declared entry excerpt"
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transport_status_and_body_bounds_cannot_be_confused() {
        assert_eq!(
            response_body(b"record\nHTTP_STATUS:200".to_vec()).unwrap(),
            "record"
        );
        for invalid in [
            b"record".as_slice(),
            b"record\nHTTP_STATUS:302",
            b"record\nHTTP_STATUS:404",
            b"\xff\nHTTP_STATUS:200",
        ] {
            assert!(response_body(invalid.to_vec()).is_err());
        }
        let mut large = vec![b'x'; MAX_SOURCE_BYTES];
        large.extend_from_slice(b"\nHTTP_STATUS:200");
        assert!(response_body(large.clone()).is_ok());
        large.insert(0, b'x');
        assert!(response_body(large).is_err());
    }

    fn source(url: &str, authority: Authority) -> Source {
        Source {
            url: url.to_owned(),
            authority,
            title: "Dictionary entry".to_owned(),
            publisher: "Publisher".to_owned(),
            locator: "Entry".to_owned(),
            excerpt: "Actual entry text".to_owned(),
        }
    }

    #[test]
    fn misleading_source_hosts_and_wrong_classes_are_rejected() {
        for url in [
            "https://ninjal.ac.jp.example.com/",
            "https://example.com/ninjal.ac.jp",
            "https://ninjal.ac.jp@localhost/",
            "http://clrd.ninjal.ac.jp/",
            "https://clrd.ninjal.ac.jp:8443/",
        ] {
            assert!(validate(&source(url, Authority::Public)).is_err(), "{url}");
        }
        assert!(
            validate(&source(
                "https://clrd.ninjal.ac.jp/unidic/",
                Authority::Public
            ))
            .is_ok()
        );
        assert!(
            validate(&source(
                "https://kotobank.jp/word/example",
                Authority::Public
            ))
            .is_err()
        );
        assert!(
            validate(&source(
                "https://kotobank.jp/word/example",
                Authority::Dictionary
            ))
            .is_ok()
        );
    }

    #[test]
    fn an_unrelated_page_cannot_supply_an_entry_excerpt() {
        let source = source("https://clrd.ninjal.ac.jp/unidic/", Authority::Public);
        let mut fetched = BTreeMap::from([(source.url.clone(), "Unrelated contents".to_owned())]);
        assert!(check(&source, &mut fetched).is_err());
        fetched.insert(source.url.clone(), format!("<p>{}</p>", source.excerpt));
        assert!(check(&source, &mut fetched).is_ok());
    }
}
