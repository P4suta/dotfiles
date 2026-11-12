#![no_main]

use platform::upstream::{Preserved, Source, parse, parse_git, preserved_members};
use sha2::{Digest, Sha256};
use std::path::{Component, Path};

const DIRECTORY: &str = "vendor/publisher";
const PREFIX: &str = "trouble-host-0.8.0";
const ORIGIN: &str = "https://crates.io/crates/trouble-host/0.8.0";

fn digest(bytes: &[u8]) -> String {
    platform::build_inputs::hex(&Sha256::digest(bytes))
}

libfuzzer_sys::fuzz_target!(|data: &[u8]| {
    if let Ok(checked) = parse_git(data) {
        assert_eq!(
            parse_git(&serde_json::to_vec(&checked).unwrap()).unwrap(),
            checked
        );
    }
    let directory = Path::new(DIRECTORY);
    let manifest = directory.join("SHA256SUMS");
    if let Ok(members) = preserved_members(directory, &manifest, data) {
        let mut encoded = String::new();
        for (path, digest) in &members {
            let suffix = Path::new(path)
                .strip_prefix(directory)
                .unwrap()
                .to_str()
                .unwrap();
            encoded.push_str(&format!("{digest}  {suffix}\n"));
        }
        assert_eq!(
            preserved_members(directory, &manifest, encoded.as_bytes()).unwrap(),
            members
        );
    }
    if let Ok(checked) = parse(data) {
        assert_eq!(
            parse(&serde_json::to_vec(&checked).unwrap()).unwrap(),
            checked
        );
    }
    if let Ok(source) = serde_json::from_slice::<Source>(data)
        && source.validate().is_ok()
    {
        assert_eq!(
            serde_json::from_slice::<Source>(&serde_json::to_vec(&source).unwrap()).unwrap(),
            source
        );
    }
    if let Ok(source) = serde_json::from_slice::<Preserved>(data)
        && source.validate().is_ok()
    {
        assert_eq!(
            serde_json::from_slice::<Preserved>(&serde_json::to_vec(&source).unwrap()).unwrap(),
            source
        );
    }
    if rules::third_party_rules::normalized_relative(data)
        && let Ok(text) = std::str::from_utf8(data)
    {
        assert!(
            Path::new(text)
                .components()
                .all(|part| matches!(part, Component::Normal(_)))
        );
    }
    let mut source = Source {
        repository: std::env::current_dir().unwrap(),
        directory: DIRECTORY.into(),
        archive: format!("{DIRECTORY}/upstream.crate").into(),
        prefix: PREFIX.into(),
        sha256: digest(data),
        origin: ORIGIN.into(),
    };
    if let Ok(originals) = source.originals(data) {
        let mut archive = tar::Builder::new(flate2::write::GzEncoder::new(
            Vec::new(),
            flate2::Compression::default(),
        ));
        for (path, bytes) in &originals {
            assert!(rules::third_party_rules::normalized_relative(
                path.as_bytes()
            ));
            let suffix = Path::new(path).strip_prefix(DIRECTORY).unwrap();
            let mut header = tar::Header::new_gnu();
            header.set_size(u64::try_from(bytes.len()).unwrap());
            header.set_mode(0o644);
            header.set_cksum();
            archive
                .append_data(
                    &mut header,
                    Path::new(PREFIX).join(suffix),
                    bytes.as_slice(),
                )
                .unwrap();
        }
        let bytes = archive.into_inner().unwrap().finish().unwrap();
        source.sha256 = digest(&bytes);
        assert_eq!(source.originals(&bytes).unwrap(), originals);
    }
});
