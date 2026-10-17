//! `model::data::parse` reads any text without panicking, and a parsed data file survives a round trip through TOML.
#![no_main]

libfuzzer_sys::fuzz_target!(|data: &[u8]| {
    let Ok(text) = std::str::from_utf8(data) else {
        return;
    };
    if let Ok(parsed) = model::data::parse("fuzz.toml", text) {
        let written = toml::to_string(&parsed).expect("a parsed data file serializes");
        assert_eq!(model::data::parse("fuzz.toml", &written), Ok(parsed));
    }
});
