//! What `~/.local/bin/git` refuses to run.
//!
//! Two families of rule live here:
//!
//! * **force** — operations that throw away committed history or uncommitted work with no undo.
//!   `push --force`, `reset --hard`, `clean -f`, `branch -D`, `stash drop`, `reflog expire`, `filter-branch`.
//! * **no-verify** — operations that would leave signing or hook enforcement switched off for one command: `--no-verify`, `--no-gpg-sign`, and the `-c key=value` spellings of the same thing.
//!
//! This layer is *convenience*, not the guarantee.
//! It only sees commands whose `git` resolved through PATH, which excludes an IDE calling `/opt/homebrew/bin/git` directly, a shell alias expanding to something else, and anything running with a different PATH.
//! The guarantee for the remote side is `dotguard pre-push`, which runs from `core.hooksPath` and therefore sees every push regardless of how git was invoked.
//! What this layer buys is the refusal arriving *before* the damage rather than after, and a hint that names the non-destructive command you probably wanted.
//!
//! Rules NOT here, deliberately:
//! * `rebase`, `commit --amend` — rewriting unpushed history is ordinary work, and pre-push catches the case where it was not unpushed.
//! * `restore` / `checkout -- <path>` — destroys uncommitted changes with no flag to key on, so refusing it means refusing the command outright.
//! * `rm -f` — common in scripts, and what it destroys is reachable from the index in every case that matters.

use crate::bypass::Category;
use crate::gate_rules::{Fixup, Spelling, Verdict, spelling, verdict};
use crate::realgit;
use crate::refusal::{Refusal, command};

pub struct Denial {
    pub category: Category,
    /// One line, for the audit log.
    pub reason: String,
    /// Why the command is refused and what to consider instead.
    /// Shown to the human, may be several lines.
    pub hint: String,
    /// The command to run instead.
    pub next: Next,
}

/// The command a refusal suggests instead.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Next {
    /// A Git command, as the words after `git`; the gate judges it again before suggesting it.
    Git(Vec<String>),
    /// Any other command line, already quoted.
    Shell(String),
    /// No command, for the reason given: one rebuilt from the refused line could do something else.
    Withheld(String),
}

impl Next {
    /// The command as one shell line, or `None` when it is withheld.
    pub fn line(&self) -> Option<String> {
        match self {
            Self::Git(words) => Some(command(
                &std::iter::once("git")
                    .chain(words.iter().map(String::as_str))
                    .collect::<Vec<_>>(),
            )),
            Self::Shell(line) => Some(line.clone()),
            Self::Withheld(_) => None,
        }
    }
}

impl From<String> for Next {
    fn from(line: String) -> Self {
        Self::Shell(line)
    }
}

impl Denial {
    fn new(
        category: Category,
        reason: impl Into<String>,
        hint: impl Into<String>,
        next: impl Into<Next>,
    ) -> Self {
        Self {
            category,
            reason: reason.into(),
            hint: hint.into(),
            next: next.into(),
        }
    }
}

/// The structured refusal for a denied invocation; `argv` is everything after the program name.
pub fn refusal(denial: &Denial, argv: &[String]) -> Refusal {
    let invoked = command(
        &std::iter::once("git")
            .chain(argv.iter().map(String::as_str))
            .collect::<Vec<_>>(),
    );
    let mut cause = format!("refusing `{}`.\n{}", denial.reason, denial.hint);
    cause.push_str(if denial.category.env().is_some() {
        "\nA waiver is recorded in ~/.local/state/git-bypass.log."
    } else {
        "\nSigning and hook checks cannot be bypassed."
    });
    if let Next::Withheld(reason) = &denial.next {
        cause.push_str("\nNo command is suggested: ");
        cause.push_str(reason);
    }
    let rule = format!("git.{}", denial.category);
    let refusal = match denial.next.line() {
        Some(next) => Refusal::new(rule, cause, next),
        None => Refusal::without_next(rule, cause),
    }
    .evidence(format!("command: {invoked}"));
    match denial.category.env() {
        Some(env) => refusal.waiver(format!("{env}=1 {invoked}")),
        None => refusal,
    }
}

/// The invocation being judged, so a refusal can name the same command with the offending part changed.
struct Invocation<'a> {
    globals: &'a [String],
    sub: &'a str,
    args: &'a [String],
}

impl Invocation<'_> {
    /// A different git command run with the same global options.
    fn git(&self, words: &[&str]) -> Next {
        Next::Git(
            self.globals
                .iter()
                .map(String::as_str)
                .chain(words.iter().copied())
                .map(str::to_owned)
                .collect(),
        )
    }

    /// The same command with each option before `--` kept, rewritten, or dropped by `edit`.
    fn rerun(&self, edit: impl Fn(&str) -> Option<String>) -> Next {
        let mut line: Vec<String> = self
            .globals
            .iter()
            .cloned()
            .chain([self.sub.to_owned()])
            .collect();
        let mut literal = false;
        for arg in self.args {
            if literal || arg == "--" {
                literal = true;
                line.push(arg.clone());
            } else if let Some(word) = edit(arg) {
                line.push(word);
            }
        }
        Next::Git(line)
    }

    /// The same command with the argument at `index` replaced by `word`.
    fn replace(&self, index: usize, word: &str) -> Next {
        Next::Git(
            self.globals
                .iter()
                .map(String::as_str)
                .chain([self.sub])
                .chain(
                    self.args
                        .iter()
                        .enumerate()
                        .map(|(at, arg)| if at == index { word } else { arg.as_str() }),
                )
                .map(str::to_owned)
                .collect(),
        )
    }

    /// The same command without the named options.
    fn without(&self, dropped: &[&str]) -> Next {
        self.rerun(|arg| (!dropped.contains(&arg)).then(|| arg.to_owned()))
    }

    /// The non-option words after the subcommand.
    fn positional(&self, index: usize) -> Option<&str> {
        self.args
            .iter()
            .take_while(|a| a.as_str() != "--")
            .filter(|a| !a.starts_with('-'))
            .nth(index)
            .map(String::as_str)
    }
}

/// Deletes remote branches through the forge API rather than a push the gate refuses.
fn forge_deletes(references: &[&str]) -> String {
    let references = if references.is_empty() {
        &["<branch>"][..]
    } else {
        references
    };
    references
        .iter()
        .map(|reference| {
            let branch = reference.strip_prefix("refs/heads/").unwrap_or(reference);
            command(&[
                "gh",
                "api",
                "-X",
                "DELETE",
                &format!("repos/{{owner}}/{{repo}}/git/refs/heads/{branch}"),
            ])
        })
        .collect::<Vec<_>>()
        .join(" && ")
}

/// How a long option takes its value.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Takes {
    Nothing,
    /// Attached with `=`, or else the next word.
    Word,
    /// Only attached with `=`.
    Attached,
}

/// A long option: its name, how it takes a value, and whether `--no-<name>` negates it.
/// A negatable name that starts with `no-` is also negated by the name without it, as `--verify` negates `--no-verify`.
struct Long(&'static str, Takes, bool);

/// The options of one subcommand, as `git <subcommand> -h` lists them.
struct Grammar {
    long: &'static [Long],
    /// Short options that take no value.
    short_flag: &'static str,
    /// Short options that take the rest of their word, or else the next word.
    short_word: &'static str,
    /// Short options that take only the rest of their word.
    short_attached: &'static str,
}

const PUSH: Grammar = Grammar {
    long: &[
        Long("verbose", Takes::Nothing, true),
        Long("quiet", Takes::Nothing, true),
        Long("repo", Takes::Word, true),
        Long("all", Takes::Nothing, true),
        Long("branches", Takes::Nothing, true),
        Long("mirror", Takes::Nothing, true),
        Long("delete", Takes::Nothing, true),
        Long("tags", Takes::Nothing, true),
        Long("dry-run", Takes::Nothing, true),
        Long("porcelain", Takes::Nothing, true),
        Long("force", Takes::Nothing, true),
        Long("force-with-lease", Takes::Attached, true),
        Long("force-if-includes", Takes::Nothing, true),
        Long("recurse-submodules", Takes::Word, true),
        Long("thin", Takes::Nothing, true),
        Long("receive-pack", Takes::Word, true),
        Long("exec", Takes::Word, true),
        Long("set-upstream", Takes::Nothing, true),
        Long("progress", Takes::Nothing, true),
        Long("prune", Takes::Nothing, true),
        Long("no-verify", Takes::Nothing, true),
        Long("follow-tags", Takes::Nothing, true),
        Long("signed", Takes::Attached, true),
        Long("atomic", Takes::Nothing, true),
        Long("push-option", Takes::Word, true),
        Long("ipv4", Takes::Nothing, false),
        Long("ipv6", Takes::Nothing, false),
    ],
    short_flag: "vqdnfu46",
    short_word: "o",
    short_attached: "",
};

const COMMIT: Grammar = Grammar {
    long: &[
        Long("quiet", Takes::Nothing, true),
        Long("verbose", Takes::Nothing, true),
        Long("file", Takes::Word, true),
        Long("author", Takes::Word, true),
        Long("date", Takes::Word, true),
        Long("message", Takes::Word, true),
        Long("reedit-message", Takes::Word, true),
        Long("reuse-message", Takes::Word, true),
        Long("fixup", Takes::Word, true),
        Long("squash", Takes::Word, true),
        Long("reset-author", Takes::Nothing, true),
        Long("trailer", Takes::Word, false),
        Long("signoff", Takes::Nothing, true),
        Long("template", Takes::Word, true),
        Long("edit", Takes::Nothing, true),
        Long("cleanup", Takes::Word, true),
        Long("status", Takes::Nothing, true),
        Long("gpg-sign", Takes::Attached, true),
        Long("all", Takes::Nothing, true),
        Long("include", Takes::Nothing, true),
        Long("interactive", Takes::Nothing, true),
        Long("patch", Takes::Nothing, true),
        Long("unified", Takes::Word, false),
        Long("inter-hunk-context", Takes::Word, false),
        Long("only", Takes::Nothing, true),
        Long("no-verify", Takes::Nothing, true),
        Long("dry-run", Takes::Nothing, true),
        Long("short", Takes::Nothing, true),
        Long("branch", Takes::Nothing, true),
        Long("ahead-behind", Takes::Nothing, true),
        Long("porcelain", Takes::Nothing, true),
        Long("long", Takes::Nothing, true),
        Long("null", Takes::Nothing, true),
        Long("amend", Takes::Nothing, true),
        Long("no-post-rewrite", Takes::Nothing, true),
        Long("untracked-files", Takes::Attached, true),
        Long("pathspec-from-file", Takes::Word, true),
        Long("pathspec-file-nul", Takes::Nothing, true),
        Long("allow-empty", Takes::Nothing, true),
        Long("allow-empty-message", Takes::Nothing, true),
    ],
    short_flag: "qvseaipnzo",
    short_word: "mFCctU",
    short_attached: "Su",
};

/// One option as Git reads it.
enum Opt<'a> {
    /// A long option by its full name; `negated` for `--no-<name>`.
    Long { name: &'static str, negated: bool },
    /// An option Git refuses: an ambiguous or unknown name, an unknown letter, or a value it does not take or lacks.
    Unread,
    /// A cluster of short options, ending with the one that takes `value` when one does.
    Short {
        letters: &'a str,
        value: Option<&'a str>,
    },
}

/// An argument list split as Git splits it.
struct Parsed<'a> {
    /// Each option with the words it spans, its value included.
    options: Vec<(std::ops::Range<usize>, Opt<'a>)>,
    /// The words that are not options, including every word after `--`.
    positionals: Vec<usize>,
    /// The `--` that ends the options.
    separator: Option<usize>,
}

impl Parsed<'_> {
    /// Whether every option is one Git accepts, so the words of the line mean what the gate read.
    fn read(&self) -> bool {
        self.options
            .iter()
            .all(|(_, option)| !matches!(option, Opt::Unread))
    }
}

/// The value of the long option spanning `range` in `args`.
fn long_value<'a>(range: &std::ops::Range<usize>, args: &'a [String]) -> Option<&'a str> {
    args[range.start]
        .split_once('=')
        .map(|(_, value)| value)
        .or_else(|| (range.len() > 1).then(|| args[range.end - 1].as_str()))
}

impl Grammar {
    /// Reads a long option name as Git does, which accepts any unique prefix of a spelling.
    fn long(&self, name: &str) -> Option<(&'static Long, bool)> {
        let mut spellings: Vec<(String, &'static Long, bool)> = Vec::new();
        for option in self.long {
            spellings.push((option.0.to_owned(), option, false));
            if option.2 {
                spellings.push((format!("no-{}", option.0), option, true));
                if let Some(positive) = option.0.strip_prefix("no-") {
                    spellings.push((positive.to_owned(), option, true));
                }
            }
        }
        let bytes: Vec<&[u8]> = spellings.iter().map(|(text, ..)| text.as_bytes()).collect();
        match spelling(name.as_bytes(), &bytes) {
            Spelling::Is(index) => Some((spellings[index].1, spellings[index].2)),
            Spelling::Ambiguous | Spelling::Unknown => None,
        }
    }

    fn parse<'a>(&self, args: &'a [String]) -> Parsed<'a> {
        let mut parsed = Parsed {
            options: Vec::new(),
            positionals: Vec::new(),
            separator: None,
        };
        let mut index = 0;
        while index < args.len() {
            let arg = args[index].as_str();
            let start = index;
            if arg == "--" {
                parsed.separator = Some(index);
                parsed.positionals.extend(index + 1..args.len());
                break;
            }
            if let Some(long) = arg.strip_prefix("--") {
                let (name, attached) = long
                    .split_once('=')
                    .map_or((long, false), |(name, _)| (name, true));
                let option = match self.long(name) {
                    // Git refuses a value on a negation or on an option that takes none, and a missing value.
                    Some((option, negated))
                        if !attached || !negated && option.1 != Takes::Nothing =>
                    {
                        if !negated && !attached && option.1 == Takes::Word {
                            index += 1;
                        }
                        if index < args.len() {
                            Opt::Long {
                                name: option.0,
                                negated,
                            }
                        } else {
                            Opt::Unread
                        }
                    }
                    _ => Opt::Unread,
                };
                parsed
                    .options
                    .push((start..(index + 1).min(args.len()), option));
            } else if let Some(cluster) = arg.strip_prefix('-').filter(|rest| !rest.is_empty()) {
                let mut letters = cluster;
                let mut value = None;
                let mut known = true;
                for (at, letter) in cluster.char_indices() {
                    let end = at + letter.len_utf8();
                    let rest = &cluster[end..];
                    if self.short_word.contains(letter) {
                        letters = &cluster[..end];
                        value = if rest.is_empty() {
                            index += 1;
                            args.get(index).map(String::as_str)
                        } else {
                            Some(rest)
                        };
                        known = value.is_some();
                        break;
                    }
                    if self.short_attached.contains(letter) {
                        letters = &cluster[..end];
                        value = (!rest.is_empty()).then_some(rest);
                        break;
                    }
                    if !self.short_flag.contains(letter) {
                        known = false;
                        break;
                    }
                }
                parsed.options.push((
                    start..(index + 1).min(args.len()),
                    if known {
                        Opt::Short { letters, value }
                    } else {
                        Opt::Unread
                    },
                ));
            } else {
                parsed.positionals.push(index);
            }
            index += 1;
        }
        parsed
    }
}

/// The options that set where a commit's message comes from or whether the editor opens, which a retry from the saved message replaces.
const MESSAGE_OPTIONS: [&str; 8] = [
    "message",
    "file",
    "reuse-message",
    "reedit-message",
    "fixup",
    "squash",
    "template",
    "edit",
];
const MESSAGE_LETTERS: &str = "mFCcte";

/// How the refused commit's `--fixup` was given, and the commit whose authorship it copied.
fn message_source<'a>(parsed: &Parsed<'a>, args: &'a [String]) -> (Fixup, Option<&'a str>) {
    let mut fixup = Fixup::Absent;
    let mut reused: Option<&str> = None;
    let mut renew = false;
    for (range, option) in &parsed.options {
        match option {
            Opt::Long {
                name: "fixup",
                negated,
            } => {
                fixup = match long_value(range, args) {
                    _ if *negated => Fixup::Absent,
                    Some(value) if value.starts_with("amend:") => Fixup::Amend,
                    Some(value) if value.starts_with("reword:") => Fixup::Reword,
                    _ => Fixup::Plain,
                };
            }
            Opt::Long {
                name: "reuse-message" | "reedit-message",
                negated,
            } => {
                reused = if *negated {
                    None
                } else {
                    long_value(range, args)
                }
            }
            Opt::Long {
                name: "reset-author",
                negated,
            } => renew = !negated,
            Opt::Short { letters, value } if letters.ends_with(['C', 'c']) => reused = *value,
            _ => {}
        }
    }
    (fixup, reused.filter(|_| !renew))
}

/// The author and date of a commit, as `--author` and `--date` take them.
pub struct Authorship {
    pub author: String,
    pub date: String,
}

/// The refused commit again, with its message read from `message` and opened in the editor.
/// `words` is the command line the wrapper ran, and `authorship` reads a commit's authorship.
/// The result is `None` when that was not `git commit`, and the reason when no command repeats it.
pub fn commit_retry(
    words: &[String],
    message: &str,
    authorship: impl Fn(&str) -> Option<Authorship>,
) -> Option<Result<Vec<String>, String>> {
    let (program, rest) = words.split_first()?;
    if program != "git" {
        return None;
    }
    let (at, _) = globals(rest);
    if rest.get(at)? != "commit" {
        return None;
    }
    let args = &rest[at + 1..];
    let parsed = COMMIT.parse(args);
    if !parsed.read() {
        return Some(Err(
            "the commit has an option Git does not accept as written, so the gate cannot tell what it meant.".to_owned(),
        ));
    }
    let (fixup, source) = message_source(&parsed, args);
    let read = source.and_then(&authorship);
    let Some(plan) = crate::gate_rules::commit_retry(fixup, source.is_some(), read.is_some())
    else {
        return Some(Err(format!(
            "the commit took its author and date from `{}`, which the gate could not read, and a retry without them would record a different author.",
            source.unwrap_or_default()
        )));
    };
    let mut replaced: Vec<Option<Vec<String>>> = vec![None; args.len()];
    for (range, option) in &parsed.options {
        let kept = match option {
            Opt::Long { name, .. } if MESSAGE_OPTIONS.contains(name) => Vec::new(),
            Opt::Short { letters, value } => {
                let kept: String = letters
                    .chars()
                    .filter(|letter| !MESSAGE_LETTERS.contains(*letter))
                    .collect();
                if kept.len() == letters.len() {
                    continue;
                }
                // Every message letter that takes a value takes it from the rest of the word or the next one, so dropping it drops that value.
                let drops_value = letters.ends_with(|letter| {
                    MESSAGE_LETTERS.contains(letter) && COMMIT.short_word.contains(letter)
                });
                let mut words = Vec::new();
                if drops_value {
                    if !kept.is_empty() {
                        words.push(format!("-{kept}"));
                    }
                } else {
                    let attached = &args[range.start][1 + letters.len()..];
                    if !kept.is_empty() || !attached.is_empty() {
                        words.push(format!("-{kept}{attached}"));
                    }
                    if range.len() > 1 {
                        words.extend(value.map(str::to_owned));
                    }
                }
                words
            }
            _ => continue,
        };
        replaced[range.start] = Some(kept);
        for index in range.clone().skip(1) {
            replaced[index] = Some(Vec::new());
        }
    }
    let mut retry: Vec<String> = Vec::new();
    if plan.only {
        retry.push("--only".to_owned());
    }
    if plan.allow_empty {
        retry.push("--allow-empty".to_owned());
    }
    retry.extend(["--edit", "--file", message].map(str::to_owned));
    let mut line = words[..at + 2].to_vec();
    // Before every kept option, so an explicit `--author` or `--date` still overrides it as it did the reused commit's.
    if let Some(read) = read.filter(|_| plan.authorship) {
        line.extend([
            "--author".to_owned(),
            read.author,
            "--date".to_owned(),
            read.date,
        ]);
    }
    for (index, arg) in args.iter().enumerate() {
        if parsed.separator == Some(index) {
            line.extend(retry.clone());
        }
        match &replaced[index] {
            Some(words) => line.extend(words.iter().cloned()),
            None => line.push(arg.clone()),
        }
    }
    if parsed.separator.is_none() {
        line.extend(retry);
    }
    Some(Ok(line))
}

/// Drops `letters` from a clustered short option, and the option itself once nothing is left.
fn drop_letters(arg: &str, letters: &[char]) -> Option<String> {
    match short_cluster(arg) {
        Some(cluster) => {
            let kept: String = cluster.chars().filter(|c| !letters.contains(c)).collect();
            (!kept.is_empty()).then(|| format!("-{kept}"))
        }
        None => Some(arg.to_owned()),
    }
}

/// Global options that consume the following argv element as their value.
/// Getting this list wrong means mistaking a value for the subcommand, so it is spelled out rather than guessed at.
/// `--exec-path` is absent on purpose: its value is optional, and the valueless form just prints a path and exits.
const GLOBAL_TAKES_VALUE: [&str; 8] = [
    "-c",
    "-C",
    "--git-dir",
    "--work-tree",
    "--namespace",
    "--config-env",
    "--super-prefix",
    "--attr-source",
];

/// Subcommands we recognise as git's own.
/// Used only to decide whether a token is worth an alias lookup — being incomplete costs one `git config` call, not correctness.
const BUILTINS: [&str; 46] = [
    "add",
    "am",
    "apply",
    "archive",
    "bisect",
    "blame",
    "branch",
    "checkout",
    "cherry-pick",
    "clean",
    "clone",
    "commit",
    "config",
    "describe",
    "diff",
    "fetch",
    "filter-branch",
    "fsck",
    "gc",
    "grep",
    "init",
    "log",
    "ls-files",
    "merge",
    "mv",
    "notes",
    "pull",
    "push",
    "rebase",
    "reflog",
    "remote",
    "reset",
    "restore",
    "revert",
    "rm",
    "shortlog",
    "show",
    "stash",
    "status",
    "submodule",
    "switch",
    "tag",
    "update-ref",
    "worktree",
    "verify-commit",
    "help",
];

/// Decide whether this git invocation may proceed.
///
/// `argv` is everything after the program name, exactly as the wrapper received it.
/// A refusal's suggested Git command is judged again, so a line combining several refused parts gets a suggestion with every one of them removed.
pub fn inspect(argv: &[String]) -> Option<Denial> {
    let mut denial = judge(argv)?;
    // Each suggestion removes at least one refused word, so the chain ends within the line's length.
    for _ in 0..=argv.len() {
        let Next::Git(words) = &denial.next else {
            break;
        };
        let Some(further) = judge(words) else {
            break;
        };
        denial.next = further.next;
    }
    Some(denial)
}

/// The index of the subcommand, and each `-c` or `--config-env` setting before it with the words it spans.
/// Settings are collected rather than judged on sight: whether one of them is a policy override depends on the subcommand.
fn globals(argv: &[String]) -> (usize, Vec<(String, std::ops::Range<usize>)>) {
    let mut i = 0;
    let mut settings: Vec<(String, std::ops::Range<usize>)> = Vec::new();
    while i < argv.len() {
        let arg = argv[i].as_str();
        if !arg.starts_with('-') {
            break;
        }
        if arg == "--" {
            i += 1;
            break;
        }
        let start = i;
        if let Some(rest) = arg.strip_prefix("-c") {
            // Both `-c key=value` and the attached `-ckey=value` form.
            if rest.is_empty() {
                i += 1;
                if let Some(v) = argv.get(i) {
                    settings.push((v.clone(), start..i + 1));
                }
            } else {
                settings.push((rest.to_owned(), start..i + 1));
            }
        } else if arg.starts_with("--config-env") {
            // `--config-env=key=ENVVAR` hides the value in the environment, so the key alone has to decide — recorded as `key=` to mean "set to something we cannot see".
            let setting = arg.strip_prefix("--config-env=").map_or_else(
                || {
                    i += 1;
                    argv.get(i).map(String::as_str).unwrap_or_default()
                },
                |s| s,
            );
            if let Some(key) = setting.split('=').next()
                && is_policy_key(key)
            {
                settings.push((format!("{key}="), start..i + 1));
            }
        } else if GLOBAL_TAKES_VALUE.contains(&arg) {
            i += 1;
        }
        i += 1;
    }
    (i, settings)
}

fn judge(argv: &[String]) -> Option<Denial> {
    let (i, settings) = globals(argv);
    let sub = argv.get(i)?;
    let rest = &argv[i + 1..];

    for (setting, range) in &settings {
        if let Some(mut denial) = config_override(setting, sub) {
            denial.next = Next::Git(
                argv.iter()
                    .enumerate()
                    .filter(|(index, _)| !range.contains(index))
                    .map(|(_, word)| word.clone())
                    .collect(),
            );
            return Some(denial);
        }
    }

    let globals = &argv[..i];
    if let Some(denial) = subcommand(&Invocation {
        globals,
        sub,
        args: rest,
    }) {
        return Some(denial);
    }

    // An alias could expand to anything, including `push --force`.
    // Resolving every token would put a `git config` call in front of `git status`, so we only ask about tokens git itself does not define.
    if !BUILTINS.contains(&sub.as_str())
        && let Some(expansion) = realgit::capture(&["config", "--get", &format!("alias.{sub}")])
    {
        let words: Vec<String> = expansion.split_whitespace().map(str::to_owned).collect();
        // `!sh -c ...` aliases run an arbitrary shell; we cannot reason about those and do not pretend to.
        if let Some(first) = words.first()
            && !first.starts_with('!')
            && let Some(denial) = subcommand(&Invocation {
                globals,
                sub: first,
                args: &words[1..],
            })
        {
            return Some(Denial::new(
                denial.category,
                format!("alias {sub} -> {}", expansion.trim()),
                denial.hint,
                denial.next,
            ));
        }
    }

    None
}

/// The per-subcommand rules.
///
/// One long `match` on purpose: every arm is a rule, each rule is three lines of condition and a paragraph of explanation, and the table reads as the policy document it is.
/// Splitting it into a dozen two-line functions would scatter the policy without shortening it.
#[allow(clippy::too_many_lines)]
fn subcommand(call: &Invocation) -> Option<Denial> {
    let (sub, args) = (call.sub, call.args);
    let flags = flags_of(args);
    let has = |f: &str| flags.iter().any(|a| a == f);
    let grammar = match sub {
        "push" => Some(&PUSH),
        "commit" => Some(&COMMIT),
        _ => None,
    };
    if let Some(grammar) = grammar {
        let parsed = grammar.parse(args);
        let denial = skips_hooks(call, &parsed).or_else(|| match sub {
            "push" => push(call, &parsed),
            _ if has("--no-gpg-sign") => Some(no_gpg_sign(call)),
            _ => None,
        });
        return read_in_full(&parsed, args, denial);
    }
    if has("--no-verify") {
        return Some(Denial::new(
            Category::NoVerify,
            "--no-verify",
            "Hook verification cannot be skipped.",
            call.without(&["--no-verify"]),
        ));
    }
    match sub {
        "reset" if has("--hard") => Some(Denial::new(
            Category::Force,
            "git reset --hard",
            "--hard discards every uncommitted change with no reflog entry to get it back.\n\
             git stash --include-untracked   keeps the work and gives you a clean tree\n\
             git reset --keep <commit>       moves the branch but refuses to clobber local edits",
            call.git(&["stash", "push", "--include-untracked"]),
        )),

        "clean" if flags.iter().any(|a| short_cluster(a).is_some_and(|c| c.contains('f'))) || has("--force") => {
            Some(Denial::new(
                Category::Force,
                "git clean -f",
                "Untracked files are not in git; once removed there is nothing to restore from.\n\
                 git clean -nd                    list exactly what would go\n\
                 git stash push --include-untracked   keep it instead of deleting it",
                call.git(&["clean", "-nd"]),
            ))
        }

        "checkout" | "switch"
            if has("--force") || has("--discard-changes") || flags.iter().any(|a| short_cluster(a).is_some_and(|c| c.contains('f'))) =>
        {
            Some(Denial::new(
                Category::Force,
                format!("git {sub} --force"),
                "--force here means 'overwrite my uncommitted changes'.\n\
                 git stash   first, then switch; the changes come back with `git stash pop`."
                    .to_owned(),
                call.git(&["stash", "push", "--include-untracked"]),
            ))
        }

        "branch" => {
            let force_delete = flags.iter().any(|a| short_cluster(a).is_some_and(|c| c.contains('D')))
                || ((has("-d") || has("--delete")) && (has("-f") || has("--force")));
            let force_move = flags.iter().any(|a| short_cluster(a).is_some_and(|c| c.contains('M')))
                || ((has("-m") || has("--move")) && (has("-f") || has("--force")));
            // The same command without forcing: `-D` becomes `-d`, `-M` becomes `-m`, and `-f` goes.
            let unforced = || {
                call.rerun(|arg| match arg {
                    "-f" | "--force" => None,
                    _ if short_cluster(arg).is_some() => {
                        drop_letters(&arg.replace('D', "d").replace('M', "m"), &['f'])
                    }
                    _ => Some(arg.to_owned()),
                })
            };
            if force_delete {
                Some(Denial::new(
                    Category::Force,
                    "git branch -D",
                    "-D deletes a branch whose commits are not merged anywhere.\n\
                     git branch -d <name>   deletes it only if it is merged — if that refuses,\n\
                     the commits really would become unreachable.",
                    unforced(),
                ))
            } else if force_move {
                Some(Denial::new(
                    Category::Force,
                    "git branch -M",
                    "-M overwrites an existing branch of the target name.",
                    unforced(),
                ))
            } else {
                None
            }
        }

        "tag" => {
            let show = || call.git(&["show", "--no-patch", call.positional(0).unwrap_or("<tag>")]);
            if has("-d") || has("--delete") {
                Some(Denial::new(Category::Force, "git tag --delete", "A deleted tag is gone from this clone's history of releases.", show()))
            } else if has("-f") || has("--force") {
                Some(Denial::new(
                    Category::Force,
                    "git tag --force",
                    "--force moves an existing tag. Anyone who fetched the old one keeps it, and the two disagree forever.",
                    show(),
                ))
            } else {
                None
            }
        }

        "stash" => match first_word(args) {
            Some("drop") => Some(Denial::new(
                Category::Force,
                "git stash drop",
                "The stash has no reflog you are likely to find in time.\n\
                 git stash show -p stash@{0}   look at it first",
                call.git(&["stash", "show", "-p", call.positional(1).unwrap_or("stash@{0}")]),
            )),
            Some("clear") => Some(Denial::new(Category::Force, "git stash clear", "This drops every stash entry at once.", call.git(&["stash", "list"]))),
            _ => None,
        },

        "update-ref" if has("-d") || has("--delete") => Some(Denial::new(
            Category::Force,
            "git update-ref -d",
            "Deleting a ref by hand bypasses every safety net the porcelain commands have.",
            call.git(&["log", "--oneline", "-1", call.positional(0).unwrap_or("<ref>")]),
        )),

        "reflog" => match first_word(args) {
            Some("expire" | "delete") => Some(Denial::new(
                Category::Force,
                format!("git reflog {}", first_word(args).unwrap_or("expire")),
                "The reflog is what makes `reset --hard` and a bad rebase recoverable. Expiring it is the one\n\
                 operation that turns a recoverable mistake into a permanent one."
                    .to_owned(),
                call.git(&["reflog"]),
            )),
            _ => None,
        },

        "gc" if flags.iter().any(|a| a.starts_with("--prune=") && matches!(&a["--prune=".len()..], "now" | "all")) => {
            Some(Denial::new(
                Category::Force,
                "git gc --prune=now",
                "This deletes unreachable objects immediately, including anything a reflog entry would have found.",
                call.rerun(|arg| (!arg.starts_with("--prune=")).then(|| arg.to_owned())),
            ))
        }

        "filter-branch" => Some(Denial::new(
            Category::Force,
            "git filter-branch",
            "Rewrites every commit it touches. git-filter-repo is the maintained replacement and is no less destructive.",
            call.git(&["filter-repo", "--analyze"]),
        )),

        "worktree" if first_word(args) == Some("remove") && (has("-f") || has("--force")) => Some(Denial::new(
            Category::Force,
            "git worktree remove --force",
            "--force removes a worktree that still has uncommitted changes in it.",
            call.without(&["-f", "--force"]),
        )),

        _ if has("--no-gpg-sign") => Some(no_gpg_sign(call)),

        _ => None,
    }
}

/// The rules for `git push`, read from `push` as Git reads its options.
#[allow(clippy::too_many_lines)]
fn push(call: &Invocation, push: &Parsed) -> Option<Denial> {
    let args = call.args;
    let integrate = call.git(&["pull", "--rebase"]);
    let refspecs: Vec<&str> = push
        .positionals
        .iter()
        .skip(1)
        .map(|&index| args[index].as_str())
        .collect();
    for (range, option) in &push.options {
        let deny = |reason: &str, hint: &str, next: Next| {
            Some(Denial::new(Category::Force, reason, hint, next))
        };
        let name = match option {
            Opt::Long {
                name,
                negated: false,
            } => *name,
            Opt::Short { letters, .. } => {
                match letters.chars().find(|letter| matches!(letter, 'f' | 'd')) {
                    Some('f') => "force",
                    Some(_) => "delete",
                    None => continue,
                }
            }
            _ => continue,
        };
        match name {
            "force" => {
                return deny(
                    "git push --force",
                    "A force push replaces history other clones already have.\n\
                     If you rebased on purpose:  ALLOW_FORCE=1 git push --force-with-lease\n\
                     --force-with-lease at least refuses when someone else has pushed since your last fetch.",
                    integrate,
                );
            }
            "force-if-includes" => {
                return deny(
                    "git push --force-if-includes",
                    "Still a force push. ALLOW_FORCE=1 to proceed.",
                    integrate,
                );
            }
            "mirror" => {
                return deny(
                    "git push --mirror",
                    "--mirror makes the remote match this clone exactly, deleting every ref you do not have.",
                    call.replace(range.start, "--all"),
                );
            }
            "delete" => {
                return deny(
                    "git push --delete",
                    "Deleting a remote ref is not recoverable from here.\n\
                     Delete merged branches through the forge, where the ref is still in the reflog.",
                    Next::Shell(forge_deletes(&refspecs)),
                );
            }
            "force-with-lease" => {
                return deny(
                    "git push --force-with-lease",
                    "Safer than --force and still a rewrite of published history.\n\
                     ALLOW_FORCE=1 git push --force-with-lease",
                    integrate,
                );
            }
            _ => {}
        }
    }
    // Refspecs: `+src:dst` forces that one ref, `:dst` deletes it.
    let forced: Vec<&str> = refspecs
        .iter()
        .copied()
        .filter(|a| a.strip_prefix('+').is_some_and(|s| s.contains(':')))
        .collect();
    let deleted: Vec<&str> = refspecs
        .iter()
        .filter_map(|a| a.strip_prefix(':'))
        .filter(|destination| !destination.is_empty())
        .collect();
    if forced.is_empty() && deleted.is_empty() {
        return None;
    }
    // The same push without forcing and without the deletions, which go through the forge instead.
    let unforced = Next::Git(
        call.globals
            .iter()
            .map(String::as_str)
            .chain([call.sub])
            .chain(args.iter().enumerate().filter_map(|(index, arg)| {
                if !push
                    .positionals
                    .iter()
                    .skip(1)
                    .any(|&refspec| refspec == index)
                {
                    return Some(arg.as_str());
                }
                (!(arg.len() > 1 && arg.starts_with(':'))).then(|| {
                    arg.strip_prefix('+')
                        .filter(|s| s.contains(':'))
                        .unwrap_or(arg)
                })
            }))
            .map(str::to_owned)
            .collect(),
    );
    let next = if deleted.is_empty() {
        unforced
    } else if refspecs.len() > deleted.len() {
        Next::Shell(format!(
            "{} && {}",
            forge_deletes(&deleted),
            unforced.line().unwrap_or_default()
        ))
    } else {
        Next::Shell(forge_deletes(&deleted))
    };
    let mut hint = Vec::new();
    if !forced.is_empty() {
        hint.push("A leading '+' on a refspec is a force push for that ref.");
    }
    if !deleted.is_empty() {
        hint.push("A refspec with an empty source deletes the remote ref.");
    }
    let refused: Vec<&str> = refspecs
        .iter()
        .copied()
        .filter(|a| forced.contains(a) || a.len() > 1 && a.starts_with(':'))
        .collect();
    Some(Denial::new(
        Category::Force,
        format!("git push refspec {}", refused.join(" ")),
        hint.join("\n"),
        next,
    ))
}

/// `denial` as a gate may give it for `parsed`: without a suggestion unless every option was read as Git reads it.
fn read_in_full(parsed: &Parsed, args: &[String], denial: Option<Denial>) -> Option<Denial> {
    match verdict(denial.is_some(), parsed.read()) {
        Verdict::Allow => None,
        Verdict::Suggest => denial,
        Verdict::Withhold => denial.map(|mut denial| {
            let unread: Vec<String> = parsed
                .options
                .iter()
                .filter(|(_, option)| matches!(option, Opt::Unread))
                .map(|(range, _)| command(&args[range.clone()]))
                .collect();
            denial.next = Next::Withheld(format!(
                "Git does not accept `{}` as written, so the gate cannot tell which words are option values, the repository, or refs, and a command rebuilt from them could act on something else.",
                unread.join("`, `")
            ));
            denial
        }),
    }
}

/// `--no-verify`, and for a commit `-n`, as Git reads them, with the same command without them.
fn skips_hooks(call: &Invocation, parsed: &Parsed) -> Option<Denial> {
    let commit = call.sub == "commit";
    let mut replaced: Vec<Option<Vec<String>>> = vec![None; call.args.len()];
    let mut skips = false;
    for (range, option) in &parsed.options {
        match option {
            Opt::Long {
                name: "no-verify",
                negated: false,
            } => {
                for index in range.clone() {
                    replaced[index] = Some(Vec::new());
                }
            }
            Opt::Short { letters, .. } if commit && letters.contains('n') => {
                let word = &call.args[range.start];
                let kept: String = letters.chars().filter(|&letter| letter != 'n').collect();
                let attached = &word[1 + letters.len()..];
                replaced[range.start] = Some(
                    if kept.is_empty() && attached.is_empty() && range.len() == 1 {
                        Vec::new()
                    } else {
                        vec![format!("-{kept}{attached}")]
                    },
                );
            }
            _ => continue,
        }
        skips = true;
    }
    skips.then(|| {
        let line = call
            .globals
            .iter()
            .cloned()
            .chain([call.sub.to_owned()])
            .chain(
                call.args
                    .iter()
                    .zip(replaced)
                    .flat_map(|(arg, words)| words.unwrap_or_else(|| vec![arg.clone()])),
            )
            .collect();
        Denial::new(
            Category::NoVerify,
            "--no-verify",
            "Hook verification cannot be skipped.",
            Next::Git(line),
        )
    })
}

fn no_gpg_sign(call: &Invocation) -> Denial {
    Denial::new(
        Category::NoVerify,
        "--no-gpg-sign",
        "Every commit on this machine is signed; an unsigned one is refused by the repository ruleset anyway.",
        call.without(&["--no-gpg-sign"]),
    )
}

/// Subcommands that actually run a hook.
/// Nothing else can be affected by `core.hooksPath`, which is the whole reason this list exists — see `config_override`.
const RUNS_HOOKS: [&str; 12] = [
    "commit",
    "merge",
    "rebase",
    "push",
    "pull",
    "am",
    "cherry-pick",
    "revert",
    "checkout",
    "switch",
    "applypatch",
    "clone",
];

/// Subcommands that create an object which would be signed.
const CREATES_SIGNED_OBJECTS: [&str; 7] = [
    "commit",
    "tag",
    "merge",
    "rebase",
    "cherry-pick",
    "revert",
    "am",
];

/// Whether a `-c key=value` sets something the signing policy owns.
fn is_policy_key(key: &str) -> bool {
    matches!(
        key,
        "commit.gpgsign" | "tag.gpgsign" | "core.hooksPath" | "gpg.format" | "gpg.ssh.program"
    )
}

/// `-c key=value` overrides that would change signing or hooks for one command; the caller fills in the command without the override.
///
/// Scoped to the subcommand, and that scoping is the whole point rather than a refinement.
/// Editors, agents and status-line tools routinely run their own read-only queries as
///
/// ```text
/// git -c core.hooksPath=/dev/null -c core.askPass= … rev-parse HEAD
/// ```
///
/// precisely so that *their* polling does not fire *your* hooks — which is correct, considerate behaviour on their part.
/// `rev-parse`, `status`, `remote get-url` and `ls-files` run no hooks, so disabling hooks for them changes nothing and refusing them breaks the tool for no gain.
/// An earlier version of this policy refused every one: 1,897 of the first 1,959 entries in the bypass log were exactly that, and the tooling behind them had been quietly broken the whole time.
///
/// So the rule is: `core.hooksPath` matters only where a hook would run, and the signing keys matter only where a signable object would be created.
fn config_override(setting: &str, sub: &str) -> Option<Denial> {
    let (key, value) = setting.split_once('=')?;
    let offends = match key {
        // `-c commit.gpgsign=false` is `--no-gpg-sign` wearing a hat.
        "commit.gpgsign" | "tag.gpgsign" => {
            CREATES_SIGNED_OBJECTS.contains(&sub)
                && matches!(value, "false" | "0" | "no" | "off" | "")
        }
        // A per-invocation hooksPath change can replace every gate in ~/.config/git/hooks.
        "core.hooksPath" => RUNS_HOOKS.contains(&sub),
        // Signing is delegated to the 1Password SSH agent; any other format means a key this machine does not actually hold.
        "gpg.format" => CREATES_SIGNED_OBJECTS.contains(&sub) && value != "ssh",
        "gpg.ssh.program" => CREATES_SIGNED_OBJECTS.contains(&sub) && value.is_empty(),
        _ => false,
    };
    offends.then(|| {
        Denial::new(
            Category::NoVerify,
            format!("-c {setting}"),
            format!("`{key}={value}` overrides signing or hook enforcement for this one command."),
            Next::Git(Vec::new()),
        )
    })
}

/// argv up to `--`, since everything after it is a pathspec and a file may legitimately be named `--force`.
fn flags_of(args: &[String]) -> Vec<String> {
    args.iter()
        .take_while(|a| a.as_str() != "--")
        .cloned()
        .collect()
}

/// The first non-flag argument — a subcommand word like `drop` or `expire`.
fn first_word(args: &[String]) -> Option<&str> {
    args.iter()
        .take_while(|a| a.as_str() != "--")
        .find(|a| !a.starts_with('-'))
        .map(String::as_str)
}

/// The letters of a clustered short-option group (`-fdx` -> `fdx`), or `None` for long options and bare `-` / `--`.
fn short_cluster(arg: &str) -> Option<&str> {
    let rest = arg.strip_prefix('-')?;
    (!rest.is_empty() && !rest.starts_with('-') && rest.chars().all(|c| c.is_ascii_alphabetic()))
        .then_some(rest)
}

#[cfg(test)]
mod tests {
    use super::{Authorship, Next, commit_retry, inspect, refusal};

    fn argv(line: &str) -> Vec<String> {
        line.split_whitespace().map(str::to_owned).collect()
    }
    /// Whether the line is refused; every refusal must also be a complete record whose suggested Git command passes.
    fn refused(line: &str) -> bool {
        let argv = argv(line);
        inspect(&argv).is_some_and(|denial| {
            let refusal = refusal(&denial, &argv);
            assert!(
                refusal.is_complete(),
                "incomplete refusal for git {line}: {refusal:?}"
            );
            if let Next::Git(words) = &denial.next {
                assert!(
                    inspect(words).is_none(),
                    "git {line} suggests a refused command: {:?}",
                    denial.next.line()
                );
            }
            true
        })
    }
    fn next(line: &str) -> String {
        inspect(&argv(line))
            .expect(line)
            .next
            .line()
            .unwrap_or_else(|| panic!("git {line} suggests no command"))
    }

    #[test]
    fn each_refusal_names_the_command_to_run_instead() {
        assert_eq!(next("commit -an -m x"), "git commit -a -m x");
        assert_eq!(
            next("-C repo commit --no-verify -m x"),
            "git -C repo commit -m x"
        );
        assert_eq!(next("push origin +main:main"), "git push origin main:main");
        assert_eq!(next("push -f"), "git pull --rebase");
        assert_eq!(
            next("push origin :refs/heads/topic"),
            "gh api -X DELETE 'repos/{owner}/{repo}/git/refs/heads/topic'"
        );
        assert_eq!(
            next("reset --hard HEAD~1"),
            "git stash push --include-untracked"
        );
        assert_eq!(next("branch -D Topic"), "git branch -d Topic");
        assert_eq!(next("branch -d -f topic"), "git branch -d topic");
        assert_eq!(next("worktree remove --force w"), "git worktree remove w");
        assert_eq!(
            next("-C repo -c core.hooksPath=/dev/null commit -m x"),
            "git -C repo commit -m x"
        );
        assert_eq!(
            next("-ccommit.gpgsign=false commit -m x"),
            "git commit -m x"
        );
    }

    /// The suggested Git command must itself pass the gate, however many refused parts the line combines.
    #[test]
    fn the_next_command_is_never_refused_again() {
        for line in [
            "-c core.hooksPath=/dev/null -c commit.gpgsign=false commit -m x",
            "-c commit.gpgsign=false commit --no-verify --no-gpg-sign -m x",
            "push origin +a:b :c",
            "push origin +a:b +c:d",
            "push --no-verify --force origin main",
            "-c core.hooksPath= push --no-verify origin +main:main",
            "branch -D -f topic",
            "commit -an --no-gpg-sign -m x",
        ] {
            let next = next(line);
            if let Some(words) = next.strip_prefix("git ") {
                assert!(
                    inspect(&argv(words)).is_none(),
                    "git {line} suggests a refused command: {next}"
                );
            }
        }
        assert_eq!(
            next("-c core.hooksPath=/dev/null -c commit.gpgsign=false commit -m x"),
            "git commit -m x"
        );
        assert_eq!(
            next("push origin +a:b :c"),
            "gh api -X DELETE 'repos/{owner}/{repo}/git/refs/heads/c' && git push origin a:b"
        );
        assert_eq!(next("push origin +a:b +c:d"), "git push origin a:b c:d");
        assert_eq!(
            next("push --delete origin a b"),
            "gh api -X DELETE 'repos/{owner}/{repo}/git/refs/heads/a' && gh api -X DELETE 'repos/{owner}/{repo}/git/refs/heads/b'"
        );
    }

    /// An option value is never a remote or a refspec, and a refspec after `--` is still a refspec.
    #[test]
    fn push_reads_its_positionals_as_git_does() {
        let delete = |branches: &[&str]| {
            branches
                .iter()
                .map(|b| format!("gh api -X DELETE 'repos/{{owner}}/{{repo}}/git/refs/heads/{b}'"))
                .collect::<Vec<_>>()
                .join(" && ")
        };
        for line in [
            "push --delete -o x origin a",
            "push --delete --push-option x origin a",
            "push --delete --receive-pack x origin a",
            "push --delete --exec x origin a",
            "push --delete --recurse-submodules check origin a",
            "push --delete -uo x origin a",
            "push --delete -ox origin a",
            "push --delete origin -- a",
            "push --delete --push-opt x origin a",
            "push --delete --rep x origin a",
            "push --delete --recu check origin a",
            "push --delete --receive x origin a",
            "push --delete --ex x origin a",
            "push --delete --no-rep origin a",
            "push --delete --no-push-option origin a",
        ] {
            assert!(refused(line), "should refuse: git {line}");
            assert_eq!(next(line), delete(&["a"]), "git {line}");
        }
        // Git takes the first positional as the repository even when --repo names one.
        assert_eq!(next("push --delete --repo=origin a"), delete(&["<branch>"]));
        assert_eq!(next("push --delete --repo origin a"), delete(&["<branch>"]));
        for line in [
            "push origin -- +main:main",
            "push origin -- :c",
            "push -- origin :c",
            "push -o x origin +a:b",
            "push --repo=origin origin +a:b",
        ] {
            assert!(refused(line), "should refuse: git {line}");
        }
        assert_eq!(
            next("push origin -- +main:main"),
            "git push origin -- main:main"
        );
        assert_eq!(next("push origin -- :c"), delete(&["c"]));
        assert_eq!(next("push -o x origin +a:b"), "git push -o x origin a:b");
        for line in [
            "push --repo=origin :c",
            "push --repo origin :c",
            "push --repo=origin +a:b",
            "push -o +a:b origin main",
            "push -o :c origin main",
            "push -o --force origin main",
        ] {
            assert!(!refused(line), "should allow: git {line}");
        }
    }

    /// Git reads a unique prefix of a long option as that option, and each letter of a short cluster as its own option.
    #[test]
    fn push_reads_abbreviated_and_clustered_options_as_git_does() {
        for line in [
            "push --force-w origin main",
            "push --force-with=main origin main",
            "push --force-i origin main",
            "push --mirr origin",
            "push --del origin a",
            "push -uf origin main",
            "push -qd origin a",
        ] {
            assert!(refused(line), "should refuse: git {line}");
        }
        assert_eq!(next("push -uf origin main"), "git pull --rebase");
        assert_eq!(next("push --mirr origin"), "git push --all origin");
        assert_eq!(
            next("push --del origin a"),
            "gh api -X DELETE 'repos/{owner}/{repo}/git/refs/heads/a'"
        );
        assert_eq!(
            next("push -qd origin a"),
            "gh api -X DELETE 'repos/{owner}/{repo}/git/refs/heads/a'"
        );
        for line in [
            // Ambiguous: Git refuses the command itself.
            "push --forc origin main",
            "push --d origin a",
            // Negations.
            "push --no-force origin main",
            "push --no-del origin a",
            // The value of `-o`, not a cluster of its own.
            "push -of origin main",
        ] {
            assert!(!refused(line), "should allow: git {line}");
        }
    }

    /// Git rejects a line with an option it does not accept, so the gate cannot know which words are values, the repository, or refs.
    /// A refused line of that kind gets no suggestion rather than one built from words Git would not read that way.
    #[test]
    fn a_refused_line_with_an_option_git_rejects_suggests_nothing() {
        for line in [
            "push --delete --re x origin a",
            "push --delete --bogus x origin a",
            "push --delete --d x origin a",
            "push --delete -x origin a",
            "push --delete -ux origin a",
            "push --delete --porcelain=x origin a",
            "push --delete --no-repo=x origin a",
            "push --force origin main --repo",
            "push --delete origin a -o",
            "push --no-verify --bogus origin main",
            "push origin +a:b --bogus",
            "commit --no-verify --bogus -m x",
            "commit -n -x -m x",
        ] {
            let words = argv(line);
            let denial = inspect(&words).unwrap_or_else(|| panic!("should refuse: git {line}"));
            assert!(
                matches!(denial.next, Next::Withheld(_)),
                "git {line} suggests {:?}",
                denial.next.line()
            );
            let refusal = refusal(&denial, &words);
            assert!(refusal.is_complete(), "{refusal:?}");
            assert_eq!(refusal.next, None, "git {line}");
            assert!(
                refusal
                    .cause
                    .contains("No command is suggested: Git does not accept"),
                "{}",
                refusal.cause
            );
        }
        for line in [
            "push --re x origin a",
            "push --bogus origin main",
            "push --delete=x origin a",
        ] {
            assert!(!refused(line), "should allow: git {line}");
        }
    }

    /// `-n` skips hooks only as an option letter, not inside the value of `-m`.
    #[test]
    fn commit_reads_n_as_git_does() {
        assert!(!refused("commit -mnote"));
        assert!(!refused("commit -m -n"));
        assert_eq!(next("commit -qn -m x"), "git commit -q -m x");
        assert_eq!(next("commit -nmx"), "git commit -mx");
        assert_eq!(next("commit -anm x"), "git commit -am x");
        assert_eq!(next("commit --no-veri -m x"), "git commit -m x");
        assert_eq!(
            next("commit --no-verify --verify -m x"),
            "git commit --verify -m x"
        );
    }

    #[test]
    fn only_waivable_rules_carry_a_waiver() {
        let line = argv("reset --hard");
        let force = refusal(&inspect(&line).unwrap(), &line);
        assert_eq!(force.rule, "git.force");
        assert_eq!(
            force.waiver.as_deref(),
            Some("ALLOW_FORCE=1 git reset --hard")
        );
        let line = argv("commit --no-verify -m 'x y'");
        let no_verify = refusal(&inspect(&line).unwrap(), &line);
        assert_eq!(no_verify.rule, "git.no-verify");
        assert_eq!(no_verify.waiver, None);
    }

    #[test]
    fn destructive_commands_are_refused() {
        for line in [
            "push --force",
            "push -f origin main",
            "push --force-with-lease",
            "push --force-if-includes",
            "push --mirror",
            "push --delete origin topic",
            "push origin :topic",
            "push origin +main:main",
            "reset --hard HEAD~1",
            "clean -fdx",
            "clean --force",
            "checkout -f main",
            "switch --discard-changes main",
            "branch -D topic",
            "branch -M main",
            "tag -d v1",
            "tag -f v1",
            "stash drop",
            "stash clear",
            "update-ref -d refs/heads/topic",
            "reflog expire --expire=now --all",
            "gc --prune=now",
            "filter-branch --tree-filter true HEAD",
            "worktree remove --force w",
            "commit --no-verify -m x",
            "commit -n -m x",
            "commit -an -m x",
            "push --no-verify origin main",
            "commit --no-gpg-sign -m x",
        ] {
            assert!(refused(line), "should refuse: git {line}");
        }
    }

    #[test]
    fn ordinary_commands_are_not() {
        for line in [
            "status",
            "commit -m x",
            "push",
            "push origin main",
            "push --tags",
            "add --force ignored.txt", // -f here means "add an ignored file", not "destroy"
            "branch -d merged-topic",
            "clean -nd",
            "checkout main",
            "switch main",
            "rebase -i main",
            "commit --amend --no-edit",
            "stash list",
            "stash pop",
            "tag v1",
            "gc --auto",
            "log --oneline",
            "rm -f path", // reachable from the index in every case that matters
        ] {
            assert!(!refused(line), "should allow: git {line}");
        }
    }

    /// The regression that motivated scoping config overrides to the subcommand.
    /// Editors and agents run read-only queries with hooks disabled so their polling does not fire yours; refusing those breaks the tool and protects nothing.
    #[test]
    fn read_only_queries_may_disable_hooks() {
        for line in [
            "-c core.hooksPath=/dev/null rev-parse HEAD",
            "-c core.hooksPath=/dev/null -c core.askPass= remote get-url origin",
            "-c core.hooksPath=/dev/null --no-optional-locks status --porcelain",
            "-c core.hooksPath=/dev/null ls-files --recurse-submodules",
            "-c commit.gpgsign=false config --get user.name",
            "-c gpg.format=openpgp log --show-signature -1",
        ] {
            assert!(!refused(line), "should allow: git {line}");
        }
    }

    #[test]
    fn the_same_overrides_are_refused_where_they_would_bite() {
        for line in [
            "-c core.hooksPath=/dev/null commit -m x",
            "-c core.hooksPath= push origin main",
            "-c core.hooksPath=/tmp/other push origin main",
            "-c commit.gpgsign=false commit -m x",
            "-c tag.gpgsign=no tag v1",
            "-c gpg.format=openpgp commit -m x",
            "-ccommit.gpgsign=false commit -m x",
        ] {
            assert!(refused(line), "should refuse: git {line}");
        }
    }

    #[test]
    fn a_pathspec_named_like_a_flag_is_not_a_flag() {
        assert!(!refused("clean -n -- --force"));
    }

    fn other(revision: &str) -> Option<Authorship> {
        (revision != "unreadable").then(|| Authorship {
            author: format!("Other <{revision}@e.invalid>"),
            date: "1700000000 +0900".to_owned(),
        })
    }

    fn retry(line: &str) -> Option<Result<String, String>> {
        commit_retry(&argv(line), "/r/.git/COMMIT_EDITMSG", other)
            .map(|retry| retry.map(|words| words.join(" ")))
    }

    /// The retry keeps every option of the refused commit and replaces only where its message comes from.
    #[test]
    fn a_refused_commit_is_retried_with_its_saved_message() {
        let edited = "--edit --file /r/.git/COMMIT_EDITMSG";
        for (line, expected) in [
            ("git commit -q --amend -m x", "git commit -q --amend"),
            (
                "git -C sub -c a.b=c commit -am x",
                "git -C sub -c a.b=c commit -a",
            ),
            ("git commit -qam x", "git commit -qa"),
            ("git commit -mx -a", "git commit -a"),
            ("git commit --mess x --am", "git commit --am"),
            ("git commit --message=x -m y --no-edit", "git commit"),
            ("git commit -e -F msg -t tpl", "git commit"),
            ("git commit --fixup x", "git commit"),
            ("git commit --squash=x -m y", "git commit"),
            ("git commit -m -F", "git commit"),
            ("git commit --author a -m x", "git commit --author a"),
            ("git commit -Smkey -m x", "git commit -Smkey"),
            ("git commit -m x a.txt", "git commit a.txt"),
        ] {
            assert_eq!(
                retry(line),
                Some(Ok(format!("{expected} {edited}"))),
                "{line}"
            );
        }
        assert_eq!(
            retry("git commit -m x -- a.txt"),
            Some(Ok(format!("git commit {edited} -- a.txt")))
        );
        for line in ["git rebase --continue", "git ci -m x", "git", "commit -m x"] {
            assert_eq!(retry(line), None, "{line}");
        }
    }

    /// An `amend!` commit may be empty and a reword ignores staged changes, so the retry says so itself once `--fixup` is gone.
    #[test]
    fn a_retried_fixup_keeps_what_its_kind_implied() {
        let edited = "--edit --file /r/.git/COMMIT_EDITMSG";
        for (line, expected) in [
            ("git commit --fixup=amend:HEAD", "git commit --allow-empty"),
            (
                "git commit -q --fixup amend:HEAD",
                "git commit -q --allow-empty",
            ),
            ("git commit --fix=amend:HEAD", "git commit --allow-empty"),
            (
                "git commit --fixup=reword:HEAD",
                "git commit --only --allow-empty",
            ),
            (
                "git commit --fixup reword:HEAD",
                "git commit --only --allow-empty",
            ),
            ("git commit --fixup=HEAD", "git commit"),
            (
                "git commit --fixup=amend:HEAD --no-fixup -m x",
                "git commit",
            ),
        ] {
            assert_eq!(
                retry(line),
                Some(Ok(format!("{expected} {edited}"))),
                "{line}"
            );
        }
    }

    /// `-C` and `-c` copy the author and date of their commit, so the retry names them, before any explicit `--author` or `--date` that overrides them.
    #[test]
    fn a_retried_reuse_keeps_the_reused_authorship() {
        let edited = "--edit --file /r/.git/COMMIT_EDITMSG";
        let copied = |revision: &str| {
            format!("--author Other <{revision}@e.invalid> --date 1700000000 +0900")
        };
        for (line, expected) in [
            ("git commit -C X", format!("git commit {}", copied("X"))),
            (
                "git commit -c HEAD",
                format!("git commit {}", copied("HEAD")),
            ),
            ("git commit -qCX", format!("git commit {} -q", copied("X"))),
            (
                "git commit --reuse-message=X --amend",
                format!("git commit {} --amend", copied("X")),
            ),
            (
                "git commit --reedit X --author Z",
                format!("git commit {} --author Z", copied("X")),
            ),
            (
                "git commit --squash=Y -C X",
                format!("git commit {}", copied("X")),
            ),
            (
                "git commit -C X --reset-author",
                "git commit --reset-author".to_owned(),
            ),
            (
                "git commit --reset-author -C X --no-reset-author",
                format!(
                    "git commit {} --reset-author --no-reset-author",
                    copied("X")
                ),
            ),
            (
                "git commit -C X --no-reuse-message -m y",
                "git commit".to_owned(),
            ),
        ] {
            assert_eq!(
                retry(line),
                Some(Ok(format!("{expected} {edited}"))),
                "{line}"
            );
        }
        let withheld = retry("git commit -C unreadable");
        assert!(
            withheld.as_ref().is_some_and(|retry| retry
                .as_ref()
                .is_err_and(|reason| reason.contains("`unreadable`"))),
            "{withheld:?}"
        );
        assert_eq!(
            retry("git commit -C unreadable --reset-author"),
            Some(Ok(format!("git commit --reset-author {edited}")))
        );
    }
}
