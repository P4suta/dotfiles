//! Bounded Codex notify chains shared by the configuration writer and the callback guard.

extern crate alloc;
use self::alloc::{string::String, vec, vec::Vec};
use core::iter::Peekable;
use core::str::Chars;

pub const MAX_DEPTH: usize = 32;
pub const MAX_ARGUMENTS: usize = 32;
const DOTFILES_WORDS: usize = 4;
const APP_WORDS: usize = 2;
const FORWARD: &str = "--forward-notify";
const PREVIOUS: &str = "--previous-notify";
const HEX_RADIX: u32 = 16;
const HEX_DIGITS: usize = 4;
const HEX_SHIFTS: [u32; HEX_DIGITS] = [12, 8, 4, 0];

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Command(pub Vec<String>);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Kind {
    Dotfiles,
    App,
    Other,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Flag {
    Forward,
    Previous,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Link<Identity = Command> {
    command: Identity,
    kind: Kind,
    flag: Flag,
}

#[derive(Debug)]
struct Chain {
    links: Vec<Link>,
    complete: bool,
}

fn executable(program: &str, name: &[u8]) -> bool {
    program
        .as_bytes()
        .strip_suffix(name)
        .is_some_and(|parent| parent.is_empty() || matches!(parent.last(), Some(b'/' | b'\\')))
}

fn kind(command: &Command) -> Kind {
    let Some(program) = command.0.first() else {
        return Kind::Other;
    };
    if (executable(program, b"dotfiles") || executable(program, b"dotfiles.exe"))
        && command.0.get(1).is_some_and(|word| word == "notifications")
        && command.0.get(2).is_some_and(|word| word == "agent")
        && command
            .0
            .get(DOTFILES_WORDS - 1)
            .is_some_and(|word| word == "codex")
    {
        Kind::Dotfiles
    } else if executable(program, b"SkyComputerUseClient")
        && command.0.get(1).is_some_and(|word| word == "turn-ended")
    {
        Kind::App
    } else {
        Kind::Other
    }
}

fn walk(mut command: Command) -> Chain {
    let mut links = Vec::new();
    for _ in 0..MAX_DEPTH {
        if command.0.is_empty()
            || command.0.len() > MAX_ARGUMENTS
            || command.0.first().is_some_and(String::is_empty)
        {
            break;
        }
        let kind = kind(&command);
        let edge = command
            .0
            .iter()
            .enumerate()
            .skip(1)
            .find_map(|(index, word)| match word.as_str() {
                FORWARD => Some((index, Flag::Forward)),
                PREVIOUS => Some((index, Flag::Previous)),
                _ => None,
            });
        let flag = edge.map(|(_, flag)| flag);
        let next = edge
            .and_then(|(index, _)| command.0.get(index + 1))
            .and_then(|json| decode(json));
        if let Some((index, _)) = edge {
            command.0.truncate(index);
        }
        if command.0.is_empty() {
            break;
        }
        let forward = match kind {
            Kind::Dotfiles => {
                command.0.truncate(DOTFILES_WORDS);
                Flag::Forward
            }
            Kind::App => {
                command.0.truncate(APP_WORDS);
                Flag::Previous
            }
            Kind::Other => flag.unwrap_or(Flag::Forward),
        };
        links.push(Link {
            command,
            kind,
            flag: forward,
        });
        let Some(next) = next else {
            return Chain {
                links,
                complete: flag.is_none(),
            };
        };
        command = next;
    }
    Chain {
        links,
        complete: false,
    }
}

fn same<Identity: Eq>(first: &Link<Identity>, second: &Link<Identity>) -> bool {
    match (first.kind, second.kind) {
        (Kind::Dotfiles, Kind::Dotfiles) | (Kind::App, Kind::App) => true,
        (Kind::Other, Kind::Other) => first.command == second.command,
        _ => false,
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Pick {
    Dotfiles,
    Existing(usize),
}

#[derive(Debug)]
struct Plan<const CAPACITY: usize> {
    picks: [Pick; CAPACITY],
    len: usize,
}

impl<const CAPACITY: usize> Plan<CAPACITY> {
    fn push(&mut self, pick: Pick) {
        if let Some(slot) = self.picks.get_mut(self.len) {
            *slot = pick;
            self.len += 1;
        }
    }
}

fn plan<Identity: Eq, const CAPACITY: usize>(links: &[Link<Identity>]) -> Plan<CAPACITY> {
    let mut selected = Plan {
        picks: [Pick::Dotfiles; CAPACITY],
        len: 0,
    };
    if let Some((index, _)) = links
        .iter()
        .take(MAX_DEPTH)
        .enumerate()
        .find(|(_, link)| link.kind == Kind::App)
    {
        selected.push(Pick::Existing(index));
    }
    selected.push(Pick::Dotfiles);
    for (index, link) in links.iter().take(MAX_DEPTH).enumerate() {
        if selected.len < MAX_DEPTH
            && link.kind == Kind::Other
            && !selected
                .picks
                .iter()
                .take(selected.len)
                .any(|pick| match pick {
                    Pick::Dotfiles => false,
                    Pick::Existing(index) => links.get(*index).is_some_and(|seen| same(seen, link)),
                })
        {
            selected.push(Pick::Existing(index));
        }
    }
    selected
}

fn select(links: &[Link], dotfiles: &Link) -> Vec<Link> {
    let selected = plan::<_, MAX_DEPTH>(links);
    selected
        .picks
        .iter()
        .take(selected.len)
        .filter_map(|pick| match pick {
            Pick::Dotfiles => Some(dotfiles.clone()),
            Pick::Existing(index) => links.get(*index).cloned(),
        })
        .collect()
}

fn safe<Identity: Eq>(links: &[Link<Identity>], complete: bool) -> bool {
    complete
        && !links.is_empty()
        && links.iter().all(|link| link.kind != Kind::Dotfiles)
        && links
            .iter()
            .enumerate()
            .all(|(index, link)| !links.iter().take(index).any(|seen| same(seen, link)))
}

/// Keep each notifier once, cutting malformed or over-depth tails at the last valid command.
#[must_use]
pub fn normalize(previous: Command, tool: &str) -> Command {
    let dotfiles = Link {
        command: Command(vec![
            tool.into(),
            "notifications".into(),
            "agent".into(),
            "codex".into(),
        ]),
        kind: Kind::Dotfiles,
        flag: Flag::Forward,
    };
    // Both writers preserve the app outside dotfiles.
    let selected = select(&walk(previous).links, &dotfiles);
    let mut result = Command(Vec::new());
    for link in selected.into_iter().rev() {
        let mut command = link.command;
        if !result.0.is_empty() {
            command.0.push(
                match link.flag {
                    Flag::Forward => FORWARD,
                    Flag::Previous => PREVIOUS,
                }
                .into(),
            );
            command.0.push(encode(&result));
        }
        result = command;
    }
    result
}

/// Admit a callback only when its complete chain can't return to dotfiles or repeat a notifier.
#[must_use]
pub fn forwarding(command: Command) -> Option<Command> {
    let chain = walk(command.clone());
    safe(&chain.links, chain.complete).then_some(command)
}

fn whitespace(chars: &mut Peekable<Chars<'_>>) {
    while chars
        .next_if(|ch| matches!(ch, ' ' | '\t' | '\r' | '\n'))
        .is_some()
    {}
}

fn hex(chars: &mut Peekable<Chars<'_>>) -> Option<u16> {
    let mut value = 0u16;
    for _ in 0..HEX_DIGITS {
        value = value
            .checked_mul(u16::try_from(HEX_RADIX).ok()?)?
            .checked_add(u16::try_from(chars.next()?.to_digit(HEX_RADIX)?).ok()?)?;
    }
    Some(value)
}

fn string(chars: &mut Peekable<Chars<'_>>) -> Option<String> {
    if chars.next()? != '"' {
        return None;
    }
    let mut text = String::new();
    loop {
        match chars.next()? {
            '"' => return Some(text),
            '\\' => text.push(match chars.next()? {
                '"' => '"',
                '\\' => '\\',
                '/' => '/',
                'b' => '\u{8}',
                'f' => '\u{c}',
                'n' => '\n',
                'r' => '\r',
                't' => '\t',
                'u' => {
                    let first = hex(chars)?;
                    if let Some(ch) = char::from_u32(u32::from(first)) {
                        ch
                    } else {
                        if chars.next()? != '\\' || chars.next()? != 'u' {
                            return None;
                        }
                        char::decode_utf16([first, hex(chars)?]).next()?.ok()?
                    }
                }
                _ => return None,
            }),
            ch if ch < ' ' => return None,
            ch => text.push(ch),
        }
    }
}

fn decode(json: &str) -> Option<Command> {
    let mut chars = json.chars().peekable();
    whitespace(&mut chars);
    if chars.next()? != '[' {
        return None;
    }
    let mut words = Vec::new();
    whitespace(&mut chars);
    if chars.peek() != Some(&']') {
        loop {
            if words.len() == MAX_ARGUMENTS {
                return None;
            }
            words.push(string(&mut chars)?);
            whitespace(&mut chars);
            if chars.peek() == Some(&']') {
                break;
            }
            if chars.next()? != ',' {
                return None;
            }
            whitespace(&mut chars);
        }
    }
    chars.next()?;
    whitespace(&mut chars);
    chars.next().is_none().then_some(Command(words))
}

fn encode(command: &Command) -> String {
    let mut json = String::from("[");
    for (index, word) in command.0.iter().enumerate() {
        if index != 0 {
            json.push(',');
        }
        json.push('"');
        for ch in word.chars() {
            match ch {
                '"' => json.push_str("\\\""),
                '\\' => json.push_str("\\\\"),
                '\n' => json.push_str("\\n"),
                '\r' => json.push_str("\\r"),
                '\t' => json.push_str("\\t"),
                '\u{8}' => json.push_str("\\b"),
                '\u{c}' => json.push_str("\\f"),
                ch if ch < ' ' || matches!(ch, '<' | '>' | '&' | '\u{2028}' | '\u{2029}') => {
                    json.push_str("\\u");
                    for shift in HEX_SHIFTS {
                        json.push(
                            char::from_digit((u32::from(ch) >> shift) % HEX_RADIX, HEX_RADIX)
                                .unwrap_or('0'),
                        );
                    }
                }
                ch => json.push(ch),
            }
        }
        json.push('"');
    }
    json.push(']');
    json
}

#[cfg(any(test, kani))]
pub(crate) mod model {
    use super::*;

    pub(super) fn link(code: u8) -> Link<u8> {
        let (kind, flag) = match code % 4 {
            0 => (Kind::Dotfiles, Flag::Forward),
            1 => (Kind::App, Flag::Previous),
            2 => (Kind::Other, Flag::Forward),
            _ => (Kind::Other, Flag::Previous),
        };
        Link {
            kind,
            command: code % 4,
            flag,
        }
    }

    #[derive(Debug, Eq, PartialEq)]
    struct Selected {
        codes: [u8; 4],
        len: usize,
    }

    fn selected(links: &[Link<u8>]) -> Selected {
        let plan = plan::<_, 4>(links);
        let mut result = Selected {
            codes: [0; 4],
            len: plan.len,
        };
        for (code, pick) in result.codes.iter_mut().zip(plan.picks).take(plan.len) {
            *code = match pick {
                Pick::Dotfiles => 0,
                Pick::Existing(index) => {
                    assert!(index < links.len());
                    links.get(index).map_or(0, |node| node.command)
                }
            };
        }
        result
    }

    fn mask(codes: &[u8]) -> u8 {
        codes.iter().fold(0, |mask, code| mask | (1 << (code % 4)))
    }

    pub(super) fn canonical(codes: [u8; 4], count: usize) -> bool {
        let nodes = codes.map(link);
        let links = nodes.get(..count).unwrap_or(&[]);
        let selected = selected(links);
        assert!(selected.len <= 4);
        let output = selected.codes.get(..selected.len).unwrap_or(&[]);
        let input = codes.get(..count).unwrap_or(&[]);
        let mut emitted = 0u8;
        for code in output {
            let bit = 1u8 << code;
            assert_eq!(emitted & bit, 0);
            emitted |= bit;
        }
        assert_eq!(emitted, mask(input) | 1);
        let app = mask(input) & 2 != 0;
        assert_eq!(output.first(), Some(&u8::from(app)));
        let dotfiles = usize::from(app);
        assert_eq!(output.get(dotfiles), Some(&0));
        assert!(output.iter().skip(dotfiles + 1).all(|code| *code >= 2));
        app
    }

    pub(super) fn forward(codes: [u8; 4], count: usize, complete: bool) -> bool {
        let nodes = codes.map(link);
        let links = nodes.get(..count).unwrap_or(&[]);
        let seen = mask(codes.get(..count).unwrap_or(&[]));
        let admitted = safe(links, complete);
        assert_eq!(
            admitted,
            complete
                && count > 0
                && seen & 1 == 0
                && seen.count_ones() == u32::try_from(count).unwrap_or(0)
        );
        admitted
    }

    // Every selected chain has this form, so checking these forms proves that a second selection is stable.
    pub(super) fn idempotent([app, first, second, reversed]: [bool; 4]) -> bool {
        let mut nodes = [link(0); 4];
        let mut len = 1;
        if app {
            if let Some(node) = nodes.first_mut() {
                *node = link(1);
            }
            len += 1;
        }
        for (present, code) in [
            (first, if reversed { 3 } else { 2 }),
            (second, if reversed { 2 } else { 3 }),
        ] {
            if present {
                assert!(len < nodes.len());
                if let Some(node) = nodes.get_mut(len) {
                    *node = link(code);
                }
                len += 1;
            }
        }
        let selected = selected(nodes.get(..len).unwrap_or(&[]));
        assert_eq!(selected.len, len);
        assert_eq!(
            selected.codes.get(..len),
            nodes.map(|node| node.command).get(..len)
        );
        app
    }

    pub fn contract(codes: [u8; 4], count: usize, complete: bool) -> (bool, bool) {
        #[cfg(test)]
        {
            let nodes = codes.map(link);
            let first = selected(nodes.get(..count).unwrap_or(&[]));
            let ordered = first.codes.map(link);
            assert_eq!(selected(ordered.get(..first.len).unwrap_or(&[])), first);
        }
        (forward(codes, count, complete), canonical(codes, count))
    }
}

#[cfg(kani)]
mod proofs {
    use super::*;

    fn input() -> ([u8; 4], usize) {
        let code: u8 = kani::any();
        let count = usize::from(kani::any::<u8>());
        kani::assume(count <= 4);
        (
            [code & 3, (code >> 2) & 3, (code >> 4) & 3, code >> 6],
            count,
        )
    }

    #[kani::proof]
    #[kani::unwind(8)]
    fn bounded_chains_keep_each_notifier_once_with_the_app_outermost() {
        let (codes, count) = input();
        let app = model::canonical(codes, count);
        kani::cover!(app);
        kani::cover!(!app);
    }

    #[kani::proof]
    #[kani::unwind(8)]
    fn bounded_chain_normalization_is_idempotent() {
        let app = model::idempotent(kani::any());
        kani::cover!(app);
        kani::cover!(!app);
    }

    #[kani::proof]
    #[kani::unwind(8)]
    fn bounded_forwarding_never_returns_to_dotfiles_or_repeats_a_notifier() {
        let (codes, count) = input();
        let complete = kani::any();
        assert!(forwarding(Command(Vec::new())).is_none());
        let admitted = model::forward(codes, count, complete);
        kani::cover!(admitted);
        kani::cover!(!admitted);
    }

    #[kani::proof]
    #[kani::unwind(32)]
    fn normalize_without_a_previous_notifier_is_plain_dotfiles() {
        let windows: bool = kani::any();
        let tool = if windows { "dotfiles.exe" } else { "dotfiles" };
        let canonical = normalize(Command(Vec::new()), tool);
        assert_eq!(canonical.0.len(), DOTFILES_WORDS);
        assert_eq!(canonical.0.first().map(String::as_str), Some(tool));
        kani::cover!(windows);
        kani::cover!(!windows);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TOOL: &str = "/synthetic/home/.local/bin/dotfiles";
    const APP: &str = "/synthetic/Codex Computer Use.app/SkyComputerUseClient";

    fn forwarding(json: &str) -> Option<Command> {
        decode(json).and_then(super::forwarding)
    }

    fn command(words: &[&str]) -> Command {
        Command(words.iter().map(|word| String::from(*word)).collect())
    }

    fn dotfiles() -> Command {
        command(&[TOOL, "notifications", "agent", "codex"])
    }
    fn app() -> Command {
        command(&[APP, "turn-ended"])
    }
    fn wrap(mut outer: Command, flag: &str, inner: &Command) -> Command {
        outer.0.extend([flag.into(), encode(inner)]);
        outer
    }

    #[test]
    fn app_around_dotfiles_and_dotfiles_around_app_have_the_same_fixed_point() {
        let expected = wrap(app(), PREVIOUS, &dotfiles());
        for previous in [expected.clone(), wrap(dotfiles(), FORWARD, &app())] {
            assert_eq!(normalize(previous, TOOL), expected);
        }
    }

    #[test]
    fn deep_alternating_nesting_collapses_to_two_notifiers() {
        let mut previous = app();
        for depth in 0..21 {
            previous = if depth % 2 == 0 {
                wrap(dotfiles(), FORWARD, &previous)
            } else {
                wrap(app(), PREVIOUS, &previous)
            };
        }
        assert_eq!(
            normalize(previous, TOOL),
            wrap(app(), PREVIOUS, &dotfiles())
        );
    }

    #[test]
    fn no_app_keeps_dotfiles_first_and_a_distinct_foreign_notifier_once() {
        let foreign = command(&["third-notifier", "an argument with spaces"]);
        let expected = wrap(dotfiles(), FORWARD, &foreign);
        for previous in [
            foreign,
            expected.clone(),
            wrap(dotfiles(), FORWARD, &expected),
        ] {
            assert_eq!(normalize(previous, TOOL), expected);
        }
        assert_eq!(normalize(Command(Vec::new()), TOOL), dotfiles());
    }

    #[test]
    fn app_and_a_third_notifier_keep_a_single_forward_path() {
        let third = command(&["third-notifier"]);
        let expected = wrap(app(), PREVIOUS, &wrap(dotfiles(), FORWARD, &third));
        let input = wrap(
            dotfiles(),
            FORWARD,
            &wrap(app(), PREVIOUS, &wrap(dotfiles(), FORWARD, &third)),
        );
        assert_eq!(normalize(input, TOOL), expected);
        let repeated = wrap(third.clone(), PREVIOUS, &wrap(app(), PREVIOUS, &third));
        assert_eq!(normalize(repeated, TOOL), expected);
    }

    #[test]
    fn malformed_json_cuts_only_the_invalid_tail_and_never_panics() {
        for invalid in [
            "not-json",
            "[",
            "null",
            "{}",
            "[1]",
            r#"["x",]"#,
            r#"["\ud800"]"#,
            "[]",
        ] {
            let mut broken = dotfiles();
            broken.0.extend([FORWARD.into(), invalid.into()]);
            assert_eq!(
                normalize(wrap(app(), PREVIOUS, &broken), TOOL),
                wrap(app(), PREVIOUS, &dotfiles())
            );
            assert!(forwarding(&encode(&wrap(app(), PREVIOUS, &broken))).is_none());
            let outer = command(&["third-notifier", PREVIOUS, invalid]);
            assert_eq!(
                normalize(outer, TOOL),
                wrap(dotfiles(), FORWARD, &command(&["third-notifier"]))
            );
        }
    }

    #[test]
    fn normalization_and_both_writers_reach_the_same_fixed_point() {
        for previous in [
            dotfiles(),
            app(),
            command(&["foreign"]),
            wrap(app(), PREVIOUS, &command(&["foreign"])),
        ] {
            let canonical = normalize(previous, TOOL);
            assert_eq!(normalize(canonical.clone(), TOOL), canonical);
            assert_eq!(
                normalize(wrap(dotfiles(), FORWARD, &canonical), TOOL),
                canonical
            );
            if kind(&canonical) == Kind::App {
                assert_eq!(
                    normalize(wrap(app(), PREVIOUS, &canonical), TOOL),
                    canonical
                );
            } else {
                let with_app = normalize(wrap(app(), PREVIOUS, &canonical), TOOL);
                assert_eq!(normalize(with_app.clone(), TOOL), with_app);
            }
        }
    }

    #[test]
    fn json_vectors_round_trip_escapes_unicode_and_argument_bounds() {
        let words = command(&[
            "quoted\"\\path",
            "\u{0}\u{8}\u{c}\n\r\t",
            "日本語😀<>&\u{2028}\u{2029}",
        ]);
        assert_eq!(decode(&encode(&words)), Some(words));
        assert_eq!(decode(r#"["\ud83d\ude00"]"#), Some(command(&["😀"])));
        for invalid in [
            r#"["\udc00"]"#,
            r#"["\ud800\u0041"]"#,
            r#"["\q"]"#,
            "[\"line\nfeed\"]",
            "[\"x\"]false",
            "[\"x\",1]",
            "\u{c}[]",
        ] {
            assert_eq!(decode(invalid), None);
        }
        let maximum = Command(vec!["x".into(); MAX_ARGUMENTS]);
        assert_eq!(decode(&encode(&maximum)), Some(maximum));
        assert_eq!(
            decode(&encode(&Command(vec!["x".into(); MAX_ARGUMENTS + 1]))),
            None
        );
    }

    #[test]
    fn forwarding_refuses_direct_indirect_windows_and_duplicate_cycles() {
        let windows = command(&[
            r"C:\synthetic\dotfiles.exe",
            "notifications",
            "agent",
            "codex",
        ]);
        for unsafe_command in [
            dotfiles(),
            windows,
            wrap(app(), PREVIOUS, &dotfiles()),
            wrap(command(&["foreign"]), FORWARD, &command(&["foreign"])),
        ] {
            assert!(forwarding(&encode(&unsafe_command)).is_none());
        }
        for valid in [
            command(&["third-notifier", "argument"]),
            app(),
            wrap(app(), PREVIOUS, &command(&["third-notifier"])),
        ] {
            assert_eq!(forwarding(&encode(&valid)), Some(valid));
        }
        assert!(forwarding(r#"[""]"#).is_none());
    }

    #[test]
    fn bounded_chain_model_checks_every_four_node_chain_and_truncates_the_plan() {
        for first in 0..4 {
            for second in 0..4 {
                for third in 0..4 {
                    for fourth in 0..4 {
                        for count in 0..=4 {
                            for complete in [false, true] {
                                model::contract([first, second, third, fourth], count, complete);
                            }
                        }
                    }
                }
            }
        }
        for app in [false, true] {
            for first in [false, true] {
                for second in [false, true] {
                    for reversed in [false, true] {
                        model::idempotent([app, first, second, reversed]);
                    }
                }
            }
        }
        let links = vec![model::link(1); MAX_DEPTH + 1];
        assert_eq!(plan::<_, MAX_DEPTH>(&links).len, 2);
        assert!(!safe(&links, false));
    }
}
