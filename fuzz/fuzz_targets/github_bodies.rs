#![no_main]

use dotfiles::claude_hook::github::{MAX_BODY_INPUT, decode, json_bodies, literal_word};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let Ok(text) = std::str::from_utf8(data) else {
        return;
    };
    let _ = literal_word(text);
    let words: Vec<_> = text.lines().collect();
    let _ = decode(&words);
    if let Ok(bodies) = json_bodies(text) {
        let canonical = serde_json::json!({
            "comments": bodies.iter().map(|body| serde_json::json!({"body": body})).collect::<Vec<_>>()
        });
        let canonical = canonical.to_string();
        if canonical.len() <= MAX_BODY_INPUT {
            assert_eq!(json_bodies(&canonical).unwrap(), bodies);
        }
    }
});
