//! Windows refuses to replace or delete a running executable but lets a process rename it.
//! An installer renames the current binary to an aside name, moves the new one in, and deletes aside copies on its next run.

/// The aside name of a binary for one installing process, which displays as `<name>.<pid>.old`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AsideName<'a> {
    name: &'a str,
    pid: u32,
}

impl core::fmt::Display for AsideName<'_> {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(formatter, "{}.{}.old", self.name, self.pid)
    }
}

/// The aside name of `name` for the installing process `pid`.
///
/// Exempt as no-decision: it pairs its two arguments without a branch.
/// Evidence: `crates/rules/src/aside_rules.rs::tests::the_aside_name_is_recognised_and_other_files_are_not`.
/// Reconsider when it gains a branch.
#[must_use]
pub const fn aside_name(name: &str, pid: u32) -> AsideName<'_> {
    AsideName { name, pid }
}

const SUFFIX: &[u8] = b".old";

/// Whether `entry` names an aside copy of `name`: `name`, a dot, one or more decimal digits, and `.old`.
#[must_use]
pub fn is_aside(name: &[u8], entry: &[u8]) -> bool {
    let Some([b'.', middle @ ..]) = after_prefix(entry, name) else {
        return false;
    };
    let Some(digits) = before_suffix(middle, SUFFIX) else {
        return false;
    };
    !digits.is_empty() && only_digits(digits)
}

/// The rest of `text` after `prefix`, walked byte by byte so the proof stays free of iterators.
fn after_prefix<'a>(mut text: &'a [u8], mut prefix: &[u8]) -> Option<&'a [u8]> {
    while let [expected, rest @ ..] = prefix {
        let [actual, tail @ ..] = text else {
            return None;
        };
        if actual != expected {
            return None;
        }
        (text, prefix) = (tail, rest);
    }
    Some(text)
}

/// The start of `text` before `suffix`.
fn before_suffix<'a>(mut text: &'a [u8], mut suffix: &[u8]) -> Option<&'a [u8]> {
    while let [rest @ .., expected] = suffix {
        let [head @ .., actual] = text else {
            return None;
        };
        if actual != expected {
            return None;
        }
        (text, suffix) = (head, rest);
    }
    Some(text)
}

fn only_digits(mut text: &[u8]) -> bool {
    while let [byte, rest @ ..] = text {
        if !byte.is_ascii_digit() {
            return false;
        }
        text = rest;
    }
    true
}

/// The properties Kani proves for every input and the tests check over every input they list.
#[cfg(any(test, kani))]
mod proofs {
    use super::*;

    /// An entry counts as aside exactly when it spells `name`, a dot, at least one digit, and the suffix.
    pub fn spec_is_aside(name: &[u8], entry: &[u8]) {
        let mut expected = entry.len() > name.len() + 1 + SUFFIX.len();
        let mut position = 0;
        while expected && position < entry.len() {
            let byte = entry.get(position);
            expected = if position < name.len() {
                byte == name.get(position)
            } else if position == name.len() {
                byte == Some(&b'.')
            } else if position + SUFFIX.len() < entry.len() {
                byte.is_some_and(u8::is_ascii_digit)
            } else {
                byte == SUFFIX.get(position + SUFFIX.len() - entry.len())
            };
            position += 1;
        }
        assert_eq!(is_aside(name, entry), expected);
        assert!(!is_aside(name, name));
    }

    #[cfg(kani)]
    fn any_name() -> ([u8; 2], usize) {
        let bytes: [u8; 2] = kani::any();
        let mut index = 0;
        while index < bytes.len() {
            kani::assume(bytes[index] == b'a' || bytes[index] == b'.' || bytes[index] == b'1');
            index += 1;
        }
        let length: usize = kani::any();
        kani::assume(length <= bytes.len());
        (bytes, length)
    }

    #[cfg(kani)]
    #[kani::proof]
    #[kani::unwind(10)]
    fn an_aside_copy_is_recognised_and_its_binary_is_not() {
        let (name, length) = any_name();
        let name = &name[..length];
        let digit: u8 = kani::any();
        kani::assume(digit.is_ascii_digit());
        let mut entry = [0u8; 8];
        let mut index = 0;
        while index < name.len() {
            entry[index] = name[index];
            index += 1;
        }
        entry[length] = b'.';
        entry[length + 1] = digit;
        index = 0;
        while index < SUFFIX.len() {
            entry[length + 2 + index] = SUFFIX[index];
            index += 1;
        }
        let entry = &entry[..length + 6];
        spec_is_aside(name, entry);
        let aside = is_aside(name, entry);
        assert!(aside);
        kani::cover!(aside && length == 2);
        kani::cover!(aside && length == 0);
    }

    #[cfg(kani)]
    #[kani::proof]
    #[kani::unwind(10)]
    fn only_entries_named_after_the_binary_are_swept() {
        let (name, length) = any_name();
        let name = &name[..length];
        let bytes: [u8; 7] = kani::any();
        let mut index = 0;
        while index < bytes.len() {
            let byte = bytes[index];
            kani::assume(matches!(byte, b'a' | b'.' | b'1' | b'o' | b'l' | b'd'));
            index += 1;
        }
        let entry_length: usize = kani::any();
        kani::assume(entry_length <= bytes.len());
        let entry = &bytes[..entry_length];
        spec_is_aside(name, entry);
        let aside = is_aside(name, entry);
        kani::cover!(aside);
        kani::cover!(!aside && entry_length == bytes.len());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::words;
    use alloc::string::ToString;

    #[test]
    #[cfg_attr(miri, ignore = "Miri takes minutes over every word")]
    fn every_short_name_and_entry_meets_the_aside_specification() {
        let entries = words(b"a.1old", 7);
        for name in words(b"a.1", 2) {
            for entry in &entries {
                proofs::spec_is_aside(&name, entry);
            }
        }
    }

    #[test]
    fn the_aside_name_is_recognised_and_other_files_are_not() {
        let name = aside_name("dotfiles-xtask.exe", 4242).to_string();
        assert_eq!(name, "dotfiles-xtask.exe.4242.old");
        assert!(is_aside(b"dotfiles-xtask.exe", name.as_bytes()));
        assert!(!is_aside(b"dotfiles-xtask.exe", b"dotfiles-xtask.exe"));
        assert!(!is_aside(b"dotfiles-xtask.exe", b"dotfiles-xtask.exe..old"));
        assert!(!is_aside(
            b"dotfiles-xtask.exe",
            b"dotfiles-xtask.exe.x1.old"
        ));
        assert!(!is_aside(b"dotguard.exe", name.as_bytes()));
        assert!(is_aside(b"git.exe", b"git.exe.12.old"));
    }
}
