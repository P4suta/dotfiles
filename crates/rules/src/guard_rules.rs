//! The classifiers of the Git guard: agent attribution lines, push verdicts, pinned Renovate images, configuration overrides, and civil dates.
//! Each receives its facts as values, so the guard asks Git and the clock and passes the answers in.

extern crate alloc;
use self::alloc::string::String;
#[cfg(test)]
use self::alloc::vec::Vec;

/// The bytes after any leading ASCII whitespace.
fn trim_start(bytes: &[u8]) -> &[u8] {
    let mut rest = bytes;
    while let [first, tail @ ..] = rest {
        if !first.is_ascii_whitespace() {
            break;
        }
        rest = tail;
    }
    rest
}

/// The bytes before any trailing ASCII whitespace.
fn trim_end(bytes: &[u8]) -> &[u8] {
    let mut rest = bytes;
    while let [head @ .., last] = rest {
        if !last.is_ascii_whitespace() {
            break;
        }
        rest = head;
    }
    rest
}

/// `bytes` without `prefix`, which matches whatever its ASCII case.
fn strip_prefix_ci<'a>(bytes: &'a [u8], prefix: &[u8]) -> Option<&'a [u8]> {
    let (head, tail) = bytes.split_at_checked(prefix.len())?;
    ascii_eq(head, prefix).then_some(tail)
}

fn ascii_eq(left: &[u8], right: &[u8]) -> bool {
    if left.len() != right.len() {
        return false;
    }
    for index in 0..left.len() {
        if !left
            .get(index)
            .zip(right.get(index))
            .is_some_and(|(left, right)| left.eq_ignore_ascii_case(right))
        {
            return false;
        }
    }
    true
}

/// Whether `bytes` ends with `suffix` in any ASCII case.
fn ends_with_ci(bytes: &[u8], suffix: &[u8]) -> bool {
    bytes
        .len()
        .checked_sub(suffix.len())
        .and_then(|start| bytes.get(start..))
        .is_some_and(|tail| ascii_eq(tail, suffix))
}

/// The session links Claude Code adds to a message.
const SESSION_LINK: &[u8] = b"https://claude.ai/code/";

/// The domain of the trailers an Anthropic agent adds, which email treats in any case.
const AGENT_DOMAIN: &[u8] = b"@anthropic.com";

/// The robot face that opens an agent attribution line.
const ROBOT: &[u8] = "\u{1f916}".as_bytes();

/// Whether this line credits an AI agent.
/// Each pattern anchors to trailer position, so human `Co-authored-by` trailers and prose that mentions CLAUDE.md survive.
#[must_use]
pub fn is_attribution(line: &[u8]) -> bool {
    let start = trim_start(line);
    let tail = trim_end(start);
    // `Claude-Session: https://…`
    if let Some(rest) = strip_prefix_ci(start, b"claude-session:") {
        let rest = trim_start(rest);
        if rest.starts_with(b"https://") || rest.starts_with(b"http://") {
            return true;
        }
    }
    // `Co-Authored-By: <anything> <…@anthropic.com>`
    if strip_prefix_ci(start, b"co-authored-by:").is_some()
        && (ends_with_ci(tail, AGENT_DOMAIN)
            || tail
                .strip_suffix(b">")
                .is_some_and(|inner| ends_with_ci(inner, AGENT_DOMAIN)))
    {
        return true;
    }
    // `🤖 Generated with [Claude Code](…)`
    if start
        .strip_prefix(ROBOT)
        .is_some_and(|rest| trim_start(rest).starts_with(b"Generated with [Claude Code]"))
    {
        return true;
    }
    // A bare session link on a line of its own.
    tail.strip_prefix(SESSION_LINK)
        .is_some_and(|rest| !rest.is_empty() && !rest.iter().any(u8::is_ascii_whitespace))
}

/// The message without its attribution lines and the trailing blank lines they leave, and how many lines it removed.
/// A message without attribution comes back byte for byte.
#[must_use]
pub fn strip_attribution(text: &str) -> (String, usize) {
    let mut removed = 0;
    let mut out = String::with_capacity(text.len());
    let mut last_content = 0;
    let mut rest = text;
    for _ in 0..text.len() {
        let Some((line, tail)) = next_line(rest) else {
            break;
        };
        rest = tail;
        if is_attribution(line.as_bytes()) {
            removed += 1;
        } else {
            out.push_str(line);
            out.push('\n');
            if !line.trim().is_empty() {
                last_content = out.len();
            }
        }
    }
    if removed == 0 {
        out.clear();
        out.push_str(text);
    } else {
        out.truncate(last_content);
    }
    (out, removed)
}

fn next_line(text: &str) -> Option<(&str, &str)> {
    if text.is_empty() {
        return None;
    }
    let Some(index) = text.as_bytes().iter().position(|byte| *byte == b'\n') else {
        return Some((text, ""));
    };
    let (line, tail) = text.split_at(index);
    let end = line.len() - usize::from(line.as_bytes().last() == Some(&b'\r'));
    Some((
        line.get(..end).unwrap_or_default(),
        tail.get(1..).unwrap_or_default(),
    ))
}

/// What a push does to one remote ref.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Push {
    /// A new ref, or a fast-forward of the remote commit.
    Advance,
    /// An all-zero local name deletes the ref.
    Delete,
    /// The local commit leaves the remote commit behind.
    Rewrite,
    /// This clone lacks the remote commit, so ancestry stays unknown, the shape a push takes after someone else rewrote the ref.
    Undecidable,
}

/// What Git answers about the remote commit of a push.
#[derive(Clone, Copy, Debug)]
pub struct Ancestry {
    /// Whether this clone holds the remote commit.
    pub known: bool,
    /// Whether the local commit descends from the remote commit.
    pub descends: bool,
}

/// Whether an object name matches the all-zero name of a missing object.
fn zero(name: &[u8]) -> bool {
    name.iter().all(|byte| *byte == b'0')
}

/// The push verdict for a ref moving from `remote` to `local`, asking `ancestry` only what the names leave open.
#[must_use]
pub fn push(local: &[u8], remote: &[u8], ancestry: Ancestry) -> Push {
    if zero(local) {
        Push::Delete
    } else if zero(remote) {
        Push::Advance
    } else if !ancestry.known {
        Push::Undecidable
    } else if ancestry.descends {
        Push::Advance
    } else {
        Push::Rewrite
    }
}

/// The hexadecimal digits of a SHA-256 digest.
pub const DIGEST_LENGTH: usize = 64;

/// The repository a Renovate image must come from.
const RENOVATE: &[u8] = b"renovate/renovate:";
const SHA256_TAG: &[u8] = b"@sha256:";

/// The bytes after a nonempty leading run of ASCII digits.
fn digits(part: &[u8]) -> Option<&[u8]> {
    if !part.first().is_some_and(u8::is_ascii_digit) {
        return None;
    }
    let mut rest = part;
    while let Some((first, tail)) = rest.split_first() {
        if !first.is_ascii_digit() {
            break;
        }
        rest = tail;
    }
    Some(rest)
}

/// Whether an image reference names an exact `renovate/renovate` version with a SHA-256 digest, so no caller can fall back to `:latest`.
#[must_use]
pub fn image_is_pinned(image: &[u8]) -> bool {
    let Some(rest) = image.strip_prefix(RENOVATE) else {
        return false;
    };
    let versioned = digits(rest)
        .and_then(|rest| rest.strip_prefix(b"."))
        .and_then(digits)
        .and_then(|rest| rest.strip_prefix(b"."))
        .and_then(digits)
        .and_then(|rest| rest.strip_prefix(SHA256_TAG));
    let Some(digest) = versioned else {
        return false;
    };
    digest.len() == DIGEST_LENGTH && digest.iter().all(u8::is_ascii_hexdigit)
}

/// Subcommands that run the commit, merge, push, and checkout hooks.
#[cfg(any(test, kani))]
const RUNS_HOOKS: [&[u8]; 12] = [
    b"commit",
    b"merge",
    b"rebase",
    b"push",
    b"pull",
    b"am",
    b"cherry-pick",
    b"revert",
    b"checkout",
    b"switch",
    b"applypatch",
    b"clone",
];

/// Subcommands that create a signable object.
#[cfg(any(test, kani))]
const CREATES_SIGNED_OBJECTS: [&[u8]; 7] = [
    b"commit",
    b"tag",
    b"merge",
    b"rebase",
    b"cherry-pick",
    b"revert",
    b"am",
];

/// The configuration keys the signing and hook policy owns.
/// Git reads the section and the key name in any case, so `core.hookspath` names `core.hooksPath`.
const POLICY_KEYS: [&[u8]; 5] = [
    b"commit.gpgsign",
    b"tag.gpgsign",
    b"core.hooksPath",
    b"gpg.format",
    b"gpg.ssh.program",
];

/// The policy key that `key` names in any case, as its index in the policy list.
fn policy(key: &[u8]) -> Option<usize> {
    if ascii_eq(key, POLICY_KEYS[0]) {
        Some(0)
    } else if ascii_eq(key, POLICY_KEYS[1]) {
        Some(1)
    } else if ascii_eq(key, POLICY_KEYS[HOOKS_PATH]) {
        Some(HOOKS_PATH)
    } else if ascii_eq(key, POLICY_KEYS[GPG_FORMAT]) {
        Some(GPG_FORMAT)
    } else if ascii_eq(key, POLICY_KEYS[SIGNING_PROGRAM]) {
        Some(SIGNING_PROGRAM)
    } else {
        None
    }
}

/// Whether a `-c key=value` or `--config-env` setting names a key the signing and hook policy owns.
#[must_use]
pub fn policy_key(key: &[u8]) -> bool {
    policy(key).is_some()
}

/// The index of `core.hooksPath` in the policy list.
const HOOKS_PATH: usize = 2;
/// The index of `gpg.format` in the policy list.
const GPG_FORMAT: usize = 3;
const SIGNING_PROGRAM: usize = 4;

/// Whether `-c key=value` would turn signing or hook enforcement off for `sub`.
/// Read-only commands such as `rev-parse` run no hook and sign nothing, so editors may poll them with their own hooks path.
#[must_use]
pub fn overrides_policy(key: &[u8], value: &[u8], sub: &[u8]) -> bool {
    match policy(key) {
        // `-c commit.gpgsign=false` acts as `--no-gpg-sign`.
        Some(0 | 1) => {
            creates_signed_objects(sub)
                && [b"false".as_slice(), b"0", b"no", b"off", b""]
                    .iter()
                    .any(|off| ascii_eq(off, value))
        }
        // A per-invocation hooksPath change can replace every gate in ~/.config/git/hooks.
        Some(HOOKS_PATH) => matches!(
            sub,
            b"commit"
                | b"merge"
                | b"rebase"
                | b"push"
                | b"pull"
                | b"am"
                | b"cherry-pick"
                | b"revert"
                | b"checkout"
                | b"switch"
                | b"applypatch"
                | b"clone"
        ),
        // The SSH agent signs every commit, so any other format names a key this machine lacks.
        Some(GPG_FORMAT) => creates_signed_objects(sub) && !ascii_eq(value, b"ssh"),
        Some(_) => creates_signed_objects(sub) && value.is_empty(),
        None => false,
    }
}

fn creates_signed_objects(sub: &[u8]) -> bool {
    matches!(
        sub,
        b"commit" | b"tag" | b"merge" | b"rebase" | b"cherry-pick" | b"revert" | b"am"
    )
}

/// A literal configuration override or a key whose value comes from the environment.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Config<'a> {
    Inline(&'a str),
    Environment(&'a str),
}

/// An override and the words that supplied it.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Setting<'a> {
    pub config: Config<'a>,
    pub span: core::ops::Range<usize>,
}

/// The global Git options that consume a separate value.
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

const INLINE_CONFIG: &[u8] = b"-c";
const OPTION_TERMINATOR: &[u8] = b"--";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Global {
    Subcommand,
    Separator,
    Option(Option<ConfigValue>),
    ValueOption(Option<ConfigValue>),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ConfigKind {
    Inline,
    Environment,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct ConfigValue {
    kind: ConfigKind,
    next: bool,
    start: usize,
    end: usize,
}

#[derive(Debug)]
struct Entry {
    config: ConfigValue,
    span: core::ops::Range<usize>,
}

fn resolve<'a>(args: &[&'a str], entry: Entry) -> Setting<'a> {
    let word = args
        .get(entry.span.start + usize::from(entry.config.next))
        .copied()
        .unwrap_or_default();
    let value = word
        .get(entry.config.start..entry.config.end)
        .unwrap_or_default();
    let config = match entry.config.kind {
        ConfigKind::Inline => Config::Inline(value),
        ConfigKind::Environment => Config::Environment(value),
    };
    Setting {
        config,
        span: entry.span,
    }
}

fn byte_prefix(text: &str, prefix: &[u8]) -> bool {
    let bytes = text.as_bytes();
    if bytes.len() < prefix.len() {
        return false;
    }
    for index in 0..prefix.len() {
        if bytes.get(index) != prefix.get(index) {
            return false;
        }
    }
    true
}

/// The subcommand index and every configuration override before it.
/// A missing value may put the index one past the argument list.
#[must_use]
pub fn global_scan<'a, 'word>(args: &'a [&'word str]) -> GlobalScan<'a, 'word> {
    GlobalScan {
        args,
        cursor: Cursor {
            index: 0,
            done: false,
        },
    }
}

/// The configuration settings before the subcommand, borrowed from the invocation.
#[derive(Debug)]
pub struct GlobalScan<'a, 'word> {
    args: &'a [&'word str],
    cursor: Cursor,
}

const VALUE_OPTION_WORDS: usize = 2;

#[derive(Debug)]
struct Cursor {
    index: usize,
    done: bool,
}

impl GlobalScan<'_, '_> {
    /// The next argument after all settings consumed so far.
    #[must_use]
    pub const fn index(&self) -> usize {
        self.cursor.index
    }
}

impl<'a> Iterator for GlobalScan<'_, 'a> {
    type Item = Setting<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        for _ in 0..self.args.len() {
            if self.cursor.done {
                return None;
            }
            let Some(argument) = self.args.get(self.cursor.index) else {
                self.cursor.done = true;
                return None;
            };
            let word = global_word(argument, self.args.get(self.cursor.index + 1).copied());
            if let Some(entry) = self.cursor.advance(word) {
                return Some(resolve(self.args, entry));
            }
        }
        None
    }
}

impl Cursor {
    fn advance(&mut self, word: Global) -> Option<Entry> {
        if self.done {
            return None;
        }
        match word {
            Global::Subcommand => {
                self.done = true;
                None
            }
            Global::Separator => {
                self.index += 1;
                self.done = true;
                None
            }
            Global::Option(config) | Global::ValueOption(config) => {
                let start = self.index;
                self.index += if matches!(word, Global::ValueOption(_)) {
                    VALUE_OPTION_WORDS
                } else {
                    1
                };
                config.map(|config| Entry {
                    config,
                    span: start..self.index,
                })
            }
        }
    }
}

fn global_word(argument: &str, next: Option<&str>) -> Global {
    if argument.as_bytes().first() != Some(&b'-') {
        return Global::Subcommand;
    }
    if argument.len() == OPTION_TERMINATOR.len() && byte_prefix(argument, OPTION_TERMINATOR) {
        return Global::Separator;
    }
    if byte_prefix(argument, INLINE_CONFIG) {
        let rest = argument.get(INLINE_CONFIG.len()..).unwrap_or_default();
        return if rest.is_empty() {
            Global::ValueOption(next.map(|value| ConfigValue {
                kind: ConfigKind::Inline,
                next: true,
                start: 0,
                end: value.len(),
            }))
        } else {
            Global::Option(Some(ConfigValue {
                kind: ConfigKind::Inline,
                next: false,
                start: INLINE_CONFIG.len(),
                end: argument.len(),
            }))
        };
    }
    if byte_prefix(argument, b"--config-env") {
        let inline = byte_prefix(argument, b"--config-env=");
        let value = if inline {
            argument.get("--config-env=".len()..).unwrap_or_default()
        } else {
            next.unwrap_or_default()
        };
        let mut end = value.len();
        for byte in 0..value.len() {
            if value.as_bytes().get(byte) == Some(&b'=') {
                end = byte;
                break;
            }
        }
        let key = value.get(..end).unwrap_or_default();
        let start = if inline { "--config-env=".len() } else { 0 };
        let config = policy_key(key.as_bytes()).then_some(ConfigValue {
            kind: ConfigKind::Environment,
            next: !inline,
            start,
            end: start + end,
        });
        return if inline {
            Global::Option(config)
        } else {
            Global::ValueOption(config)
        };
    }
    for option in GLOBAL_TAKES_VALUE {
        if argument.len() == option.len() && byte_prefix(argument, option.as_bytes()) {
            return Global::ValueOption(None);
        }
    }
    Global::Option(None)
}

pub const SECONDS_PER_MINUTE: i64 = 60;
pub const SECONDS_PER_HOUR: i64 = 60 * SECONDS_PER_MINUTE;
pub const SECONDS_PER_DAY: i64 = 24 * SECONDS_PER_HOUR;

pub const MONTHS_PER_YEAR: i64 = 12;
pub const LAST_DAY_OF_MONTH: i64 = 31;
pub const LAST_HOUR: i64 = 23;
pub const LAST_MINUTE: i64 = 59;
/// A leap second adds one second to a minute.
pub const LAST_SECOND: i64 = 60;

/// The days `1970-01-01` lies after `0000-03-01`, the start of era zero.
const EPOCH_SHIFT: i64 = 719_468;
/// The calendar repeats every 400 years of 146,097 days.
pub const DAYS_PER_ERA: i64 = 146_097;
const YEARS_PER_ERA: i64 = 400;
const DAYS_PER_YEAR: i64 = 365;
/// The days of four years and of a century, which mark where a leap day falls in an era.
const DAYS_PER_FOUR_YEARS: i64 = 1460;
const DAYS_PER_CENTURY: i64 = 36_524;
const YEARS_PER_LEAP: i64 = 4;
const YEARS_PER_CENTURY: i64 = 100;
/// The five months from March through July hold 153 days, which spreads month lengths evenly from March.
const DAYS_PER_FIVE_MONTHS: i64 = 153;
const MONTHS_PER_FIVE: i64 = 5;
/// The day of the year where each month's shift rounds to its first day.
const MONTH_ROUNDING: i64 = 2;
/// March counts as month zero, so January and February belong to the next civil year.
const MARCH: i64 = 3;
const FEBRUARY: i64 = 2;

/// Days since the Unix epoch of the date `year-month-day`, by Howard Hinnant's `days_from_civil`.
#[must_use]
pub const fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
    let year = if month <= FEBRUARY { year - 1 } else { year };
    let era = if year >= 0 {
        year
    } else {
        year - (YEARS_PER_ERA - 1)
    } / YEARS_PER_ERA;
    let yoe = year - era * YEARS_PER_ERA;
    let mp = (month + MONTHS_PER_YEAR - MARCH) % MONTHS_PER_YEAR;
    let doy = (DAYS_PER_FIVE_MONTHS * mp + MONTH_ROUNDING) / MONTHS_PER_FIVE + day - 1;
    let doe = yoe * DAYS_PER_YEAR + yoe / YEARS_PER_LEAP - yoe / YEARS_PER_CENTURY + doy;
    era * DAYS_PER_ERA + doe - EPOCH_SHIFT
}

/// The date `(year, month, day)` of the day `days` after the Unix epoch, by Howard Hinnant's `civil_from_days`.
#[must_use]
pub const fn civil_from_days(days: i64) -> (i64, i64, i64) {
    let z = days + EPOCH_SHIFT;
    let era = if z >= 0 { z } else { z - (DAYS_PER_ERA - 1) } / DAYS_PER_ERA;
    let doe = z - era * DAYS_PER_ERA;
    let yoe = (doe - doe / DAYS_PER_FOUR_YEARS + doe / DAYS_PER_CENTURY - doe / (DAYS_PER_ERA - 1))
        / DAYS_PER_YEAR;
    let year = yoe + era * YEARS_PER_ERA;
    let doy = doe - (DAYS_PER_YEAR * yoe + yoe / YEARS_PER_LEAP - yoe / YEARS_PER_CENTURY);
    let mp = (MONTHS_PER_FIVE * doy + MONTH_ROUNDING) / DAYS_PER_FIVE_MONTHS;
    let day = doy - (DAYS_PER_FIVE_MONTHS * mp + MONTH_ROUNDING) / MONTHS_PER_FIVE + 1;
    let month = (mp + MARCH - 1) % MONTHS_PER_YEAR + 1;
    (if month <= FEBRUARY { year + 1 } else { year }, month, day)
}

/// The properties Kani proves for every input and the tests check over every input they list.
#[cfg(any(test, kani))]
mod proofs {
    use super::*;

    pub static ARGUMENTS: [&str; 14] = [
        "commit",
        "status",
        "-c",
        "core.hooksPath=x",
        "-ccore.hooksPath=x",
        "--config-env=core.hooksPath=ENV",
        "--config-env",
        "core.hooksPath=ENV",
        "-C",
        "fixture",
        "--",
        "--force",
        "--forc",
        "--no-verify",
    ];

    const HOOKS_KEY: &str = "core.hooksPath";
    const ENVIRONMENT_PREFIX: &str = "--config-env=";
    const ARGUMENT_LENGTHS: [u8; 14] = [6, 6, 2, 16, 18, 31, 12, 18, 2, 7, 2, 7, 6, 11];

    pub fn argument_length(code: u8) -> usize {
        usize::from(
            ARGUMENT_LENGTHS
                .get(usize::from(code))
                .copied()
                .unwrap_or_default(),
        )
    }

    fn argument(code: u8) -> &'static str {
        ARGUMENTS
            .get(usize::from(code))
            .copied()
            .unwrap_or_default()
    }

    fn inline_setting() -> ConfigValue {
        ConfigValue {
            kind: ConfigKind::Inline,
            next: false,
            start: INLINE_CONFIG.len(),
            end: argument_length(4),
        }
    }

    fn environment_setting(code: u8) -> ConfigValue {
        let start = if code == 5 {
            ENVIRONMENT_PREFIX.len()
        } else {
            0
        };
        ConfigValue {
            kind: ConfigKind::Environment,
            next: code != 5,
            start,
            end: start + HOOKS_KEY.len(),
        }
    }

    #[derive(Debug, Eq, PartialEq)]
    struct ScanConfig {
        kind: ConfigKind,
        next: bool,
        start: u8,
        end: u8,
    }

    struct ScanEntry {
        config: ScanConfig,
        span: core::ops::Range<u8>,
    }

    fn scan_offset(value: usize) -> u8 {
        let compact = u8::try_from(value);
        assert!(compact.is_ok());
        let compact = compact.unwrap_or_default();
        assert_eq!(usize::from(compact), value);
        compact
    }

    pub struct ScanResult {
        index: usize,
        settings: [Option<ScanEntry>; 4],
        length: usize,
    }

    impl ScanResult {
        fn empty() -> Self {
            Self {
                index: 0,
                settings: [const { None }; 4],
                length: 0,
            }
        }

        fn push(&mut self, setting: &Entry) {
            assert!(self.length < self.settings.len());
            let setting = ScanEntry {
                config: ScanConfig {
                    kind: setting.config.kind,
                    next: setting.config.next,
                    start: scan_offset(setting.config.start),
                    end: scan_offset(setting.config.end),
                },
                span: scan_offset(setting.span.start)..scan_offset(setting.span.end),
            };
            if let Some(slot) = self.settings.get_mut(self.length) {
                *slot = Some(setting);
            }
            self.length += 1;
        }
    }

    pub fn spec_scan(codes: &[u8]) -> ScanResult {
        assert!(codes.len() <= 4);
        let args = [
            argument(codes.first().copied().unwrap_or_default()),
            argument(codes.get(1).copied().unwrap_or_default()),
            argument(codes.get(2).copied().unwrap_or_default()),
            argument(codes.get(3).copied().unwrap_or_default()),
        ];
        let mut scan = global_scan(args.get(..codes.len()).unwrap_or_default());
        let mut actual = ScanResult::empty();
        for _ in 0..=codes.len() {
            let Some(setting) = scan.next() else {
                break;
            };
            let word = grammar_word(codes, setting.span.start);
            let config = match word {
                Some(Global::Option(config) | Global::ValueOption(config)) => config,
                _ => None,
            };
            assert!(config.is_some());
            if let Some(config) = config {
                assert_resolution(
                    &args,
                    &Entry {
                        config,
                        span: setting.span.clone(),
                    },
                    setting.config,
                );
                actual.push(&Entry {
                    config,
                    span: setting.span,
                });
            }
        }
        actual.index = scan.index();
        assert!(scan.next().is_none());
        check_scan(codes, actual)
    }

    fn grammar_word(codes: &[u8], index: usize) -> Option<Global> {
        Some(match codes.get(index)? {
            0 | 1 | 3 | 7 | 9 => Global::Subcommand,
            10 => Global::Separator,
            2 => Global::ValueOption(codes.get(index + 1).map(|code| ConfigValue {
                kind: ConfigKind::Inline,
                next: true,
                start: 0,
                end: argument_length(*code),
            })),
            4 => Global::Option(Some(inline_setting())),
            5 => Global::Option(Some(environment_setting(5))),
            6 => Global::ValueOption(
                codes
                    .get(index + 1)
                    .filter(|code| matches!(code, 3 | 7))
                    .map(|code| environment_setting(*code)),
            ),
            8 => Global::ValueOption(None),
            _ => Global::Option(None),
        })
    }

    fn assert_resolution(args: &[&str], entry: &Entry, actual: Config<'_>) {
        let base = args.get(entry.span.start + usize::from(entry.config.next));
        assert!(base.is_some());
        let base = base.copied().unwrap_or_default();
        assert!(entry.config.start <= entry.config.end && entry.config.end <= base.len());
        assert!(matches!(
            (actual, entry.config.kind),
            (Config::Inline(_), ConfigKind::Inline)
                | (Config::Environment(_), ConfigKind::Environment)
        ));
        let (Config::Inline(value) | Config::Environment(value)) = actual;
        assert_eq!(value.len(), entry.config.end - entry.config.start);
        assert!(core::ptr::eq(
            value.as_ptr(),
            base.as_ptr().wrapping_add(entry.config.start)
        ));
    }

    pub fn spec_word(code: u8, next: Option<u8>) -> Global {
        assert!(usize::from(code) < ARGUMENTS.len());
        assert!(next.is_none_or(|code| usize::from(code) < ARGUMENTS.len()));
        let argument = ARGUMENTS
            .get(usize::from(code))
            .copied()
            .unwrap_or_default();
        let next_word = next.and_then(|code| ARGUMENTS.get(usize::from(code)).copied());
        let actual = global_word(argument, next_word);
        let codes = [code, next.unwrap_or_default()];
        let length = if next.is_some() { 2 } else { 1 };
        let expected = grammar_word(codes.get(..length).unwrap_or_default(), 0);
        assert!(expected.is_some());
        let Some(expected) = expected else {
            return actual;
        };
        assert_eq!(actual, expected);
        actual
    }

    pub fn spec_scan_words(codes: &[u8]) -> ScanResult {
        let mut cursor = Cursor {
            index: 0,
            done: false,
        };
        let mut actual = ScanResult::empty();
        for _ in 0..codes.len() {
            let Some(word) = grammar_word(codes, cursor.index) else {
                break;
            };
            if let Some(setting) = cursor.advance(word) {
                actual.push(&setting);
            }
            if cursor.done {
                break;
            }
        }
        actual.index = cursor.index;
        check_scan(codes, actual)
    }

    fn check_scan(codes: &[u8], actual: ScanResult) -> ScanResult {
        assert!(codes.len() <= 4);
        let mut index = 0;
        let mut expected = ScanResult::empty();
        while let Some(code) = codes.get(index) {
            let start = index;
            match code {
                0 | 1 | 3 | 7 | 9 => break,
                10 => {
                    index += 1;
                    break;
                }
                2 => {
                    index += 1;
                    if let Some(code) = codes.get(index) {
                        expected.push(&Entry {
                            config: ConfigValue {
                                kind: ConfigKind::Inline,
                                next: true,
                                start: 0,
                                end: argument_length(*code),
                            },
                            span: start..index + 1,
                        });
                    }
                }
                4 => expected.push(&Entry {
                    config: inline_setting(),
                    span: start..index + 1,
                }),
                5 => expected.push(&Entry {
                    config: environment_setting(5),
                    span: start..index + 1,
                }),
                6 => {
                    index += 1;
                    if matches!(codes.get(index), Some(3 | 7)) {
                        expected.push(&Entry {
                            config: environment_setting(
                                codes.get(index).copied().unwrap_or_default(),
                            ),
                            span: start..index + 1,
                        });
                    }
                }
                8 => index += 1,
                _ => {}
            }
            index += 1;
        }
        assert_eq!(actual.index, index);
        assert_eq!(actual.length, expected.length);
        for index in 0..actual.length {
            let setting = actual.settings.get(index).and_then(Option::as_ref);
            let expected = expected.settings.get(index).and_then(Option::as_ref);
            assert!(setting.is_some() && expected.is_some());
            let (Some(setting), Some(expected)) = (setting, expected) else {
                continue;
            };
            assert_eq!(setting.span.start, expected.span.start);
            assert_eq!(setting.span.end, expected.span.end);
            assert_eq!(setting.config, expected.config);
        }
        actual
    }

    #[cfg(kani)]
    #[kani::proof]
    #[kani::unwind(5)]
    fn every_bounded_global_argument_scan_keeps_exact_override_spans() {
        let codes = kani::any::<[u8; 4]>().map(|code| code % 14);
        let length = usize::from(kani::any::<u8>() % 5);
        let scan = spec_scan_words(&codes[..length]);
        kani::cover!(scan.index < length && scan.length != 0);
        kani::cover!(scan.index > length);
    }

    #[cfg(kani)]
    #[kani::proof]
    #[kani::unwind(35)]
    fn every_global_word_has_its_exact_grammar_facts() {
        bounded_word(0);
    }

    #[cfg(kani)]
    fn bounded_word(code: u8) {
        let next = kani::any::<bool>().then(|| kani::any::<u8>() % 14);
        let _word = spec_word(code, next);
        kani::cover!(next.is_none());
        kani::cover!(next.is_some());
    }

    #[cfg(kani)]
    #[kani::proof]
    #[kani::unwind(35)]
    fn the_global_scan_adapter_keeps_empty_and_single_word_inputs() {
        bounded_adapter(0);
    }

    #[cfg(kani)]
    fn bounded_adapter(code: u8) {
        let codes = [code];
        let empty = spec_scan(&[]);
        let present = spec_scan(&codes);
        kani::cover!(empty.index == 0 && empty.length == 0);
        kani::cover!(present.index <= codes.len() + 1);
    }

    #[cfg(kani)]
    #[kani::proof]
    #[kani::unwind(9)]
    fn the_status_word_has_its_exact_grammar_facts() {
        bounded_word(1);
    }

    #[cfg(kani)]
    #[kani::proof]
    #[kani::unwind(9)]
    fn the_scan_adapter_keeps_the_status_word() {
        bounded_adapter(1);
    }

    #[cfg(kani)]
    #[kani::proof]
    #[kani::unwind(9)]
    fn the_separate_inline_config_word_has_its_exact_grammar_facts() {
        bounded_word(2);
    }

    #[cfg(kani)]
    #[kani::proof]
    #[kani::unwind(9)]
    fn the_scan_adapter_keeps_the_separate_inline_config_word() {
        bounded_adapter(2);
    }

    #[cfg(kani)]
    #[kani::proof]
    #[kani::unwind(9)]
    fn the_value_text_word_has_its_exact_grammar_facts() {
        bounded_word(3);
    }

    #[cfg(kani)]
    #[kani::proof]
    #[kani::unwind(9)]
    fn the_scan_adapter_keeps_the_value_text_word() {
        bounded_adapter(3);
    }

    #[cfg(kani)]
    #[kani::proof]
    #[kani::unwind(9)]
    fn the_attached_inline_config_word_has_its_exact_grammar_facts() {
        bounded_word(4);
    }

    #[cfg(kani)]
    #[kani::proof]
    #[kani::unwind(9)]
    fn the_scan_adapter_keeps_the_attached_inline_config_word() {
        bounded_adapter(4);
    }

    #[cfg(kani)]
    #[kani::proof]
    #[kani::unwind(21)]
    fn the_attached_environment_config_word_has_its_exact_grammar_facts() {
        bounded_word(5);
    }

    #[cfg(kani)]
    #[kani::proof]
    #[kani::unwind(21)]
    fn the_scan_adapter_keeps_the_attached_environment_config_word() {
        bounded_adapter(5);
    }

    #[cfg(kani)]
    #[kani::proof]
    #[kani::unwind(35)]
    fn the_separate_environment_config_word_has_its_exact_grammar_facts() {
        bounded_word(6);
    }

    #[cfg(kani)]
    #[kani::proof]
    #[kani::unwind(35)]
    fn the_scan_adapter_keeps_the_separate_environment_config_word() {
        bounded_adapter(6);
    }

    #[cfg(kani)]
    #[kani::proof]
    #[kani::unwind(9)]
    fn the_environment_value_text_word_has_its_exact_grammar_facts() {
        bounded_word(7);
    }

    #[cfg(kani)]
    #[kani::proof]
    #[kani::unwind(9)]
    fn the_scan_adapter_keeps_the_environment_value_text_word() {
        bounded_adapter(7);
    }

    #[cfg(kani)]
    #[kani::proof]
    #[kani::unwind(9)]
    fn the_directory_option_word_has_its_exact_grammar_facts() {
        bounded_word(8);
    }

    #[cfg(kani)]
    #[kani::proof]
    #[kani::unwind(9)]
    fn the_scan_adapter_keeps_the_directory_option_word() {
        bounded_adapter(8);
    }

    #[cfg(kani)]
    #[kani::proof]
    #[kani::unwind(9)]
    fn the_directory_value_word_has_its_exact_grammar_facts() {
        bounded_word(9);
    }

    #[cfg(kani)]
    #[kani::proof]
    #[kani::unwind(9)]
    fn the_scan_adapter_keeps_the_directory_value_word() {
        bounded_adapter(9);
    }

    #[cfg(kani)]
    #[kani::proof]
    #[kani::unwind(9)]
    fn the_terminator_word_has_its_exact_grammar_facts() {
        bounded_word(10);
    }

    #[cfg(kani)]
    #[kani::proof]
    #[kani::unwind(9)]
    fn the_scan_adapter_keeps_the_terminator_word() {
        bounded_adapter(10);
    }

    #[cfg(kani)]
    #[kani::proof]
    #[kani::unwind(9)]
    fn the_force_option_word_has_its_exact_grammar_facts() {
        bounded_word(11);
    }

    #[cfg(kani)]
    #[kani::proof]
    #[kani::unwind(9)]
    fn the_scan_adapter_keeps_the_force_option_word() {
        bounded_adapter(11);
    }

    #[cfg(kani)]
    #[kani::proof]
    #[kani::unwind(9)]
    fn the_force_prefix_word_has_its_exact_grammar_facts() {
        bounded_word(12);
    }

    #[cfg(kani)]
    #[kani::proof]
    #[kani::unwind(9)]
    fn the_scan_adapter_keeps_the_force_prefix_word() {
        bounded_adapter(12);
    }

    #[cfg(kani)]
    #[kani::proof]
    #[kani::unwind(9)]
    fn the_no_verify_option_word_has_its_exact_grammar_facts() {
        bounded_word(13);
    }

    #[cfg(kani)]
    #[kani::proof]
    #[kani::unwind(9)]
    fn the_scan_adapter_keeps_the_no_verify_option_word() {
        bounded_adapter(13);
    }

    /// The longest line the bounded checks build.
    pub const LINE: usize = 128;

    /// `parts` written one after another, with its length.
    pub fn joined(parts: &[&[u8]]) -> ([u8; LINE], usize) {
        let mut line = [0; LINE];
        let mut length = 0;
        for part in parts {
            let count = part.len().min(LINE - length);
            if let (Some(destination), Some(source)) =
                (line.get_mut(length..length + count), part.get(..count))
            {
                destination.copy_from_slice(source);
                length += count;
            }
        }
        (line, length)
    }

    /// An agent's `Co-authored-by` trailer counts in any case of its domain, and a human's never counts.
    pub fn spec_trailer(head: &[u8], domain: &[u8], closed: bool) -> bool {
        let close: &[u8] = if closed { b">" } else { b"" };
        let (line, length) = joined(&[head, b" Agent <noreply", domain, close]);
        let text = line.get(..length).unwrap_or_default();
        let agent = domain.eq_ignore_ascii_case(AGENT_DOMAIN);
        let trailer = head.eq_ignore_ascii_case(b"co-authored-by:");
        assert_eq!(is_attribution(text), agent && trailer);
        is_attribution(text)
    }

    /// What Git answers about a remote commit.
    /// A push advances only a new ref or a descendant of a remote commit this clone holds, and refuses a deletion and an unknown remote commit.
    pub fn spec_push(local_zero: bool, remote_zero: bool, answers: Ancestry) -> Push {
        let name = |zeroed: bool| -> &[u8] { if zeroed { b"0000" } else { b"0a1b" } };
        let verdict = push(name(local_zero), name(remote_zero), answers);
        assert_eq!(verdict == Push::Delete, local_zero);
        assert_eq!(
            verdict == Push::Advance,
            !local_zero && (remote_zero || (answers.known && answers.descends))
        );
        assert_eq!(
            verdict == Push::Undecidable,
            !local_zero && !remote_zero && !answers.known
        );
        verdict
    }

    /// The digests the bounded checks pair with an image: a valid one, one too short, one too long, and one with a letter outside hexadecimal.
    pub const DIGESTS: [&[u8]; 4] = [
        b"778ff1b404d79ef7763f0c904ebb279a4138ffc1017181ad076b1648e37f6dbd",
        b"778ff1b404d79ef7763f0c904ebb279a4138ffc1017181ad076b1648e37f6db",
        b"778ff1b404d79ef7763f0c904ebb279a4138ffc1017181ad076b1648e37f6dbdd",
        b"778ff1b404d79ef7763f0c904ebb279a4138ffc1017181ad076b1648e37f6dbg",
    ];
    pub const PREFIXES: [&[u8]; 3] = [b"renovate/renovate:", b"renovate/renovatex:", b"alpine:"];
    pub const VERSIONS: [&[u8]; 6] = [
        b"44.52.0", b"44.52", b"44.x.0", b"latest", b"1.2.3.4", b"1..2",
    ];
    pub const SEPARATORS: [&[u8]; 3] = [b"@sha256:", b"@sha512:", b""];

    /// A pinned image names the Renovate repository, a numeric version with three components, and a hexadecimal SHA-256 digest of exactly 64 digits.
    pub fn spec_image(prefix: usize, version: usize, separator: usize, digest: usize) -> bool {
        let pick =
            |list: &[&'static [u8]], index: usize| list.get(index).copied().unwrap_or_default();
        let (line, length) = joined(&[
            pick(&PREFIXES, prefix),
            pick(&VERSIONS, version),
            pick(&SEPARATORS, separator),
            pick(&DIGESTS, digest),
        ]);
        let pinned = image_is_pinned(line.get(..length).unwrap_or_default());
        assert_eq!(
            pinned,
            prefix == 0 && version == 0 && separator == 0 && digest == 0
        );
        pinned
    }

    /// A setting overrides the policy exactly as the table of keys and subcommands says, whatever the case of its key.
    pub fn spec_override(key: &[u8], value: &[u8], sub: &[u8]) -> bool {
        let offends = overrides_policy(key, value, sub);
        let mut lower = [0; LINE];
        let length = key.len().min(LINE);
        for (slot, byte) in lower.iter_mut().zip(key) {
            *slot = byte.to_ascii_lowercase();
        }
        assert_eq!(
            overrides_policy(lower.get(..length).unwrap_or_default(), value, sub),
            offends
        );
        if offends {
            assert!(policy_key(key));
        }
        let off = [b"false".as_slice(), b"0", b"no", b"off", b""]
            .iter()
            .any(|word| word.eq_ignore_ascii_case(value));
        let expected = if key.eq_ignore_ascii_case(b"core.hooksPath") {
            RUNS_HOOKS.contains(&sub)
        } else if key.eq_ignore_ascii_case(b"commit.gpgsign")
            || key.eq_ignore_ascii_case(b"tag.gpgsign")
        {
            CREATES_SIGNED_OBJECTS.contains(&sub) && off
        } else if key.eq_ignore_ascii_case(b"gpg.format") {
            CREATES_SIGNED_OBJECTS.contains(&sub) && !value.eq_ignore_ascii_case(b"ssh")
        } else if key.eq_ignore_ascii_case(b"gpg.ssh.program") {
            CREATES_SIGNED_OBJECTS.contains(&sub) && value.is_empty()
        } else {
            false
        };
        assert_eq!(offends, expected);
        offends
    }

    pub const KEYS: [&[u8]; 8] = [
        b"core.hooksPath",
        b"CORE.HOOKSPATH",
        b"core.hookspath",
        b"commit.gpgsign",
        b"Tag.GpgSign",
        b"gpg.format",
        b"gpg.ssh.program",
        b"user.name",
    ];
    pub const VALUES: [&[u8]; 6] = [b"false", b"OFF", b"", b"ssh", b"x509", b"true"];
    pub const SUBCOMMANDS: [&[u8]; 5] = [b"commit", b"rev-parse", b"push", b"tag", b"status"];

    /// Every day maps to a valid date that maps back to it, and the next day maps to a later date.
    pub fn spec_date(days: i64) -> (i64, i64, i64) {
        let date = civil_from_days(days);
        let (year, month, day) = date;
        assert!((1..=MONTHS_PER_YEAR).contains(&month));
        assert!((1..=LAST_DAY_OF_MONTH).contains(&day));
        assert_eq!(days_from_civil(year, month, day), days);
        assert!(civil_from_days(days + 1) > date);
        date
    }

    /// The lines a bounded commit message holds: a subject, blanks, a body, human trailers, and agent attribution.
    pub const MESSAGE_LINES: [&str; 6] = [
        "feat: x",
        "",
        "Body.",
        "Co-authored-by: A <a@example.com>",
        "Co-Authored-By: C <n@anthropic.com>",
        "https://claude.ai/code/session_1",
    ];

    /// Stripping removes exactly the attribution lines and the blank lines they leave at the end, keeps every other line in order, and changes nothing a second time.
    pub fn spec_strip(text: &str) -> usize {
        let (stripped, removed) = strip_attribution(text);
        assert_eq!(
            removed,
            text.lines()
                .filter(|line| is_attribution(line.as_bytes()))
                .count()
        );
        if removed == 0 {
            assert_eq!(stripped, text);
        } else {
            let kept: Vec<&str> = text
                .lines()
                .filter(|line| !is_attribution(line.as_bytes()))
                .collect();
            let length = kept
                .iter()
                .rposition(|line| !line.trim().is_empty())
                .map_or(0, |last| last + 1);
            assert_eq!(
                stripped.lines().collect::<Vec<_>>(),
                kept.get(..length).unwrap_or_default()
            );
        }
        assert_eq!(strip_attribution(&stripped), (stripped.clone(), 0));
        removed
    }

    #[cfg(kani)]
    fn any_case(byte: u8) -> u8 {
        if kani::any() {
            byte.to_ascii_uppercase()
        } else {
            byte
        }
    }

    #[cfg(kani)]
    #[kani::proof]
    #[kani::unwind(36)]
    fn stripping_is_idempotent_and_preserves_every_human_line() {
        let terminated = single_line(
            MESSAGE_LINES[3],
            "Co-authored-by: A <a@example.com>\n",
            false,
        );
        kani::cover!(terminated);
        kani::cover!(!terminated);
    }

    #[cfg(kani)]
    fn single_line(line: &str, with_newline: &str, attribution: bool) -> bool {
        let terminated: bool = kani::any();
        if terminated {
            fixed_line(with_newline, attribution);
        } else {
            fixed_line(line, attribution);
        }
        terminated
    }

    #[cfg(kani)]
    fn fixed_line(input: &str, attribution: bool) {
        let (stripped, removed) = strip_attribution(input);
        assert_eq!(removed, usize::from(attribution));
        let expected = if attribution { "" } else { input };
        assert_eq!(stripped, expected);
        // The byte equality supplies the next call's input without another symbolic allocation.
        assert_eq!(strip_attribution(expected), (stripped, 0));
    }

    #[cfg(kani)]
    #[kani::proof]
    #[kani::unwind(36)]
    fn stripping_preserves_the_subject() {
        let terminated = single_line(MESSAGE_LINES[0], "feat: x\n", false);
        kani::cover!(terminated);
        kani::cover!(!terminated);
    }

    #[cfg(kani)]
    #[kani::proof]
    #[kani::unwind(36)]
    fn stripping_preserves_a_blank_line() {
        let terminated = single_line(MESSAGE_LINES[1], "\n", false);
        kani::cover!(terminated);
        kani::cover!(!terminated);
    }

    #[cfg(kani)]
    #[kani::proof]
    #[kani::unwind(36)]
    fn stripping_preserves_the_body() {
        let terminated = single_line(MESSAGE_LINES[2], "Body.\n", false);
        kani::cover!(terminated);
        kani::cover!(!terminated);
    }

    #[cfg(kani)]
    #[kani::proof]
    #[kani::unwind(36)]
    fn stripping_removes_an_agent_trailer() {
        let terminated = single_line(
            MESSAGE_LINES[4],
            "Co-Authored-By: C <n@anthropic.com>\n",
            true,
        );
        kani::cover!(terminated);
        kani::cover!(!terminated);
    }

    #[cfg(kani)]
    #[kani::proof]
    #[kani::unwind(36)]
    fn stripping_removes_a_session_link() {
        let terminated = single_line(MESSAGE_LINES[5], "https://claude.ai/code/session_1\n", true);
        kani::cover!(terminated);
        kani::cover!(!terminated);
    }

    #[cfg(kani)]
    #[kani::proof]
    #[kani::unwind(16)]
    fn an_agent_trailer_counts_in_any_case_and_a_human_one_never() {
        let (counted, closed) = bounded_trailer(b"Co-Authored-By:", false);
        kani::cover!(counted & closed);
        kani::cover!(counted & !closed);
    }

    #[cfg(kani)]
    #[kani::proof]
    #[kani::unwind(16)]
    fn a_human_trailer_never_counts_with_either_closing_form() {
        let (counted, closed) = bounded_trailer(b"Co-Authored-By:", true);
        kani::cover!(!counted & closed);
        kani::cover!(!counted & !closed);
    }

    #[cfg(kani)]
    #[kani::proof]
    #[kani::unwind(16)]
    fn a_signed_off_agent_never_counts_with_either_closing_form() {
        let (counted, closed) = bounded_trailer(b"Signed-off-by:", false);
        kani::cover!(!counted & closed);
        kani::cover!(!counted & !closed);
    }

    #[cfg(kani)]
    #[kani::proof]
    #[kani::unwind(16)]
    fn a_signed_off_human_never_counts_with_either_closing_form() {
        let (counted, closed) = bounded_trailer(b"Signed-off-by:", true);
        kani::cover!(!counted & closed);
        kani::cover!(!counted & !closed);
    }

    #[cfg(kani)]
    fn bounded_trailer(head: &[u8], human: bool) -> (bool, bool) {
        let coauthor = head == b"Co-Authored-By:";
        let mut line = *b"Co-Authored-By: Agent <noreply@anthropic.com>";
        let mut length = line.len();
        if !coauthor {
            line.copy_within(1.., 0);
            length -= 1;
            line[..head.len()].copy_from_slice(head);
        }
        let domain_start = length - AGENT_DOMAIN.len() - 1;
        let domain_end = length - 1;
        if human {
            line[domain_start..domain_end].copy_from_slice(b"@example.co.uk");
        } else {
            for byte in &mut line[domain_start..domain_end] {
                *byte = any_case(*byte);
            }
        }
        let closed: bool = kani::any();
        let end = length - usize::from(!closed);
        let counted = is_attribution(&line[..end]);
        assert_eq!(counted, coauthor && !human);
        (counted, closed)
    }

    #[cfg(kani)]
    #[kani::proof]
    #[kani::unwind(6)]
    fn a_push_refuses_a_deletion_and_an_unknown_remote_commit() {
        let answers = Ancestry {
            known: kani::any(),
            descends: kani::any(),
        };
        let verdict = spec_push(kani::any(), kani::any(), answers);
        kani::cover!(verdict == Push::Undecidable);
        kani::cover!(verdict == Push::Advance);
    }

    pub const IMAGES_0: [&[u8]; 18] = [
        b"renovate/renovate:44.52.0@sha256:778ff1b404d79ef7763f0c904ebb279a4138ffc1017181ad076b1648e37f6dbd",
        b"renovate/renovate:44.52.0@sha512:778ff1b404d79ef7763f0c904ebb279a4138ffc1017181ad076b1648e37f6dbd",
        b"renovate/renovate:44.52.0778ff1b404d79ef7763f0c904ebb279a4138ffc1017181ad076b1648e37f6dbd",
        b"renovate/renovate:44.52@sha256:778ff1b404d79ef7763f0c904ebb279a4138ffc1017181ad076b1648e37f6dbd",
        b"renovate/renovate:44.52@sha512:778ff1b404d79ef7763f0c904ebb279a4138ffc1017181ad076b1648e37f6dbd",
        b"renovate/renovate:44.52778ff1b404d79ef7763f0c904ebb279a4138ffc1017181ad076b1648e37f6dbd",
        b"renovate/renovate:44.x.0@sha256:778ff1b404d79ef7763f0c904ebb279a4138ffc1017181ad076b1648e37f6dbd",
        b"renovate/renovate:44.x.0@sha512:778ff1b404d79ef7763f0c904ebb279a4138ffc1017181ad076b1648e37f6dbd",
        b"renovate/renovate:44.x.0778ff1b404d79ef7763f0c904ebb279a4138ffc1017181ad076b1648e37f6dbd",
        b"renovate/renovate:latest@sha256:778ff1b404d79ef7763f0c904ebb279a4138ffc1017181ad076b1648e37f6dbd",
        b"renovate/renovate:latest@sha512:778ff1b404d79ef7763f0c904ebb279a4138ffc1017181ad076b1648e37f6dbd",
        b"renovate/renovate:latest778ff1b404d79ef7763f0c904ebb279a4138ffc1017181ad076b1648e37f6dbd",
        b"renovate/renovate:1.2.3.4@sha256:778ff1b404d79ef7763f0c904ebb279a4138ffc1017181ad076b1648e37f6dbd",
        b"renovate/renovate:1.2.3.4@sha512:778ff1b404d79ef7763f0c904ebb279a4138ffc1017181ad076b1648e37f6dbd",
        b"renovate/renovate:1.2.3.4778ff1b404d79ef7763f0c904ebb279a4138ffc1017181ad076b1648e37f6dbd",
        b"renovate/renovate:1..2@sha256:778ff1b404d79ef7763f0c904ebb279a4138ffc1017181ad076b1648e37f6dbd",
        b"renovate/renovate:1..2@sha512:778ff1b404d79ef7763f0c904ebb279a4138ffc1017181ad076b1648e37f6dbd",
        b"renovate/renovate:1..2778ff1b404d79ef7763f0c904ebb279a4138ffc1017181ad076b1648e37f6dbd",
    ];

    pub const IMAGES_1: [&[u8]; 18] = [
        b"renovate/renovatex:44.52.0@sha256:778ff1b404d79ef7763f0c904ebb279a4138ffc1017181ad076b1648e37f6dbd",
        b"renovate/renovatex:44.52.0@sha512:778ff1b404d79ef7763f0c904ebb279a4138ffc1017181ad076b1648e37f6dbd",
        b"renovate/renovatex:44.52.0778ff1b404d79ef7763f0c904ebb279a4138ffc1017181ad076b1648e37f6dbd",
        b"renovate/renovatex:44.52@sha256:778ff1b404d79ef7763f0c904ebb279a4138ffc1017181ad076b1648e37f6dbd",
        b"renovate/renovatex:44.52@sha512:778ff1b404d79ef7763f0c904ebb279a4138ffc1017181ad076b1648e37f6dbd",
        b"renovate/renovatex:44.52778ff1b404d79ef7763f0c904ebb279a4138ffc1017181ad076b1648e37f6dbd",
        b"renovate/renovatex:44.x.0@sha256:778ff1b404d79ef7763f0c904ebb279a4138ffc1017181ad076b1648e37f6dbd",
        b"renovate/renovatex:44.x.0@sha512:778ff1b404d79ef7763f0c904ebb279a4138ffc1017181ad076b1648e37f6dbd",
        b"renovate/renovatex:44.x.0778ff1b404d79ef7763f0c904ebb279a4138ffc1017181ad076b1648e37f6dbd",
        b"renovate/renovatex:latest@sha256:778ff1b404d79ef7763f0c904ebb279a4138ffc1017181ad076b1648e37f6dbd",
        b"renovate/renovatex:latest@sha512:778ff1b404d79ef7763f0c904ebb279a4138ffc1017181ad076b1648e37f6dbd",
        b"renovate/renovatex:latest778ff1b404d79ef7763f0c904ebb279a4138ffc1017181ad076b1648e37f6dbd",
        b"renovate/renovatex:1.2.3.4@sha256:778ff1b404d79ef7763f0c904ebb279a4138ffc1017181ad076b1648e37f6dbd",
        b"renovate/renovatex:1.2.3.4@sha512:778ff1b404d79ef7763f0c904ebb279a4138ffc1017181ad076b1648e37f6dbd",
        b"renovate/renovatex:1.2.3.4778ff1b404d79ef7763f0c904ebb279a4138ffc1017181ad076b1648e37f6dbd",
        b"renovate/renovatex:1..2@sha256:778ff1b404d79ef7763f0c904ebb279a4138ffc1017181ad076b1648e37f6dbd",
        b"renovate/renovatex:1..2@sha512:778ff1b404d79ef7763f0c904ebb279a4138ffc1017181ad076b1648e37f6dbd",
        b"renovate/renovatex:1..2778ff1b404d79ef7763f0c904ebb279a4138ffc1017181ad076b1648e37f6dbd",
    ];

    pub const IMAGES_2: [&[u8]; 18] = [
        b"alpine:44.52.0@sha256:778ff1b404d79ef7763f0c904ebb279a4138ffc1017181ad076b1648e37f6dbd",
        b"alpine:44.52.0@sha512:778ff1b404d79ef7763f0c904ebb279a4138ffc1017181ad076b1648e37f6dbd",
        b"alpine:44.52.0778ff1b404d79ef7763f0c904ebb279a4138ffc1017181ad076b1648e37f6dbd",
        b"alpine:44.52@sha256:778ff1b404d79ef7763f0c904ebb279a4138ffc1017181ad076b1648e37f6dbd",
        b"alpine:44.52@sha512:778ff1b404d79ef7763f0c904ebb279a4138ffc1017181ad076b1648e37f6dbd",
        b"alpine:44.52778ff1b404d79ef7763f0c904ebb279a4138ffc1017181ad076b1648e37f6dbd",
        b"alpine:44.x.0@sha256:778ff1b404d79ef7763f0c904ebb279a4138ffc1017181ad076b1648e37f6dbd",
        b"alpine:44.x.0@sha512:778ff1b404d79ef7763f0c904ebb279a4138ffc1017181ad076b1648e37f6dbd",
        b"alpine:44.x.0778ff1b404d79ef7763f0c904ebb279a4138ffc1017181ad076b1648e37f6dbd",
        b"alpine:latest@sha256:778ff1b404d79ef7763f0c904ebb279a4138ffc1017181ad076b1648e37f6dbd",
        b"alpine:latest@sha512:778ff1b404d79ef7763f0c904ebb279a4138ffc1017181ad076b1648e37f6dbd",
        b"alpine:latest778ff1b404d79ef7763f0c904ebb279a4138ffc1017181ad076b1648e37f6dbd",
        b"alpine:1.2.3.4@sha256:778ff1b404d79ef7763f0c904ebb279a4138ffc1017181ad076b1648e37f6dbd",
        b"alpine:1.2.3.4@sha512:778ff1b404d79ef7763f0c904ebb279a4138ffc1017181ad076b1648e37f6dbd",
        b"alpine:1.2.3.4778ff1b404d79ef7763f0c904ebb279a4138ffc1017181ad076b1648e37f6dbd",
        b"alpine:1..2@sha256:778ff1b404d79ef7763f0c904ebb279a4138ffc1017181ad076b1648e37f6dbd",
        b"alpine:1..2@sha512:778ff1b404d79ef7763f0c904ebb279a4138ffc1017181ad076b1648e37f6dbd",
        b"alpine:1..2778ff1b404d79ef7763f0c904ebb279a4138ffc1017181ad076b1648e37f6dbd",
    ];

    #[cfg(kani)]
    fn image_0(case: usize) -> bool {
        match case {
            0 => image_is_pinned(IMAGES_0[0]),
            1 => image_is_pinned(IMAGES_0[1]),
            2 => image_is_pinned(IMAGES_0[2]),
            3 => image_is_pinned(IMAGES_0[3]),
            4 => image_is_pinned(IMAGES_0[4]),
            5 => image_is_pinned(IMAGES_0[5]),
            6 => image_is_pinned(IMAGES_0[6]),
            7 => image_is_pinned(IMAGES_0[7]),
            8 => image_is_pinned(IMAGES_0[8]),
            9 => image_is_pinned(IMAGES_0[9]),
            10 => image_is_pinned(IMAGES_0[10]),
            11 => image_is_pinned(IMAGES_0[11]),
            12 => image_is_pinned(IMAGES_0[12]),
            13 => image_is_pinned(IMAGES_0[13]),
            14 => image_is_pinned(IMAGES_0[14]),
            15 => image_is_pinned(IMAGES_0[15]),
            16 => image_is_pinned(IMAGES_0[16]),
            _ => image_is_pinned(IMAGES_0[17]),
        }
    }

    #[cfg(kani)]
    fn image_1(case: usize) -> bool {
        match case {
            0 => image_is_pinned(IMAGES_1[0]),
            1 => image_is_pinned(IMAGES_1[1]),
            2 => image_is_pinned(IMAGES_1[2]),
            3 => image_is_pinned(IMAGES_1[3]),
            4 => image_is_pinned(IMAGES_1[4]),
            5 => image_is_pinned(IMAGES_1[5]),
            6 => image_is_pinned(IMAGES_1[6]),
            7 => image_is_pinned(IMAGES_1[7]),
            8 => image_is_pinned(IMAGES_1[8]),
            9 => image_is_pinned(IMAGES_1[9]),
            10 => image_is_pinned(IMAGES_1[10]),
            11 => image_is_pinned(IMAGES_1[11]),
            12 => image_is_pinned(IMAGES_1[12]),
            13 => image_is_pinned(IMAGES_1[13]),
            14 => image_is_pinned(IMAGES_1[14]),
            15 => image_is_pinned(IMAGES_1[15]),
            16 => image_is_pinned(IMAGES_1[16]),
            _ => image_is_pinned(IMAGES_1[17]),
        }
    }

    #[cfg(kani)]
    fn image_2(case: usize) -> bool {
        match case {
            0 => image_is_pinned(IMAGES_2[0]),
            1 => image_is_pinned(IMAGES_2[1]),
            2 => image_is_pinned(IMAGES_2[2]),
            3 => image_is_pinned(IMAGES_2[3]),
            4 => image_is_pinned(IMAGES_2[4]),
            5 => image_is_pinned(IMAGES_2[5]),
            6 => image_is_pinned(IMAGES_2[6]),
            7 => image_is_pinned(IMAGES_2[7]),
            8 => image_is_pinned(IMAGES_2[8]),
            9 => image_is_pinned(IMAGES_2[9]),
            10 => image_is_pinned(IMAGES_2[10]),
            11 => image_is_pinned(IMAGES_2[11]),
            12 => image_is_pinned(IMAGES_2[12]),
            13 => image_is_pinned(IMAGES_2[13]),
            14 => image_is_pinned(IMAGES_2[14]),
            15 => image_is_pinned(IMAGES_2[15]),
            16 => image_is_pinned(IMAGES_2[16]),
            _ => image_is_pinned(IMAGES_2[17]),
        }
    }

    #[cfg(kani)]
    #[kani::proof]
    #[kani::unwind(66)]
    fn only_a_versioned_digest_pins_the_renovate_image() {
        let case = usize::from(kani::any::<u8>() % 18);
        let pinned = image_0(case);
        assert_eq!(pinned, case == 0);
        kani::cover!(pinned);
        kani::cover!(!pinned);
    }

    #[cfg(kani)]
    #[kani::proof]
    #[kani::unwind(66)]
    fn a_similar_repository_never_pins_a_renovate_image() {
        let case = usize::from(kani::any::<u8>() % 18);
        assert!(!image_1(case));
        kani::cover!(case == 0);
        kani::cover!(case == 17);
    }

    #[cfg(kani)]
    #[kani::proof]
    #[kani::unwind(66)]
    fn another_repository_never_pins_a_renovate_image() {
        let case = usize::from(kani::any::<u8>() % 18);
        assert!(!image_2(case));
        kani::cover!(case == 0);
        kani::cover!(case == 17);
    }

    pub fn spec_override_case<const VALUE: usize>(
        key: &[u8],
        lower: &[u8],
        kind: u8,
        sub: u8,
    ) -> bool {
        let actual_value = VALUES.get(VALUE).copied().unwrap_or_default();
        let actual_sub = SUBCOMMANDS
            .get(usize::from(sub))
            .copied()
            .unwrap_or_default();
        let offends = overrides_policy(key, actual_value, actual_sub);
        assert_eq!(overrides_policy(lower, actual_value, actual_sub), offends);
        if offends {
            assert!(policy_key(key));
        }
        let signs = matches!(sub, 0 | 3);
        let expected = match kind {
            0 => matches!(sub, 0 | 2),
            1 => signs && VALUE <= 2,
            2 => signs && VALUE != 3,
            _ => signs && VALUE == 2,
        };
        assert_eq!(offends, expected);
        #[cfg(test)]
        assert_eq!(offends, spec_override(key, actual_value, actual_sub));
        offends
    }

    #[cfg(kani)]
    fn any_override(key: &[u8], lower: &[u8], kind: u8) -> bool {
        override_pair::<0, 1>(key, lower, kind)
    }

    #[cfg(kani)]
    fn override_pair<const FIRST: usize, const SECOND: usize>(
        key: &[u8],
        lower: &[u8],
        kind: u8,
    ) -> bool {
        let sub = kani::any::<u8>() % 5;
        let offends = spec_override_case::<0>(key, lower, kind, sub)
            | spec_override_case::<2>(key, lower, kind, sub)
            | spec_override_case::<FIRST>(key, lower, kind, sub)
            | spec_override_case::<SECOND>(key, lower, kind, sub);
        kani::cover!(offends);
        kani::cover!(!offends);
        offends
    }

    #[cfg(kani)]
    #[kani::proof]
    #[kani::unwind(20)]
    fn a_hooks_path_overrides_only_for_a_hook_subcommand() {
        any_override(b"core.hooksPath", b"core.hookspath", 0);
    }

    #[cfg(kani)]
    #[kani::proof]
    #[kani::unwind(20)]
    fn an_uppercase_hooks_path_overrides_only_for_a_hook_subcommand() {
        any_override(b"CORE.HOOKSPATH", b"core.hookspath", 0);
    }

    #[cfg(kani)]
    #[kani::proof]
    #[kani::unwind(20)]
    fn a_lowercase_hooks_path_overrides_only_for_a_hook_subcommand() {
        any_override(b"core.hookspath", b"core.hookspath", 0);
    }

    #[cfg(kani)]
    #[kani::proof]
    #[kani::unwind(20)]
    fn commit_signing_overrides_only_when_disabled_for_a_signing_subcommand() {
        any_override(b"commit.gpgsign", b"commit.gpgsign", 1);
    }

    #[cfg(kani)]
    #[kani::proof]
    #[kani::unwind(20)]
    fn tag_signing_overrides_only_when_disabled_for_a_signing_subcommand() {
        any_override(b"Tag.GpgSign", b"tag.gpgsign", 1);
    }

    #[cfg(kani)]
    #[kani::proof]
    #[kani::unwind(20)]
    fn a_format_overrides_only_when_non_ssh_for_a_signing_subcommand() {
        any_override(b"gpg.format", b"gpg.format", 2);
    }

    #[cfg(kani)]
    #[kani::proof]
    #[kani::unwind(20)]
    fn an_empty_signing_program_overrides_only_for_a_signing_subcommand() {
        any_override(b"gpg.ssh.program", b"gpg.ssh.program", 3);
    }

    #[cfg(kani)]
    #[kani::proof]
    #[kani::unwind(20)]
    fn an_unrelated_key_never_overrides() {
        let value = VALUES[usize::from(kani::any::<u8>() % 6)];
        let sub = SUBCOMMANDS[usize::from(kani::any::<u8>() % 5)];
        assert!(!spec_override(KEYS[7], value, sub));
        kani::cover!(sub == b"commit");
        kani::cover!(sub == b"rev-parse");
    }

    #[cfg(kani)]
    #[kani::proof]
    #[kani::unwind(20)]
    fn hooks_path_values_2_and_3_keep_the_exact_override_specification() {
        override_pair::<2, 3>(b"core.hooksPath", b"core.hookspath", 0);
    }

    #[cfg(kani)]
    #[kani::proof]
    #[kani::unwind(20)]
    fn hooks_path_values_4_and_5_keep_the_exact_override_specification() {
        override_pair::<4, 5>(b"core.hooksPath", b"core.hookspath", 0);
    }

    #[cfg(kani)]
    #[kani::proof]
    #[kani::unwind(20)]
    fn uppercase_hooks_path_values_2_and_3_keep_the_exact_override_specification() {
        override_pair::<2, 3>(b"CORE.HOOKSPATH", b"core.hookspath", 0);
    }

    #[cfg(kani)]
    #[kani::proof]
    #[kani::unwind(20)]
    fn uppercase_hooks_path_values_4_and_5_keep_the_exact_override_specification() {
        override_pair::<4, 5>(b"CORE.HOOKSPATH", b"core.hookspath", 0);
    }

    #[cfg(kani)]
    #[kani::proof]
    #[kani::unwind(20)]
    fn lowercase_hooks_path_values_2_and_3_keep_the_exact_override_specification() {
        override_pair::<2, 3>(b"core.hookspath", b"core.hookspath", 0);
    }

    #[cfg(kani)]
    #[kani::proof]
    #[kani::unwind(20)]
    fn lowercase_hooks_path_values_4_and_5_keep_the_exact_override_specification() {
        override_pair::<4, 5>(b"core.hookspath", b"core.hookspath", 0);
    }

    #[cfg(kani)]
    #[kani::proof]
    #[kani::unwind(20)]
    fn commit_signing_values_2_and_3_keep_the_exact_override_specification() {
        override_pair::<2, 3>(b"commit.gpgsign", b"commit.gpgsign", 1);
    }

    #[cfg(kani)]
    #[kani::proof]
    #[kani::unwind(20)]
    fn commit_signing_values_4_and_5_keep_the_exact_override_specification() {
        override_pair::<4, 5>(b"commit.gpgsign", b"commit.gpgsign", 1);
    }

    #[cfg(kani)]
    #[kani::proof]
    #[kani::unwind(20)]
    fn tag_signing_values_2_and_3_keep_the_exact_override_specification() {
        override_pair::<2, 3>(b"Tag.GpgSign", b"tag.gpgsign", 1);
    }

    #[cfg(kani)]
    #[kani::proof]
    #[kani::unwind(20)]
    fn tag_signing_values_4_and_5_keep_the_exact_override_specification() {
        override_pair::<4, 5>(b"Tag.GpgSign", b"tag.gpgsign", 1);
    }

    #[cfg(kani)]
    #[kani::proof]
    #[kani::unwind(20)]
    fn format_values_2_and_3_keep_the_exact_override_specification() {
        override_pair::<2, 3>(b"gpg.format", b"gpg.format", 2);
    }

    #[cfg(kani)]
    #[kani::proof]
    #[kani::unwind(20)]
    fn format_values_4_and_5_keep_the_exact_override_specification() {
        override_pair::<4, 5>(b"gpg.format", b"gpg.format", 2);
    }

    #[cfg(kani)]
    #[kani::proof]
    #[kani::unwind(20)]
    fn signing_program_values_2_and_3_keep_the_exact_override_specification() {
        override_pair::<2, 3>(b"gpg.ssh.program", b"gpg.ssh.program", 3);
    }

    #[cfg(kani)]
    #[kani::proof]
    #[kani::unwind(20)]
    fn signing_program_values_4_and_5_keep_the_exact_override_specification() {
        override_pair::<4, 5>(b"gpg.ssh.program", b"gpg.ssh.program", 3);
    }

    #[cfg(kani)]
    const DATE_PARTITION: i64 = DAYS_PER_CENTURY;

    #[cfg(kani)]
    #[kani::proof]
    fn every_day_of_an_era_round_trips_through_its_date() {
        let (year, month, _) = date_range(-DAYS_PER_ERA, -DAYS_PER_ERA + 1 * DATE_PARTITION);
        kani::cover!(month == 2 && year % 4 == 0);
        kani::cover!(year < 1970);
    }

    #[cfg(kani)]
    #[kani::proof]
    fn date_partition_1_round_trips_and_the_next_day_is_later() {
        let (year, month, _) = date_range(
            -DAYS_PER_ERA + 1 * DATE_PARTITION,
            -DAYS_PER_ERA + 2 * DATE_PARTITION,
        );
        kani::cover!(month == 2 && year % 4 == 0);
        kani::cover!(year < 1970);
    }

    #[cfg(kani)]
    #[kani::proof]
    fn date_partition_2_round_trips_and_the_next_day_is_later() {
        let (year, month, _) = date_range(
            -DAYS_PER_ERA + 2 * DATE_PARTITION,
            -DAYS_PER_ERA + 3 * DATE_PARTITION,
        );
        kani::cover!(month == 2 && year % 4 == 0);
        kani::cover!(year < 1970);
    }

    #[cfg(kani)]
    #[kani::proof]
    fn date_partition_3_round_trips_and_the_next_day_is_later() {
        let (year, month, _) = date_range(
            -DAYS_PER_ERA + 3 * DATE_PARTITION,
            -DAYS_PER_ERA + 4 * DATE_PARTITION,
        );
        kani::cover!(month == 2 && year % 4 == 0);
        kani::cover!(year < 1970);
    }

    #[cfg(kani)]
    #[kani::proof]
    fn date_partition_4_round_trips_and_the_next_day_is_later() {
        let (year, month, _) = date_range(
            -DAYS_PER_ERA + 4 * DATE_PARTITION,
            -DAYS_PER_ERA + 5 * DATE_PARTITION,
        );
        kani::cover!(month == 2 && year % 4 == 0);
        kani::cover!(year < 1970);
    }

    #[cfg(kani)]
    #[kani::proof]
    fn date_partition_5_round_trips_and_the_next_day_is_later() {
        let (year, month, _) = date_range(
            -DAYS_PER_ERA + 5 * DATE_PARTITION,
            -DAYS_PER_ERA + 6 * DATE_PARTITION,
        );
        kani::cover!(month == 2 && year % 4 == 0);
        kani::cover!(year >= 1970);
    }

    #[cfg(kani)]
    #[kani::proof]
    fn date_partition_6_round_trips_and_the_next_day_is_later() {
        let (year, month, _) = date_range(
            -DAYS_PER_ERA + 6 * DATE_PARTITION,
            -DAYS_PER_ERA + 7 * DATE_PARTITION,
        );
        kani::cover!(month == 2 && year % 4 == 0);
        kani::cover!(year >= 1970);
    }

    #[cfg(kani)]
    #[kani::proof]
    fn date_partition_7_round_trips_and_the_next_day_is_later() {
        let (year, month, _) = date_range(-DAYS_PER_ERA + 7 * DATE_PARTITION, DAYS_PER_ERA);
        kani::cover!(month == 2 && year % 4 == 0);
        kani::cover!(year >= 1970);
    }

    #[cfg(kani)]
    fn date_range(start: i64, end: i64) -> (i64, i64, i64) {
        let days: i64 = kani::any();
        kani::assume(start <= days && days < end);
        let compact = i32::try_from(days);
        assert!(compact.is_ok());
        if let Ok(compact) = compact {
            assert_eq!(i64::from(compact), days);
            spec_date(i64::from(compact))
        } else {
            (0, 0, 0)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[cfg_attr(
        miri,
        ignore = "Miri takes seconds over every four-word option sequence"
    )]
    fn every_four_word_option_sequence_matches_the_token_grammar() {
        for length in 0..=4u32 {
            for mut encoded in 0..proofs::ARGUMENTS.len().pow(length) {
                let mut codes = Vec::new();
                for _ in 0..length {
                    codes.push(u8::try_from(encoded % proofs::ARGUMENTS.len()).unwrap());
                    encoded /= proofs::ARGUMENTS.len();
                }
                let _scan = proofs::spec_scan(&codes);
                let _scan = proofs::spec_scan_words(&codes);
            }
        }
    }

    #[test]
    #[cfg_attr(
        miri,
        ignore = "Miri takes seconds over every global word and successor"
    )]
    fn every_global_word_matches_its_grammar_facts() {
        for code in 0..proofs::ARGUMENTS.len() {
            let code = u8::try_from(code).unwrap();
            assert_eq!(
                proofs::argument_length(code),
                proofs::ARGUMENTS[usize::from(code)].len()
            );
            let _word = proofs::spec_word(code, None);
            for next in 0..proofs::ARGUMENTS.len() {
                let _word = proofs::spec_word(code, Some(u8::try_from(next).unwrap()));
            }
        }
    }

    #[test]
    #[cfg_attr(miri, ignore = "Miri takes seconds over every override and subcommand")]
    fn the_override_case_partitions_match_the_byte_specification() {
        let keys = [
            (
                b"core.hooksPath".as_slice(),
                b"core.hookspath".as_slice(),
                0,
            ),
            (b"CORE.HOOKSPATH", b"core.hookspath", 0),
            (b"core.hookspath", b"core.hookspath", 0),
            (b"commit.gpgsign", b"commit.gpgsign", 1),
            (b"Tag.GpgSign", b"tag.gpgsign", 1),
            (b"gpg.format", b"gpg.format", 2),
            (b"gpg.ssh.program", b"gpg.ssh.program", 3),
        ];
        for (key, lower, kind) in keys {
            for sub in 0..5 {
                proofs::spec_override_case::<0>(key, lower, kind, sub);
                proofs::spec_override_case::<1>(key, lower, kind, sub);
                proofs::spec_override_case::<2>(key, lower, kind, sub);
                proofs::spec_override_case::<3>(key, lower, kind, sub);
                proofs::spec_override_case::<4>(key, lower, kind, sub);
                proofs::spec_override_case::<5>(key, lower, kind, sub);
            }
        }
    }

    #[test]
    fn the_literal_image_partitions_cover_the_same_token_grammar() {
        let matrices = [proofs::IMAGES_0, proofs::IMAGES_1, proofs::IMAGES_2];
        for (prefix, matrix) in matrices.iter().enumerate() {
            for (version, tag) in proofs::VERSIONS.iter().enumerate() {
                for (separator, marker) in proofs::SEPARATORS.iter().enumerate() {
                    let (bytes, length) = proofs::joined(&[
                        proofs::PREFIXES[prefix],
                        tag,
                        marker,
                        proofs::DIGESTS[0],
                    ]);
                    assert_eq!(
                        matrix[version * proofs::SEPARATORS.len() + separator],
                        &bytes[..length]
                    );
                }
            }
        }
    }

    #[test]
    #[cfg_attr(miri, ignore = "Miri takes seconds over the generated line endings")]
    fn scalar_lines_match_standard_line_endings() {
        const ALPHABET: [char; 3] = ['a', '\r', '\n'];
        assert_eq!(next_line(""), None);
        for length in 0..=6u32 {
            for mut code in 0..ALPHABET.len().pow(length) {
                let mut text = String::new();
                for _ in 0..length {
                    text.push(ALPHABET[code % ALPHABET.len()]);
                    code /= ALPHABET.len();
                }
                let mut actual = Vec::new();
                let mut rest = text.as_str();
                while !rest.is_empty() {
                    let (line, tail) = next_line(rest).unwrap();
                    assert!(tail.len() < rest.len());
                    actual.push(line);
                    rest = tail;
                }
                assert_eq!(actual, text.lines().collect::<Vec<_>>(), "{text:?}");
            }
        }
    }

    #[test]
    #[cfg_attr(miri, ignore = "Miri takes minutes over every day of an era")]
    fn every_bounded_input_meets_the_specifications() {
        const EXAMPLE_UNIX_SECONDS: i64 = 1_791_331_200;
        let mut session = String::from(core::str::from_utf8(SESSION_LINK).unwrap());
        assert!(!is_attribution(session.as_bytes()));
        session.push_str("has space");
        assert!(!is_attribution(session.as_bytes()));
        assert_eq!(
            days_from_civil(2026, 10, 7) * SECONDS_PER_DAY,
            EXAMPLE_UNIX_SECONDS
        );
        let count = proofs::MESSAGE_LINES.len();
        for length in 0..=4u32 {
            for mut number in 0..count.pow(length) {
                let mut text = String::new();
                for _ in 0..length {
                    text.push_str(
                        proofs::MESSAGE_LINES
                            .get(number % count)
                            .copied()
                            .unwrap_or_default(),
                    );
                    text.push('\n');
                    number /= count;
                }
                proofs::spec_strip(&text);
            }
        }
        for head in [
            b"Co-authored-by:".as_slice(),
            b"co-authored-by:",
            b"Signed-off-by:",
            b"x",
        ] {
            for domain in [
                AGENT_DOMAIN,
                b"@ANTHROPIC.COM",
                b"@Anthropic.Com",
                b"@example.com",
                b"anthropic.com",
            ] {
                for closed in [false, true] {
                    proofs::spec_trailer(head, domain, closed);
                }
            }
        }
        for [local, remote, known, descends] in crate::domain::flags() {
            proofs::spec_push(local, remote, Ancestry { known, descends });
        }
        for prefix in 0..proofs::PREFIXES.len() {
            for version in 0..proofs::VERSIONS.len() {
                for separator in 0..proofs::SEPARATORS.len() {
                    for digest in 0..proofs::DIGESTS.len() {
                        proofs::spec_image(prefix, version, separator, digest);
                    }
                }
            }
        }
        for key in proofs::KEYS {
            for value in proofs::VALUES {
                for sub in proofs::SUBCOMMANDS {
                    proofs::spec_override(key, value, sub);
                }
            }
        }
        for days in -DAYS_PER_ERA..DAYS_PER_ERA {
            proofs::spec_date(days);
            proofs::spec_date(days - EPOCH_SHIFT);
        }
    }

    #[test]
    fn the_examples_hold() {
        for line in [
            "Claude-Session: https://claude.ai/code/session_01Qesu",
            "  Claude-Session:   https://claude.ai/code/session_01Qesu",
            "Co-Authored-By: Claude Opus 5 (1M context) <noreply@anthropic.com>",
            "co-authored-by: Claude <noreply@ANTHROPIC.com>",
            "\u{1f916} Generated with [Claude Code](https://claude.com/claude-code)",
            "https://claude.ai/code/session_01Qesu",
        ] {
            assert!(is_attribution(line.as_bytes()), "{line}");
        }
        for line in [
            "Co-authored-by: Example User <example@example.com>",
            "Document the CLAUDE.md contract for new contributors",
            "The Claude-Session: prefix is what this hook looks for.",
            "See https://claude.ai/code/session_01Qesu for the transcript.",
            "Claude-Session: none",
        ] {
            assert!(!is_attribution(line.as_bytes()), "{line}");
        }
        assert_eq!(
            strip_attribution("feat: x\n\nBody.\n\nCo-Authored-By: C <n@anthropic.com>\n"),
            (String::from("feat: x\n\nBody.\n"), 1)
        );
        assert_eq!(strip_attribution("a\n\nb\n"), (String::from("a\n\nb\n"), 0));
        assert_eq!(civil_from_days(20_733), (2026, 10, 7));
        assert_eq!(days_from_civil(2000, 2, 29), 11_016);
    }
}
