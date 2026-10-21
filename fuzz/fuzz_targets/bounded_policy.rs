#![no_main]

use dotfiles::bounded::Policy;

const VALID: &[u8] = include_bytes!("../corpus/bounded_policy/valid");
const ZERO: &[u8] = include_bytes!("../corpus/bounded_policy/zero");
const UNSAFE_TOOL: &[u8] = include_bytes!("../corpus/bounded_policy/unsafe-tool");

libfuzzer_sys::fuzz_target!(|data: &[u8]| {
    let Ok(text) = core::str::from_utf8(data) else {
        return;
    };
    let parsed = Policy::parse(text);
    if data == VALID {
        assert!(parsed.is_ok());
    }
    if data == ZERO || data == UNSAFE_TOOL {
        assert!(parsed.is_err());
    }
    if let Ok(policy) = parsed {
        let value: toml::Value = toml::from_str(text).expect("an admitted policy is TOML");
        let encoded = toml::to_string(&value).expect("TOML values serialize");
        assert_eq!(
            Policy::parse(&encoded).expect("an admitted policy round trips"),
            policy
        );
        assert!(policy.limits.memory > 0 && policy.limits.memory_swap >= policy.limits.memory);
        assert!(policy.limits.cpus > 0 && policy.limits.pids > 0 && policy.limits.seconds > 0);
    }
});
