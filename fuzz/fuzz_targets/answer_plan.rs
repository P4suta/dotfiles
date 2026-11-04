#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let Ok(text) = std::str::from_utf8(data) else {
        return;
    };
    if let Ok(request) = dotfiles::answer::inspect_request(text) {
        let encoded = serde_json::to_string(&request).unwrap();
        let decoded = dotfiles::answer::inspect_request(&encoded).unwrap();
        assert_eq!(serde_json::to_string(&decoded).unwrap(), encoded);
    }
    let (draft, evidence) = text
        .split_once("\n---\n")
        .unwrap_or(("Can we test this?", text));
    let _ = dotfiles::answer::inspect(draft, evidence, "request");
});
