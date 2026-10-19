use anyhow::{Context, Result, ensure};
use dotfiles::artifacts::{
    Inputs, NativeConfiguration, NativeInputs, NativeWorkload, Reason, Target,
};
use std::fs;
use std::path::{Path, PathBuf};

#[path = "support/host_materials.rs"]
mod host_materials;

#[path = "../examples/support/artifact_material_corpus.rs"]
mod corpus;

const PIN: &str = "2.73.0";
const CHECKSUM: &str = "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";
const GLOBAL: &str = "[tools]\nglobal = '1.2.3'\n";

#[test]
fn native_decoder_corpus_preserves_provider_lock_and_answers_refusals() -> Result<()> {
    let mut count = 0;
    let mut mise = 0;
    for case in corpus::cases()? {
        let packet: corpus::Packet = serde_json::from_slice(&case.bytes)?;
        if packet.native.is_none() && !corpus::has_mise(&packet) {
            continue;
        }
        count += usize::from(packet.native.is_some());
        mise += usize::from(corpus::has_mise(&packet));
        assert!(case.bytes.len() <= corpus::INPUT_BYTE_LIMIT);
        let result = corpus::validate_native(&packet).and_then(|()| corpus::validate_mise(&packet));
        match case.expected {
            corpus::Expected::Refused => assert!(result.is_err(), "{}", case.name),
            corpus::Expected::Plan => result.with_context(|| case.name)?,
            corpus::Expected::Materials => {
                anyhow::bail!("native metadata corpus cannot assert consumed materials")
            }
        }
    }
    assert_eq!(count, 9);
    assert_eq!(mise, corpus::MISE_CASE_COUNT);
    assert_eq!(count + mise, 25);
    Ok(())
}

fn a_single_verifier_archive(
    name: &str,
    version: &str,
    backend: &str,
) -> Result<NativeConfiguration> {
    let project = format!("[tools]\n{name:?}={version:?}\n");
    let lock = format!(
        "lockfile_version=3\n[[tools.{name:?}]]\nversion={version:?}\nbackend={backend:?}\nspecifiers=[{version:?}]\n[tools.{name:?}.\"platforms.macos-arm64\"]\nurl='https://example.com/verifier.tar.gz'\nchecksum='sha256:{CHECKSUM}'\n"
    );
    let versions =
        format!("[mise]\nversion='2026.10.7'\n[mise.sha256]\nmacos-arm64='{CHECKSUM}'\n");
    NativeConfiguration::parse(
        Inputs {
            project: &project,
            mise_lock: &lock,
            cargo_lock: "version=4\n",
            versions: &versions,
        },
        Target::MacosArm64,
    )
}

#[test]
fn a_rust_archive_does_not_close_component_materials() -> Result<()> {
    let materials = a_single_verifier_archive("rust", "1.99.0", "core:rust")?;
    assert!(
        materials.complete().is_err(),
        "one archive cannot supply the requested compiler, standard library, components and installer"
    );
    Ok(())
}

#[test]
fn a_kani_launcher_archive_does_not_close_verifier_materials() -> Result<()> {
    let materials =
        a_single_verifier_archive("cargo:kani-verifier", "0.68.0", "cargo:kani-verifier")?;
    assert!(
        materials.complete().is_err(),
        "the launcher archive cannot supply its locked source dependencies, verifier bundle and compiler"
    );
    Ok(())
}

fn locked(name: &str, version: &str, backend: &str) -> String {
    format!(
        "[[tools.{name}]]\nversion={version:?}\nbackend={backend:?}\nspecifiers=[{version:?}]\n[tools.{name}.\"platforms.windows-x64\"]\nurl='https://example.com/{name}.zip'\nchecksum='sha256:{CHECKSUM}'\n"
    )
}

struct Fixture {
    _scope: tempfile::TempDir,
    root: PathBuf,
    home: PathBuf,
    configuration: PathBuf,
    lock: PathBuf,
    renderer: PathBuf,
}

impl Fixture {
    fn new() -> Result<Self> {
        let scope = tempfile::tempdir()?;
        let root = scope.path().join("source");
        let home = scope.path().join("destination");
        fs::create_dir_all(root.join("home/dot_config/mise"))?;
        fs::create_dir_all(root.join("home/.chezmoidata"))?;
        fs::create_dir(&home)?;
        fs::write(root.join(".chezmoiroot"), "home\n")?;
        host_materials::seed(&root, &home)?;
        fs::write(
            root.join("mise.toml"),
            format!("[tools]\nchezmoi='{PIN}'\nproject='1.2.3'\n"),
        )?;
        fs::write(
            root.join("mise.lock"),
            format!(
                "lockfile_version=3\n{}{}",
                locked("chezmoi", PIN, "aqua:twpayne/chezmoi"),
                locked("project", "1.2.3", "aqua:owner/project")
            ),
        )?;
        fs::write(
            root.join("home/.chezmoidata/versions.toml"),
            format!("[mise]\nversion='2026.10.6'\n[mise.sha256]\nwindows-x64='{CHECKSUM}'\n"),
        )?;
        fs::write(root.join("home/dot_config/mise/config.toml.tmpl"), GLOBAL)?;
        let configuration = scope.path().join("chezmoi.toml");
        fs::write(&configuration, "[data]\nprofile='windows'\n")?;
        let lock = scope.path().join("native-mise.lock");
        fs::write(
            &lock,
            format!(
                "lockfile_version=3\n{}",
                locked("global", "1.2.3", "aqua:owner/global")
            ),
        )?;
        let resolved = platform::tool::Tool::Mise
            .command()
            .args(["which", "chezmoi"])
            .output()?;
        ensure!(
            resolved.status.success(),
            "the native material fixture requires the installed pinned renderer"
        );
        let renderer =
            platform::paths::canonical(Path::new(std::str::from_utf8(&resolved.stdout)?.trim()))?;
        Ok(Self {
            _scope: scope,
            root,
            home,
            configuration,
            lock,
            renderer,
        })
    }

    fn capture(&self) -> Result<NativeWorkload> {
        NativeWorkload::capture(
            &self.root,
            Target::WindowsX64,
            NativeInputs {
                configuration: &self.configuration,
                lock: &self.lock,
                renderer: &self.renderer,
                destination: &self.home,
            },
        )
    }
}

#[test]
fn actual_rendered_globals_and_all_source_locks_are_named_acquisition_members() -> Result<()> {
    let fixture = Fixture::new()?;
    let workload = fixture.capture()?;
    let plan = workload.plan()?;
    plan.complete_acquisition()?;
    assert_eq!(plan.members().len(), 3);
    assert!(
        plan.members()[1]
            .artifacts()
            .iter()
            .any(|artifact| artifact.subject().starts_with("global@"))
    );
    assert!(
        plan.members()[2]
            .artifacts()
            .iter()
            .any(|artifact| artifact.subject() == "crate:host-fixture-material@1.2.3")
    );
    let report = serde_json::to_string(&plan)?;
    assert!(report.contains("native-tool-installation"));
    assert!(report.contains("independent-live-health"));
    assert!(!report.contains(fixture.home.to_str().context("fixture destination")?));
    assert!(!report.contains(fixture.root.to_str().context("fixture Source")?));
    Ok(())
}

#[test]
fn rendered_global_tool_omission_and_wrong_locks_refuse_the_complete_scope() -> Result<()> {
    let fixture = Fixture::new()?;
    for global in [
        "[tools]\nglobal='1.2.4'\n",
        "[tools]\nglobal='1.2.3'\nmissing='9.0.0'\n",
        "[tools]\nglobal='latest'\n",
    ] {
        fs::write(
            fixture.root.join("home/dot_config/mise/config.toml.tmpl"),
            global,
        )?;
        let plan = fixture.capture()?.plan()?;
        assert!(plan.complete_acquisition().is_err(), "{global}");
        assert_ne!(plan.members()[1].diagnostics(), []);
    }
    Ok(())
}

#[test]
fn aliases_and_unknown_native_providers_require_their_own_material_closure() -> Result<()> {
    let fixture = Fixture::new()?;
    fs::write(
        fixture.root.join("home/dot_config/mise/config.toml.tmpl"),
        format!("{GLOBAL}\n[tool_alias]\nglobal='asdf:owner/provider'\n"),
    )?;
    let plan = fixture.capture()?.plan()?;
    assert!(plan.complete_acquisition().is_err());
    assert!(
        plan.diagnostics()
            .iter()
            .any(|diagnostic| diagnostic.reason == Reason::MissingDependency)
    );
    fs::write(
        fixture.root.join("home/dot_config/mise/config.toml.tmpl"),
        GLOBAL,
    )?;
    fs::write(
        &fixture.lock,
        format!(
            "lockfile_version=3\n{}",
            locked("global", "1.2.3", "vfox:unknown/provider")
        ),
    )?;
    let plan = fixture.capture()?.plan()?;
    assert!(plan.complete_acquisition().is_err());
    assert!(
        plan.diagnostics()
            .iter()
            .any(|diagnostic| diagnostic.reason == Reason::UnsupportedBackend)
    );
    fs::write(
        &fixture.lock,
        format!(
            "lockfile_version=3\n{}",
            locked("global", "1.2.3", "aqua:owner/global")
        ),
    )?;
    fs::write(
        fixture.root.join("mise.njutest.toml"),
        "[tools]\nassessment='1.0.0'\n",
    )?;
    let plan = fixture.capture()?.plan()?;
    assert!(plan.complete_acquisition().is_err());
    assert!(
        plan.diagnostics()
            .iter()
            .any(
                |diagnostic| diagnostic.subject == "project-environment:mise.njutest.toml"
                    && diagnostic.reason == Reason::MissingDependency
            )
    );
    Ok(())
}

#[test]
fn current_source_and_native_profile_cannot_be_substituted_after_rendering() -> Result<()> {
    let fixture = Fixture::new()?;
    let workload = fixture.capture()?;
    let before = workload.plan()?.digest();
    fs::write(
        fixture.root.join("mise.assessment.TOML"),
        "[tools]\nassessment='1.0.0'\n",
    )?;
    assert!(workload.plan().is_err());
    let additional = fixture.capture()?.plan()?;
    assert!(additional.complete_acquisition().is_err());
    assert!(
        additional
            .diagnostics()
            .iter()
            .any(
                |diagnostic| diagnostic.subject == "project-environment:mise.assessment.TOML"
                    && diagnostic.reason == Reason::MissingDependency
            )
    );
    fs::write(
        fixture.root.join("home/dot_config/mise/config.toml.tmpl"),
        format!("{GLOBAL}\n# changed Source\n"),
    )?;
    assert!(workload.plan().is_err());
    assert_ne!(fixture.capture()?.plan()?.digest(), before);
    fs::write(&fixture.configuration, "[data]\nprofile='mac'\n")?;
    assert!(fixture.capture().is_err());
    Ok(())
}

#[test]
fn caller_rendered_bytes_and_a_different_renderer_pin_cannot_replace_source_capture() -> Result<()>
{
    let fixture = Fixture::new()?;
    let caller_configuration = fixture.root.join("caller-chezmoi.toml");
    fs::write(&caller_configuration, "[data]\nprofile='windows'\n")?;
    assert!(
        NativeWorkload::capture(
            &fixture.root,
            Target::WindowsX64,
            NativeInputs {
                configuration: &caller_configuration,
                lock: &fixture.lock,
                renderer: &fixture.renderer,
                destination: &fixture.home,
            }
        )
        .is_err()
    );
    fs::write(
        fixture.root.join("mise.toml"),
        "[tools]\nchezmoi='2.72.0'\nproject='1.2.3'\n",
    )?;
    assert!(fixture.capture().is_err());
    fs::write(
        fixture.root.join("mise.toml"),
        format!("[tools]\nchezmoi='{PIN}'\nproject='1.2.3'\n"),
    )?;
    fs::write(
        fixture.root.join("home/dot_config/mise/config.toml.tmpl"),
        "{{ fail \"fixture refuses actual Source rendering\" }}",
    )?;
    assert!(fixture.capture().is_err());
    Ok(())
}
