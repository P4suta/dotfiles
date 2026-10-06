//! Windows refuses to replace or delete a running executable but lets a process rename it.
//! An installer renames the current binary to an aside name, moves the new one in, and deletes aside copies on its next run.

/// The aside name of `name` for the installing process `pid`: `<name>.<pid>.old`.
pub fn aside_name(name: &str, pid: u32) -> String {
    format!("{name}.{pid}.old")
}

const SUFFIX: &[u8] = b".old";

/// Whether `entry` names an aside copy of `name`: `name`, a dot, one or more decimal digits, and `.old`.
pub fn is_aside(name: &[u8], entry: &[u8]) -> bool {
    let fixed = name.len() + 1 + SUFFIX.len();
    if entry.len() <= fixed {
        return false;
    }
    let mut index = 0;
    while index < name.len() {
        if entry[index] != name[index] {
            return false;
        }
        index += 1;
    }
    if entry[name.len()] != b'.' {
        return false;
    }
    let digits_end = entry.len() - SUFFIX.len();
    index = name.len() + 1;
    while index < digits_end {
        if !entry[index].is_ascii_digit() {
            return false;
        }
        index += 1;
    }
    index = 0;
    while index < SUFFIX.len() {
        if entry[digits_end + index] != SUFFIX[index] {
            return false;
        }
        index += 1;
    }
    true
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
    let aside = is_aside(name, entry);
    assert!(aside);
    assert!(!is_aside(name, name));
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
    let aside = is_aside(name, entry);
    if aside {
        assert!(entry_length > length + 5);
        let mut position = 0;
        while position < entry_length {
            let byte = entry[position];
            let expected = if position < length {
                byte == name[position]
            } else if position == length {
                byte == b'.'
            } else if position + 4 < entry_length {
                byte.is_ascii_digit()
            } else {
                byte == SUFFIX[position + 4 - entry_length]
            };
            assert!(expected);
            position += 1;
        }
    }
    kani::cover!(aside);
    kani::cover!(!aside && entry_length == bytes.len());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_aside_name_is_recognised_and_other_files_are_not() {
        let name = aside_name("dotfiles-xtask.exe", 4242);
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
