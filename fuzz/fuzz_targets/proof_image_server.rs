#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|bytes: &[u8]| {
    if let Ok(server) = xtask::proof_image_prepare::decode(bytes) {
        let encoded = serde_json::to_vec(&server).unwrap();
        let decoded = xtask::proof_image_prepare::decode(&encoded).unwrap();
        assert_eq!(serde_json::to_vec(&decoded).unwrap(), encoded);
    }
});
