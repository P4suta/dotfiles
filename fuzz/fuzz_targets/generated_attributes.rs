#![no_main]

use dotfiles::assure::attributes::decode;
use dotfiles::shape_rules::GeneratedMarker;
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let paths = ["src/client.ts", "src/client with 'quotes'.ts"];
    let output: Vec<_> = data
        .iter()
        .map(|byte| if *byte == b'\n' { 0 } else { *byte })
        .collect();
    if let Ok(markers) = decode(&output, &paths) {
        let mut canonical = Vec::new();
        for (path, marker) in paths.iter().zip(&markers) {
            let value = match marker {
                GeneratedMarker::Set => "set",
                GeneratedMarker::True => "true",
                GeneratedMarker::False => "false",
                GeneratedMarker::Unset => "unset",
                GeneratedMarker::Unspecified => "unspecified",
                GeneratedMarker::Other => "other",
            };
            for field in [*path, "linguist-generated", value] {
                canonical.extend_from_slice(field.as_bytes());
                canonical.push(0);
            }
        }
        assert_eq!(decode(&canonical, &paths).unwrap(), markers);
    }
});
