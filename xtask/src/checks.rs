use anyhow::Result;

#[derive(Debug, Default)]
pub struct Checks {
    inner: platform::checks::Checks,
}

impl Checks {
    pub fn run<T>(&mut self, name: &str, check: impl FnOnce() -> Result<T>) {
        let public = crate::phase_evidence::public_label(name);
        self.inner.run(&public, || {
            check().map_err(|error| crate::phase_evidence::private_failure(name, error))
        });
    }

    /// Reports every selected failure after all independent checks finish.
    ///
    /// # Errors
    ///
    /// Refuses any failed check and keeps active proof diagnostics in their private evidence scope.
    pub fn finish(self) -> Result<()> {
        self.inner.finish()
    }
}
