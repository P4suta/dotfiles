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

/// Whether the effective Git hooks path resolves to the managed directory.
#[must_use]
pub const fn hooks_current(configured: bool, managed: bool) -> bool {
    configured && managed
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
    let mut rest = osrelease;
    loop {
        if starts_with_microsoft(rest) {
            return true;
        }
        let [_, tail @ ..] = rest else {
            return false;
        };
        rest = tail;
    }
}

/// The vendor name a WSL kernel release carries, in any letter case.
const MICROSOFT: &[u8] = b"microsoft";

/// Whether `text` starts with the vendor name in any letter case.
/// Walking both slices keeps the proof free of the iterator code that Kani can't reach.
fn starts_with_microsoft(mut text: &[u8]) -> bool {
    let mut letters = MICROSOFT;
    while let [letter, rest @ ..] = letters {
        let [byte, tail @ ..] = text else {
            return false;
        };
        if byte.to_ascii_lowercase() != *letter {
            return false;
        }
        (text, letters) = (tail, rest);
    }
    true
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

/// The properties Kani proves for every input and the tests check over every input they list.
#[cfg(any(test, kani))]
mod proofs {
    use super::*;

    #[cfg(kani)]
    #[kani::proof]
    fn a_hooks_installation_requires_the_effective_managed_path() {
        let configured: bool = kani::any();
        let managed: bool = kani::any();
        assert_eq!(hooks_current(configured, managed), configured && managed);
        kani::cover!(hooks_current(configured, managed));
        kani::cover!(!hooks_current(configured, managed));
    }

    pub const SYSTEMS: [System; 3] = [System::Mac, System::Linux, System::Windows];
    pub const MEMORY: [Memory; 3] = [Memory::Unset, Memory::Enabled, Memory::Disabled];

    /// Only a release that names Microsoft in some letter case marks WSL.
    pub fn spec_microsoft_kernel(release: &[u8]) {
        let mut named = false;
        let mut start = 0;
        while start + MICROSOFT.len() <= release.len() {
            let mut matching = true;
            let mut index = 0;
            while index < MICROSOFT.len() {
                matching &= release.get(start + index).map(u8::to_ascii_lowercase)
                    == MICROSOFT.get(index).copied();
                index += 1;
            }
            named |= matching;
            start += 1;
        }
        assert_eq!(microsoft_kernel(release), named);
    }

    pub fn spec_detected(system: System, microsoft: bool) {
        let profile = detected(system, microsoft);
        assert_eq!(
            profile == Profile::Wsl,
            system == System::Linux && microsoft
        );
        assert_eq!(
            profile == Profile::Linux,
            system == System::Linux && !microsoft
        );
        assert_eq!(profile == Profile::Mac, system == System::Mac);
        assert_eq!(profile == Profile::Windows, system == System::Windows);
    }

    /// Each flag decision admits exactly what its rule names.
    pub fn spec_flags(first: bool, second: bool, third: bool) {
        assert_eq!(
            context7_needed(first, second, third),
            first && second && !third
        );
        assert_eq!(translate_file(first, second), first && !second);
        assert_eq!(
            secret_ready(first, second, third),
            first && second && !third
        );
        assert_eq!(
            host_edit_refused(first, second, third),
            first && !(second && third)
        );
        assert_eq!(permitted(Mode::FilesOnly, first, second, third), second);
        assert_eq!(permitted(Mode::Full, first, second, third), first && third);
        assert!(permitted(Mode::Preview, first, second, third));
    }

    pub fn spec_forge_bundle(count: usize, unpinned: usize, current: bool) {
        assert_eq!(
            forge_bundle_accepted(count, unpinned, current),
            count > 0 && unpinned == 0 && current
        );
    }

    /// Memory stays off only when every listed switch says `Disabled`.
    pub fn spec_memory_off(listed: &[Memory]) {
        let mut every_disabled = true;
        let mut index = 0;
        while let Some(&switch) = listed.get(index) {
            every_disabled &= switch == Memory::Disabled;
            index += 1;
        }
        assert_eq!(memory_off(listed), !listed.is_empty() && every_disabled);
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
        spec_microsoft_kernel(&bytes);
        let result = microsoft_kernel(&bytes);
        kani::cover!(result && !planted);
        kani::cover!(!result);
    }

    #[cfg(kani)]
    #[kani::proof]
    fn wsl_is_detected_only_on_a_microsoft_linux_kernel() {
        let system = SYSTEMS[usize::from(kani::any::<u8>() % 3)];
        let microsoft: bool = kani::any();
        spec_detected(system, microsoft);
        kani::cover!(detected(system, microsoft) == Profile::Wsl);
        kani::cover!(detected(system, microsoft) == Profile::Linux);
    }

    #[cfg(kani)]
    #[kani::proof]
    fn each_host_decision_admits_exactly_what_its_rule_names() {
        let flags: [bool; 3] = [kani::any(), kani::any(), kani::any()];
        let [first, second, third] = flags;
        spec_flags(first, second, third);
        let count: usize = kani::any();
        let unpinned: usize = kani::any();
        spec_forge_bundle(count, unpinned, third);
        kani::cover!(forge_bundle_accepted(count, unpinned, third));
        kani::cover!(host_edit_refused(first, second, third) && second);
    }

    #[cfg(kani)]
    #[kani::proof]
    #[kani::unwind(8)]
    fn memory_is_off_only_when_every_switch_is_explicitly_disabled() {
        let switches: [Memory; MEMORY_SWITCHES_PROVED] = [
            MEMORY[usize::from(kani::any::<u8>() % 3)],
            MEMORY[usize::from(kani::any::<u8>() % 3)],
            MEMORY[usize::from(kani::any::<u8>() % 3)],
            MEMORY[usize::from(kani::any::<u8>() % 3)],
            MEMORY[usize::from(kani::any::<u8>() % 3)],
            MEMORY[usize::from(kani::any::<u8>() % 3)],
        ];
        let length: usize = kani::any();
        kani::assume(length <= MEMORY_SWITCHES_PROVED);
        let listed = &switches[..length];
        spec_memory_off(listed);
        let off = memory_off(listed);
        kani::cover!(off && length == MEMORY_SWITCHES_PROVED);
        kani::cover!(!off && length == 0);
        kani::cover!(
            !off && length == MEMORY_SWITCHES_PROVED && listed[length - 1] == Memory::Unset
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{flags, words};
    use alloc::vec::Vec;

    #[test]
    fn every_input_meets_the_profile_specifications() {
        for [configured, managed] in flags() {
            assert_eq!(hooks_current(configured, managed), configured && managed);
        }
        for system in proofs::SYSTEMS {
            for microsoft in [false, true] {
                proofs::spec_detected(system, microsoft);
            }
        }
        for [first, second, third] in flags() {
            proofs::spec_flags(first, second, third);
            for count in [0, 1, 2, usize::MAX] {
                for unpinned in [0, 1, usize::MAX] {
                    proofs::spec_forge_bundle(count, unpinned, third);
                }
            }
        }
    }

    #[test]
    #[cfg_attr(miri, ignore = "Miri takes seconds over every switch list")]
    fn every_switch_list_up_to_the_proved_length_meets_the_memory_specification() {
        for code in words(&[0, 1, 2], MEMORY_SWITCHES_PROVED) {
            let listed: Vec<Memory> = code
                .iter()
                .map(|&choice| {
                    proofs::MEMORY
                        .get(usize::from(choice))
                        .copied()
                        .unwrap_or(Memory::Unset)
                })
                .collect();
            proofs::spec_memory_off(&listed);
        }
    }

    #[test]
    #[cfg_attr(miri, ignore = "Miri takes seconds over every release")]
    fn every_short_release_meets_the_kernel_specification() {
        let truncated = MICROSOFT.split_last().map_or(&[][..], |(_, rest)| rest);
        for release in [
            &b"5.15.167.4-microsoft-standard-WSL2"[..],
            b"4.4.0-19041-Microsoft",
            b"MICROSOFT",
            truncated,
            b"6.8.0-45-generic",
            b"",
        ] {
            proofs::spec_microsoft_kernel(release);
        }
        for tail in words(b"mT", 3) {
            proofs::spec_microsoft_kernel(&[truncated, &tail].concat());
            proofs::spec_microsoft_kernel(&[&tail[..], b"MICROSOFT"].concat());
        }
        assert!(microsoft_kernel(b"xMicrosoft"));
        assert!(!microsoft_kernel(&[b"x", truncated].concat()));
    }

    #[test]
    fn each_profile_has_its_own_name_and_system() {
        assert_eq!(
            Profile::ALL.map(Profile::name),
            ["mac", "linux", "windows", "wsl"]
        );
        assert_eq!(
            Profile::ALL.map(Profile::os),
            ["darwin", "linux", "windows", "linux"]
        );
    }
}
