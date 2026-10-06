//! Edits Windows' `sshd_config` in place.
//!
//! The OpenSSH capability owns the file and creates it on the first sshd start, so this tool edits it instead of rendering a template.
//!
//! A directive after a `Match` line belongs to that block.
//! The stock config ends with `Match Group administrators`, which routes administrator logins to `administrators_authorized_keys`, so every edit lands before the first `Match`.

/// Marks directives that this tool appended.
const MARKER: &str = "# --- managed by dotfiles-win ---";

#[derive(Debug, PartialEq, Eq)]
enum Kind {
    /// `Port 22`, in force.
    Active,
    /// `#Port 22`, the stock file's commented default.
    Commented,
    Other,
}

/// sshd keywords ignore case, and the stock file writes commented defaults with no space after the `#`.
fn classify(line: &str, name: &str) -> Kind {
    let trimmed = line.trim_start();
    let (body, commented) = match trimmed.strip_prefix('#') {
        Some(rest) => (rest.trim_start(), true),
        None => (trimmed, false),
    };

    let Some(rest) = body.get(..name.len()) else {
        return Kind::Other;
    };
    if !rest.eq_ignore_ascii_case(name) {
        return Kind::Other;
    }
    // A keyword needs whitespace and a value, so `PortForwarding` differs from `Port`, and a bare `Port` sets nothing.
    let after = &body[name.len()..];
    if !after.starts_with(char::is_whitespace) || after.trim().is_empty() {
        return Kind::Other;
    }
    if commented {
        Kind::Commented
    } else {
        Kind::Active
    }
}

/// Applies `directives` to `original`.
///
pub fn apply(original: &str, directives: &[(&str, String)]) -> String {
    let lines: Vec<&str> = original
        .split('\n')
        .map(|l| l.trim_end_matches('\r'))
        .collect();

    let match_index = lines
        .iter()
        .position(|line| {
            let trimmed = line.trim_start();
            trimmed
                .get(..5)
                .is_some_and(|keyword| keyword.eq_ignore_ascii_case("match"))
                && trimmed
                    .get(5..)
                    .is_some_and(|rest| rest.starts_with(char::is_whitespace))
        })
        .unwrap_or(lines.len());

    let mut head: Vec<String> = lines[..match_index]
        .iter()
        .map(|l| (*l).to_owned())
        .collect();
    let tail: Vec<String> = lines[match_index..]
        .iter()
        .map(|l| (*l).to_owned())
        .collect();

    for (name, value) in directives {
        let desired = format!("{name} {value}");

        let active: Vec<usize> = head
            .iter()
            .enumerate()
            .filter(|(_, line)| classify(line, name) == Kind::Active)
            .map(|(index, _)| index)
            .collect();

        if let Some(&first) = active.first() {
            head[first] = desired;
            // sshd honors the first occurrence, so later ones do nothing.
            for &index in active.iter().skip(1).rev() {
                head.remove(index);
            }
            continue;
        }

        if let Some(index) = head
            .iter()
            .position(|line| classify(line, name) == Kind::Commented)
        {
            // Replace the commented default in place, keeping its context.
            head[index] = desired;
            continue;
        }

        if !head.iter().any(|line| line == MARKER) {
            head.push(MARKER.to_owned());
        }
        head.push(desired);
    }

    let mut out: Vec<String> = head;
    out.extend(tail);
    format!("{}\r\n", out.join("\r\n").trim_end())
}

/// True when the config still routes administrator logins to `administrators_authorized_keys`.
/// Without this block, sshd never reads keys written there.
pub fn has_admin_match_block(config: &str) -> bool {
    config.lines().any(|line| {
        let mut words = line.split_whitespace();
        matches!(
            (words.next(), words.next(), words.next(), words.next()),
            (Some(a), Some(b), Some(c), None)
                if a.eq_ignore_ascii_case("match")
                    && b.eq_ignore_ascii_case("group")
                    && c.eq_ignore_ascii_case("administrators")
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The layout Windows ships, with every directive commented out.
    const STOCK: &str = "#\tThis is the sshd server system-wide configuration file.\r
\r
#Port 22\r
#AddressFamily any\r
#ListenAddress 0.0.0.0\r
\r
#PubkeyAuthentication yes\r
\r
# To disable tunneled clear text passwords, change to no here!\r
#PasswordAuthentication yes\r
#PermitEmptyPasswords no\r
\r
Subsystem\tsftp\tsftp-server.exe\r
\r
Match Group administrators\r
       AuthorizedKeysFile __PROGRAMDATA__/ssh/administrators_authorized_keys\r
";

    fn wanted(port: u16, password_auth: bool) -> Vec<(&'static str, String)> {
        vec![
            ("Port", port.to_string()),
            ("PubkeyAuthentication", "yes".to_owned()),
            (
                "PasswordAuthentication",
                if password_auth { "yes" } else { "no" }.to_owned(),
            ),
        ]
    }

    fn lines(text: &str) -> Vec<&str> {
        text.split("\r\n").collect()
    }

    fn index_of(text: &str, needle: &str) -> usize {
        lines(text)
            .iter()
            .position(|line| line.starts_with(needle))
            .unwrap_or_else(|| panic!("{needle} not found in:\n{text}"))
    }

    #[test]
    fn stock_defaults_are_uncommented_in_place() {
        let out = apply(STOCK, &wanted(22, false));
        let out_lines = lines(&out);

        assert!(out_lines.contains(&"Port 22"));
        assert!(out_lines.contains(&"PubkeyAuthentication yes"));
        assert!(out_lines.contains(&"PasswordAuthentication no"));
        assert!(!out_lines.contains(&"#Port 22"));
        // In place, so the comment still precedes PasswordAuthentication.
        assert_eq!(
            out_lines[index_of(&out, "PasswordAuthentication") - 1],
            "# To disable tunneled clear text passwords, change to no here!"
        );
        // Nothing appended, so no marker.
        assert!(!out.contains(MARKER));
    }

    #[test]
    fn edits_stay_above_the_match_block() {
        let out = apply(STOCK, &wanted(22, false));
        assert!(index_of(&out, "PasswordAuthentication") < index_of(&out, "Match Group"));
        assert!(out.contains(
            "       AuthorizedKeysFile __PROGRAMDATA__/ssh/administrators_authorized_keys"
        ));
    }

    #[test]
    fn a_second_pass_changes_nothing() {
        let once = apply(STOCK, &wanted(22, false));
        let twice = apply(&once, &wanted(22, false));
        assert_eq!(once, twice);
    }

    #[test]
    fn a_changed_port_and_password_auth_are_rewritten() {
        let once = apply(STOCK, &wanted(22, false));
        let out = apply(&once, &wanted(2222, true));
        assert!(lines(&out).contains(&"Port 2222"));
        assert!(lines(&out).contains(&"PasswordAuthentication yes"));
        assert_eq!(
            lines(&out)
                .iter()
                .filter(|l| l.starts_with("Port "))
                .count(),
            1
        );
    }

    /// sshd honors the first occurrence, so duplicates add noise, but a duplicate inside the Match block belongs to that block and must stay.
    #[test]
    fn duplicates_collapse_without_touching_the_match_block() {
        let hostile = "Port 22\r\nPasswordAuthentication yes\r\nPort 2200\r\n\
                       PasswordAuthentication yes\r\nMatch Group administrators\r\n\
                       \x20      PasswordAuthentication yes\r\n";
        let out = apply(hostile, &wanted(22, false));

        assert_eq!(
            lines(&out)
                .iter()
                .filter(|l| l.starts_with("Port "))
                .count(),
            1
        );
        assert_eq!(
            lines(&out)
                .iter()
                .filter(|l| l.starts_with("PasswordAuthentication"))
                .count(),
            1
        );
        assert!(lines(&out).contains(&"       PasswordAuthentication yes"));
    }

    #[test]
    fn a_config_starting_with_match_gets_the_directives_above_it() {
        let config = "Match Group administrators\r\n       AuthorizedKeysFile x\r\n";
        let out = apply(config, &wanted(22, false));

        assert!(index_of(&out, "Port 22") < index_of(&out, "Match Group"));
        assert!(out.contains(MARKER));
    }

    #[test]
    fn a_config_without_a_match_block_keeps_its_own_directives() {
        let config = "#Port 22\r\nX11Forwarding no\r\n";
        let out = apply(config, &wanted(22, false));

        assert!(lines(&out).contains(&"Port 22"));
        assert!(lines(&out).contains(&"X11Forwarding no"));
    }

    /// `PortForwarding` starts with `Port`, and a keyword with no value sets nothing.
    #[test]
    fn a_longer_keyword_is_not_the_keyword() {
        assert_eq!(classify("PortForwarding yes", "Port"), Kind::Other);
        assert_eq!(classify("Port", "Port"), Kind::Other);
        assert_eq!(classify("Port   ", "Port"), Kind::Other);
        assert_eq!(classify("  port 22", "Port"), Kind::Active);
        assert_eq!(classify("#  PORT 22", "Port"), Kind::Commented);
    }

    #[test]
    fn lf_input_comes_back_as_crlf() {
        let out = apply("#Port 22\n#PubkeyAuthentication yes\n", &wanted(22, false));
        assert!(out.ends_with("\r\n"));
        assert!(!out.contains("\n\n"));
        assert!(lines(&out).contains(&"Port 22"));
    }

    #[test]
    fn the_admin_match_block_is_detected() {
        assert!(has_admin_match_block(STOCK));
        assert!(has_admin_block_variant("match group administrators"));
        assert!(!has_admin_block_variant("Match Group administrators extra"));
        assert!(!has_admin_block_variant("Match User someone"));
    }

    fn has_admin_block_variant(line: &str) -> bool {
        has_admin_match_block(&format!("Port 22\r\n{line}\r\n"))
    }
}
