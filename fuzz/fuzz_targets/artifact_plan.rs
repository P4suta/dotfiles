//! Acquisition plans preserve their pinned identity and scoped target through parsing and reporting.
#![no_main]

use dotfiles::artifacts::{CurlRelease, Digest, Inputs, Plan, Request, Target};
use serde_json::Value;

const SELECTED_TOOL: &str = "example";
const SELECTED_TARGET: &str = "windows-x64";
const VALID: &[u8] = include_bytes!("../corpus/artifact_plan/valid");
const MISSING_DIGEST: &[u8] = include_bytes!("../corpus/artifact_plan/missing-digest");
const UNSUPPORTED_BACKEND: &[u8] = include_bytes!("../corpus/artifact_plan/unsupported-backend");
const CURL_BOUNDED: &[u8] = include_bytes!("../corpus/artifact_plan/curl-bounded");
const CURL_UNBOUNDED: &[u8] = include_bytes!("../corpus/artifact_plan/curl-unbounded");

#[derive(Clone, Copy, Debug)]
struct Envelope<'a> {
    project: &'a str,
    mise_lock: &'a str,
    cargo_lock: &'a str,
    versions: &'a str,
}

impl<'a> Envelope<'a> {
    fn inputs(self) -> Inputs<'a> {
        Inputs {
            project: self.project,
            mise_lock: self.mise_lock,
            cargo_lock: self.cargo_lock,
            versions: self.versions,
        }
    }
}

fn envelope(value: &Value) -> Option<Envelope<'_>> {
    Some(Envelope {
        project: value.get("project")?.as_str()?,
        mise_lock: value.get("mise_lock")?.as_str()?,
        cargo_lock: value.get("cargo_lock")?.as_str()?,
        versions: value.get("versions")?.as_str()?,
    })
}

fn request() -> Request {
    Request {
        targets: vec![Target::WindowsX64],
        tools: vec![SELECTED_TOOL.into()],
    }
}

libfuzzer_sys::fuzz_target!(|data: &[u8]| {
    if let Ok(text) = core::str::from_utf8(data) {
        let release = CurlRelease::parse(text);
        if data == CURL_BOUNDED {
            assert!(release.is_ok(), "bounded transport corpus must be admitted");
        } else if data == CURL_UNBOUNDED {
            assert!(
                release.is_err(),
                "unbounded transport corpus must be refused"
            );
        }
        if let Ok(release) = release {
            let version = release.version();
            assert!(rules::artifact_rules::bounded_curl(version));
            let [major, minor, patch] = version;
            let encoded = format!("curl {major}.{minor}.{patch} (native)");
            assert_eq!(
                CurlRelease::parse(&encoded)
                    .expect("a checked transport release round trips")
                    .version(),
                version
            );
        }
    }

    let digest = Digest::of(data);
    let hex = digest.hex();
    assert_eq!(Digest::try_from(hex.clone()), Ok(digest));
    assert_eq!(
        rules::artifact_rules::sha256(hex.as_bytes())
            .map(|bytes| platform::build_inputs::hex(&bytes)),
        Some(hex)
    );
    if let Some(bytes) = rules::artifact_rules::sha256(data) {
        assert_eq!(platform::build_inputs::hex(&bytes).as_bytes(), data);
    }

    let Ok(value) = serde_json::from_slice::<Value>(data) else {
        return;
    };
    let Some(envelope) = envelope(&value) else {
        return;
    };
    let parsed = Plan::parse(envelope.inputs(), request());
    if data == VALID {
        assert!(parsed.is_ok(), "the valid scoped corpus must parse");
    }
    let Ok(plan) = parsed else {
        return;
    };
    if data == VALID {
        assert!(
            plan.complete().is_ok(),
            "the valid scoped corpus must be complete"
        );
    } else if data == MISSING_DIGEST || data == UNSUPPORTED_BACKEND {
        assert!(
            plan.complete().is_err(),
            "unresolved corpus material must be refused"
        );
    }

    let repeated = Plan::parse(envelope.inputs(), request())
        .expect("identical inputs produce another parsed acquisition plan");
    assert_eq!(repeated.digest(), plan.digest());
    let report = serde_json::to_vec(&plan).expect("a parsed acquisition plan serializes");
    assert_eq!(
        serde_json::to_vec(&repeated).expect("the repeated acquisition plan serializes"),
        report
    );
    let report: Value = serde_json::from_slice(&report).expect("the plan report is valid JSON");
    assert_eq!(
        report.get("digest").and_then(Value::as_str),
        Some(plan.digest().hex().as_str())
    );

    for artifact in plan.artifacts() {
        assert!(
            artifact.subject().starts_with("example@")
                || artifact.subject().starts_with("example:")
        );
        assert_eq!(
            Digest::try_from(artifact.expected().hex()),
            Ok(artifact.expected())
        );
        let artifact =
            serde_json::to_value(artifact).expect("a scoped acquisition artifact serializes");
        assert_eq!(
            artifact.get("target").and_then(Value::as_str),
            Some(SELECTED_TARGET)
        );
    }
    if !plan.diagnostics().is_empty() || plan.artifacts().is_empty() {
        assert!(plan.complete().is_err());
    }
});
