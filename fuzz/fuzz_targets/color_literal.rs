//! `contrast_rules::parse` reads any byte slice without panicking, and a parsed color reads back from its lowercase literal.
#![no_main]

use rules::contrast_rules::{Rgb, parse};

libfuzzer_sys::fuzz_target!(|data: &[u8]| {
    if let Some(Rgb { red, green, blue }) = parse(data) {
        let literal = format!("#{red:02x}{green:02x}{blue:02x}");
        assert_eq!(parse(literal.as_bytes()), Some(Rgb { red, green, blue }));
    }
});
