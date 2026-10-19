//! Independent checks collect their failures until every selected stage finishes.

use anyhow::{Result, ensure};

#[derive(Debug, Default)]
pub struct Checks {
    failures: Vec<String>,
}

impl Checks {
    pub fn run<T>(&mut self, name: &str, check: impl FnOnce() -> Result<T>) {
        println!("Checking {name}");
        if let Err(error) = check() {
            let failure = format!("{name}: {error:#}");
            eprintln!("Failed {failure}");
            self.failures.push(failure);
        }
    }

    /// Report every failure after the selected checks finish.
    ///
    /// # Errors
    ///
    /// Returns an error listing every failed stage.
    pub fn finish(self) -> Result<()> {
        ensure!(
            self.failures.is_empty(),
            "{} selected stages failed:\n{}",
            self.failures.len(),
            self.failures.join("\n")
        );
        Ok(())
    }
}
