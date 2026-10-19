//! `Gates::parse` reads any text without panicking, and a parsed gate list reads back from its display form.
#![no_main]

use rules::change_rules::Gates;

libfuzzer_sys::fuzz_target!(|data: &[u8]| {
    let Ok(text) = std::str::from_utf8(data) else {
        return;
    };
    if let Ok(gates) = Gates::parse(text) {
        assert_eq!(Gates::parse(&gates.to_string()), Ok(gates));
    }
});
