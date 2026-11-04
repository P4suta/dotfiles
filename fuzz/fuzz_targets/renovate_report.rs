#![no_main]

use dotguard::renovate::report::parse;
use dotguard::renovate::tarball::official_tarball_version;
use libfuzzer_sys::fuzz_target;

const CANONICAL_TARBALL_PREFIX: &str = "https://cdn.sheetjs.com/xlsx-";

fuzz_target!(|bytes: &[u8]| {
    if let Ok(url) = std::str::from_utf8(bytes) {
        let url = url.strip_suffix('\n').unwrap_or(url);
        if let Some(version) = official_tarball_version("xlsx", url) {
            assert_eq!(
                url,
                format!("{CANONICAL_TARBALL_PREFIX}{version}/xlsx-{version}.tgz")
            );
            assert!(official_tarball_version("other", url).is_none());
            assert!(official_tarball_version("xlsx", &format!("{url}?other")).is_none());
            assert!(official_tarball_version("xlsx", &format!("{url}#other")).is_none());
        }
    }
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
