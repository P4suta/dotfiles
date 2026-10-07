/// A chezmoi source entry type, which decides the attribute prefixes its name may carry.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Entry {
    File,
    Directory,
}

/// The target name of a source entry that chezmoi copies verbatim: an optional leading dot and the remaining name.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Target<'a> {
    pub dot: bool,
    pub name: &'a [u8],
}

const FILE_ATTRIBUTES: [&[u8]; 4] = [b"private_", b"readonly_", b"empty_", b"executable_"];
const DIRECTORY_ATTRIBUTES: [&[u8]; 3] = [b"exact_", b"private_", b"readonly_"];
const UNCOPIED_PREFIXES: [&[u8]; 8] = [
    b"create_",
    b"modify_",
    b"remove_",
    b"run_",
    b"symlink_",
    b"encrypted_",
    b"external_",
    b"literal_",
];
const UNCOPIED_SUFFIXES: [&[u8]; 2] = [b".tmpl", b".literal"];

/// The rest of `text` after `prefix`, or `None` when `text` doesn't start with it.
/// Each walk compares one byte per step from its end, which keeps the proof free of iterators and offsets.
fn after<'a>(mut text: &'a [u8], mut prefix: &[u8]) -> Option<&'a [u8]> {
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

fn starts_with(text: &[u8], prefix: &[u8]) -> bool {
    after(text, prefix).is_some()
}

fn ends_with(mut text: &[u8], mut suffix: &[u8]) -> bool {
    while let [rest @ .., expected] = suffix {
        let [head @ .., actual] = text else {
            return false;
        };
        if actual != expected {
            return false;
        }
        (text, suffix) = (head, rest);
    }
    true
}

/// Whether `text` starts with any of `prefixes`.
fn any_prefix(text: &[u8], mut prefixes: &[&[u8]]) -> bool {
    while let [prefix, rest @ ..] = prefixes {
        if starts_with(text, prefix) {
            return true;
        }
        prefixes = rest;
    }
    false
}

/// Whether `text` ends with any of `suffixes`.
fn any_suffix(text: &[u8], mut suffixes: &[&[u8]]) -> bool {
    while let [suffix, rest @ ..] = suffixes {
        if ends_with(text, suffix) {
            return true;
        }
        suffixes = rest;
    }
    false
}

/// The target name chezmoi gives a source entry that it copies byte for byte.
/// A template, script, symlink, modification, encrypted, or literal name has no byte-identical target, so it has no answer here.
#[must_use]
pub fn target(entry: Entry, source: &[u8]) -> Option<Target<'_>> {
    if any_prefix(source, &UNCOPIED_PREFIXES) || any_suffix(source, &UNCOPIED_SUFFIXES) {
        return None;
    }
    let mut attributes: &[&[u8]] = match entry {
        Entry::File => &FILE_ATTRIBUTES,
        Entry::Directory => &DIRECTORY_ATTRIBUTES,
    };
    let mut name = source;
    while let [attribute, rest @ ..] = attributes {
        if let Some(stripped) = after(name, attribute) {
            name = stripped;
        }
        attributes = rest;
    }
    let rest = after(name, b"dot_");
    let dot = rest.is_some();
    if let Some(rest) = rest {
        name = rest;
    }
    if name.is_empty() {
        None
    } else {
        Some(Target { dot, name })
    }
}

/// The properties Kani proves for every input and the tests check over every input they list.
#[cfg(any(test, kani))]
mod proofs {
    use super::*;

    /// A target name forms a nonempty end of its source name, and templates and scripts have none.
    pub fn spec_target(entry: Entry, source: &[u8]) {
        let result = target(entry, source);
        if let Some(found) = result {
            assert!(!found.name.is_empty() && ends_with(source, found.name));
            assert!(found.name.len() <= source.len());
        }
        if ends_with(source, b".tmpl") || starts_with(source, b"run_") {
            assert!(result.is_none());
        }
    }

    #[cfg(kani)]
    fn names_have_targets(entry: Entry) {
        let bytes: [u8; 9] = kani::any();
        let length: usize = kani::any();
        kani::assume(length <= bytes.len());
        let source = &bytes[..length];
        spec_target(entry, source);
        let result = target(entry, source);
        kani::cover!(result.is_some_and(|found| found.name.len() == source.len()));
        kani::cover!(result.is_none() && ends_with(source, b".tmpl"));
    }

    /// Each entry type gets its own harness, so each stays well inside the proof budget.
    #[cfg(kani)]
    #[kani::proof]
    #[kani::unwind(16)]
    fn a_file_target_name_is_a_nonempty_suffix_of_its_source_name_and_never_a_template() {
        names_have_targets(Entry::File);
    }

    #[cfg(kani)]
    #[kani::proof]
    #[kani::unwind(16)]
    fn a_directory_target_name_is_a_nonempty_suffix_of_its_source_name_and_never_a_template() {
        names_have_targets(Entry::Directory);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[cfg_attr(miri, ignore = "Miri takes minutes over every name")]
    fn every_short_name_over_the_attribute_letters_meets_the_specification() {
        for source in crate::domain::words(b"dot_.l", 6) {
            for entry in [Entry::File, Entry::Directory] {
                proofs::spec_target(entry, &source);
                if let Some(found) = target(entry, &source) {
                    let before = source.len() - found.name.len();
                    assert_eq!(
                        found.dot,
                        before >= 4 && source.get(before - 4..before) == Some(&b"dot_"[..]),
                        "{source:?}"
                    );
                }
            }
        }
    }

    #[test]
    fn a_name_that_is_exactly_a_prefix_or_suffix_has_no_target() {
        for source in [&b".tmpl"[..], b".literal", b"run_", b"dot_", b"private_"] {
            assert_eq!(target(Entry::File, source), None, "{source:?}");
            assert_eq!(target(Entry::Directory, source), None, "{source:?}");
        }
        assert_eq!(target(Entry::Directory, b"exact_"), None);
        assert_eq!(
            target(Entry::File, b"exact_"),
            Some(Target {
                dot: false,
                name: b"exact_"
            })
        );
        assert_eq!(
            target(Entry::File, b"x.tmplx"),
            Some(Target {
                dot: false,
                name: b"x.tmplx"
            })
        );
        assert_eq!(
            target(Entry::File, b"readonly_empty_dot_x"),
            Some(Target {
                dot: true,
                name: b"x"
            })
        );
        assert_eq!(
            target(Entry::Directory, b"exact_private_readonly_x"),
            Some(Target {
                dot: false,
                name: b"x"
            })
        );
    }

    #[test]
    fn an_empty_file_is_named_without_its_attribute() {
        assert_eq!(
            target(Entry::File, b"empty_vocab.txt"),
            Some(Target {
                dot: false,
                name: b"vocab.txt"
            })
        );
    }

    #[test]
    fn attributes_strip_only_in_chezmoi_order() {
        assert_eq!(
            target(Entry::File, b"private_executable_dot_tool"),
            Some(Target {
                dot: true,
                name: b"tool"
            })
        );
        assert_eq!(
            target(Entry::File, b"executable_private_tool"),
            Some(Target {
                dot: false,
                name: b"private_tool"
            })
        );
        assert_eq!(
            target(Entry::Directory, b"exact_dot_config"),
            Some(Target {
                dot: true,
                name: b"config"
            })
        );
        assert_eq!(
            target(Entry::Directory, b"empty_styles"),
            Some(Target {
                dot: false,
                name: b"empty_styles"
            })
        );
    }

    #[test]
    fn entries_without_a_verbatim_copy_have_no_target() {
        for source in [
            &b"vale.ini.tmpl"[..],
            b"run_onchange_x.sh",
            b"symlink_link",
            b"modify_config",
            b"literal_dot_x",
            b"empty_",
            b"dot_",
        ] {
            assert_eq!(target(Entry::File, source), None, "{source:?}");
        }
        assert_eq!(target(Entry::Directory, b"remove_old"), None);
    }
}
