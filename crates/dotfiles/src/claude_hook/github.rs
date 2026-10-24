//! Inspects the exact literal arguments of GitHub publication commands.

use crate::body_rules::{
    EscapeStep, Quote, RestArgument, RestRead, WordStep, escape_step, rest_read, word_step,
};
use anyhow::{Context, Result, ensure};
use serde::Deserialize;
use serde::de::{Error, MapAccess, SeqAccess, Visitor};
use std::collections::BTreeSet;
use std::fmt;
use std::io::Read;
use std::path::Path;

/// The byte budget for a literal body or one JSON input.
pub const MAX_BODY_INPUT: usize = 1024 * 1024;
const MAX_BODIES: usize = 64;

/// Decodes a parsed shell word when its quoting and values are static.
#[must_use]
pub fn literal_word(spelling: &str) -> Option<String> {
    if spelling.is_empty() || spelling.len() > MAX_BODY_INPUT {
        return None;
    }
    let mut quote = Quote::Unquoted;
    let mut characters = spelling.chars();
    let mut value = String::new();
    while let Some(character) = characters.next() {
        match word_step(quote, character) {
            WordStep::Quote(next) => quote = next,
            WordStep::Emit => value.push(character),
            WordStep::Opaque => return None,
            WordStep::Escape(mode) => {
                let escaped = characters.next()?;
                match escape_step(mode, escaped) {
                    EscapeStep::Emit => value.push(escaped),
                    EscapeStep::KeepBackslash => {
                        value.push('\\');
                        value.push(escaped);
                    }
                    EscapeStep::Continuation => {}
                    EscapeStep::Opaque => return None,
                }
            }
        }
    }
    (quote == Quote::Unquoted).then_some(value)
}

#[derive(Default)]
struct JsonBodies {
    text: Option<String>,
    bodies: Vec<String>,
}

impl JsonBodies {
    fn append<E: Error>(&mut self, mut child: Self) -> std::result::Result<(), E> {
        self.bodies.append(&mut child.bodies);
        if self.bodies.len() > MAX_BODIES {
            return Err(E::custom(
                "GitHub body inspection exceeds its body-count limit",
            ));
        }
        Ok(())
    }
}

struct JsonVisitor;

impl<'de> Visitor<'de> for JsonVisitor {
    type Value = JsonBodies;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("a JSON request with string-valued bodies")
    }

    fn visit_str<E: Error>(self, value: &str) -> std::result::Result<Self::Value, E> {
        Ok(JsonBodies {
            text: Some(value.to_owned()),
            ..JsonBodies::default()
        })
    }

    fn visit_string<E: Error>(self, value: String) -> std::result::Result<Self::Value, E> {
        Ok(JsonBodies {
            text: Some(value),
            ..JsonBodies::default()
        })
    }

    fn visit_bool<E: Error>(self, _: bool) -> std::result::Result<Self::Value, E> {
        Ok(JsonBodies::default())
    }

    fn visit_i64<E: Error>(self, _: i64) -> std::result::Result<Self::Value, E> {
        Ok(JsonBodies::default())
    }

    fn visit_u64<E: Error>(self, _: u64) -> std::result::Result<Self::Value, E> {
        Ok(JsonBodies::default())
    }

    fn visit_f64<E: Error>(self, _: f64) -> std::result::Result<Self::Value, E> {
        Ok(JsonBodies::default())
    }

    fn visit_unit<E: Error>(self) -> std::result::Result<Self::Value, E> {
        Ok(JsonBodies::default())
    }

    fn visit_seq<A: SeqAccess<'de>>(
        self,
        mut sequence: A,
    ) -> std::result::Result<Self::Value, A::Error> {
        let mut result = JsonBodies::default();
        while let Some(child) = sequence.next_element::<JsonBodies>()? {
            result.append(child)?;
        }
        Ok(result)
    }

    fn visit_map<A: MapAccess<'de>>(
        self,
        mut map: A,
    ) -> std::result::Result<Self::Value, A::Error> {
        let mut keys = BTreeSet::new();
        let mut result = JsonBodies::default();
        while let Some(key) = map.next_key::<String>()? {
            if !keys.insert(key.clone()) {
                return Err(A::Error::custom("duplicate JSON request key"));
            }
            let mut child = map.next_value::<JsonBodies>()?;
            if key == "body" {
                let text = child
                    .text
                    .take()
                    .ok_or_else(|| A::Error::custom("GitHub body is not a string"))?;
                if text.contains('\0') {
                    return Err(A::Error::custom("GitHub body contains NUL"));
                }
                child.bodies.push(text);
            } else if key == "query"
                && child
                    .text
                    .as_ref()
                    .is_some_and(|query| query.contains("mutation") && query.contains("body"))
            {
                return Err(A::Error::custom("GraphQL mutation body cannot be resolved"));
            }
            result.append(child)?;
        }
        Ok(result)
    }
}

impl<'de> Deserialize<'de> for JsonBodies {
    fn deserialize<D: serde::Deserializer<'de>>(
        deserializer: D,
    ) -> std::result::Result<Self, D::Error> {
        deserializer.deserialize_any(JsonVisitor)
    }
}

/// Reads every nested JSON body without accepting duplicate keys or opaque GraphQL body literals.
///
/// # Errors
///
/// Refuses malformed, oversize, ambiguous, non-string, or unsupported request bodies.
pub fn json_bodies(text: &str) -> Result<Vec<String>> {
    ensure!(
        text.len() <= MAX_BODY_INPUT,
        "GitHub JSON input exceeds its byte limit"
    );
    let parsed: JsonBodies = serde_json::from_str(text).context("inspect GitHub JSON bodies")?;
    Ok(parsed.bodies)
}

#[derive(Debug)]
pub(crate) enum Body {
    Text {
        channel: prose::Channel,
        text: String,
    },
    Unreadable(String),
}

#[derive(Debug)]
enum Input {
    Text(String),
    File { path: String, json: bool },
}

struct Target<'a> {
    channel: prose::Channel,
    verb: &'a str,
    arguments: &'a [String],
    api: bool,
}

/// A decoded publication request whose file inputs still require an external read.
#[derive(Debug)]
pub struct Publication {
    channel: prose::Channel,
    inputs: Vec<Input>,
    direct: bool,
}

/// Decodes exact static shell words without reading a file or executing a command.
///
/// # Errors
///
/// Refuses unresolved words, unsupported command contracts, and implicit body sources.
pub fn decode(spellings: &[&str]) -> Result<Option<Publication>> {
    let values: Vec<String> = spellings
        .iter()
        .map(|word| literal_word(word).context("GitHub argument contains an unresolved expansion"))
        .collect::<Result<_>>()?;
    let Some(start) = gh_start(&values)? else {
        return Ok(None);
    };
    let Some(target) = target(&values[start + 1..])? else {
        return Ok(None);
    };
    Ok(Some(Publication {
        channel: target.channel,
        inputs: inputs(&target)?,
        direct: start == 0,
    }))
}

fn option(
    arguments: &[String],
    index: &mut usize,
    long: &str,
    short: Option<&str>,
) -> Result<Option<String>> {
    let argument = &arguments[*index];
    if argument == long || short == Some(argument.as_str()) {
        let value = arguments
            .get(*index + 1)
            .context("GitHub body option has no value")?;
        *index += 2;
        return Ok(Some(value.clone()));
    }
    if let Some(value) = argument
        .strip_prefix(long)
        .and_then(|value| value.strip_prefix('='))
    {
        *index += 1;
        return Ok(Some(value.to_owned()));
    }
    if let Some(value) = short
        .and_then(|short| argument.strip_prefix(short))
        .filter(|value| !value.is_empty())
    {
        *index += 1;
        return Ok(Some(value.strip_prefix('=').unwrap_or(value).to_owned()));
    }
    Ok(None)
}

fn gh_start(words: &[String]) -> Result<Option<usize>> {
    let mut index = 0;
    while let Some(word) = words.get(index) {
        if word.split_once('=').is_some_and(|(name, _)| {
            !name.is_empty()
                && name
                    .chars()
                    .all(|character| character.is_ascii_alphanumeric() || character == '_')
        }) {
            index += 1;
            continue;
        }
        match crate::shell_write_rules::program(word).as_str() {
            "gh" => return Ok(Some(index)),
            "env" => {
                index += 1;
                while let Some(flag) = words.get(index) {
                    if matches!(flag.as_str(), "--help" | "--version") {
                        return Ok(None);
                    }
                    if matches!(flag.as_str(), "-u" | "--unset" | "-C" | "--chdir") {
                        ensure!(
                            words.get(index + 1).is_some(),
                            "environment option has no value"
                        );
                        index += 2;
                    } else if flag.starts_with("--unset=")
                        || flag.starts_with("--chdir=")
                        || matches!(flag.as_str(), "-i" | "--ignore-environment")
                    {
                        index += 1;
                    } else if flag == "--" {
                        index += 1;
                        break;
                    } else if flag.starts_with('-') {
                        anyhow::bail!(
                            "environment command cannot supply a known GitHub argument vector"
                        );
                    } else {
                        break;
                    }
                }
            }
            "command" | "exec" | "nohup" => {
                index += 1;
                if words.get(index).is_some_and(|word| {
                    matches!(word.as_str(), "-v" | "-V" | "--help" | "--version")
                }) {
                    return Ok(None);
                }
                if words.get(index).is_some_and(|word| word == "--") {
                    index += 1;
                }
                ensure!(
                    !words.get(index).is_some_and(|word| word.starts_with('-')),
                    "command wrapper options are unavailable for GitHub inspection"
                );
            }
            "mise" => {
                if !words
                    .get(index + 1)
                    .is_some_and(|word| matches!(word.as_str(), "x" | "exec"))
                {
                    return Ok(None);
                }
                index += 2;
                if words.get(index).is_some_and(|word| word == "--") {
                    index += 1;
                }
            }
            "sudo" | "nice" | "xargs" | "time" | "timeout" | "gtimeout" | "setsid" | "stdbuf"
            | "builtin"
                if words[index + 1..]
                    .iter()
                    .any(|word| crate::shell_write_rules::program(word) == "gh") =>
            {
                anyhow::bail!("GitHub wrapper can change arguments or the execution context");
            }
            _ => return Ok(None),
        }
    }
    Ok(None)
}

fn rest_endpoint(argument: &str) -> bool {
    let path = argument.strip_prefix('/').unwrap_or(argument);
    (path.starts_with("repos/") || path.starts_with("repositories/"))
        && path
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"./_-".contains(&byte))
        && path.split('/').all(|part| !matches!(part, "" | "." | ".."))
}

fn explicit_rest_get(arguments: &[String]) -> bool {
    let mut state = RestRead::Empty;
    let mut index = 0;
    while let Some(argument) = arguments.get(index) {
        let kind = match argument.as_str() {
            "--method" | "-X" if arguments.get(index + 1).is_some_and(|value| value == "GET") => {
                index += 1;
                RestArgument::GetMethod
            }
            "--method=GET" | "-XGET" => RestArgument::GetMethod,
            "--paginate" | "--slurp" | "--include" | "--silent" | "--verbose" => {
                RestArgument::Output
            }
            path if rest_endpoint(path) => RestArgument::Endpoint,
            _ => RestArgument::Other,
        };
        state = rest_read(state, kind);
        index += 1;
    }
    state == RestRead::Complete
}

fn api_target(arguments: &[String]) -> Option<Target<'_>> {
    if explicit_rest_get(arguments) {
        return None;
    }
    let channel = if arguments
        .iter()
        .any(|argument| argument.contains("/pulls/"))
    {
        prose::Channel::Pr
    } else {
        prose::Channel::Issue
    };
    Some(Target {
        channel,
        verb: "api",
        arguments,
        api: true,
    })
}

fn target(arguments: &[String]) -> Result<Option<Target<'_>>> {
    let mut index = 0;
    while index < arguments.len() {
        if option(arguments, &mut index, "--repo", Some("-R"))?.is_some() {
            continue;
        }
        break;
    }
    let Some(group) = arguments.get(index) else {
        return Ok(None);
    };
    index += 1;
    if matches!(group.as_str(), "--help" | "--version" | "help") {
        return Ok(None);
    }
    if group == "api" {
        return Ok(api_target(&arguments[index..]));
    }
    while index < arguments.len() {
        if option(arguments, &mut index, "--repo", Some("-R"))?.is_some() {
            continue;
        }
        break;
    }
    if group == "release"
        && arguments
            .get(index)
            .is_some_and(|verb| matches!(verb.as_str(), "view" | "list"))
    {
        return Ok(None);
    }
    if !matches!(group.as_str(), "pr" | "issue") {
        ensure!(
            matches!(
                group.as_str(),
                "auth"
                    | "config"
                    | "alias"
                    | "completion"
                    | "cache"
                    | "run"
                    | "workflow"
                    | "search"
                    | "repo"
                    | "label"
                    | "org"
                    | "secret"
                    | "variable"
                    | "ssh-key"
                    | "gpg-key"
                    | "attestation"
                    | "status"
            ),
            "GitHub command has no supported body contract"
        );
        return Ok(None);
    }
    let Some(verb) = arguments.get(index) else {
        return Ok(None);
    };
    if matches!(
        verb.as_str(),
        "list" | "view" | "checks" | "status" | "diff" | "checkout" | "help" | "--help"
    ) {
        return Ok(None);
    }
    if matches!(
        verb.as_str(),
        "delete" | "develop" | "lock" | "unlock" | "pin" | "unpin" | "ready" | "transfer"
    ) {
        return Ok(None);
    }
    ensure!(
        matches!(
            verb.as_str(),
            "create" | "edit" | "comment" | "review" | "close" | "reopen" | "merge"
        ),
        "GitHub command has no supported body contract"
    );
    Ok(Some(Target {
        channel: if group == "pr" {
            prose::Channel::Pr
        } else {
            prose::Channel::Issue
        },
        verb,
        arguments: &arguments[index + 1..],
        api: false,
    }))
}

enum ArgumentInput {
    Body(Input),
    Metadata,
    Unmatched,
}

fn api_input(arguments: &[String], index: &mut usize) -> Result<ArgumentInput> {
    let raw = option(arguments, index, "--raw-field", Some("-f"))?;
    let field = if raw.is_none() {
        option(arguments, index, "--field", Some("-F"))?
    } else {
        None
    };
    if let Some(value) = raw.as_ref().or(field.as_ref()) {
        let (key, value) = value.split_once('=').context("GitHub field has no value")?;
        let body = key.split(['[', ']']).any(|part| part == "body");
        if !body && key != "query" {
            return Ok(ArgumentInput::Metadata);
        }
        if field.is_some() && !value.starts_with('@') {
            ensure!(
                !["{owner}", "{repo}", "{branch}"]
                    .iter()
                    .any(|placeholder| value.contains(placeholder)),
                "typed GitHub field contains an unresolved placeholder"
            );
            ensure!(
                !matches!(value, "true" | "false" | "null") && value.parse::<isize>().is_err(),
                "typed GitHub body is not a string"
            );
        }
        if key == "query" {
            ensure!(
                !(value.contains("mutation") && value.contains("body")
                    || field.is_some() && value.starts_with('@')),
                "GraphQL query body cannot be resolved"
            );
            return Ok(ArgumentInput::Metadata);
        }
        return Ok(ArgumentInput::Body(
            match value.strip_prefix('@').filter(|_| field.is_some()) {
                Some(path) => Input::File {
                    path: path.to_owned(),
                    json: false,
                },
                None => Input::Text(value.to_owned()),
            },
        ));
    }
    if let Some(path) = option(arguments, index, "--input", None)? {
        return Ok(ArgumentInput::Body(Input::File { path, json: true }));
    }
    Ok(ArgumentInput::Unmatched)
}

fn publication_input(target: &Target<'_>, index: &mut usize) -> Result<ArgumentInput> {
    if target.api {
        return api_input(target.arguments, index);
    }
    if let Some(text) = option(target.arguments, index, "--body", Some("-b"))? {
        return Ok(ArgumentInput::Body(Input::Text(text)));
    }
    if let Some(path) = option(target.arguments, index, "--body-file", Some("-F"))? {
        return Ok(ArgumentInput::Body(Input::File { path, json: false }));
    }
    if matches!(target.verb, "close" | "reopen")
        && let Some(text) = option(target.arguments, index, "--comment", Some("-c"))?
    {
        return Ok(ArgumentInput::Body(Input::Text(text)));
    }
    Ok(ArgumentInput::Unmatched)
}

fn metadata_option(arguments: &[String], index: &mut usize) -> Result<Option<&'static str>> {
    for (long, short) in [
        ("--repo", Some("-R")),
        ("--title", Some("-t")),
        ("--assignee", Some("-a")),
        ("--reviewer", Some("-r")),
        ("--label", Some("-l")),
        ("--milestone", Some("-m")),
        ("--project", Some("-p")),
        ("--base", Some("-B")),
        ("--head", Some("-H")),
        ("--reason", None),
        ("--duplicate-of", None),
        ("--author-email", Some("-A")),
        ("--subject", None),
        ("--match-head-commit", None),
        ("--header", None),
        ("--method", Some("-X")),
        ("--hostname", None),
        ("--cache", None),
        ("--jq", Some("-q")),
        ("--template", None),
        ("--preview", None),
        ("--add-label", None),
        ("--remove-label", None),
        ("--add-assignee", None),
        ("--remove-assignee", None),
        ("--add-reviewer", None),
        ("--remove-reviewer", None),
        ("--add-project", None),
        ("--remove-project", None),
    ] {
        if option(arguments, index, long, short)?.is_some() {
            return Ok(Some(long));
        }
    }
    Ok(None)
}

fn reject_implicit_source(argument: &str) -> Result<()> {
    ensure!(
        !matches!(
            argument,
            "--editor"
                | "-e"
                | "--web"
                | "-w"
                | "--fill"
                | "--fill-first"
                | "--fill-verbose"
                | "--recover"
        ),
        "GitHub body depends on an editor, generated text, or recovery input"
    );
    Ok(())
}

fn inputs(target: &Target<'_>) -> Result<Vec<Input>> {
    let mut found = Vec::new();
    let mut index = 0;
    let mut implicit = matches!(target.verb, "create" | "comment" | "review" | "edit");
    let mut review_approval = false;
    let mut review_comment = false;
    while index < target.arguments.len() {
        let argument = &target.arguments[index];
        if argument == "--help" {
            return Ok(Vec::new());
        }
        if argument == "--" {
            break;
        }
        reject_implicit_source(argument)?;
        match publication_input(target, &mut index)? {
            ArgumentInput::Body(input) => {
                found.push(input);
                continue;
            }
            ArgumentInput::Metadata => continue,
            ArgumentInput::Unmatched => {}
        }
        if !target.api {
            if target.verb == "review" && matches!(argument.as_str(), "-a" | "--approve") {
                review_approval = true;
                index += 1;
                continue;
            }
            if target.verb == "review"
                && matches!(
                    argument.as_str(),
                    "-c" | "--comment" | "-r" | "--request-changes"
                )
            {
                review_comment = true;
                index += 1;
                continue;
            }
            if target.verb == "merge"
                && matches!(
                    argument.as_str(),
                    "-r" | "--rebase" | "-m" | "--merge" | "-s" | "--squash"
                )
            {
                index += 1;
                continue;
            }
        }
        if let Some(long) = metadata_option(target.arguments, &mut index)? {
            if target.verb == "edit" && long != "--repo" {
                implicit = false;
            }
            continue;
        }
        ensure!(
            !argument.starts_with('-')
                || matches!(
                    argument.as_str(),
                    "--draft"
                        | "-d"
                        | "--delete-branch"
                        | "--auto"
                        | "--disable-auto"
                        | "--admin"
                        | "--dry-run"
                        | "--verbose"
                        | "--edit-last"
                        | "--create-if-none"
                        | "--include"
                        | "-i"
                        | "--paginate"
                        | "--slurp"
                        | "--silent"
                        | "--allow-escape-sequences"
                ),
            "GitHub option has no supported body contract"
        );
        index += 1;
    }
    if target.verb == "review" {
        implicit = !review_approval || review_comment;
    }
    ensure!(
        !implicit || !found.is_empty(),
        "GitHub body depends on interactive or implicit input"
    );
    ensure!(
        found.len() <= MAX_BODIES,
        "GitHub body inspection exceeds its body-count limit"
    );
    Ok(found)
}

fn read_file(path: &str, directory: Option<&Path>) -> Result<String> {
    let path = super::native_path(path);
    ensure!(
        !matches!(
            path.components().next(),
            Some(std::path::Component::Prefix(_))
        ) || path.is_absolute(),
        "GitHub body file has an unresolved drive-relative path"
    );
    ensure!(
        path.is_absolute() || directory.is_some_and(Path::is_absolute),
        "GitHub relative body file has no known execution directory"
    );
    let path = match directory {
        Some(root) if path.is_relative() => root.join(path),
        _ => path,
    };
    ensure!(
        path.metadata()
            .with_context(|| format!("inspect GitHub body file {}", path.display()))?
            .is_file(),
        "GitHub body input is not a regular file"
    );
    let mut bytes = Vec::new();
    std::fs::File::open(path)?
        .take(u64::try_from(MAX_BODY_INPUT)? + 1)
        .read_to_end(&mut bytes)?;
    ensure!(
        bytes.len() <= MAX_BODY_INPUT,
        "GitHub body input exceeds its byte limit"
    );
    String::from_utf8(bytes).context("GitHub body input is not UTF-8")
}

pub(crate) fn collect(
    words: &[String],
    spellings: Option<&[&str]>,
    stdin: Option<&str>,
    directory: Option<&Path>,
    files_safe: bool,
) -> Vec<Body> {
    let resolve = || -> Result<Vec<Body>> {
        let Some(start) = gh_start(words)? else {
            return Ok(Vec::new());
        };
        if target(&words[start + 1..])?.is_none() {
            return Ok(Vec::new());
        }
        let spellings = spellings.context("GitHub argument spelling is unavailable")?;
        let selected =
            decode(spellings)?.context("GitHub command identity changed during inspection")?;
        let mut bodies = Vec::new();
        for input in selected.inputs {
            let (text, json) = match input {
                Input::Text(text) => (text, false),
                Input::File { path, json } if path == "-" => (
                    stdin
                        .context("GitHub standard input is unavailable")?
                        .to_owned(),
                    json,
                ),
                Input::File { path, json } => {
                    ensure!(
                        files_safe && selected.direct,
                        "GitHub file input has an uncertain execution context"
                    );
                    (read_file(&path, directory)?, json)
                }
            };
            ensure!(
                text.len() <= MAX_BODY_INPUT && !text.contains('\0'),
                "GitHub body is oversized or contains NUL"
            );
            let texts = if json {
                json_bodies(&text)?
            } else {
                vec![text]
            };
            bodies.extend(texts.into_iter().map(|text| Body::Text {
                channel: selected.channel,
                text,
            }));
            ensure!(
                bodies.len() <= MAX_BODIES,
                "GitHub body inspection exceeds its body-count limit"
            );
        }
        Ok(bodies)
    };
    resolve().unwrap_or_else(|error| vec![Body::Unreadable(format!("{error:#}"))])
}

fn decision(inspection: crate::body_rules::Inspection, detail: &str) -> Option<String> {
    match crate::body_rules::publication(inspection) {
        crate::body_rules::Verdict::Admit => None,
        crate::body_rules::Verdict::Refuse(reason) => Some(format!(
            "Refused: GitHub body inspection: {detail}\nNext action: {}",
            crate::body_rules::next_action(reason)
        )),
    }
}

pub(crate) fn refusals(
    bodies: &[Body],
    bundle: impl FnOnce() -> Result<prose::Bundle>,
) -> Vec<String> {
    use crate::body_rules::Inspection;
    if bodies.is_empty() {
        return Vec::new();
    }
    if let Some(message) = bodies.iter().find_map(|body| match body {
        Body::Unreadable(message) => Some(message.as_str()),
        Body::Text { .. } => None,
    }) {
        return decision(Inspection::Unreadable, message)
            .into_iter()
            .collect();
    }
    let bundle = match bundle() {
        Ok(bundle) => bundle,
        Err(error) => {
            return decision(
                Inspection::Unreadable,
                &format!("the prose configuration is unavailable: {error:#}"),
            )
            .into_iter()
            .collect();
        }
    };
    let mut messages = Vec::new();
    for (index, body) in bodies.iter().enumerate() {
        let Body::Text { channel, text } = body else {
            continue;
        };
        if matches!(
            crate::body_rules::verdict(dotguard::attribution::find(text).is_some()),
            crate::body_rules::Verdict::Refuse(_)
        ) {
            messages.push(format!(
                "Refused: the GitHub body contains an AI attribution line.\nNext action: {}",
                crate::body_rules::next_action(crate::body_rules::Reason::Attribution)
            ));
            continue;
        }
        if dotguard::provenance_rules::verdict(dotguard::provenance_rules::find(text).is_some())
            == dotguard::provenance_rules::Verdict::Refuse
        {
            messages.push(format!(
                "Refused: the GitHub body says how its text came about.\nNext action: {}",
                dotguard::provenance_rules::NEXT_ACTION
            ));
            continue;
        }
        let path = format!("GitHub body {}", index + 1);
        let (inspection, detail) = match prose::check(&bundle, *channel, &path, text) {
            Ok(findings) if findings.is_empty() => (Inspection::Clear, String::new()),
            Ok(findings) => (Inspection::Findings, prose::render(&findings)),
            Err(error) => (
                Inspection::Unreadable,
                format!("the prose check did not finish: {error:#}"),
            ),
        };
        if let Some(message) = decision(inspection, &detail) {
            messages.push(message);
        }
    }
    messages
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn explicit_github_reads_do_not_require_publication_spellings() {
        for arguments in [
            vec![
                "gh",
                "api",
                "--method",
                "GET",
                "repos/P4suta/beamtrace/actions/runs/1",
            ],
            vec![
                "gh",
                "api",
                "repos/P4suta/beamtrace/pulls/60",
                "-XGET",
                "--paginate",
                "--slurp",
            ],
            vec![
                "gh",
                "api",
                "--method=GET",
                "/repositories/123/actions/permissions/selected-actions",
            ],
            vec!["gh", "release", "view", "--repo", "oven-sh/bun"],
            vec!["gh", "release", "list", "--repo", "oven-sh/bun"],
        ] {
            let words: Vec<String> = arguments.iter().map(|word| (*word).to_owned()).collect();
            let bodies = collect(&words, None, None, None, false);
            assert!(bodies.is_empty(), "{arguments:?}: {bodies:?}");
        }
    }

    #[test]
    fn ambiguous_or_mutating_api_arguments_still_require_inspection() {
        for arguments in [
            vec!["--method", "POST", "repos/o/r/issues"],
            vec!["--method", "PUT", "repos/o/r/issues"],
            vec!["--method", "PATCH", "repos/o/r/issues"],
            vec!["--method", "DELETE", "repos/o/r/issues"],
            vec!["--method", "GET", "repos/o/r", "-XPOST"],
            vec!["--method", "GET", "repos/o/r", "-XGET"],
            vec!["--method", "GET", "repos/o/r", "repos/o/other"],
            vec!["repos/o/r"],
            vec!["--method", "GET"],
            vec!["--method", "GET", "graphql"],
            vec!["--method", "GET", "repos/../../graphql"],
            vec!["--method", "GET", "repos/$OWNER/r"],
            vec!["--method", "GET", "repos/{owner}/r"],
            vec!["--method", "GET", "repos/o/r", "--input", "request.json"],
            vec!["--method", "GET", "repos/o/r", "-fbody=Text."],
            vec!["--method", "GET", "repos/o/r", "-Fbody=@body.md"],
            vec!["--method", "GET", "repos/o/r", "--unknown"],
        ] {
            let words: Vec<String> = ["gh", "api"]
                .into_iter()
                .chain(arguments.iter().copied())
                .map(str::to_owned)
                .collect();
            assert!(
                matches!(
                    &collect(&words, None, None, None, false)[..],
                    [Body::Unreadable(_)]
                ),
                "{arguments:?}"
            );
        }
    }
    use anyhow::Result;

    #[test]
    fn typed_file_fields_keep_placeholder_spellings_in_literal_filenames() -> Result<()> {
        let request = decode(&["gh", "api", "x", "-F", "'body=@body{branch}.md'"])?.unwrap();
        assert!(
            matches!(&request.inputs[..], [Input::File {path, json: false}] if path == "body{branch}.md")
        );
        Ok(())
    }

    #[test]
    fn regular_utf8_files_are_bounded_and_need_a_known_relative_context() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let root = directory.path();
        std::fs::write(root.join("body.md"), "The check passed.\n")?;
        assert_eq!(read_file("body.md", Some(root))?, "The check passed.\n");
        assert!(read_file("body.md", None).is_err());
        assert!(read_file(".", Some(root)).is_err());
        std::fs::write(root.join("body.md"), [0xff])?;
        assert!(read_file("body.md", Some(root)).is_err());
        std::fs::File::create(root.join("body.md"))?.set_len(u64::try_from(MAX_BODY_INPUT)? + 1)?;
        assert!(read_file("body.md", Some(root)).is_err());
        #[cfg(windows)]
        assert!(read_file("C:body.md", Some(root)).is_err());
        Ok(())
    }

    #[test]
    fn the_registered_corpus_exercises_available_and_unresolved_sources() -> Result<()> {
        let arguments: Vec<_> = include_str!("../../../../fuzz/corpus/github_bodies/arguments")
            .lines()
            .collect();
        let request = decode(&arguments)?.unwrap();
        assert!(matches!(&request.inputs[..], [Input::Text(text)] if text == "The check passed."));
        let expansion: Vec<_> = include_str!("../../../../fuzz/corpus/github_bodies/expansion")
            .lines()
            .collect();
        assert!(decode(&expansion).is_err());
        assert_eq!(
            json_bodies(include_str!(
                "../../../../fuzz/corpus/github_bodies/nested-json"
            ))?
            .len(),
            2
        );
        assert!(
            json_bodies(include_str!(
                "../../../../fuzz/corpus/github_bodies/duplicate-json"
            ))
            .is_err()
        );
        Ok(())
    }

    #[test]
    fn execution_wrappers_cannot_hide_an_uninspected_publication() {
        for arguments in [
            vec![
                "/usr/bin/time",
                "gh",
                "pr",
                "create",
                "--body",
                "'First. Second.'",
            ],
            vec![
                "timeout",
                "10",
                "gh",
                "issue",
                "close",
                "3",
                "-c",
                "'First. Second.'",
            ],
            vec!["stdbuf", "-oL", "gh", "pr", "comment", "3", "-F", "body.md"],
        ] {
            assert!(decode(&arguments).is_err(), "{arguments:?}");
        }
    }

    #[test]
    fn command_roles_keep_flags_and_their_values_separate() -> Result<()> {
        for (arguments, expected) in [
            (
                vec!["gh", "pr", "create", "--title", "-bhidden", "-bText."],
                "Text.",
            ),
            (vec!["gh", "issue", "close", "3", "-cText."], "Text."),
            (vec!["gh", "pr", "reopen", "3", "--comment=Text."], "Text."),
            (
                vec!["gh", "pr", "review", "3", "-c", "-b", "Text."],
                "Text.",
            ),
            (vec!["gh", "pr", "merge", "3", "-r", "-b", "Text."], "Text."),
            (
                vec!["gh", "api", "x", "-H", "-fbody=hidden", "-fbody=Text."],
                "Text.",
            ),
            (
                vec!["gh", "api", "x", "--raw-field=body=@literal"],
                "@literal",
            ),
            (
                vec!["gh", "api", "x", "-f", "'comments[][body]=Text.'"],
                "Text.",
            ),
        ] {
            let request = decode(&arguments)
                .with_context(|| format!("{arguments:?}"))?
                .expect("publication request");
            assert!(
                matches!(&request.inputs[..], [Input::Text(text)] if text == expected),
                "{arguments:?}: {request:?}"
            );
        }
        for arguments in [
            vec!["gh", "pr", "review", "3", "--approve"],
            vec!["gh", "pr", "merge", "3", "--squash", "--auto"],
            vec!["gh", "issue", "edit", "3", "--add-label", "-bhidden"],
            vec!["gh", "api", "x", "-f", "title=Text."],
            vec![
                "gh",
                "api",
                "graphql",
                "-f",
                "'query=query { viewer { login } }'",
            ],
        ] {
            assert!(
                decode(&arguments)?.unwrap().inputs.is_empty(),
                "{arguments:?}"
            );
        }
        for arguments in [
            vec!["gh", "pr", "review", "3", "--approve", "--comment"],
            vec!["gh", "pr", "review", "3", "--comment", "--approve"],
            vec!["gh", "pr", "create", "--fill"],
            vec!["gh", "issue", "edit", "3"],
            vec!["gh", "pr", "comment", "3", "--editor"],
            vec!["gh", "api", "x", "-Fbody=true"],
            vec!["gh", "api", "x", "-Fbody={branch}"],
            vec!["gh", "api", "graphql", "-f", "'query=mutation { body }'"],
            vec!["gh", "api", "graphql", "-F", "query=@query.graphql"],
            vec!["gh", "publish-alias", "list"],
        ] {
            assert!(decode(&arguments).is_err(), "{arguments:?}");
        }
        for arguments in [vec!["gh", "pr", "view", "3"], vec!["gh", "issue", "list"]] {
            assert!(decode(&arguments)?.is_none(), "{arguments:?}");
        }
        Ok(())
    }

    #[test]
    fn supported_literal_words_match_native_bash_arguments() -> Result<()> {
        const GIT_BASH: &str = "bin/bash.exe";
        for (spelling, expected) in [
            ("plain", "plain"),
            ("''", ""),
            ("\"\"", ""),
            ("'two words'", "two words"),
            ("'$HOME and `name`'", "$HOME and `name`"),
            (r#""cash \$HOME and \`name\`""#, "cash $HOME and `name`"),
            (r"one\ two", "one two"),
            ("one\\\ntwo", "onetwo"),
            (r#""backslash \q""#, r"backslash \q"),
            ("'one'\" two\"", "one two"),
            ("''\"\"", ""),
            ("'x'\\\n''", "x"),
            ("'é'", "é"),
        ] {
            assert_eq!(literal_word(spelling).as_deref(), Some(expected));
            let mut bash = if cfg!(unix) {
                crate::tool::Tool::Bash.command()
            } else {
                let git = dotguard::real_git::find()
                    .context("the native Bash oracle requires Git for Windows")?;
                let root = git
                    .parent()
                    .and_then(Path::parent)
                    .context("Git for Windows has no installation root")?;
                let bash = root.join(GIT_BASH);
                ensure!(
                    bash.is_file(),
                    "Git for Windows Bash is unavailable at {}",
                    bash.display()
                );
                platform::process::command(bash)
            };
            let output = bash
                .args([
                    "--noprofile",
                    "--norc",
                    "-c",
                    &format!("builtin set -- {spelling}; builtin printf '%s\\0' \"$#\" \"$@\""),
                ])
                .env_remove("BASH_ENV")
                .env_remove("ENV")
                .env_remove("SHELLOPTS")
                .env_remove("BASHOPTS")
                .output()?;
            assert!(output.status.success(), "{spelling}: {output:?}");
            let mut bytes = b"1\0".to_vec();
            bytes.extend_from_slice(expected.as_bytes());
            bytes.push(0);
            assert_eq!(output.stdout, bytes, "{spelling}");
        }
        Ok(())
    }

    #[test]
    fn unresolved_and_incomplete_words_supply_no_literal_body() {
        for spelling in [
            "",
            "$BODY",
            "\"$BODY\"",
            "$(echo body)",
            "`echo body`",
            "$'body\\n'",
            "$\"body\"",
            "file*",
            "?",
            "~/file",
            "'unfinished",
            "\"unfinished",
            "\\",
            "\"trailing\\",
            "first second",
            "one\0two",
            "<(printf body)",
            "{one,two}",
        ] {
            assert_eq!(literal_word(spelling), None, "{spelling}");
        }
    }

    #[test]
    fn nested_json_bodies_keep_their_exact_decoded_bytes() -> Result<()> {
        let bodies = json_bodies(
            r#"{"body":"First.\nSecond.","comments":[{"body":"Quoted \"text\"."},{"body":""}],"line":1,"draft":false,"other":null}"#,
        )?;
        assert_eq!(bodies, ["First.\nSecond.", "Quoted \"text\".", ""]);
        assert_eq!(
            json_bodies(r#"{"query":"query { viewer { login } }"}"#)?,
            [] as [String; 0]
        );
        Ok(())
    }

    #[test]
    fn malformed_or_ambiguous_json_never_supplies_a_checked_body() {
        for text in [
            "not JSON",
            r#"{"body":"first","body":"second"}"#,
            r#"{"comments":[{"body":42}]}"#,
            r#"{"body":null}"#,
            r#"{"body":"hidden\u0000text"}"#,
            r#"{"query":"mutation { addComment(input:{body:\"Uninspected.\"}) { id } }"}"#,
        ] {
            assert!(json_bodies(text).is_err(), "{text}");
        }
        let too_many = serde_json::json!({
            "comments": (0..=MAX_BODIES).map(|_| serde_json::json!({"body": "Body."})).collect::<Vec<_>>()
        });
        assert!(json_bodies(&too_many.to_string()).is_err());
    }
}
