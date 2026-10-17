//! The production native PATH parser retains one field and refuses ambiguous or malformed responses.
#![no_main]

use platform::code_environment::NativePathResponse;

libfuzzer_sys::fuzz_target!(|data: &[u8]| {
    let parsed = match NativePathResponse::parse(data) {
        Ok(parsed) => parsed,
        Err(error) => {
            assert!(matches!(
                error.to_string().as_str(),
                "native PATH response is invalid (Io)"
                    | "native PATH response is invalid (Syntax)"
                    | "native PATH response is invalid (Data)"
                    | "native PATH response is invalid (Eof)"
            ));
            return;
        }
    };
    let serialized = serde_json::to_vec(&parsed).expect("a checked native PATH serializes");
    assert_eq!(
        NativePathResponse::parse(&serialized).expect("a serialized native PATH parses"),
        parsed
    );
    let mut fields: serde_json::Map<String, serde_json::Value> =
        serde_json::from_slice(&serialized).expect("a native PATH serializes as an object");
    assert_eq!(fields.len(), 1);
    assert_eq!(fields["PATH"].as_str(), Some(parsed.as_str()));
    fields.insert(
        "private".into(),
        serde_json::json!({"PATH": "unretained", "token": [1, null, {"nested": true}]}),
    );
    let extended = serde_json::to_vec(&fields).expect("unrelated environment data serializes");
    assert_eq!(
        NativePathResponse::parse(&extended).expect("unrelated values do not replace PATH"),
        parsed
    );
    assert!(fields.remove("PATH").is_some());
    let absent = serde_json::to_vec(&fields).expect("an environment without PATH serializes");
    assert!(NativePathResponse::parse(&absent).is_err());
    for invalid in [
        serde_json::Value::Null,
        serde_json::json!([parsed.as_str()]),
    ] {
        fields.insert("PATH".into(), invalid);
        let wrong_type = serde_json::to_vec(&fields).expect("invalid PATH types serialize");
        assert!(NativePathResponse::parse(&wrong_type).is_err());
    }
    let path = serde_json::to_string(parsed.as_str()).expect("a checked PATH serializes");
    let duplicate = format!(r#"{{"PATH":{path},"PATH":{path}}}"#);
    assert!(NativePathResponse::parse(duplicate.as_bytes()).is_err());
    let mixed_case = format!(r#"{{"PATH":{path},"Path":{path}}}"#);
    assert_eq!(
        NativePathResponse::parse(mixed_case.as_bytes()).is_err(),
        cfg!(windows)
    );
});
