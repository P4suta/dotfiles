//! Reads one byte past a slice, which Miri reports as undefined behavior.

#[must_use]
pub fn past_the_end(bytes: &[u8]) -> u8 {
    // SAFETY: none, because the gate must catch this read past the end.
    unsafe { *bytes.as_ptr().add(bytes.len()) }
}

#[cfg(test)]
mod tests {
    #[test]
    fn reads_past_the_end() {
        let bytes = [1u8, 2];
        assert_ne!(super::past_the_end(&bytes[..1]), u8::MAX);
    }
}
