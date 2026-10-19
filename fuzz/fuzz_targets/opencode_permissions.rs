//! The memory audit reads any rendered `OpenCode` configuration, whose permission rules keep their file order, without panicking, and decides it the same way twice.
#![no_main]

libfuzzer_sys::fuzz_target!(|data: &[u8]| {
    let Ok(text) = std::str::from_utf8(data) else {
        return;
    };
    let dump = serde_json::json!({
        ".config/opencode/AGENTS.md": {},
        ".config/opencode/opencode.json": { "contents": text },
    });
    let first = xtask::agent_memory::violations(&dump).ok();
    assert_eq!(xtask::agent_memory::violations(&dump).ok(), first);
});
