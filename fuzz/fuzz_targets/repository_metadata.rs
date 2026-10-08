//! `model::repository::parse` reads any text without panicking, and a parsed metadata file survives a round trip through TOML.
#![no_main]

libfuzzer_sys::fuzz_target!(|data: &[u8]| {
    let Ok(text) = std::str::from_utf8(data) else {
        return;
    };
    if let Ok(parsed) = model::repository::parse(text) {
        let written = toml::to_string(&parsed).expect("a parsed metadata file serializes");
        assert_eq!(model::repository::parse(&written), Ok(parsed));
    }
});
