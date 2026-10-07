//! `Refusal::parse` reads any line without panicking, and a parsed refusal reads back from the line it writes.
#![no_main]

use platform::refusal::Refusal;

libfuzzer_sys::fuzz_target!(|data: &[u8]| {
    let Ok(line) = std::str::from_utf8(data) else {
        return;
    };
    if let Some(refusal) = Refusal::parse(line) {
        assert_eq!(Refusal::parse(&refusal.line()), Some(refusal));
    }
});
