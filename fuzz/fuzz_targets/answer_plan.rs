#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let Ok(text) = std::str::from_utf8(data) else {
        return;
    };
    let (draft, evidence) = text
        .split_once("\n---\n")
        .unwrap_or(("Can we test this?", text));
    let _ = dotfiles::answer::inspect(draft, evidence, "request");
});
