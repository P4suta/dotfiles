#![no_main]

use dotguard::renovate::ci_pin::{Reader, parse};
use dotguard::renovate::mise::parse_http_release;
use libfuzzer_sys::fuzz_target;

fuzz_target!(|source: &str| {
    for expression in [
        "${{ steps.pin.outputs.version }}",
        "${{ steps.rust-pin.outputs.version }}",
    ] {
        if let Some(readers) = parse(source, expression) {
            assert!(!readers.is_empty());
            assert!(parse(&format!("{source}\n---\njobs: {{}}\n"), expression).is_none());
        }
    }
    for reader in [Reader::CompositeBash, Reader::ControllerPowerShell] {
        if reader.reads(source, "1.97") {
            assert!(!reader.reads(source, "1.96"));
            assert!(!reader.reads(&format!("{source}\n[other]\nrust = \"1.96\"\n"), "1.97"));
        }
    }
    if let Some(pin) = parse_http_release(source, "http:winappcli") {
        assert!(!pin.version().is_empty());
        let (owner, repository) = pin.package().split_once('/').unwrap();
        assert!(!owner.is_empty() && !repository.is_empty() && !repository.contains('/'));
        assert!(parse_http_release(source, "winappcli").is_none());
    }
});
