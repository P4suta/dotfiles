#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Profile {
    Mac,
    Linux,
    Windows,
    Wsl,
}

impl Profile {
    pub const ALL: [Self; 4] = [Self::Mac, Self::Linux, Self::Windows, Self::Wsl];

    pub const fn name(self) -> &'static str {
        match self {
            Self::Mac => "mac",
            Self::Linux => "linux",
            Self::Windows => "windows",
            Self::Wsl => "wsl",
        }
    }

    pub const fn bit(self) -> u8 {
        match self {
            Self::Mac => 1,
            Self::Linux => 2,
            Self::Windows => 4,
            Self::Wsl => 8,
        }
    }

    pub const fn os(self) -> &'static str {
        match self {
            Self::Mac => "darwin",
            Self::Linux | Self::Wsl => "linux",
            Self::Windows => "windows",
        }
    }
}

pub fn selected(profile: Profile, available: u8) -> bool {
    available & profile.bit() != 0
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Mode {
    Preview,
    FilesOnly,
    Full,
}

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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Reconcile {
    KeepLocal,
    TakeUpstream,
    Conflict,
}

pub fn reconcile(
    base_is_local: bool,
    base_is_upstream: bool,
    local_is_upstream: bool,
) -> Reconcile {
    if base_is_upstream || local_is_upstream {
        Reconcile::KeepLocal
    } else if base_is_local {
        Reconcile::TakeUpstream
    } else {
        Reconcile::Conflict
    }
}

pub fn secret_ready(present: bool, nonempty: bool, unresolved_reference: bool) -> bool {
    present && nonempty && !unresolved_reference
}

pub fn translate_file(file_argument: bool, inline_public_key: bool) -> bool {
    file_argument && !inline_public_key
}

pub fn context7_needed(local_gui: bool, tool_exists: bool, installed: bool) -> bool {
    local_gui && tool_exists && !installed
}

pub fn forge_key_accepted(primary_count: usize, first_fingerprint_matches: bool) -> bool {
    primary_count == 1 && first_fingerprint_matches
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
fn forge_import_requires_exactly_one_matching_primary_key() {
    let count: usize = kani::any();
    let matched: bool = kani::any();
    assert_eq!(forge_key_accepted(count, matched), count == 1 && matched);
    kani::cover!(forge_key_accepted(count, matched));
    kani::cover!(!forge_key_accepted(count, matched));
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
fn arbitrary_profile() -> Profile {
    match kani::any::<u8>() % 4 {
        0 => Profile::Mac,
        1 => Profile::Linux,
        2 => Profile::Windows,
        _ => Profile::Wsl,
    }
}

#[cfg(kani)]
#[kani::proof]
fn only_available_profiles_are_selected() {
    let profile = arbitrary_profile();
    let available: u8 = kani::any();
    assert_eq!(selected(profile, available), available & profile.bit() != 0);
    assert!(profile.bit().is_power_of_two());
    kani::cover!(selected(profile, available));
    kani::cover!(!selected(profile, available));
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

#[cfg(kani)]
#[kani::proof]
fn concurrent_source_and_local_edits_are_never_overwritten() {
    let base_is_local: bool = kani::any();
    let base_is_upstream: bool = kani::any();
    let local_is_upstream: bool = kani::any();
    let decision = reconcile(base_is_local, base_is_upstream, local_is_upstream);
    assert_eq!(
        decision == Reconcile::Conflict,
        !base_is_local && !base_is_upstream && !local_is_upstream
    );
    assert!(!matches!(decision, Reconcile::TakeUpstream) || base_is_local);
    kani::cover!(decision == Reconcile::Conflict);
    kani::cover!(decision == Reconcile::TakeUpstream);
}
