/// Where a hook gate builds its Cargo artifacts.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Target {
    /// The absolute `CARGO_TARGET_DIR` the caller already chose.
    Configured,
    /// One shared directory on the volume that holds `TEMP`, which is the development drive on Windows.
    Development,
    /// One shared directory in the home directory, for hosts without such a volume.
    Home,
}

/// A relative `CARGO_TARGET_DIR` resolves against the worktree, so only an absolute one is honored.
pub fn target(configured_absolute: bool, temp_volume: bool) -> Target {
    if configured_absolute {
        Target::Configured
    } else if temp_volume {
        Target::Development
    } else {
        Target::Home
    }
}

#[cfg(kani)]
#[kani::proof]
fn an_absolute_configured_target_is_always_honored() {
    let temp_volume: bool = kani::any();
    assert_eq!(target(true, temp_volume), Target::Configured);
    kani::cover!(temp_volume);
    kani::cover!(!temp_volume);
}

#[cfg(kani)]
#[kani::proof]
fn an_unconfigured_target_is_shared_and_never_inside_the_worktree() {
    let temp_volume: bool = kani::any();
    let decided = target(false, temp_volume);
    assert_ne!(decided, Target::Configured);
    assert_eq!(decided == Target::Development, temp_volume);
    assert_eq!(decided == Target::Home, !temp_volume);
    kani::cover!(decided == Target::Development);
    kani::cover!(decided == Target::Home);
}
