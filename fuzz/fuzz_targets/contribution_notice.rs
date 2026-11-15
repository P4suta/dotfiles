#![no_main]

use platform::contribution::Notice;

const REPOSITORY: &str = "example/project";
const BODY: &str = "The operation completed.\n\nAI generated this contribution.\n";

libfuzzer_sys::fuzz_target!(|data: &[u8]| {
    if let Ok(policy) = Notice::parse(data, REPOSITORY) {
        if let Ok(remaining) = policy.project(BODY) {
            assert!(BODY.starts_with(&remaining));
        }
        if let Ok(body) = std::str::from_utf8(data)
            && let Ok(remaining) = policy.project(body)
        {
            assert!(body.starts_with(&remaining));
        }
    }
});
