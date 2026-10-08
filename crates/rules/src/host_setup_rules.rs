//! The pure decisions of the Windows host setup: the `sshd_config` edit, the authorized key merge, the herdr release retention, the filter keys reset, the Store Python stub test, and PowerShell quoting.
//! Each runs in the Linux proof container, although only Windows runs the setup.

extern crate alloc;
use self::alloc::string::String;
use self::alloc::vec::Vec;

/// Marks the `sshd_config` directives that the setup appended.
pub const SSHD_MARKER: &str = "# --- managed by dotfiles-win ---";

/// The `sshd_config` keyword that starts a conditional block.
const MATCH: &str = "match";

/// How a line of `sshd_config` relates to one keyword.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Kind {
    /// `Port 22`, in force.
    Active,
    /// `#Port 22`, the stock file's commented default.
    Commented,
    Other,
}

/// sshd splits fields only on ASCII whitespace.
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

/// sshd keywords ignore case, and the stock file writes commented defaults with no space after the `#`.
fn classify(line: &[u8], name: &[u8]) -> Kind {
    let trimmed = trim_start(line);
    let (body, commented) = match trimmed {
        [b'#', rest @ ..] => (trim_start(rest), true),
        _ => (trimmed, false),
    };
    let (Some(keyword), Some(after)) = (body.get(..name.len()), body.get(name.len()..)) else {
        return Kind::Other;
    };
    // A keyword needs whitespace and a value, so `PortForwarding` differs from `Port`, and a bare `Port` sets nothing.
    if !keyword.eq_ignore_ascii_case(name)
        || !after.first().is_some_and(u8::is_ascii_whitespace)
        || trim_start(after).is_empty()
    {
        Kind::Other
    } else if commented {
        Kind::Commented
    } else {
        Kind::Active
    }
}

/// Whether a line opens a `Match` block, whose directives apply only inside it.
fn opens_match(line: &[u8]) -> bool {
    let trimmed = trim_start(line);
    trimmed
        .get(..MATCH.len())
        .is_some_and(|keyword| keyword.eq_ignore_ascii_case(MATCH.as_bytes()))
        && trimmed
            .get(MATCH.len()..)
            .is_some_and(|rest| rest.first().is_some_and(u8::is_ascii_whitespace))
}

fn directive(name: &str, value: &str) -> String {
    let mut line = String::from(name);
    line.push(' ');
    line.push_str(value);
    line
}

/// An `sshd_config` text with every directive applied, which ends with exactly one `\r\n`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SshdConfig(String);

impl SshdConfig {
    /// The file's text.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl PartialEq<&str> for SshdConfig {
    fn eq(&self, other: &&str) -> bool {
        self.0 == *other
    }
}

/// Applies `directives` to the `sshd_config` text `original`, before its first `Match` block.
/// An active directive takes the value and its later duplicates go, a commented default becomes the directive in place, and otherwise the directive follows the marker.
#[must_use]
pub fn sshd_config(original: &str, directives: &[(&str, &str)]) -> SshdConfig {
    let lines: Vec<&str> = original
        .split('\n')
        .map(|line| line.trim_end_matches('\r'))
        .collect();
    let match_index = lines
        .iter()
        .position(|line| opens_match(line.as_bytes()))
        .unwrap_or(lines.len());
    let (above, tail) = lines.split_at_checked(match_index).unwrap_or((&lines, &[]));
    let mut head: Vec<String> = above.iter().map(|line| String::from(*line)).collect();
    for (name, value) in directives {
        let desired = directive(name, value);
        let active: Vec<usize> = head
            .iter()
            .enumerate()
            .filter(|(_, line)| classify(line.as_bytes(), name.as_bytes()) == Kind::Active)
            .map(|(index, _)| index)
            .collect();
        if let Some(first) = active.first().and_then(|first| head.get_mut(*first)) {
            *first = desired;
            // sshd honors the first occurrence, so later ones do nothing.
            for index in active.iter().skip(1).rev() {
                head.remove(*index);
            }
            continue;
        }
        if let Some(commented) = head
            .iter_mut()
            .find(|line| classify(line.as_bytes(), name.as_bytes()) == Kind::Commented)
        {
            *commented = desired;
            continue;
        }
        if !head.iter().any(|line| line == SSHD_MARKER) {
            head.push(String::from(SSHD_MARKER));
        }
        head.push(desired);
    }
    let mut text = String::new();
    for (index, line) in head
        .iter()
        .map(String::as_str)
        .chain(tail.iter().copied())
        .enumerate()
    {
        if index > 0 {
            text.push_str("\r\n");
        }
        text.push_str(line);
    }
    let mut text = String::from(text.trim_end());
    text.push_str("\r\n");
    SshdConfig(text)
}

/// The fields of an `authorized_keys` line that name the key: its type and its base64 body.
const KEY_MATERIAL_FIELDS: usize = 2;

/// The `<type> <base64>` half of an `authorized_keys` line, without the trailing comment, with an empty field for each one missing.
fn material(line: &[u8]) -> [&[u8]; KEY_MATERIAL_FIELDS] {
    let mut fields: [&[u8]; KEY_MATERIAL_FIELDS] = [&[]; KEY_MATERIAL_FIELDS];
    let mut rest = trim_start(line);
    for field in &mut fields {
        let end = rest
            .iter()
            .position(u8::is_ascii_whitespace)
            .unwrap_or(rest.len());
        let (word, tail) = rest.split_at_checked(end).unwrap_or((rest, &[]));
        *field = word;
        rest = trim_start(tail);
    }
    fields
}

/// Whether two `authorized_keys` lines name the same key, whatever their comments say.
fn same_key(line: &[u8], key: &[u8]) -> bool {
    material(line) == material(key)
}

/// An `authorized_keys` file with every other key kept and this one appended, one `\r\n` line ending per line.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AuthorizedKeys(String);

impl AuthorizedKeys {
    /// The file's text.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl PartialEq<&str> for AuthorizedKeys {
    fn eq(&self, other: &&str) -> bool {
        self.0 == *other
    }
}

/// Keeps every other key of `existing` and appends `key`, comparing only type and base64, because the comment changes on each 1Password rename.
#[must_use]
pub fn authorized_keys(existing: &str, key: &str) -> AuthorizedKeys {
    let mut text = String::new();
    for line in existing
        .lines()
        .map(str::trim_end)
        .filter(|line| !line.trim().is_empty())
        .filter(|line| !same_key(line.as_bytes(), key.as_bytes()))
        .chain([key])
    {
        text.push_str(line);
        text.push_str("\r\n");
    }
    AuthorizedKeys(text)
}

/// The herdr releases beyond the newest `retain` by installation time, never `current` nor a staging or backup directory.
/// Each release carries its name and installation time, because version names sort as text and put `0.10` before `0.9`.
#[must_use]
pub fn stale_releases<'a>(
    releases: &[(&'a str, i128)],
    retain: usize,
    current: &str,
) -> Vec<&'a str> {
    releases
        .iter()
        .filter(|release| stale(releases, release, retain, current))
        .map(|(name, _)| *name)
        .collect()
}

fn eligible(name: &str) -> bool {
    !name.starts_with(".staging.") && !name.starts_with(".backup.")
}

/// Selects eligible releases other than `current` after `retain` newer releases, with names breaking time ties.
fn stale(releases: &[(&str, i128)], release: &(&str, i128), retain: usize, current: &str) -> bool {
    let (name, time) = *release;
    eligible(name)
        && name != current
        && releases
            .iter()
            .filter(|(other, at)| eligible(other) && (*at, *other) > (time, name))
            .count()
            >= retain
}

/// `FKF_FILTERKEYSON`, which turns the filter keys on.
pub const FILTER_KEYS_ON: u32 = 0x0000_0001;

/// `FKF_AVAILABLE`, which leaves the filter keys available in the settings.
pub const FILTER_KEYS_AVAILABLE: u32 = 0x0000_0002;

/// The filter keys the old setup turned on: available and on, with these delays.
const LEGACY_FLAGS: u32 = FILTER_KEYS_ON | FILTER_KEYS_AVAILABLE;
const LEGACY_DELAY_MS: u32 = 150;
const LEGACY_REPEAT_MS: u32 = 15;

/// The fields of `FILTERKEYS` that the setup reads.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FilterKeys {
    pub flags: u32,
    pub wait_ms: u32,
    pub delay_ms: u32,
    pub repeat_ms: u32,
    pub bounce_ms: u32,
}

/// The filter keys with only the switch turned off, when they hold exactly the settings the old setup made, and `None` for anything a person chose.
#[must_use]
pub const fn disable_managed_filter_keys(keys: FilterKeys) -> Option<FilterKeys> {
    if keys.flags == LEGACY_FLAGS
        && keys.wait_ms == 0
        && keys.delay_ms == LEGACY_DELAY_MS
        && keys.repeat_ms == LEGACY_REPEAT_MS
        && keys.bounce_ms == 0
    {
        Some(FilterKeys {
            flags: keys.flags & !FILTER_KEYS_ON,
            ..keys
        })
    } else {
        None
    }
}

/// `FILE_ATTRIBUTE_REPARSE_POINT`.
pub const REPARSE_POINT: u32 = 0x400;

/// Requires both halves of the Store stub signature, an empty file and a reparse point, so a real interpreter survives.
#[must_use]
pub const fn looks_like_stub(len: u64, attributes: u32) -> bool {
    len == 0 && attributes & REPARSE_POINT != 0
}

/// The quote that opens and closes a PowerShell literal, and that a literal doubles inside it.
const QUOTE: u8 = b'\'';

/// Each byte of the text at most doubles inside the literal, and the opening and the closing quote add a byte each.
const WORST_CASE_PER_BYTE: usize = 2;
const QUOTES: usize = 2;

/// Writes the PowerShell single-quoted literal of `text` into `out` and returns its length, or `None` when `out` can't hold it.
/// UTF-8 never encodes another character with the quote's byte, so doubling bytes doubles exactly the quotes.
fn quote_into(text: &[u8], out: &mut [u8]) -> Option<usize> {
    fn put(out: &mut [u8], length: &mut usize, byte: u8) -> Option<()> {
        *out.get_mut(*length)? = byte;
        *length += 1;
        Some(())
    }
    let mut length = 0;
    put(out, &mut length, QUOTE)?;
    for byte in text {
        if *byte == QUOTE {
            put(out, &mut length, QUOTE)?;
        }
        put(out, &mut length, *byte)?;
    }
    put(out, &mut length, QUOTE)?;
    Some(length)
}

/// A PowerShell single-quoted literal: only a single quote needs escaping, by doubling it.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PowerShellLiteral(String);

impl PowerShellLiteral {
    /// The literal's text, which carries no character that PowerShell reads inside it.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl PartialEq<&str> for PowerShellLiteral {
    fn eq(&self, other: &&str) -> bool {
        self.0 == *other
    }
}

/// The PowerShell single-quoted literal of `text`: only a single quote needs escaping, by doubling it.
#[must_use]
pub fn powershell_literal(text: &str) -> PowerShellLiteral {
    let mut out = self::alloc::vec![0; WORST_CASE_PER_BYTE * text.len() + QUOTES];
    let length = quote_into(text.as_bytes(), &mut out).unwrap_or(0);
    out.truncate(length);
    PowerShellLiteral(String::from_utf8(out).unwrap_or_default())
}

/// The properties Kani proves for every input and the tests check over every input they list.
#[cfg(any(test, kani))]
mod proofs {
    use super::*;

    /// The lines a bounded `sshd_config` holds: directives, a prefix keyword, comments, the match block, and blanks.
    pub const LINES: [&str; 8] = ["P 2", "#P 2", "PF y", "p 1", "Match a", "  P 1", "", "# n"];

    /// Two keywords, one of which prefixes another keyword of the lines.
    pub const DIRECTIVES: [(&str, &str); 2] = [("P", "9"), ("Q", "y")];

    /// The file the lines at `picks` make, with Windows line endings.
    pub fn file(picks: &[usize]) -> String {
        let mut text = String::new();
        for pick in picks {
            text.push_str(LINES.get(*pick).copied().unwrap_or_default());
            text.push_str("\r\n");
        }
        text
    }

    fn related(line: &str) -> bool {
        DIRECTIVES
            .iter()
            .any(|(name, _)| classify(line.as_bytes(), name.as_bytes()) != Kind::Other)
            || line == SSHD_MARKER
    }

    /// The edit changes nothing a second time, keeps unrelated lines and the match block in order, and applies each directive exactly once before the block.
    pub fn spec_sshd(original: &str) -> String {
        let edited = sshd_config(original, &DIRECTIVES);
        assert_eq!(sshd_config(edited.as_str(), &DIRECTIVES), edited.as_str());
        let lines: Vec<&str> = edited.as_str().split("\r\n").collect();
        let before: Vec<&str> = original.split("\r\n").collect();
        let boundary = |lines: &[&str]| lines.iter().position(|line| opens_match(line.as_bytes()));
        let (above, block) = lines.split_at(boundary(&lines).unwrap_or(lines.len()));
        let (old_above, old_block) = before.split_at(boundary(&before).unwrap_or(before.len()));
        let kept: Vec<&&str> = above.iter().filter(|line| !related(line)).collect();
        let unrelated: Vec<&&str> = old_above.iter().filter(|line| !related(line)).collect();
        assert_eq!(
            kept.iter()
                .filter(|line| !line.is_empty())
                .collect::<Vec<_>>(),
            unrelated
                .iter()
                .filter(|line| !line.is_empty())
                .collect::<Vec<_>>()
        );
        if !old_block.is_empty() {
            assert_eq!(
                block
                    .iter()
                    .filter(|line| !line.is_empty())
                    .collect::<Vec<_>>(),
                old_block
                    .iter()
                    .filter(|line| !line.is_empty())
                    .collect::<Vec<_>>()
            );
        }
        for (name, value) in DIRECTIVES {
            let active: Vec<&&str> = above
                .iter()
                .filter(|line| classify(line.as_bytes(), name.as_bytes()) == Kind::Active)
                .collect();
            assert_eq!(active.len(), 1);
            assert_eq!(
                active.first().map(|line| **line),
                Some(directive(name, value).as_str())
            );
        }
        String::from(edited.as_str())
    }

    /// The merge changes nothing a second time, keeps every other key in order, and holds the key once.
    pub fn spec_keys(existing: &str, key: &str) -> String {
        let merged = authorized_keys(existing, key);
        assert_eq!(authorized_keys(merged.as_str(), key), merged.as_str());
        assert!(merged.as_str().ends_with("\r\n"));
        let lines: Vec<&str> = merged.as_str().lines().map(str::trim_end).collect();
        assert_eq!(
            lines
                .iter()
                .filter(|line| same_key(line.as_bytes(), key.as_bytes()))
                .count(),
            1
        );
        assert_eq!(lines.last().copied(), Some(key.trim_end()));
        let others: Vec<&str> = existing
            .lines()
            .map(str::trim_end)
            .filter(|line| !line.trim().is_empty() && !same_key(line.as_bytes(), key.as_bytes()))
            .collect();
        assert_eq!(
            lines.get(..lines.len().saturating_sub(1)),
            Some(others.as_slice())
        );
        String::from(merged.as_str())
    }

    /// Retention keeps the newest `retain`, `current`, and every staging or backup directory, and removes the rest.
    pub fn spec_stale(releases: &[(&str, i128)], retain: usize, current: &str) -> usize {
        let stale = stale_releases(releases, retain, current);
        for name in &stale {
            assert_ne!(*name, current);
            assert!(!name.starts_with(".staging.") && !name.starts_with(".backup."));
            let time = releases
                .iter()
                .find(|(other, _)| other == name)
                .map(|(_, time)| *time);
            let newer = releases
                .iter()
                .filter(|(other, at)| {
                    !other.starts_with(".staging.")
                        && !other.starts_with(".backup.")
                        && (Some(*at), *other) > (time, *name)
                })
                .count();
            assert!(newer >= retain);
        }
        let eligible = |name: &str| !name.starts_with(".staging.") && !name.starts_with(".backup.");
        for (name, time) in releases {
            let newer = releases
                .iter()
                .filter(|(other, at)| eligible(other) && (*at, *other) > (*time, *name))
                .count();
            if eligible(name) && name != &current && newer >= retain {
                assert!(stale.contains(name));
            }
        }
        let eligible = releases
            .iter()
            .filter(|(name, _)| !name.starts_with(".staging.") && !name.starts_with(".backup."))
            .count();
        assert!(stale.len() + retain.min(eligible) <= eligible);
        stale.len()
    }

    /// Retention never removes `current`, staging, or backups, and requires `retain` newer eligible releases.
    /// Keeping more never removes more.
    pub fn spec_stale_release(
        releases: &[(&str, i128)],
        release: &(&str, i128),
        retain: usize,
        current: &str,
    ) -> bool {
        let decided = stale(releases, release, retain, current);
        let (name, time) = *release;
        let newer = releases
            .iter()
            .filter(|(other, at)| eligible(other) && (*at, *other) > (time, name))
            .count();
        assert_eq!(
            decided,
            eligible(name) && name != current && newer >= retain
        );
        if decided && retain > 0 {
            assert!(stale(releases, release, retain - 1, current));
        }
        decided
    }

    /// Only the exact settings of the old setup lose their switch, and nothing else changes.
    pub fn spec_filter_keys(keys: FilterKeys) -> bool {
        let reset = disable_managed_filter_keys(keys);
        let managed = keys
            == FilterKeys {
                flags: LEGACY_FLAGS,
                wait_ms: 0,
                delay_ms: LEGACY_DELAY_MS,
                repeat_ms: LEGACY_REPEAT_MS,
                bounce_ms: 0,
            };
        assert_eq!(reset.is_some(), managed);
        if let Some(reset) = reset {
            assert_eq!(reset.flags, FILTER_KEYS_AVAILABLE);
            assert_eq!(
                FilterKeys {
                    flags: keys.flags,
                    ..reset
                },
                keys
            );
        }
        managed
    }

    /// Deletion requires an empty reparse point, so a plain file or a file with content survives.
    pub fn spec_stub(len: u64, attributes: u32) -> bool {
        let stub = looks_like_stub(len, attributes);
        assert_eq!(stub, len == 0 && attributes & REPARSE_POINT != 0);
        stub
    }

    /// Reads exactly one PowerShell single-quoted literal into `out` and returns its length, or `None` for malformed input.
    pub fn unquote_into(literal: &[u8], out: &mut [u8]) -> Option<usize> {
        let [QUOTE, inner @ .., QUOTE] = literal else {
            return None;
        };
        let mut length = 0;
        let mut rest = inner;
        while let [byte, tail @ ..] = rest {
            rest = tail;
            if *byte == QUOTE {
                let [QUOTE, tail @ ..] = rest else {
                    return None;
                };
                rest = tail;
            }
            *out.get_mut(length)? = *byte;
            length += 1;
        }
        Some(length)
    }

    /// The longest text the bounded literal checks quote, and the room its literal and read-back need.
    pub const TEXT: usize = 3;
    pub const ROOM: usize = 2 * TEXT + 2;

    /// PowerShell reads the literal back as the text, so two texts never share a literal.
    pub fn spec_literal(left: &[u8], right: &[u8]) -> bool {
        let mut quoted = [0; ROOM];
        let length = quote_into(left, &mut quoted).unwrap_or(ROOM + 1);
        let literal = quoted.get(..length).unwrap_or_default();
        let mut read = [0; ROOM];
        let read_length = unquote_into(literal, &mut read);
        assert_eq!(
            read_length.and_then(|length| read.get(..length)),
            Some(left)
        );
        let mut other = [0; ROOM];
        let other_length = quote_into(right, &mut other).unwrap_or(ROOM + 1);
        let same = other.get(..other_length) == Some(literal);
        assert_eq!(same, left == right);
        same
    }

    /// The setup and the bounded writer produce the same literal.
    pub fn spec_powershell_literal(text: &str) -> String {
        let literal = powershell_literal(text);
        let mut quoted = [0; ROOM];
        let length = quote_into(text.as_bytes(), &mut quoted).unwrap_or(ROOM + 1);
        assert_eq!(Some(literal.as_str().as_bytes()), quoted.get(..length));
        String::from(literal.as_str())
    }

    /// Each edited keyword produces its active line without starting a match block, so a second edit finds it and changes nothing.
    pub fn spec_written(value: [u8; 2]) -> Kind {
        let line = [b'P', b' ', value[0], value[1]];
        let kind = classify(&line, b"P");
        if value.iter().any(|byte| !byte.is_ascii_whitespace()) {
            assert_eq!(kind, Kind::Active);
        }
        assert_eq!(classify(&line, b"Q"), Kind::Other);
        assert!(!opens_match(&line));
        kind
    }

    /// One byte of a bounded directive value.
    #[cfg(kani)]
    fn value_byte() -> u8 {
        const BYTES: [u8; 4] = [b'9', b'#', b' ', b'P'];
        BYTES[usize::from(kani::any::<u8>() % 4)]
    }

    #[cfg(kani)]
    #[kani::proof]
    #[kani::unwind(8)]
    fn the_line_an_edit_writes_is_active_so_a_second_edit_changes_nothing() {
        let kind = spec_written([value_byte(), value_byte()]);
        kani::cover!(kind == Kind::Active);
        kani::cover!(kind == Kind::Other);
    }

    /// One byte of a bounded `sshd_config` line.
    #[cfg(kani)]
    fn line_byte() -> u8 {
        const BYTES: [u8; 6] = [b'P', b'p', b'F', b'#', b' ', b'9'];
        BYTES[usize::from(kani::any::<u8>() % 6)]
    }

    /// A keyword counts only when whitespace and a value follow it, so a longer keyword it prefixes never counts, and case never matters.
    pub fn spec_classify(line: &[u8]) -> Kind {
        let kind = classify(line, b"P");
        let trimmed = trim_start(line);
        let body = match trimmed {
            [b'#', rest @ ..] => trim_start(rest),
            _ => trimmed,
        };
        let keyword = body
            .first()
            .is_some_and(|byte| byte.eq_ignore_ascii_case(&b'P'));
        let spaced = body.get(1).is_some_and(u8::is_ascii_whitespace);
        let valued = body
            .get(1..)
            .is_some_and(|after| after.iter().any(|byte| !byte.is_ascii_whitespace()));
        assert_eq!(kind != Kind::Other, keyword && spaced && valued);
        assert_eq!(
            kind == Kind::Commented,
            kind != Kind::Other && trimmed.first() == Some(&b'#')
        );
        assert_eq!(classify(line, b"p"), kind);
        kind
    }

    /// One letter of a bounded `authorized_keys` field.
    #[cfg(kani)]
    fn key_letter() -> u8 {
        if kani::any() { b't' } else { b'a' }
    }

    /// Two key lines of the form `<type> <body> <comment>` name one key exactly when type and body agree, and a line without a comment names the key its commented form does.
    pub fn spec_material(left: [u8; 3], right: [u8; 3]) -> bool {
        let line = |[kind, body, comment]: [u8; 3]| [kind, b' ', body, b' ', comment];
        let same = same_key(&line(left), &line(right));
        assert_eq!(same, left[0] == right[0] && left[1] == right[1]);
        assert!(same_key(&line(left), &[left[0], b'\t', left[1]]));
        same
    }

    /// Two lines name one key when their first two fields agree, whatever follows.
    pub fn spec_same_key(line: &[u8]) -> bool {
        let same = same_key(line, b"t a new");
        let mut fields = line
            .split(u8::is_ascii_whitespace)
            .filter(|field| !field.is_empty());
        let expected =
            fields.next() == Some(b"t".as_slice()) && fields.next() == Some(b"a".as_slice());
        assert_eq!(same, expected);
        assert!(same_key(line, line));
        same
    }

    #[cfg(kani)]
    #[kani::proof]
    #[kani::unwind(8)]
    fn a_keyword_counts_only_with_whitespace_and_a_value() {
        let line = [
            line_byte(),
            line_byte(),
            line_byte(),
            line_byte(),
            line_byte(),
        ];
        let kind = spec_classify(&line);
        kani::cover!(kind == Kind::Commented);
        kani::cover!(kind == Kind::Other && line[0] == b'P');
    }

    #[cfg(kani)]
    #[kani::proof]
    #[kani::unwind(8)]
    fn two_key_lines_match_on_type_and_body_alone() {
        let left = [b't', b'a', b'c'];
        let right = [key_letter(), key_letter(), key_letter()];
        let same = spec_material(left, right);
        kani::cover!(same && left[2] != right[2]);
        kani::cover!(!same);
    }

    #[cfg(kani)]
    #[kani::proof]
    #[kani::unwind(12)]
    fn retention_never_removes_the_current_or_a_newer_release() {
        const NAMES: [&str; 3] = ["a", ".backup.b", "c"];
        let releases = [
            (NAMES[0], i128::from(kani::any::<u64>() % 3)),
            (NAMES[1], i128::from(kani::any::<u64>() % 3)),
            (NAMES[2], i128::from(kani::any::<u64>() % 3)),
        ];
        let release = releases[usize::from(kani::any::<u8>() % 3)];
        let current = NAMES[usize::from(kani::any::<u8>() % 3)];
        let removed = spec_stale_release(
            &releases,
            &release,
            usize::from(kani::any::<u8>() % 3),
            current,
        );
        kani::cover!(removed);
        kani::cover!(!removed && release.0 == "a");
    }

    #[cfg(kani)]
    #[kani::proof]
    fn only_the_old_setups_filter_keys_lose_their_switch() {
        let keys = FilterKeys {
            flags: kani::any(),
            wait_ms: kani::any(),
            delay_ms: kani::any(),
            repeat_ms: kani::any(),
            bounce_ms: kani::any(),
        };
        let managed = spec_filter_keys(keys);
        kani::cover!(managed);
        kani::cover!(!managed && keys.flags & FILTER_KEYS_ON != 0);
    }

    #[cfg(kani)]
    #[kani::proof]
    fn only_an_empty_reparse_point_is_a_stub() {
        let stub = spec_stub(kani::any(), kani::any());
        kani::cover!(stub);
        kani::cover!(!stub);
    }

    #[cfg(kani)]
    #[kani::proof]
    #[kani::unwind(10)]
    fn powershell_reads_every_literal_back_as_its_text() {
        let text = |bytes: [bool; TEXT], length: u8| -> ([u8; TEXT], usize) {
            (
                bytes.map(|quote| if quote { QUOTE } else { b'a' }),
                usize::from(length) % (TEXT + 1),
            )
        };
        let (left, left_length) = text(kani::any(), kani::any());
        let (right, right_length) = text(kani::any(), kani::any());
        let same = spec_literal(&left[..left_length], &right[..right_length]);
        kani::cover!(same);
        kani::cover!(!same);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every file of up to four bounded lines, every key file, every retention, and every literal over a quote and a letter.
    #[test]
    #[cfg_attr(miri, ignore = "Miri takes minutes over every bounded file")]
    fn every_bounded_input_meets_the_specifications() {
        let count = proofs::LINES.len();
        for length in 0..=4u32 {
            for mut number in 0..count.pow(length) {
                let mut picks = Vec::new();
                for _ in 0..length {
                    picks.push(number % count);
                    number /= count;
                }
                proofs::spec_sshd(&proofs::file(&picks));
            }
        }
        for existing in [
            "",
            "\r\n\r\n",
            "t a old\r\n",
            "t b x\r\nt a\r\nt c\r\n",
            "t a\nt a y\n",
        ] {
            proofs::spec_keys(existing, "t a new");
        }
        let names = ["a", "b", ".staging.c", ".backup.d"];
        for retain in 0..4 {
            for times in 0..4u64.pow(4) {
                let releases: Vec<(&str, i128)> = names
                    .iter()
                    .enumerate()
                    .map(|(index, name)| {
                        (
                            *name,
                            i128::from(times / 4u64.pow(u32::try_from(index).unwrap_or(0)) % 4),
                        )
                    })
                    .collect();
                for current in names {
                    proofs::spec_stale(&releases, retain, current);
                    for release in &releases {
                        proofs::spec_stale_release(&releases, release, retain, current);
                    }
                }
            }
        }
        for word in crate::domain::words(b"'a", proofs::TEXT) {
            for other in crate::domain::words(b"'a", proofs::TEXT) {
                proofs::spec_literal(&word, &other);
            }
            proofs::spec_powershell_literal(core::str::from_utf8(&word).unwrap_or_default());
        }
        for line in crate::domain::words(b"Pp#F 9", 5) {
            proofs::spec_classify(&line);
        }
        for value in crate::domain::words(b"9# P", 2) {
            if let [first, second] = value[..] {
                proofs::spec_written([first, second]);
            }
        }
        for left in crate::domain::words(b"tac", 3) {
            for right in crate::domain::words(b"tac", 3) {
                if let ([a, b, c], [d, e, f]) = (&left[..], &right[..]) {
                    proofs::spec_material([*a, *b, *c], [*d, *e, *f]);
                }
            }
        }
        for line in crate::domain::words(b"tac ", 5) {
            proofs::spec_same_key(&line);
        }
    }

    #[test]
    fn the_stock_file_gets_its_defaults_in_place_and_the_examples_hold() {
        let stock = "#Port 22\r\n#PasswordAuthentication yes\r\nSubsystem\tsftp\tsftp-server.exe\r\n\r\nMatch Group administrators\r\n       AuthorizedKeysFile keys\r\n";
        let edited = sshd_config(stock, &[("Port", "22"), ("PasswordAuthentication", "no")]);
        assert_eq!(
            edited,
            "Port 22\r\nPasswordAuthentication no\r\nSubsystem\tsftp\tsftp-server.exe\r\n\r\nMatch Group administrators\r\n       AuthorizedKeysFile keys\r\n"
        );
        assert_eq!(
            sshd_config("", &[("Port", "22")]),
            "\r\n# --- managed by dotfiles-win ---\r\nPort 22\r\n"
        );
        assert_eq!(
            authorized_keys("t a old\r\nt b\r\n", "t a new"),
            "t b\r\nt a new\r\n"
        );
        assert_eq!(
            stale_releases(
                &[("0.9.9", 1), ("0.10.0", 2), (".staging.x", 3)],
                1,
                "0.10.0"
            ),
            ["0.9.9"]
        );
        assert_eq!(stale_releases(&[("b", 1), ("a", 1)], 1, "x"), ["a"]);
        let managed = FilterKeys {
            flags: LEGACY_FLAGS,
            wait_ms: 0,
            delay_ms: LEGACY_DELAY_MS,
            repeat_ms: LEGACY_REPEAT_MS,
            bounce_ms: 0,
        };
        for flags in [0, FILTER_KEYS_ON, LEGACY_FLAGS | 4] {
            proofs::spec_filter_keys(FilterKeys { flags, ..managed });
        }
        for keys in [
            managed,
            FilterKeys {
                wait_ms: 1,
                ..managed
            },
            FilterKeys {
                delay_ms: 151,
                ..managed
            },
            FilterKeys {
                repeat_ms: 16,
                ..managed
            },
            FilterKeys {
                bounce_ms: 1,
                ..managed
            },
        ] {
            proofs::spec_filter_keys(keys);
        }
        for (len, attributes) in [
            (0, REPARSE_POINT),
            (0, REPARSE_POINT | 0x20),
            (1, REPARSE_POINT),
            (0, 0x20),
            (0, 0),
        ] {
            proofs::spec_stub(len, attributes);
        }
        assert_eq!(powershell_literal("it's"), "'it''s'");
    }
}
