#![no_main]

use dotfiles::verification::mutation::{
    MutationIdentity, MutationReport, validate_mutation_report,
};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let Ok(text) = std::str::from_utf8(data) else {
        return;
    };
    let flags = data.first().copied().unwrap_or_default();
    let primary_gated = flags & 1 != 0;
    let alias = flags & 2 != 0;
    let alias_gated = flags & 4 != 0;
    let opaque = flags & 8 != 0;
    let entry = flags & 16 != 0;
    const OPAQUE_GUARD: u8 = 1 << 5;
    const AMBIGUOUS_ATTRIBUTE: u8 = 1 << 6;
    let opaque_gated = flags & OPAQUE_GUARD != 0;
    let ambiguous = flags & AMBIGUOUS_ATTRIBUTE != 0;
    let inline_path = data.get(1).is_some_and(|value| value & 1 != 0);
    let expression = data.get(1).is_some_and(|value| value & 2 != 0);
    let guard = |gated| if gated { "#[cfg(kani)]" } else { "" };
    let attributes = if ambiguous {
        let choices = [
            "#[cfg_attr(any(),path=\"other.rs\")]",
            "#[path=concat!(\"contract\",\".rs\")]",
            "#[path=123]",
            "#[path]",
            "#[path(\"contract.rs\")]",
            "#[path=\"contract.rs\"] #[path=\"contract.rs\"]",
        ];
        choices[usize::from(data.get(2).copied().unwrap_or_default()) % choices.len()]
    } else {
        ""
    };
    let mut declaration = format!("{} {attributes} mod contract;", guard(primary_gated));
    if alias {
        declaration.push_str(&format!(
            "{} #[path=\"contract.rs\"] mod alias;",
            guard(alias_gated)
        ));
    }
    if opaque {
        let inclusion = if expression {
            "mod scope { fn run() { let _ = std::include!(concat!(env!(\"OUT_DIR\"),\"/generated.rs\")); } }"
        } else {
            "std::include!(concat!(env!(\"OUT_DIR\"),\"/generated.rs\"));"
        };
        declaration.push_str(&format!("{} {inclusion}", guard(opaque_gated)));
    }
    if inline_path {
        declaration.push_str("#[cfg(kani)] #[path=\"nested\"] mod inline {}");
    }
    declaration.push_str("mod ordinary;");
    let mut sources: std::collections::BTreeMap<String, Vec<u8>> = [
        ("Cargo.toml", "[package]\nname='fixture'\nversion='0.1.0'\n"),
        ("src/lib.rs", declaration.as_str()),
        ("src/contract.rs", "fn helper() {}"),
        ("src/ordinary.rs", "fn ordinary() {}"),
    ]
    .into_iter()
    .map(|(path, text)| (path.to_owned(), text.as_bytes().to_vec()))
    .collect();
    if entry {
        sources
            .get_mut("Cargo.toml")
            .unwrap()
            .extend_from_slice(b"build='src/contract.rs'\n");
    } else {
        let choices = ["", "build=false\n", "build=true\n", "build='build.rs'\n"];
        let choice = usize::from(data.get(3).copied().unwrap_or_default()) % choices.len();
        sources
            .get_mut("Cargo.toml")
            .unwrap()
            .extend_from_slice(choices[choice].as_bytes());
    }
    let expected = primary_gated
        && (!alias || alias_gated)
        && (!opaque || opaque_gated)
        && !entry
        && !ambiguous
        && !inline_path;
    assert_eq!(
        dotfiles::verification::source::verification_module_context(&sources, "src/contract.rs")
            .unwrap(),
        expected
    );
    for (path, bytes) in &mut sources {
        if std::path::Path::new(path)
            .extension()
            .is_some_and(|extension| extension == "rs")
        {
            use quote::ToTokens as _;
            *bytes = syn::parse_file(std::str::from_utf8(bytes).unwrap())
                .unwrap()
                .to_token_stream()
                .to_string()
                .into_bytes();
        }
    }
    assert_eq!(
        dotfiles::verification::source::verification_module_context(&sources, "src/contract.rs")
            .unwrap(),
        expected
    );
    for (path, scope_admitted) in [
        ("src/invalid.rs", false),
        ("tests/fixtures/invalid.rs", true),
        ("other/src/invalid.rs", true),
        ("other/tests/fixtures/invalid.rs", true),
    ] {
        for payload in [b"\xff".as_slice(), b"mod broken(".as_slice()] {
            let mut scoped = sources.clone();
            scoped.insert(
                "other/Cargo.toml".into(),
                b"[package]\nname='other'\n".to_vec(),
            );
            scoped.insert(path.into(), payload.to_vec());
            assert_eq!(
                dotfiles::verification::source::verification_module_context(
                    &scoped,
                    "src/contract.rs"
                )
                .unwrap(),
                expected && scope_admitted
            );
        }
    }
    sources.insert("src/extra.rs".into(), data.to_vec());
    if !expected {
        assert!(
            !dotfiles::verification::source::verification_module_context(
                &sources,
                "src/contract.rs"
            )
            .unwrap()
        );
    }
    sources.insert("Cargo.toml".into(), data.to_vec());
    let arbitrary =
        dotfiles::verification::source::verification_module_context(&sources, "src/contract.rs")
            .ok();
    if let Ok(manifest) = text.parse::<toml::Table>() {
        sources.insert(
            "Cargo.toml".into(),
            toml::to_string(&manifest).unwrap().into_bytes(),
        );
        assert_eq!(
            dotfiles::verification::source::verification_module_context(
                &sources,
                "src/contract.rs"
            )
            .ok(),
            arbitrary
        );
    }
    if let Ok(normalized) = dotfiles::verification::inspect_plan(text) {
        assert_eq!(
            dotfiles::verification::inspect_plan(&normalized).unwrap(),
            normalized
        );
    }
    for red in [false, true] {
        let _ = dotfiles::verification::inspect_producer_report(text, red);
    }
    let _ = dotfiles::verification::source::production("src/lib.rs", data);
    let attributes = [
        ("#[cfg(test)]", true),
        ("#[cfg(kani)]", true),
        ("#[cfg(any(test,kani))]", true),
        ("#[cfg(any(kani,test))]", true),
        ("#[test]", true),
        ("#[kani::proof]", true),
        ("#[cfg(feature=\"std\")]", false),
        ("#[cfg_attr(test,test)]", false),
        ("#[other::test]", false),
        ("#[kani::unwind(3)]", false),
        ("#[doc=\"description\"]", false),
    ];
    let first = attributes[usize::from(flags) % attributes.len()];
    let second =
        attributes[usize::from(data.get(1).copied().unwrap_or_default()) % attributes.len()];
    let program = |value| format!("{} {} fn sample() -> bool {{ {value} }}", first.0, second.0);
    assert_eq!(
        dotfiles::verification::source::production("src/lib.rs", program(false).as_bytes())
            .unwrap()
            == dotfiles::verification::source::production("src/lib.rs", program(true).as_bytes())
                .unwrap(),
        first.1 || second.1
    );
    if let Ok(normalized) = dotfiles::verification::proof::inspect_receipt(text) {
        assert_eq!(
            dotfiles::verification::proof::inspect_receipt(&normalized).unwrap(),
            normalized
        );
    }
    if let Ok(normalized) = dotfiles::verification::proof::inspect_phases(text) {
        assert_eq!(
            dotfiles::verification::proof::inspect_phases(&normalized).unwrap(),
            normalized
        );
    }
    let arguments = format!("[\"kani\", {text:?}]");
    let phase = |arguments: &str| {
        format!(
            "phase=verification name=proof_inputs args={arguments} source_sha256={}\n",
            "a".repeat(64)
        )
    };
    assert!(dotfiles::verification::proof::inspect_phases(&phase(&arguments)).is_ok());
    for decorated in [
        format!("#[cfg(test)] {arguments}"),
        format!("[#[cfg(test)] \"kani\", {text:?}]"),
    ] {
        assert!(dotfiles::verification::proof::inspect_phases(&phase(&decorated)).is_err());
    }
    let satisfied = data.first().copied().unwrap_or_default();
    let total = data.get(1).copied().unwrap_or_default();
    let stdout = format!(
        "Checking harness proofs::cover_contract...\n ** 0 of 1 failed\n ** {satisfied} of {total} cover properties satisfied\nVERIFICATION:- SUCCESSFUL\nComplete - 1 successfully verified harnesses, 0 failures, 1 total.\n"
    );
    let mut receipt = serde_json::json!({"version":3,"command":["cargo","kani"],"source_sha256":"a".repeat(64),"artifact_sha256":null,"container_image":format!("sha256:{}", "b".repeat(64)),"exit":0,"stdout":stdout,"stderr":""});
    assert_eq!(
        dotfiles::verification::proof::inspect_receipt(&receipt.to_string()).is_ok(),
        total != 0 && satisfied == total
    );
    let exit =
        i32::from_le_bytes([2, 3, 4, 5].map(|index| data.get(index).copied().unwrap_or_default()));
    receipt["exit"] = exit.into();
    receipt["stdout"] = "Checking harness proofs::counter...\n ** 1 of 1 failed\nFailed Checks: assertion failed: false\nVERIFICATION:- FAILED\nVerification failed for - proofs::counter\nComplete - 0 successfully verified harnesses, 1 failures, 1 total.\n".into();
    assert_eq!(
        dotfiles::verification::proof::inspect_receipt(&receipt.to_string()).is_ok(),
        exit == 1
    );
    let (before, after) = data.split_at(data.len() / 2);
    let forward = dotfiles::verification::source::configuration_change(
        "src/lib.rs",
        Some(before),
        Some(after),
    );
    let reverse = dotfiles::verification::source::configuration_change(
        "src/lib.rs",
        Some(after),
        Some(before),
    );
    assert_eq!(forward.ok(), reverse.ok());
    if let Ok((inventory, report, status)) =
        serde_json::from_str::<(Vec<MutationIdentity>, MutationReport, Option<i32>)>(text)
    {
        let package = inventory
            .first()
            .map(|mutant| mutant.package.clone())
            .unwrap_or_default();
        let version = report.cargo_mutants_version.clone();
        let _ = validate_mutation_report(&inventory, report, status, &package, &version);
    }
});
