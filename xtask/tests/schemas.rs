//! The vendored consumer schemas govern the rendered third-party configurations offline, refuse a planted key, and keep their digests.

use serde_json::json;
use std::fs;
use std::path::Path;
use xtask::obligations::{Obligation, Vendored, closure, manifest, parse_manifest};
use xtask::quality::{Contract, PLANTED_KEY, conform, contracts};

#[path = "../../crates/dotguard/tests/support/fixture_git.rs"]
mod fixture_git;

/// An empty checkout with the directories the closure check reads.
fn checkout() -> tempfile::TempDir {
    let checkout = tempfile::tempdir().unwrap();
    for directory in ["crates/rules/src", "xtask/src", "schemas/vendor"] {
        fs::create_dir_all(checkout.path().join(directory)).unwrap();
    }
    checkout
}

fn root() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap()
}

#[test]
fn vendor_text_keeps_its_spelling_while_authored_and_neighboring_files_are_checked() {
    let checkout = checkout();
    assert!(
        fixture_git::command("git", &checkout.path().join("global.gitconfig"))
            .current_dir(checkout.path())
            .args(["init", "--quiet"])
            .status()
            .unwrap()
            .success()
    );
    fs::copy(
        root().join(".typos.toml"),
        checkout.path().join(".typos.toml"),
    )
    .unwrap();
    fs::create_dir(checkout.path().join("schemas/vendor-copy")).unwrap();
    let paths: Vec<String> = [
        "schemas/vendor/fixture.json",
        "schemas/vendor-copy/fixture.json",
        "schemas/authored.json",
    ]
    .map(str::to_owned)
    .into();
    let word = ["f", "pr"].concat();
    for path in &paths {
        fs::write(
            checkout.path().join(path),
            format!("{{\"text\":\"{word}\"}}\n"),
        )
        .unwrap();
    }
    let found = xtask::lint::findings(checkout.path(), xtask::lint::Stage::Typos, &paths).unwrap();
    assert_eq!(found.len(), 2, "{found:?}");
    assert!(!found.contains_key(&paths[0]), "{found:?}");
    assert!(found.contains_key(&paths[1]), "{found:?}");
    assert!(found.contains_key(&paths[2]), "{found:?}");
}

#[test]
fn opaque_transport_and_imported_documents_do_not_hide_authored_neighbors() {
    let checkout = checkout();
    assert!(
        fixture_git::command("git", &checkout.path().join("global.gitconfig"))
            .current_dir(checkout.path())
            .args(["init", "--quiet"])
            .status()
            .unwrap()
            .success()
    );
    fs::copy(
        root().join(".typos.toml"),
        checkout.path().join(".typos.toml"),
    )
    .unwrap();
    let manifest: serde_json::Value = serde_json::from_slice(
        &fs::read(root().join("xtask/schema/njutest/manifest.json")).unwrap(),
    )
    .unwrap();
    let mut excluded = vec![".pr-workflow/contexts.json".to_owned()];
    excluded.extend(
        manifest["documents"]
            .as_array()
            .unwrap()
            .iter()
            .map(|document| document["path"].as_str().unwrap().to_owned()),
    );
    let authored = [
        ".pr-workflow/authored.json",
        "xtask/schema/njutest/manifest.json",
        "xtask/tests/fixtures/njutest/authored.json",
    ];
    let mut paths = excluded.clone();
    paths.extend(authored.map(str::to_owned));
    let word = ["f", "pr"].concat();
    for path in &paths {
        let destination = checkout.path().join(path);
        fs::create_dir_all(destination.parent().unwrap()).unwrap();
        fs::write(destination, format!("{{\"text\":\"{word}\"}}\n")).unwrap();
    }
    let found = xtask::lint::findings(checkout.path(), xtask::lint::Stage::Typos, &paths).unwrap();
    assert_eq!(found.len(), authored.len(), "{found:?}");
    for path in authored {
        assert!(found.contains_key(path), "{found:?}");
    }
    for path in excluded {
        assert!(!found.contains_key(&path), "{found:?}");
    }
}

fn starship() -> Contract {
    let manifest = manifest(root()).unwrap();
    let Some(Obligation::JsonSchema { schema, sha256, encoded_sha256, .. }) =
        manifest.obligations.iter().find(|obligation| {
            matches!(obligation, Obligation::JsonSchema { schema, .. } if schema.ends_with("starship.schema.json.gz"))
        })
    else {
        panic!("the manifest governs starship.toml");
    };
    Contract::load(
        root(),
        &Vendored {
            schema: schema.clone(),
            sha256: sha256.clone(),
            encoded_sha256: encoded_sha256.clone(),
        },
        &[".config/starship.toml".to_owned()],
        &[],
        false,
        &[],
        &[],
    )
    .unwrap()
}

#[test]
fn a_planted_unknown_key_in_a_mapped_target_fails_and_names_the_target_and_profile() {
    let mut contracts = vec![starship()];
    let clean = json!({".config/starship.toml": {"contents": "add_newline = false\n"}});
    conform("mac", &clean, &mut contracts).unwrap();
    assert_eq!(contracts[0].witnesses, ["mac:.config/starship.toml"]);
    let planted = json!({".config/starship.toml": {"contents": format!("add_newline = false\n{PLANTED_KEY} = 1\n")}});
    let error = conform("linux", &planted, &mut contracts).unwrap_err();
    let refusal = format!("{error:#}");
    assert!(
        refusal.contains("linux") && refusal.contains(".config/starship.toml"),
        "{refusal}"
    );
    let misspelled = json!({".config/starship.toml": {"contents": "add_newlines = false\n"}});
    assert!(conform("wsl", &misspelled, &mut contracts).is_err());
}

#[test]
fn document_schema_hints_never_replace_the_original_vendored_contract() {
    let mut contracts = vec![starship()];
    for contents in [
        "\"$schema\" = \"file:///dotfiles-missing-consumer-schema.json\"\nadd_newline = false\n"
            .to_owned(),
        "#:schema file:///dotfiles-missing-consumer-schema.json\nadd_newline = false\n".to_owned(),
    ] {
        let clean = json!({".config/starship.toml": {"contents": contents}});
        conform("windows", &clean, &mut contracts).unwrap();
        let bad_type = json!({".config/starship.toml": {"contents": format!("{contents}command_timeout = 'wrong type'\n")}});
        let error = conform("windows", &bad_type, &mut contracts).unwrap_err();
        let refusal = format!("{error:#}");
        assert!(
            refusal.contains("windows") && refusal.contains(".config/starship.toml"),
            "{refusal}"
        );
        let unknown = json!({".config/starship.toml": {"contents": format!("{contents}{PLANTED_KEY} = 1\n")}});
        assert!(conform("windows", &unknown, &mut contracts).is_err());
    }
}

#[test]
fn every_governed_schema_of_the_repository_compiles_offline_and_refuses_a_planted_key() {
    for contract in contracts(root(), &manifest(root()).unwrap()).unwrap() {
        let planted = json!({ PLANTED_KEY: true });
        assert!(
            contract.refusal("mac", "fixture", &planted).is_some(),
            "{} admits a planted key",
            contract.schema
        );
    }
}

fn claude() -> Contract {
    contracts(root(), &manifest(root()).unwrap())
        .unwrap()
        .into_iter()
        .find(|contract| {
            contract
                .schema
                .ends_with("claude-code-settings.schema.json.gz")
        })
        .unwrap()
}

#[test]
fn the_documented_command_failure_policy_is_typed_and_keeps_unknown_hook_fields_closed() {
    let contract = claude();
    let target = ".claude/settings.json";
    for policy in ["continue", "block"] {
        let valid = json!({"hooks": {"PreToolUse": [{"matcher": "Bash", "hooks": [{"type": "command", "command": "dotfiles claude-hook", "onFailure": policy}]}]}});
        assert!(contract.refusal("mac", target, &valid).is_none(), "{valid}");
    }
    for hook in [
        json!({"type": "command", "command": "dotfiles claude-hook", "onFailure": "allow"}),
        json!({"type": "command", "command": "dotfiles claude-hook", "onFailure": true}),
        json!({"type": "command", "command": "dotfiles claude-hook", "onFailure": "block", "onFailurre": "block"}),
        json!({"type": "prompt", "prompt": "check", "onFailure": "block"}),
    ] {
        let invalid = json!({"hooks": {"PreToolUse": [{"matcher": "Bash", "hooks": [hook]}]}});
        assert!(
            contract.refusal("mac", target, &invalid).is_some(),
            "{invalid}"
        );
    }
    let manifest = manifest(root()).unwrap();
    let vendored = manifest
        .obligations
        .iter()
        .find_map(|obligation| match obligation {
            Obligation::JsonSchema {
                schema,
                sha256,
                encoded_sha256,
                ..
            } if schema.ends_with("claude-code-settings.schema.json.gz") => Some(Vendored {
                schema: schema.clone(),
                sha256: sha256.clone(),
                encoded_sha256: encoded_sha256.clone(),
            }),
            _ => None,
        })
        .unwrap();
    assert!(
        xtask::obligations::vendored_bytes(root(), &vendored)
            .unwrap()
            .0
    );
    let mut drifted = vendored;
    drifted.sha256 = "0".to_owned();
    assert!(
        Contract::load(
            root(),
            &drifted,
            &[target.to_owned()],
            &[],
            true,
            &[],
            &[xtask::obligations::SchemaExtension::ClaudeCommandFailurePolicy]
        )
        .is_err()
    );
    assert!(
        parse_manifest(
            r#"[[obligation]]
verifier = "json-schema"
schema = "schemas/vendor/claude-code-settings.schema.json.gz"
sha256 = "0"
targets = [".claude/settings.json"]
extensions = ["allow-all-unknown-fields"]
"#
        )
        .is_err()
    );
}

#[test]
fn a_reference_to_an_unvendored_schema_fails_instead_of_fetching_it() {
    let checkout = checkout();
    let schema = "schemas/vendor/remote.schema.json";
    fs::write(
        checkout.path().join(schema),
        r#"{"$ref": "https://example.com/remote.json"}"#,
    )
    .unwrap();
    let error = Contract::load(
        checkout.path(),
        &Vendored {
            schema: schema.to_owned(),
            sha256: String::new(),
            encoded_sha256: None,
        },
        &[],
        &[],
        false,
        &[],
        &[],
    )
    .unwrap_err();
    assert!(format!("{error:#}").contains("example.com"), "{error:#}");
}

#[test]
fn a_vendored_schema_whose_hash_differs_from_the_manifest_fails() {
    let checkout = checkout();
    fs::write(
        checkout.path().join("schemas/vendor/a.schema.json"),
        r#"{"type": "object", "additionalProperties": false}"#,
    )
    .unwrap();
    let mut contract = Contract::load(
        checkout.path(),
        &Vendored {
            schema: "schemas/vendor/a.schema.json".to_owned(),
            sha256: "0000".to_owned(),
            encoded_sha256: None,
        },
        &["a.json".to_owned()],
        &[],
        false,
        &[],
        &[],
    )
    .unwrap();
    contract.witnesses.push("mac:a.json".to_owned());
    assert!(
        contract
            .standing_refusal()
            .is_some_and(|refusal| refusal.contains("SHA-256"))
    );
    let text = "[[obligation]]\nverifier = \"json-schema\"\nschema = \"schemas/vendor/a.schema.json\"\nsha256 = \"0000\"\ntargets = [\"a.json\"]\n";
    let measured = closure(checkout.path(), &parse_manifest(text).unwrap()).unwrap();
    assert!(
        measured
            .refusals
            .iter()
            .any(|refusal| refusal.contains("a.schema.json differs from the SHA-256")),
        "{:?}",
        measured.refusals
    );
}

#[test]
fn a_schema_this_repository_exports_never_governs_a_target() {
    let checkout = checkout();
    let text = "[[obligation]]\nverifier = \"json-schema\"\nschema = \"schemas/baselines.schema.json\"\nsha256 = \"0\"\ntargets = [\".config/skill-ops/policy.json\"]\n";
    let measured = closure(checkout.path(), &parse_manifest(text).unwrap()).unwrap();
    assert!(
        measured
            .refusals
            .iter()
            .any(|refusal| refusal.contains("lies outside schemas/vendor/")),
        "{:?}",
        measured.refusals
    );
}

#[test]
fn compressed_schemas_keep_the_original_bytes_and_require_both_digests() {
    use sha2::{Digest as _, Sha256};
    use std::io::Write as _;
    let checkout = checkout();
    let original = br#"{"type":"object","additionalProperties":false}"#;
    let mut compressor = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::best());
    compressor.write_all(original).unwrap();
    let encoded = compressor.finish().unwrap();
    let schema = "schemas/vendor/a.schema.json.gz";
    fs::write(checkout.path().join(schema), &encoded).unwrap();
    let mut vendored = Vendored {
        schema: schema.to_owned(),
        sha256: platform::build_inputs::hex(&Sha256::digest(original)),
        encoded_sha256: Some(platform::build_inputs::hex(&Sha256::digest(&encoded))),
    };
    assert_eq!(
        xtask::obligations::vendored_bytes(checkout.path(), &vendored).unwrap(),
        (true, original.to_vec())
    );
    vendored.encoded_sha256 = Some("0".to_owned());
    assert!(
        !xtask::obligations::vendored_bytes(checkout.path(), &vendored)
            .unwrap()
            .0
    );
    vendored.encoded_sha256 = Some(platform::build_inputs::hex(&Sha256::digest(&encoded)));
    vendored.sha256 = "0".to_owned();
    assert!(
        !xtask::obligations::vendored_bytes(checkout.path(), &vendored)
            .unwrap()
            .0
    );
    vendored.encoded_sha256 = None;
    assert!(xtask::obligations::vendored_bytes(checkout.path(), &vendored).is_err());
    vendored.encoded_sha256 = Some("0".to_owned());
    fs::write(checkout.path().join(schema), b"broken archive").unwrap();
    assert!(xtask::obligations::vendored_bytes(checkout.path(), &vendored).is_err());
}

#[test]
fn a_schema_no_profile_renders_a_target_for_checks_nothing_and_fails() {
    let starship = starship();
    assert!(
        starship
            .standing_refusal()
            .is_some_and(|refusal| refusal.contains("checks nothing"))
    );
    let checkout = checkout();
    fs::write(checkout.path().join("schemas/vendor/a.schema.json"), "{}").unwrap();
    let referencing = Contract::load(
        checkout.path(),
        &Vendored {
            schema: "schemas/vendor/a.schema.json".to_owned(),
            sha256: String::new(),
            encoded_sha256: None,
        },
        &[],
        &[Vendored {
            schema: "schemas/vendor/a.schema.json".to_owned(),
            sha256: String::new(),
            encoded_sha256: None,
        }],
        false,
        &[],
        &[],
    );
    assert!(
        referencing.is_err(),
        "a resource without a $id has nothing to register"
    );
}
