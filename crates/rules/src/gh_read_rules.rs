//! Which GitHub CLI invocations only read, so an answer may record their output as an observation.

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Verdict {
    Admit,
    Refuse,
}

/// The long `gh api` flags, without their dashes, that send a request body or parameters.
const BODY_FLAGS: [&[u8]; 3] = [b"field", b"raw-field", b"input"];
/// The only HTTP method an observation may send.
const READ_METHOD: &[u8] = b"GET";

/// Admits the named read subcommands and a `gh api` request that sends only GET without a body.
#[must_use]
pub fn verdict(arguments: &[&str]) -> Verdict {
    let admitted = match arguments {
        ["api", rest @ ..] => {
            rest.iter().fold(Reading::Open, |state, argument| {
                step(state, token(argument.as_bytes()))
            }) == Reading::Open
        }
        [command, action, ..] => named_read(command, action),
        _ => false,
    };
    if admitted {
        Verdict::Admit
    } else {
        Verdict::Refuse
    }
}

fn named_read(command: &str, action: &str) -> bool {
    matches!(
        (command, action),
        ("pr", "view" | "checks")
            | ("issue" | "repo" | "release", "view")
            | ("run", "view" | "list")
            | ("variable", "list")
    )
}

/// What one `gh api` argument does to the request.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(kani, derive(kani::Arbitrary))]
enum Token {
    /// A field or input flag, which sends a body.
    Body,
    /// A method flag whose value is the next argument.
    Method,
    /// The word GET in any case.
    Get,
    AttachedGet,
    AttachedOther,
    Other,
}

/// How much of a `gh api` call the arguments so far keep a read.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(kani, derive(kani::Arbitrary))]
enum Reading {
    Open,
    AwaitingMethod,
    Refused,
}

fn token(argument: &[u8]) -> Token {
    match argument {
        [b'-', b'-', long @ ..] => {
            if BODY_FLAGS.iter().any(|name| {
                long.strip_prefix(*name)
                    .is_some_and(|rest| matches!(rest, [] | [b'=', ..]))
            }) {
                Token::Body
            } else if long == b"method" {
                Token::Method
            } else if let Some(value) = long.strip_prefix(b"method=") {
                attached(value)
            } else {
                Token::Other
            }
        }
        [b'-', cluster @ ..] => {
            // `-i` and `-h` take no value, so a cluster may hold them before a flag that takes one.
            let mut flags = cluster;
            while let [b'i' | b'h', tail @ ..] = flags {
                flags = tail;
            }
            match flags {
                [b'f' | b'F', ..] => Token::Body,
                [b'X'] => Token::Method,
                [b'X', value @ ..] => attached(value),
                _ => Token::Other,
            }
        }
        value if value.eq_ignore_ascii_case(READ_METHOD) => Token::Get,
        _ => Token::Other,
    }
}

fn attached(method: &[u8]) -> Token {
    if method.eq_ignore_ascii_case(READ_METHOD) {
        Token::AttachedGet
    } else {
        Token::AttachedOther
    }
}

/// The value after a method flag must be GET, so a value that looks like a flag refuses instead of admitting.
const fn step(state: Reading, token: Token) -> Reading {
    match (state, token) {
        (Reading::Open, Token::Body | Token::AttachedOther)
        | (
            Reading::AwaitingMethod,
            Token::Body | Token::Method | Token::AttachedGet | Token::AttachedOther | Token::Other,
        )
        | (Reading::Refused, _) => Reading::Refused,
        (Reading::Open, Token::Method) => Reading::AwaitingMethod,
        (Reading::Open, Token::Get | Token::AttachedGet | Token::Other)
        | (Reading::AwaitingMethod, Token::Get) => Reading::Open,
    }
}

#[cfg(any(test, kani))]
mod proofs {
    use super::*;

    pub fn spec_step(state: Reading, token: Token) {
        let next = step(state, token);
        let expected = match state {
            Reading::Open => match token {
                Token::Body | Token::AttachedOther => Reading::Refused,
                Token::Method => Reading::AwaitingMethod,
                Token::Get | Token::AttachedGet | Token::Other => Reading::Open,
            },
            Reading::AwaitingMethod if token == Token::Get => Reading::Open,
            Reading::AwaitingMethod | Reading::Refused => Reading::Refused,
        };
        assert_eq!(next, expected);
    }

    fn get(value: &[u8]) -> bool {
        value.len() == 3
            && value
                .iter()
                .zip(b"get")
                .all(|(byte, lower)| byte.to_ascii_lowercase() == *lower)
    }

    fn attached_method(value: &[u8]) -> Token {
        if get(value) {
            Token::AttachedGet
        } else {
            Token::AttachedOther
        }
    }

    /// The token that `argument` should be, judged by its shape.
    pub fn spec_token(argument: &[u8]) {
        let expected = if let Some(long) = argument.strip_prefix(b"--") {
            let named = |name: &[u8]| {
                long == name
                    || long.len() > name.len()
                        && long.starts_with(name)
                        && long.get(name.len()) == Some(&b'=')
            };
            if named(b"field") || named(b"raw-field") || named(b"input") {
                Token::Body
            } else if long == b"method" {
                Token::Method
            } else if long.starts_with(b"method=") {
                attached_method(long.get(b"method=".len()..).unwrap_or_default())
            } else {
                Token::Other
            }
        } else if let Some(cluster) = argument.strip_prefix(b"-") {
            let start = cluster
                .iter()
                .position(|byte| *byte != b'i' && *byte != b'h')
                .unwrap_or(cluster.len());
            match cluster.get(start..).unwrap_or_default() {
                [first, ..] if *first == b'f' || *first == b'F' => Token::Body,
                [first] if *first == b'X' => Token::Method,
                [first, value @ ..] if *first == b'X' => attached_method(value),
                _ => Token::Other,
            }
        } else if get(argument) {
            Token::Get
        } else {
            Token::Other
        };
        assert_eq!(token(argument), expected);
    }

    pub const COMMANDS: [&str; 13] = [
        "api", "pr", "issue", "repo", "release", "run", "variable", "view", "checks", "list",
        "merge", "set", "-X",
    ];

    pub const LONG_WORDS: [&str; 18] = [
        "--method",
        "--method=GET",
        "--method=get",
        "--method=post",
        "--field",
        "--field=x",
        "--raw-field",
        "--raw-field=x",
        "--input",
        "--input=-",
        "--fields=x",
        "--jq",
        "-iXGET",
        "-iXPOST",
        "-hXPUT",
        "-ihfq=x",
        "-Fq=1",
        "repos/o/r",
    ];

    pub fn spec_verdict_of_a_pair(first: &str, second: &str) {
        let expected = if first == "api" {
            step(Reading::Open, token(second.as_bytes())) == Reading::Open
        } else {
            named_read(first, second)
        };
        assert_eq!(verdict(&[first, second]) == Verdict::Admit, expected);
    }

    #[cfg(kani)]
    #[kani::proof]
    fn a_body_or_another_method_refuses_the_rest_of_the_request() {
        let state: Reading = kani::any();
        let token: Token = kani::any();
        spec_step(state, token);
        kani::cover!(step(state, token) == Reading::Open);
        kani::cover!(step(state, token) == Reading::Refused);
    }

    #[cfg(kani)]
    #[kani::proof]
    #[kani::unwind(8)]
    fn every_short_argument_gets_the_token_of_its_shape() {
        let bytes: [u8; 5] = kani::any();
        let length: usize = kani::any();
        kani::assume(length <= bytes.len());
        if let Some(argument) = bytes.get(..length) {
            spec_token(argument);
            kani::cover!(token(argument) == Token::Body);
            kani::cover!(token(argument) == Token::AttachedOther);
        }
    }

    /// The longest tail a prefixed harness appends, which spells `=GET` after a method flag.
    pub const TAIL: usize = 4;

    /// Checks `prefix` followed by every tail of up to `TAIL` bytes, and returns the token of one such argument.
    #[cfg(kani)]
    fn prefixed(prefix: &[u8]) -> Option<Token> {
        let tail: [u8; TAIL] = kani::any();
        let length: usize = kani::any();
        kani::assume(length <= TAIL);
        let mut bytes = [0_u8; 16];
        let (head, rest) = bytes.split_at_mut(prefix.len());
        head.copy_from_slice(prefix);
        rest.get_mut(..TAIL)?.copy_from_slice(&tail);
        let argument = bytes.get(..prefix.len() + length)?;
        spec_token(argument);
        Some(token(argument))
    }

    #[cfg(kani)]
    #[kani::proof]
    #[kani::unwind(18)]
    fn a_long_method_flag_reads_its_attached_value() {
        let token = prefixed(b"--method");
        kani::cover!(token == Some(Token::Method));
        kani::cover!(token == Some(Token::AttachedGet));
        kani::cover!(token == Some(Token::AttachedOther));
    }

    #[cfg(kani)]
    #[kani::proof]
    #[kani::unwind(18)]
    fn a_field_flag_refuses_with_or_without_its_value() {
        let token = prefixed(b"--field");
        kani::cover!(token == Some(Token::Body));
        kani::cover!(token == Some(Token::Other));
    }

    #[cfg(kani)]
    #[kani::proof]
    #[kani::unwind(18)]
    fn a_raw_field_flag_refuses_with_or_without_its_value() {
        let token = prefixed(b"--raw-field");
        kani::cover!(token == Some(Token::Body));
        kani::cover!(token == Some(Token::Other));
    }

    #[cfg(kani)]
    #[kani::proof]
    #[kani::unwind(18)]
    fn an_input_flag_refuses_with_or_without_its_value() {
        let token = prefixed(b"--input");
        kani::cover!(token == Some(Token::Body));
        kani::cover!(token == Some(Token::Other));
    }

    #[cfg(kani)]
    #[kani::proof]
    #[kani::unwind(12)]
    fn a_flag_cluster_reads_its_attached_method() {
        let token = prefixed(b"-iX");
        kani::cover!(token == Some(Token::AttachedGet));
        kani::cover!(token == Some(Token::AttachedOther));
    }

    #[cfg(kani)]
    #[kani::proof]
    #[kani::unwind(12)]
    fn a_pair_is_admitted_only_as_a_named_read_or_a_reading_api_call() {
        let first = usize::from(kani::any::<u8>());
        let second = usize::from(kani::any::<u8>());
        kani::assume(first < COMMANDS.len() && second < COMMANDS.len());
        if let (Some(first), Some(second)) = (COMMANDS.get(first), COMMANDS.get(second)) {
            spec_verdict_of_a_pair(first, second);
            kani::cover!(verdict(&[first, second]) == Verdict::Admit);
            kani::cover!(verdict(&[first, second]) == Verdict::Refuse);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const READERS: [(&str, &str); 8] = [
        ("pr", "view"),
        ("pr", "checks"),
        ("issue", "view"),
        ("repo", "view"),
        ("release", "view"),
        ("run", "view"),
        ("run", "list"),
        ("variable", "list"),
    ];

    const FLAGS: [&str; 24] = [
        "repos/o/r",
        "-X",
        "--method",
        "GET",
        "get",
        "POST",
        "-XGET",
        "-XPOST",
        "-iXPOST",
        "-hXPUT",
        "--method=GET",
        "--method=post",
        "-f",
        "-F",
        "-if",
        "-Fq=1",
        "--field",
        "--raw-field",
        "--input=-",
        "--fields=x",
        "-i",
        "-H",
        "-pX",
        "",
    ];

    /// Whether `argument` sends a body, judged by its listed shape.
    fn sends_body(argument: &str) -> bool {
        matches!(
            argument,
            "-f" | "-F" | "-if" | "--field" | "--raw-field" | "--input"
        ) || [
            "-F",
            "-f",
            "-if",
            "-iF",
            "-hf",
            "-hF",
            "--field=",
            "--raw-field=",
            "--input=",
        ]
        .iter()
        .any(|prefix| argument.starts_with(prefix))
    }

    /// Whether a method flag in `arguments` sets anything but GET or lacks its value.
    fn sets_another_method(arguments: &[&str]) -> bool {
        let mut index = 0;
        while let Some(argument) = arguments.get(index) {
            let attached = ["--method=", "-X", "-iX", "-hX"]
                .iter()
                .find_map(|prefix| argument.strip_prefix(prefix))
                .filter(|value| !value.is_empty());
            let method = if let Some(value) = attached {
                Some(Some(value))
            } else if matches!(*argument, "--method" | "-X" | "-iX" | "-hX") {
                index += 1;
                Some(arguments.get(index).copied())
            } else {
                None
            };
            if method
                .is_some_and(|value| !value.is_some_and(|value| value.eq_ignore_ascii_case("GET")))
            {
                return true;
            }
            index += 1;
        }
        false
    }

    fn spec_verdict(arguments: &[&str]) {
        let expected = match arguments {
            ["api", rest @ ..] => {
                !(rest.iter().any(|argument| sends_body(argument)) || sets_another_method(rest))
            }
            [first, second, ..] => READERS.contains(&(*first, *second)),
            _ => false,
        };
        assert_eq!(
            verdict(arguments) == Verdict::Admit,
            expected,
            "{arguments:?}"
        );
    }

    #[test]
    fn every_state_and_token_meets_the_step_specification() {
        let states = [Reading::Open, Reading::AwaitingMethod, Reading::Refused];
        let tokens = [
            Token::Body,
            Token::Method,
            Token::Get,
            Token::AttachedGet,
            Token::AttachedOther,
            Token::Other,
        ];
        for state in states {
            for token in tokens {
                proofs::spec_step(state, token);
            }
        }
    }

    #[test]
    fn every_short_word_and_long_flag_meets_the_token_specification() {
        for word in crate::domain::words(b"-ihfFXGEgTt=x", 5) {
            proofs::spec_token(&word);
        }
        for word in proofs::LONG_WORDS {
            proofs::spec_token(word.as_bytes());
        }
        let tails = crate::domain::words(b"=GETgetPOSx-", proofs::TAIL);
        for prefix in [
            b"--method".as_slice(),
            b"--field",
            b"--raw-field",
            b"--input",
            b"-iX",
        ] {
            for tail in &tails {
                proofs::spec_token(&[prefix, tail.as_slice()].concat());
            }
        }
    }

    #[test]
    fn every_command_pair_meets_the_verdict_specification() {
        for first in proofs::COMMANDS {
            for second in proofs::COMMANDS {
                proofs::spec_verdict_of_a_pair(first, second);
            }
        }
    }

    #[test]
    fn every_bounded_api_call_meets_the_flag_specification() {
        spec_verdict(&[]);
        spec_verdict(&["api"]);
        for first in FLAGS {
            spec_verdict(&["api", first]);
            for second in FLAGS {
                spec_verdict(&["api", first, second]);
                for third in FLAGS {
                    spec_verdict(&["api", first, second, third]);
                }
            }
        }
        for (command, action) in READERS {
            spec_verdict(&[command, action, "--json", "x"]);
        }
    }

    #[test]
    fn a_body_or_another_method_refuses_a_longer_api_call() {
        for arguments in [
            &["api", "repos/o/r/issues", "-X", "POST"][..],
            &["api", "repos/o/r/issues", "-i", "-f", "title=x"],
            &[
                "api",
                "repos/o/r/issues",
                "-H",
                "Accept: x",
                "--input",
                "body.json",
            ],
            &["api", "repos/o/r", "--jq", ".x", "--method=PATCH"],
            &["api", "repos/o/r", "-ihXDELETE"],
        ] {
            assert_eq!(verdict(arguments), Verdict::Refuse, "{arguments:?}");
        }
        assert_eq!(
            verdict(&[
                "api",
                "repos/o/r/pulls",
                "-H",
                "Accept: x",
                "--jq",
                ".[].number",
                "--paginate"
            ]),
            Verdict::Admit
        );
    }
}
