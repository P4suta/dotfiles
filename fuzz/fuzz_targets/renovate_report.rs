#![no_main]

use dotguard::renovate::report::parse;
use libfuzzer_sys::fuzz_target;

fuzz_target!(|bytes: &[u8]| {
    let checked = parse(bytes, "minor patch pin pinDigest");
    if let Ok(checked) = checked {
        let mut value: serde_json::Value = serde_json::from_slice(bytes).unwrap();
        let canonical = serde_json::to_vec(&value).unwrap();
        assert_eq!(
            parse(&canonical, "minor patch pin pinDigest").unwrap(),
            checked
        );
        value["unconsumedMetadata"] = serde_json::json!({"futureSchema":true});
        assert_eq!(
            parse(
                &serde_json::to_vec(&value).unwrap(),
                "minor patch pin pinDigest"
            )
            .unwrap(),
            checked
        );
    }
});
