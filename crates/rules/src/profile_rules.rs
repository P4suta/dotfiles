#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Profile {
    Mac,
    Linux,
    Windows,
    Wsl,
}

impl Profile {
    pub const ALL: [Self; 4] = [Self::Mac, Self::Linux, Self::Windows, Self::Wsl];

    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Mac => "mac",
            Self::Linux => "linux",
            Self::Windows => "windows",
            Self::Wsl => "wsl",
        }
    }

    #[must_use]
    pub const fn os(self) -> &'static str {
        match self {
            Self::Mac => "darwin",
            Self::Linux | Self::Wsl => "linux",
            Self::Windows => "windows",
        }
    }
}

/// The operating system a host reports.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum System {
    Mac,
    Linux,
    Windows,
}

/// The profile a host's operating system and kernel imply.
/// WSL runs Linux on a Microsoft kernel, the rule that chezmoi's Windows guide and the configuration template both apply.
#[must_use]
pub const fn detected(system: System, microsoft_kernel: bool) -> Profile {
    match system {
        System::Mac => Profile::Mac,
        System::Windows => Profile::Windows,
        System::Linux if microsoft_kernel => Profile::Wsl,
        System::Linux => Profile::Linux,
    }
}

/// Whether a Linux kernel release names Microsoft, which marks WSL.
#[must_use]
pub fn microsoft_kernel(osrelease: &[u8]) -> bool {
    let mut start: usize = 0;
    while let Some(rest) = osrelease.get(start..) {
        if starts_with_microsoft(rest) {
            return true;
        }
        start += 1;
    }
    false
}

/// The vendor name a WSL kernel release carries, in any letter case.
const MICROSOFT: &[u8] = b"microsoft";

/// Whether `text` starts with the vendor name in any letter case.
/// Indexing keeps the proof free of the iterator adapters that Kani can't reach.
fn starts_with_microsoft(text: &[u8]) -> bool {
    let mut index = 0;
    while let (Some(byte), Some(letter)) = (text.get(index), MICROSOFT.get(index)) {
        if byte.to_ascii_lowercase() != *letter {
            return false;
        }
        index += 1;
    }
    index == MICROSOFT.len()
}

#[cfg(kani)]
#[kani::proof]
#[kani::unwind(12)]
fn only_a_release_that_names_microsoft_marks_wsl() {
    const PLANTED: &[u8] = b"MicroSoft";
    let mut bytes: [u8; 10] = kani::any();
    let planted: bool = kani::any();
    if planted {
        let mut index = 0;
        while index < PLANTED.len() {
            bytes[index + 1] = PLANTED[index];
            index += 1;
        }
    }
    let result = microsoft_kernel(&bytes);
    if planted {
        assert!(result);
    }
    if result {
        assert!(bytes[0].to_ascii_lowercase() == b'm' || bytes[1].to_ascii_lowercase() == b'm');
    }
    kani::cover!(result && !planted);
    kani::cover!(!result);
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Mode {
    Preview,
    FilesOnly,
    Full,
}

#[must_use]
pub fn permitted(
    mode: Mode,
    native_matches: bool,
    owned_target: bool,
    live_authorized: bool,
) -> bool {
    match mode {
        Mode::Preview => true,
        Mode::FilesOnly => owned_target,
        Mode::Full => native_matches && live_authorized,
    }
}

#[must_use]
pub fn secret_ready(present: bool, nonempty: bool, unresolved_reference: bool) -> bool {
    present && nonempty && !unresolved_reference
}

#[must_use]
pub fn translate_file(file_argument: bool, inline_public_key: bool) -> bool {
    file_argument && !inline_public_key
}

#[must_use]
pub fn context7_needed(local_gui: bool, tool_exists: bool, installed: bool) -> bool {
    local_gui && tool_exists && !installed
}

/// GitHub publishes its web-flow signing keys as one bundle.
#[must_use]
pub fn forge_bundle_accepted(
    primary_count: usize,
    unpinned: usize,
    contains_current: bool,
) -> bool {
    primary_count > 0 && unpinned == 0 && contains_current
}

/// How a rendered client profile sets one persistent-memory switch.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Memory {
    Unset,
    Enabled,
    Disabled,
}

/// The longest switch list the memory proof covers.
/// Every client's list stays within it.
pub const MEMORY_SWITCHES_PROVED: usize = 6;

/// Every managed client enables persistent memory by default, so only an explicit `Disabled` on every switch turns it off.
#[must_use]
pub fn memory_off(switches: &[Memory]) -> bool {
    fn all_disabled(switches: &[Memory]) -> bool {
        match switches {
            [] => true,
            [Memory::Disabled, rest @ ..] => all_disabled(rest),
            _ => false,
        }
    }
    !switches.is_empty() && all_disabled(switches)
}

/// A target changed outside chezmoi refuses the apply unless its `modify_` template merges and its rendering keeps every key the host file has.
#[must_use]
pub fn host_edit_refused(changed: bool, merged: bool, keeps_host_keys: bool) -> bool {
    changed && !(merged && keeps_host_keys)
}

#[cfg(kani)]
#[kani::proof]
fn wsl_is_detected_only_on_a_microsoft_linux_kernel() {
    let system = match kani::any::<u8>() % 3 {
        0 => System::Mac,
        1 => System::Linux,
        _ => System::Windows,
    };
    let microsoft: bool = kani::any();
    let profile = detected(system, microsoft);
    assert_eq!(
        profile == Profile::Wsl,
        system == System::Linux && microsoft
    );
    assert_eq!(
        matches!(profile, Profile::Linux | Profile::Wsl),
        system == System::Linux
    );
    kani::cover!(profile == Profile::Wsl);
    kani::cover!(profile == Profile::Linux);
}

#[cfg(kani)]
#[kani::proof]
fn remote_or_installed_context7_never_requires_a_mutation() {
    let local: bool = kani::any();
    let tool: bool = kani::any();
    let installed: bool = kani::any();
    assert_eq!(
        context7_needed(local, tool, installed),
        local && tool && !installed
    );
    kani::cover!(context7_needed(local, tool, installed));
    kani::cover!(!context7_needed(local, tool, installed));
}

#[cfg(kani)]
#[kani::proof]
fn forge_import_requires_only_pinned_keys_including_the_current_one() {
    let count: usize = kani::any();
    let unpinned: usize = kani::any();
    let current: bool = kani::any();
    kani::assume(unpinned <= count);
    let accepted = forge_bundle_accepted(count, unpinned, current);
    assert_eq!(accepted, count > 0 && unpinned == 0 && current);
    if unpinned > 0 || !current {
        assert!(!accepted);
    }
    kani::cover!(accepted);
    kani::cover!(!accepted && current);
}

#[cfg(kani)]
#[kani::proof]
fn windows_path_translation_preserves_inline_public_keys() {
    let file: bool = kani::any();
    let inline: bool = kani::any();
    assert_eq!(translate_file(file, inline), file && !inline);
    kani::cover!(translate_file(file, inline));
    kani::cover!(!translate_file(file, inline));
}

#[cfg(kani)]
#[kani::proof]
fn secrets_require_present_nonempty_resolved_values() {
    let present: bool = kani::any();
    let nonempty: bool = kani::any();
    let unresolved: bool = kani::any();
    let ready = secret_ready(present, nonempty, unresolved);
    assert_eq!(ready, present && nonempty && !unresolved);
    kani::cover!(ready);
    kani::cover!(!ready);
}

#[cfg(kani)]
fn arbitrary_memory() -> Memory {
    match kani::any::<u8>() % 3 {
        0 => Memory::Unset,
        1 => Memory::Enabled,
        _ => Memory::Disabled,
    }
}

#[cfg(kani)]
#[kani::proof]
#[kani::unwind(8)]
fn memory_is_off_only_when_every_switch_is_explicitly_disabled() {
    let switches: [Memory; MEMORY_SWITCHES_PROVED] = [
        arbitrary_memory(),
        arbitrary_memory(),
        arbitrary_memory(),
        arbitrary_memory(),
        arbitrary_memory(),
        arbitrary_memory(),
    ];
    let length: usize = kani::any();
    kani::assume(length <= MEMORY_SWITCHES_PROVED);
    let listed = &switches[..length];
    let off = memory_off(listed);
    let mut every_disabled = true;
    let mut index = 0;
    while index < length {
        every_disabled &= listed[index] == Memory::Disabled;
        index += 1;
    }
    assert_eq!(off, length > 0 && every_disabled);
    kani::cover!(off && length == 1);
    kani::cover!(off && length == MEMORY_SWITCHES_PROVED);
    kani::cover!(!off && length == 0);
    kani::cover!(!off && length == MEMORY_SWITCHES_PROVED && listed[length - 1] == Memory::Unset);
}

#[cfg(kani)]
#[kani::proof]
fn host_edits_are_overwritten_only_by_a_merge_that_keeps_every_host_key() {
    let changed: bool = kani::any();
    let merged: bool = kani::any();
    let keeps: bool = kani::any();
    let refused = host_edit_refused(changed, merged, keeps);
    assert!(!refused || changed);
    assert!(refused || !changed || merged && keeps);
    kani::cover!(refused);
    kani::cover!(changed && !refused);
    kani::cover!(refused && merged);
}

#[cfg(kani)]
#[kani::proof]
fn application_requires_owned_destination_or_native_authorization() {
    let native: bool = kani::any();
    let owned: bool = kani::any();
    let authorized: bool = kani::any();
    assert_eq!(permitted(Mode::FilesOnly, native, owned, authorized), owned);
    assert_eq!(
        permitted(Mode::Full, native, owned, authorized),
        native && authorized
    );
    assert!(permitted(Mode::Preview, native, owned, authorized));
    kani::cover!(permitted(Mode::Full, native, owned, authorized));
    kani::cover!(!permitted(Mode::Full, native, owned, authorized));
}
