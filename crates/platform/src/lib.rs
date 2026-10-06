//! The host facts every dotfiles crate shares: the home and its known folders, the managed `PATH`, the one process constructor, and the refusal format.
//! With the default tools feature it adds the tool registry, the setup steps, the process runner, and the build stamp of the installed tools.

pub mod home;
pub mod order_rules;
pub mod process;
pub mod refusal;

#[cfg(feature = "tools")]
pub mod aside;
#[cfg(feature = "tools")]
pub mod aside_rules;
#[cfg(feature = "tools")]
pub mod build_inputs;
#[cfg(feature = "tools")]
pub mod freshness;
#[cfg(feature = "tools")]
pub mod paths;
#[cfg(feature = "tools")]
pub mod runtime;
#[cfg(feature = "tools")]
pub mod stale_rules;
#[cfg(feature = "tools")]
pub mod step;
#[cfg(feature = "tools")]
pub mod target_rules;
#[cfg(feature = "tools")]
pub mod tool;

/// A failure whose structured refusal a child process already printed, so nothing follows it.
#[derive(Debug)]
pub struct Relayed;

impl std::fmt::Display for Relayed {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("refused; the refusal above names the cause")
    }
}

impl std::error::Error for Relayed {}

/// Prints a failure: the structured refusal when the error carries one, nothing when a child already printed it, the error chain otherwise.
#[cfg(feature = "tools")]
pub fn report(error: &anyhow::Error) {
    if let Some(refusal) = error.downcast_ref::<refusal::Refusal>() {
        refusal.emit();
    } else if error.downcast_ref::<Relayed>().is_none() {
        eprintln!("Error: {error:#}");
    }
}
