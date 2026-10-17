//! `model::repository::parse` reads any text without panicking, and a parsed metadata file survives a round trip through TOML.
#![no_main]

const GITHUB_REMOTE_PREFIXES: [&str; 3] = [
    "https://github.com/",
    "git@github.com:",
    "ssh://git@github.com/",
];

libfuzzer_sys::fuzz_target!(|data: &[u8]| {
    let Ok(text) = std::str::from_utf8(data) else {
        return;
    };
    if let Some(repository) = platform::source_identity::github_slug(text.trim()) {
        for prefix in GITHUB_REMOTE_PREFIXES {
            assert_eq!(
                platform::source_identity::github_slug(&format!("{prefix}{repository}.git")),
                Some(repository.clone())
            );
        }
    }
    let _ = serde_json::from_slice::<platform::source_identity::Repository>(data);
    if let Ok(parsed) = model::repository::parse(text) {
        let written = toml::to_string(&parsed).expect("a parsed metadata file serializes");
        assert_eq!(model::repository::parse(&written), Ok(parsed));
    }
});
