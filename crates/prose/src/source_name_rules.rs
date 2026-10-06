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

const fn starts_with(text: &[u8], prefix: &[u8]) -> bool {
    if prefix.len() > text.len() {
        return false;
    }
    let mut index = 0;
    while index < prefix.len() {
        if text[index] != prefix[index] {
            return false;
        }
        index += 1;
    }
    true
}

const fn ends_with(text: &[u8], suffix: &[u8]) -> bool {
    if suffix.len() > text.len() {
        return false;
    }
    let offset = text.len() - suffix.len();
    let mut index = 0;
    while index < suffix.len() {
        if text[offset + index] != suffix[index] {
            return false;
        }
        index += 1;
    }
    true
}

/// The target name chezmoi gives a source entry that it copies byte for byte.
/// A template, script, symlink, modification, encrypted, or literal name has no byte-identical target, so it has no answer here.
#[must_use]
pub fn target(entry: Entry, source: &[u8]) -> Option<Target<'_>> {
    let mut index = 0;
    while index < UNCOPIED_PREFIXES.len() {
        if starts_with(source, UNCOPIED_PREFIXES[index]) {
            return None;
        }
        index += 1;
    }
    index = 0;
    while index < UNCOPIED_SUFFIXES.len() {
        if ends_with(source, UNCOPIED_SUFFIXES[index]) {
            return None;
        }
        index += 1;
    }
    let attributes: &[&[u8]] = match entry {
        Entry::File => &FILE_ATTRIBUTES,
        Entry::Directory => &DIRECTORY_ATTRIBUTES,
    };
    let mut name = source;
    index = 0;
    while index < attributes.len() {
        if starts_with(name, attributes[index]) {
            name = &name[attributes[index].len()..];
        }
        index += 1;
    }
    let dot = starts_with(name, b"dot_");
    if dot {
        name = &name[4..];
    }
    if name.is_empty() {
        None
    } else {
        Some(Target { dot, name })
    }
}

/// Each entry type gets its own harnesses, so each stays well inside the proof budget.
#[cfg(kani)]
fn a_target_name_is_a_nonempty_suffix_of_its_source_name(entry: Entry) {
    let bytes: [u8; 9] = kani::any();
    let length: usize = kani::any();
    kani::assume(length <= bytes.len());
    let source = &bytes[..length];
    let result = target(entry, source);
    if let Some(found) = result {
        assert!(!found.name.is_empty() && ends_with(source, found.name));
        assert!(found.name.len() <= source.len());
    }
    kani::cover!(result.is_some_and(|found| found.name.len() == source.len()));
    kani::cover!(result.is_none());
}

#[cfg(kani)]
fn templates_and_scripts_are_never_copied_targets(entry: Entry) {
    let bytes: [u8; 9] = kani::any();
    let length: usize = kani::any();
    kani::assume(length <= bytes.len());
    let source = &bytes[..length];
    let result = target(entry, source);
    if ends_with(source, b".tmpl") || starts_with(source, b"run_") {
        assert!(result.is_none());
    }
    kani::cover!(result.is_some());
    kani::cover!(result.is_none() && ends_with(source, b".tmpl"));
}

#[cfg(kani)]
#[kani::proof]
#[kani::unwind(16)]
fn a_file_target_name_is_a_nonempty_suffix_of_its_source_name() {
    a_target_name_is_a_nonempty_suffix_of_its_source_name(Entry::File);
}

#[cfg(kani)]
#[kani::proof]
#[kani::unwind(16)]
fn a_directory_target_name_is_a_nonempty_suffix_of_its_source_name() {
    a_target_name_is_a_nonempty_suffix_of_its_source_name(Entry::Directory);
}

#[cfg(kani)]
#[kani::proof]
#[kani::unwind(16)]
fn file_templates_and_scripts_are_never_copied_targets() {
    templates_and_scripts_are_never_copied_targets(Entry::File);
}

#[cfg(kani)]
#[kani::proof]
#[kani::unwind(16)]
fn directory_templates_and_scripts_are_never_copied_targets() {
    templates_and_scripts_are_never_copied_targets(Entry::Directory);
}

#[cfg(test)]
mod tests {
    use super::*;

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
