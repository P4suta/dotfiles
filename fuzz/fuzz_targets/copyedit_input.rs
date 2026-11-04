#![no_main]

use libfuzzer_sys::fuzz_target;
use prose::copyedit::{Annotation, Report, project};

const MAX_INPUT_BYTES: usize = 16_384;

fuzz_target!(|data: &[u8]| {
    if data.len() > MAX_INPUT_BYTES {
        return;
    }
    let Ok(input) = std::str::from_utf8(data) else {
        return;
    };
    let (source, annotation) = input.split_once("\n---JSON---\n").unwrap_or((input, "{}"));
    let _ = prose::house::Policy::parse(input);
    if let Ok(findings) = prose::house::layout_findings("fuzz.md", source) {
        for finding in findings {
            assert!(finding.start < finding.end && finding.end <= source.len());
            assert_eq!(
                source.get(finding.start..finding.end),
                Some(finding.excerpt.as_str())
            );
            assert!(matches!(
                finding.rule.as_str(),
                prose::house::LIST | prose::house::TABLE
            ));
        }
    }
    if let Ok(message) = serde_json::from_str::<prose::copyedit::NativeStyleMessage>(annotation) {
        let document = prose::copyedit::Document {
            path: "fuzz.md".to_owned(),
            text: source.to_owned(),
        };
        if let Ok(finding) = message.diagnostic(&document) {
            assert!(finding.start <= finding.end && finding.end <= source.len());
            assert_eq!(
                source.get(finding.start..finding.end),
                Some(finding.excerpt.as_str())
            );
        }
    }
    for profile in [
        prose::editorial::Profile::Expository,
        prose::editorial::Profile::General,
    ] {
        if let Ok(report) = prose::editorial::inspect("fuzz.md", source, profile) {
            for finding in report.findings {
                assert!(finding.line > 0 && finding.column > 0);
                assert!(source.contains(&finding.snippet));
            }
        }
    }
    if let Ok(segments) = project("fuzz.md", source) {
        for segment in &segments {
            let length = segment.text.chars().count();
            if length == 0 {
                continue;
            }
            for (start, end) in [(0, length), (0, 1), (length - 1, length)] {
                let span = segment
                    .source_span(start, end)
                    .expect("projected character span");
                assert!(span.start < span.end && span.end <= source.len());
                assert!(source.is_char_boundary(span.start) && source.is_char_boundary(span.end));
            }
        }
        if let Ok(parsed) = serde_json::from_str::<Annotation>(annotation) {
            if parsed.validate(&segments[0]).is_ok() {
                let policy = prose::house::Policy::parse(include_str!(
                    "../../home/dot_config/prose/house.toml"
                ))
                .expect("checked house policy");
                if let Ok(findings) = policy.findings(
                    prose::copyedit::Language::Japanese,
                    &segments[0],
                    &parsed,
                    source,
                ) {
                    for finding in findings {
                        assert!(finding.start < finding.end && finding.end <= source.len());
                        assert_eq!(
                            source.get(finding.start..finding.end),
                            Some(finding.excerpt.as_str())
                        );
                    }
                }
            }
        }
    }
    let _ = serde_json::from_str::<Report>(input);
});
