#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let paths = vec!["src/main.rs".to_owned()];
    if coderabbit_guard::validate_agent_stream(data, "/review", "feature", &paths).is_ok() {
        let original = std::str::from_utf8(data).unwrap();
        let mut encoded = Vec::new();
        for line in original.lines() {
            let value: serde_json::Value = serde_json::from_str(line).unwrap();
            serde_json::to_writer(&mut encoded, &value).unwrap();
            encoded.push(b'\n');
        }
        assert!(
            coderabbit_guard::validate_agent_stream(&encoded, "/review", "feature", &paths).is_ok()
        );
    }
});
