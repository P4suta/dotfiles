#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Verdict {
    Admit,
    Refuse,
}

/// Only named read-only subcommands may run in an agent shell.
#[must_use]
pub fn verdict(arguments: &[&str]) -> Verdict {
    match arguments {
        [
            "status" | "ping" | "netcheck" | "ip" | "whois" | "version" | "metrics" | "bugreport",
            ..,
        ]
        | ["dns" | "lock", "status", ..]
        | ["exit-node", "list", ..]
        | [
            "debug",
            "derp-map"
            | "daemon-goroutines"
            | "daemon-logs"
            | "daemon-bus-events"
            | "daemon-bus-graph"
            | "daemon-bus-queues"
            | "metrics"
            | "env"
            | "stat"
            | "hostinfo"
            | "control-knobs"
            | "prefs"
            | "watch-ipn"
            | "netmap"
            | "via"
            | "peer-endpoint-changes"
            | "resolve"
            | "go-buildinfo"
            | "peer-relay-servers"
            | "statedir",
            ..,
        ] => Verdict::Admit,
        _ => Verdict::Refuse,
    }
}

pub const NEXT_ACTION: &str =
    "ask the owner to run it; a tailscale command can open a sign-in window on the owner's desktop";

#[cfg(any(test, kani))]
mod proofs {
    use super::*;

    const READ_ONLY: &[&str] = &[
        "status",
        "ping",
        "netcheck",
        "ip",
        "whois",
        "version",
        "metrics",
        "bugreport",
    ];
    const DEBUG_READ_ONLY: &[&str] = &[
        "derp-map",
        "daemon-goroutines",
        "daemon-logs",
        "daemon-bus-events",
        "daemon-bus-graph",
        "daemon-bus-queues",
        "metrics",
        "env",
        "stat",
        "hostinfo",
        "control-knobs",
        "prefs",
        "watch-ipn",
        "netmap",
        "via",
        "peer-endpoint-changes",
        "resolve",
        "go-buildinfo",
        "peer-relay-servers",
        "statedir",
    ];
    pub const WORDS: &[&str] = &[
        "status",
        "ping",
        "netcheck",
        "ip",
        "whois",
        "version",
        "metrics",
        "bugreport",
        "dns",
        "exit-node",
        "list",
        "lock",
        "debug",
        "derp-map",
        "daemon-goroutines",
        "daemon-logs",
        "daemon-bus-events",
        "daemon-bus-graph",
        "daemon-bus-queues",
        "env",
        "stat",
        "hostinfo",
        "control-knobs",
        "prefs",
        "watch-ipn",
        "netmap",
        "via",
        "peer-endpoint-changes",
        "resolve",
        "go-buildinfo",
        "peer-relay-servers",
        "statedir",
        "up",
        "down",
        "login",
        "logout",
        "set",
        "switch",
        "ssh",
        "serve",
        "funnel",
        "cert",
        "file",
        "drive",
        "web",
        "configure",
        "update",
        "syspolicy",
        "init",
        "add",
        "remove",
        "sign",
        "disable",
        "log",
        "rebind",
        "restun",
        "localapi",
        "local-creds",
        "component-logs",
        "set-expire",
        "reload-config",
        "--file=delete:x",
        "--help",
        "unknown",
        "",
    ];

    pub fn spec_verdict(arguments: &[&str]) -> bool {
        let expected = match arguments.first() {
            Some(&"dns" | &"lock") => arguments.get(1) == Some(&"status"),
            Some(&"exit-node") => arguments.get(1) == Some(&"list"),
            Some(&"debug") => arguments
                .get(1)
                .is_some_and(|word| DEBUG_READ_ONLY.contains(word)),
            Some(word) => READ_ONLY.contains(word),
            None => false,
        };
        let result = verdict(arguments);
        assert_eq!(result == Verdict::Admit, expected);
        result == Verdict::Admit
    }

    #[cfg(kani)]
    #[kani::proof]
    #[kani::unwind(32)]
    fn bounded_words_admit_only_read_only_subcommands() {
        let first = usize::from(kani::any::<u8>());
        let second = usize::from(kani::any::<u8>());
        let length = usize::from(kani::any::<u8>());
        kani::assume(first < WORDS.len() && second < WORDS.len() && length <= 3);
        if let (Some(&first), Some(&second)) = (WORDS.get(first), WORDS.get(second)) {
            let words = [first, second, "up"];
            if let Some(arguments) = words.get(..length) {
                let admitted = spec_verdict(arguments);
                kani::cover!(admitted);
                kani::cover!(!admitted);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_bounded_word_pair_meets_the_specification() {
        proofs::spec_verdict(&[]);
        for first in proofs::WORDS {
            proofs::spec_verdict(&[first]);
            for second in proofs::WORDS {
                proofs::spec_verdict(&[first, second]);
                proofs::spec_verdict(&[first, second, "up"]);
            }
        }
    }

    #[test]
    fn group_names_never_authorize_mutating_children() {
        for group in ["dns", "debug", "exit-node", "lock"] {
            assert_eq!(verdict(&[group]), Verdict::Refuse);
            for child in ["up", "set", "ssh", "unknown", ""] {
                assert_eq!(verdict(&[group, child]), Verdict::Refuse);
            }
        }
    }

    #[test]
    fn refusal_explains_the_owner_action_and_desktop_sign_in() {
        assert_eq!(
            NEXT_ACTION,
            "ask the owner to run it; a tailscale command can open a sign-in window on the owner's desktop"
        );
    }
}
