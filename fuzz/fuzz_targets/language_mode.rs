//! `Mode::parse` reads any `guard.lang` value without panicking, and surrounding blanks never change its meaning.
#![no_main]

use dotguard::lang::Mode;

libfuzzer_sys::fuzz_target!(|data: &[u8]| {
    let Ok(text) = std::str::from_utf8(data) else {
        return;
    };
    let mode = Mode::parse(Some(text));
    assert_eq!(Mode::parse(Some(text.trim())), mode);
    assert_eq!(Mode::parse(Some(&format!(" {text}\t"))), mode);
});
