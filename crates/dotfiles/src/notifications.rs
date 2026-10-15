//! Fixed prepared speech with optional lifecycle callbacks and a bounded native player.

use anyhow::{Context, Result, bail, ensure};
use clap::{Subcommand, ValueEnum};
use platform::speech;
use rules::notification_rules::{Decision, Desktop, Event, Frequency, Origin};
use sha2::{Digest, Sha256};
use std::fmt::Write as _;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

const MAX_CLIP_BYTES: u64 = 262_144;
const MAX_CLIP_MILLIS: u64 = 5_000;
const SAMPLE_RATE: u32 = 22_050;
const PCM_BYTES_PER_SECOND: u32 = SAMPLE_RATE * 2;
const PLAYER_TIMEOUT: Duration = Duration::from_secs(8);
const PLAYER_POLL: Duration = Duration::from_millis(20);
const MAX_CALLBACK_BYTES: u64 = 65_536;
const PREPARE_TIMEOUT: Duration = Duration::from_secs(60);
const MANIFEST_HEADER: &str = "dotfiles-notifications-v1";
const AUDIO: &str = "audio";
const MAC_PLAYER: &str = "/usr/bin/afplay";
const LINUX_PLAYERS: [&str; 2] = ["/usr/bin/paplay", "/usr/bin/aplay"];
const NIX_PLAYER: &str = ".nix-profile/bin/paplay";
const POWERSHELL: &str = "System32/WindowsPowerShell/v1.0/powershell.exe";
#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;
const LAST_PLAYBACK: &str = "last-playback.json";
const MAX_FORWARD_ARGUMENTS: usize = 32;
const MATERIAL_FIELD_COUNT: usize = 4;
const MATERIAL_SIZE_FIELD: usize = 2;
const MATERIAL_DURATION_FIELD: usize = 3;
const WAV_TAG_BYTES: usize = 4;
const RIFF_SIZE_OFFSET: usize = WAV_TAG_BYTES;
const RIFF_SIZE_PREFIX_BYTES: usize = 8;
const RIFF_HEADER_BYTES: usize = 12;
const WAV_CHUNK_HEADER_BYTES: usize = 8;
const PCM_FORMAT_BYTES: usize = 16;
const PCM_CHANNELS_OFFSET: usize = 2;
const PCM_SAMPLE_RATE_OFFSET: usize = 4;
const PCM_BYTE_RATE_OFFSET: usize = 8;
const PCM_BLOCK_ALIGN_OFFSET: usize = 12;
const PCM_SAMPLE_BITS_OFFSET: usize = 14;
const PCM_BLOCK_ALIGN: u16 = 2;
const PCM_SAMPLE_BITS: u16 = 16;
const WAV_PADDING_ALIGNMENT: usize = 2;
const MILLIS_PER_SECOND: u64 = 1_000;
const PRESETS: [(&str, &str, Event); 8] = [
    (
        "commit-started",
        "コミットを開始しました。",
        Event::CommitStarted,
    ),
    (
        "commit-failed",
        "コミットに失敗しました。",
        Event::CommitFailed,
    ),
    (
        "commit-completed",
        "コミットが完了しました。",
        Event::CommitCompleted,
    ),
    (
        "push-started",
        "プッシュを開始しました。",
        Event::PushStarted,
    ),
    ("push-failed", "プッシュに失敗しました。", Event::PushFailed),
    (
        "push-completed",
        "プッシュが完了しました。",
        Event::PushCompleted,
    ),
    (
        "turn-completed",
        "応答が完了しました。",
        Event::TurnCompleted,
    ),
    ("input-waiting", "入力をお願いします。", Event::InputWaiting),
];

#[derive(Clone, Copy, Debug, ValueEnum)]
pub enum Mode {
    Off,
    Attention,
    Results,
    All,
}
impl From<Mode> for Frequency {
    fn from(value: Mode) -> Self {
        match value {
            Mode::Off => Self::Off,
            Mode::Attention => Self::Attention,
            Mode::Results => Self::Results,
            Mode::All => Self::All,
        }
    }
}

#[derive(Debug, Subcommand)]
pub enum Action {
    /// Report configuration, prepared material, and native player availability without playing sound.
    Status,
    /// Generate the fixed Japanese waveform audio assets with an installed offline Windows voice.
    /// This command plays no audio.
    Prepare {
        #[arg(long)]
        output: PathBuf,
    },
    /// Preserve an owner frequency override across ordinary profile applications.
    Frequency { mode: Mode },
    /// Intentionally play one of the fixed prepared presets.
    Test {
        #[arg(value_parser = preset_names())]
        event: String,
    },
    /// Optional callback that receives a fixed event name and opaque identity.
    Emit {
        #[arg(value_parser = preset_names())]
        event: String,
        #[arg(long)]
        id: String,
    },
    /// Cancel a queued waiting request after the client observes its resolution.
    Cancel {
        #[arg(long)]
        id: String,
    },
    /// Supported client callback with no standard output and no assessment or tool invocation.
    Agent {
        #[arg(value_parser = ["codex", "claude"])]
        client: String,
        #[arg(long)]
        forward_notify: Option<String>,
        payload: Option<String>,
    },
    #[command(hide = true)]
    Worker {
        #[arg(long)]
        producer: String,
    },
}

fn preset_names() -> Vec<&'static str> {
    PRESETS.iter().map(|(name, _, _)| *name).collect()
}
fn selected(name: &str) -> Result<Event> {
    PRESETS
        .iter()
        .find(|(key, _, _)| *key == name)
        .map(|(_, _, event)| *event)
        .context("unknown spoken event")
}
#[cfg(test)]
fn key(event: Event) -> Result<&'static str> {
    PRESETS
        .iter()
        .find(|(_, _, candidate)| *candidate == event)
        .map(|(key, _, _)| *key)
        .context("unknown spoken event")
}
fn home() -> Result<PathBuf> {
    platform::home::home().context("native home is unavailable")
}
fn callback_id(raw: &str) -> Result<String> {
    ensure!(
        !raw.is_empty() && u64::try_from(raw.len())? <= MAX_CALLBACK_BYTES,
        "invalid notification identity"
    );
    Ok(speech::identity("callback", raw))
}

/// Optional callbacks preserve their host's original result and standard output.
/// Failed notification operations produce fixed nonfatal warnings on standard error.
///
/// # Errors
///
/// Returns an error when home discovery or an explicit preparation, configuration, or playback action fails.
pub fn run(action: Action) -> Result<()> {
    match action {
        Action::Status => {
            status(&home()?);
            Ok(())
        }
        Action::Prepare { output } => prepare(&output),
        Action::Frequency { mode } => {
            let path = home()?.join(speech::STATE);
            std::fs::create_dir_all(&path)?;
            std::fs::write(path.join(speech::MODE), speech::frequency_text(mode.into()))?;
            Ok(())
        }
        Action::Test { event } => {
            let home = home()?;
            ensure!(
                speech::desktop() == Desktop::Present,
                "no supported local desktop was observed"
            );
            play(&home, selected(&event)?)
        }
        Action::Emit { event, id } => {
            speech::report(emit(selected(&event)?, &id), speech::Operation::Queue);
            Ok(())
        }
        Action::Cancel { id } => {
            speech::report(cancel_callback(&id), speech::Operation::Cancel);
            Ok(())
        }
        Action::Agent {
            client,
            forward_notify,
            payload,
        } => {
            speech::report(
                agent(&client, forward_notify.as_deref(), payload.as_deref()),
                speech::Operation::Callback,
            );
            Ok(())
        }
        Action::Worker { producer } => {
            if producer == platform::BUILD_HASH {
                speech::report(worker(&home()?), speech::Operation::Worker);
            }
            Ok(())
        }
    }
}

fn emit(event: Event, raw_id: &str) -> Result<()> {
    let Some(home) = platform::home::home() else {
        return Ok(());
    };
    if speech::frequency(&home).unwrap_or(Frequency::Off) == Frequency::Off
        || speech::desktop() != Desktop::Present
    {
        return Ok(());
    }
    if !home.join(speech::CONFIG).join(speech::MATERIALS).is_file() {
        return Ok(());
    }
    let Ok(id) = callback_id(raw_id) else {
        return Ok(());
    };
    if matches!(
        speech::enqueue(&home, event, &id, speech::now()?, Origin::Agent)?,
        Decision::Queue(_)
    ) {
        speech::launch_worker(&home)?;
    }
    Ok(())
}

fn cancel_callback(raw_id: &str) -> Result<()> {
    let Some(home) = platform::home::home() else {
        return Ok(());
    };
    let Ok(id) = callback_id(raw_id) else {
        return Ok(());
    };
    Ok(speech::cancel(&home, &id)?)
}

fn agent(client: &str, forward: Option<&str>, argument: Option<&str>) -> Result<()> {
    let payload = if let Some(text) = argument {
        text.to_owned()
    } else {
        let mut text = String::new();
        match std::io::stdin()
            .take(MAX_CALLBACK_BYTES + 1)
            .read_to_string(&mut text)
        {
            Ok(_) => {}
            Err(cause) if cause.kind() == std::io::ErrorKind::InvalidData => return Ok(()),
            Err(cause) => return Err(cause.into()),
        }
        text
    };
    if u64::try_from(payload.len())? > MAX_CALLBACK_BYTES {
        return Ok(());
    }
    // A pre-existing callback keeps its exact payload regardless of speech availability.
    if let Some(forward) = forward {
        speech::report(
            forward_notify(forward, &payload),
            speech::Operation::Forward,
        );
    }
    let Some(native_home) = platform::home::home() else {
        return Ok(());
    };
    if speech::frequency(&native_home).unwrap_or(Frequency::Off) == Frequency::Off
        || speech::desktop() != Desktop::Present
    {
        return Ok(());
    }
    let Ok(input) = serde_json::from_str::<serde_json::Value>(&payload) else {
        return Ok(());
    };
    match client {
        "codex"
            if input.get("type").and_then(serde_json::Value::as_str)
                == Some("agent-turn-complete") =>
        {
            let Some(thread) = input.get("thread-id").and_then(serde_json::Value::as_str) else {
                return Ok(());
            };
            let turn = input.get("turn-id").and_then(serde_json::Value::as_str);
            let opaque = turn.map_or_else(
                || speech::identity("codex-legacy-payload", &payload),
                str::to_owned,
            );
            emit(Event::TurnCompleted, &format!("codex:{thread}:{opaque}"))?;
        }
        "claude" => claude_callback(&native_home, &input, &payload)?,
        _ => {}
    }
    Ok(())
}

fn claude_callback(home: &Path, input: &serde_json::Value, payload: &str) -> Result<()> {
    let Some(session) = input.get("session_id").and_then(serde_json::Value::as_str) else {
        return Ok(());
    };
    let scope = speech::identity("claude-waiting-scope", session);
    let hook_event = input
        .get("hook_event_name")
        .and_then(serde_json::Value::as_str);
    if matches!(
        hook_event,
        Some("UserPromptSubmit" | "PostToolUse" | "PostToolUseFailure")
    ) {
        speech::resolve_waiting(home, &scope)?;
        return Ok(());
    }
    if hook_event != Some("Notification") {
        return Ok(());
    }
    let kind = input
        .get("notification_type")
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default();
    if !matches!(
        kind,
        "permission_prompt" | "idle_prompt" | "elicitation_dialog"
    ) {
        return Ok(());
    }
    let Some(transcript) = input
        .get("transcript_path")
        .and_then(serde_json::Value::as_str)
    else {
        return Ok(());
    };
    let metadata = match std::fs::symlink_metadata(transcript) {
        Ok(metadata) => metadata,
        Err(cause) if cause.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(cause) => return Err(cause.into()),
    };
    if !metadata.is_file() {
        return Ok(());
    }
    let Ok(changed) = metadata.modified()?.duration_since(std::time::UNIX_EPOCH) else {
        return Ok(());
    };
    let identity = speech::identity("claude-notification-payload", payload);
    let raw = format!(
        "claude:{session}:{identity}:{}:{}",
        metadata.len(),
        changed.as_nanos()
    );
    let Ok(id) = callback_id(&raw) else {
        return Ok(());
    };
    speech::remember_waiting(home, &scope, &id)?;
    emit(Event::InputWaiting, &raw)
}

fn forward_notify(vector: &str, payload: &str) -> Result<()> {
    let Ok(callback) = serde_json::from_str::<Vec<String>>(vector) else {
        return Ok(());
    };
    if callback.len() > MAX_FORWARD_ARGUMENTS {
        return Ok(());
    }
    let Some((program, args)) = callback.split_first() else {
        return Ok(());
    };
    if program.is_empty() {
        return Ok(());
    }
    let mut command = platform::process::command(program);
    command.args(args).arg(payload).stdin(Stdio::null());
    bounded(&mut command, PLAYER_TIMEOUT)?;
    Ok(())
}

fn status(home: &Path) {
    let materials = prepared_materials(&home.join(speech::CONFIG));
    let player = player();
    println!(
        "{}",
        serde_json::json!({
            "frequency": speech::frequency(home).map_or_else(|_| "unavailable".into(), |mode| speech::frequency_text(mode).trim().to_owned()),
            "desktop": format!("{:?}", speech::desktop()),
            "materials_valid": materials.is_ok(),
            "materials_error": materials.err().map(|cause| cause.to_string()),
            "player": player.as_ref().ok().map(|command| command.get_program().to_string_lossy().into_owned()),
            "player_error": player.err().map(|cause| cause.to_string()),
            "last_playback": std::fs::read(home.join(speech::STATE).join(LAST_PLAYBACK)).ok().and_then(|bytes| serde_json::from_slice::<serde_json::Value>(&bytes).ok()),
            "playback_attempted_by_status": false, "audible_confirmation": false,
            "agent_boundaries": { "codex_complete": "notify:agent-turn-complete", "codex_input_wait": "unavailable", "claude_complete": "unavailable", "claude_input_wait": "Notification", "opencode": "observed run and supported V1 events" }
        })
    );
}

fn worker(home: &Path) -> Result<()> {
    let mut lock = Some(speech::worker_lock(home)?);
    while let Some(packet) = speech::next(home, &mut lock)? {
        if matches!(speech::pending_decision(home, &packet)?, Decision::Queue(_)) {
            let result = play(home, packet.event);
            let receipt = serde_json::json!({ "event": packet.event as u8, "opaque_id": packet.id,
                "request_observed": true, "player_completed": result.is_ok(), "audible_confirmation": false,
                "observed_millis": speech::now()? });
            std::fs::write(
                home.join(speech::STATE).join(LAST_PLAYBACK),
                serde_json::to_vec(&receipt)?,
            )?;
        }
    }
    Ok(())
}

#[derive(Debug)]
pub struct PreparedMaterial {
    pub event: Event,
    pub name: &'static str,
    pub bytes: Vec<u8>,
}

/// Reads each fixed pulse-code modulation asset only after its format, size, duration, and manifest digest match.
///
/// # Errors
///
/// Returns an error for missing, malformed, linked, or changed material.
pub fn prepared_materials(directory: &Path) -> Result<Vec<PreparedMaterial>> {
    let manifest = directory.join(speech::MATERIALS);
    let metadata = std::fs::symlink_metadata(&manifest)?;
    ensure!(
        metadata.is_file() && metadata.len() <= MAX_CALLBACK_BYTES,
        "speech manifest must be a bounded regular file"
    );
    let text = std::fs::read_to_string(manifest)?;
    let mut lines = text.lines();
    ensure!(
        lines.next() == Some(MANIFEST_HEADER),
        "unsupported speech manifest"
    );
    let mut found = Vec::new();
    for (name, _, event) in PRESETS {
        let line = lines.next().context("missing fixed speech material")?;
        let fields = line.split('\t').collect::<Vec<_>>();
        ensure!(
            fields.len() == MATERIAL_FIELD_COUNT && fields.first() == Some(&name),
            "unexpected or duplicate speech material"
        );
        let expected_hash = fields.get(1).context("missing material hash")?;
        let expected_bytes = fields
            .get(MATERIAL_SIZE_FIELD)
            .context("missing material size")?
            .parse::<u64>()?;
        let expected_duration = fields
            .get(MATERIAL_DURATION_FIELD)
            .context("missing material duration")?
            .parse::<u64>()?;
        let path = directory.join(AUDIO).join(format!("{name}.wav"));
        let (bytes, duration) = wav(&path)?;
        let hash = platform::build_inputs::hex(&Sha256::digest(&bytes));
        ensure!(
            hash.as_str() == *expected_hash
                && u64::try_from(bytes.len())? == expected_bytes
                && duration == expected_duration,
            "prepared speech differs from its manifest"
        );
        found.push(PreparedMaterial { event, name, bytes });
    }
    ensure!(lines.next().is_none(), "unexpected extra speech material");
    Ok(found)
}

fn le_u32(bytes: &[u8], at: usize) -> Result<u32> {
    Ok(u32::from_le_bytes(
        bytes
            .get(at..at + std::mem::size_of::<u32>())
            .context("truncated WAV integer")?
            .try_into()?,
    ))
}
fn le_u16(bytes: &[u8], at: usize) -> Result<u16> {
    Ok(u16::from_le_bytes(
        bytes
            .get(at..at + std::mem::size_of::<u16>())
            .context("truncated WAV integer")?
            .try_into()?,
    ))
}

fn wav(path: &Path) -> Result<(Vec<u8>, u64)> {
    let metadata = std::fs::symlink_metadata(path)?;
    ensure!(
        metadata.is_file() && metadata.len() <= MAX_CLIP_BYTES,
        "speech must be a bounded regular WAV file"
    );
    let bytes = std::fs::read(path)?;
    let duration = pcm_duration(&bytes)?;
    Ok((bytes, duration))
}

fn pcm_duration(bytes: &[u8]) -> Result<u64> {
    ensure!(
        bytes.get(..WAV_TAG_BYTES) == Some(b"RIFF")
            && bytes.get(RIFF_SIZE_PREFIX_BYTES..RIFF_HEADER_BYTES) == Some(b"WAVE"),
        "speech must use RIFF/WAVE"
    );
    ensure!(
        usize::try_from(le_u32(bytes, RIFF_SIZE_OFFSET)?)?.checked_add(RIFF_SIZE_PREFIX_BYTES)
            == Some(bytes.len()),
        "WAV length differs from its header"
    );
    let mut at = RIFF_HEADER_BYTES;
    let mut format = false;
    let mut data = None;
    while at < bytes.len() {
        let tag = bytes
            .get(at..at + WAV_TAG_BYTES)
            .context("truncated WAV chunk")?;
        let len = usize::try_from(le_u32(bytes, at + WAV_TAG_BYTES)?)?;
        let start = at
            .checked_add(WAV_CHUNK_HEADER_BYTES)
            .context("WAV offset overflow")?;
        let end = start.checked_add(len).context("WAV length overflow")?;
        ensure!(bytes.get(start..end).is_some(), "truncated WAV data");
        if tag == b"fmt " {
            ensure!(
                !format
                    && len >= PCM_FORMAT_BYTES
                    && le_u16(bytes, start)? == 1
                    && le_u16(bytes, start + PCM_CHANNELS_OFFSET)? == 1
                    && le_u32(bytes, start + PCM_SAMPLE_RATE_OFFSET)? == SAMPLE_RATE
                    && le_u32(bytes, start + PCM_BYTE_RATE_OFFSET)? == PCM_BYTES_PER_SECOND
                    && le_u16(bytes, start + PCM_BLOCK_ALIGN_OFFSET)? == PCM_BLOCK_ALIGN
                    && le_u16(bytes, start + PCM_SAMPLE_BITS_OFFSET)? == PCM_SAMPLE_BITS,
                "speech must be mono 22050 Hz 16-bit PCM"
            );
            format = true;
        } else if tag == b"data" {
            ensure!(
                data.is_none() && len > 0 && len % usize::from(PCM_BLOCK_ALIGN) == 0,
                "missing or duplicate PCM samples"
            );
            data = Some(u64::try_from(len)?);
        }
        at = end
            .checked_add(len % WAV_PADDING_ALIGNMENT)
            .context("WAV padding overflow")?;
    }
    let length = data.context("WAV samples are unavailable")?;
    let duration = length * MILLIS_PER_SECOND / u64::from(PCM_BYTES_PER_SECOND);
    ensure!(
        format && at == bytes.len() && duration > 0 && duration <= MAX_CLIP_MILLIS,
        "speech duration exceeds its bound"
    );
    Ok(duration)
}

fn native_powershell() -> Result<PathBuf> {
    let system = std::env::var_os("SystemRoot").map_or_else(
        || PathBuf::from(platform::home::DEFAULT_SYSTEM_ROOT),
        PathBuf::from,
    );
    let executable = system.join(POWERSHELL);
    ensure!(
        executable.is_absolute() && executable.is_file(),
        "native Windows PowerShell is unavailable"
    );
    Ok(executable)
}

fn player() -> Result<Command> {
    if cfg!(target_os = "macos") {
        ensure!(
            Path::new(MAC_PLAYER).is_file(),
            "native afplay is unavailable"
        );
        return Ok(platform::process::command(MAC_PLAYER));
    }
    if cfg!(windows) {
        let mut command = platform::process::command(native_powershell()?);
        command.args([
            "-NoLogo",
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            WINDOWS_PLAY,
        ]);
        hidden(&mut command);
        return Ok(command);
    }
    if let Some(nix_player) = platform::home::home().map(|home| home.join(NIX_PLAYER))
        && nix_player.is_absolute()
        && nix_player.is_file()
    {
        return Ok(platform::process::command(nix_player));
    }
    for path in LINUX_PLAYERS {
        if Path::new(path).is_file() {
            return Ok(platform::process::command(path));
        }
    }
    bail!("no supported native PCM player is available")
}

fn hidden(command: &mut Command) {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    #[cfg(not(windows))]
    let _ = command;
}

fn play(home: &Path, event: Event) -> Result<()> {
    let directory = home.join(speech::CONFIG);
    let found = prepared_materials(&directory)?;
    let material = found
        .iter()
        .find(|clip| clip.event == event)
        .context("fixed material is unavailable")?;
    let path = directory.join(AUDIO).join(format!("{}.wav", material.name));
    let mut command = player()?;
    if cfg!(windows) {
        command.env("DOTFILES_SPEECH_WAV", &path);
    } else {
        command.arg(&path);
    }
    command
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    bounded(&mut command, PLAYER_TIMEOUT)
}

fn bounded(command: &mut Command, timeout: Duration) -> Result<()> {
    let mut child = command.spawn()?;
    let began = Instant::now();
    loop {
        if let Some(status) = child.try_wait()? {
            ensure!(status.success(), "native notification process failed");
            return Ok(());
        }
        if began.elapsed() >= timeout {
            speech::report(child.kill(), speech::Operation::Stop);
            speech::report(child.wait(), speech::Operation::Reap);
            bail!("native notification process exceeded its deadline");
        }
        std::thread::sleep(PLAYER_POLL);
    }
}

const WINDOWS_PLAY: &str = r"$ErrorActionPreference='Stop'; $player=New-Object System.Media.SoundPlayer $env:DOTFILES_SPEECH_WAV; try { $player.Load(); $player.PlaySync() } finally { $player.Dispose() }";
const WINDOWS_PREPARE: &str = r"
$ErrorActionPreference='Stop'
Add-Type -AssemblyName System.Speech
$presets=ConvertFrom-Json $env:DOTFILES_SPEECH_PRESETS
foreach($preset in $presets) {
  $voice=New-Object System.Speech.Synthesis.SpeechSynthesizer
  try {
    $japanese=@($voice.GetInstalledVoices() | Where-Object { $_.Enabled -and $_.VoiceInfo.Culture.Name -eq 'ja-JP' } | Sort-Object { $_.VoiceInfo.Name })
    if($japanese.Count -eq 0) { throw 'No enabled offline Japanese voice is installed' }
    $voice.SelectVoice($japanese[0].VoiceInfo.Name)
    $format=New-Object System.Speech.AudioFormat.SpeechAudioFormatInfo(22050,[System.Speech.AudioFormat.AudioBitsPerSample]::Sixteen,[System.Speech.AudioFormat.AudioChannel]::Mono)
    $voice.SetOutputToWaveFile((Join-Path $env:DOTFILES_SPEECH_OUTPUT ($preset.key+'.wav')),$format)
    $voice.Speak($preset.text)
    $voice.SetOutputToNull()
  } finally { $voice.Dispose() }
}
";

fn prepare(output: &Path) -> Result<()> {
    ensure!(
        cfg!(windows),
        "the managed offline preparation currently requires an installed Windows Japanese voice; prepared WAVs work on all supported desktops"
    );
    ensure!(
        !output.exists(),
        "preparation output already exists; preserve it and choose a new owned directory"
    );
    let parent = output
        .parent()
        .context("preparation output has no parent")?;
    std::fs::create_dir_all(parent)?;
    let staging = tempfile::tempdir_in(parent)?;
    std::fs::create_dir(staging.path().join(AUDIO))?;
    let presets = PRESETS
        .iter()
        .map(|(key, text, _)| serde_json::json!({ "key": key, "text": text }))
        .collect::<Vec<_>>();
    let mut command = platform::process::command(native_powershell()?);
    command
        .args([
            "-NoLogo",
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            WINDOWS_PREPARE,
        ])
        .env("DOTFILES_SPEECH_PRESETS", serde_json::to_string(&presets)?)
        .env("DOTFILES_SPEECH_OUTPUT", staging.path().join(AUDIO))
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    hidden(&mut command);
    bounded(&mut command, PREPARE_TIMEOUT)?;
    let mut manifest = format!("{MANIFEST_HEADER}\n");
    for (name, _, _) in PRESETS {
        let (bytes, duration) = wav(&staging.path().join(AUDIO).join(format!("{name}.wav")))?;
        let hash = platform::build_inputs::hex(&Sha256::digest(&bytes));
        writeln!(manifest, "{name}\t{hash}\t{}\t{duration}", bytes.len())?;
    }
    std::fs::write(staging.path().join(speech::MATERIALS), manifest)?;
    prepared_materials(staging.path())?;
    std::fs::rename(staging.path(), output)?;
    println!(
        "prepared {} fixed Japanese WAVs; no playback attempted",
        PRESETS.len()
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn pcm() -> Vec<u8> {
        let length = PCM_BYTES_PER_SECOND;
        let mut bytes = Vec::new();
        bytes.extend(b"RIFF");
        bytes.extend((length + 36).to_le_bytes());
        bytes.extend(b"WAVEfmt ");
        bytes.extend(16u32.to_le_bytes());
        bytes.extend(1u16.to_le_bytes());
        bytes.extend(1u16.to_le_bytes());
        bytes.extend(SAMPLE_RATE.to_le_bytes());
        bytes.extend(PCM_BYTES_PER_SECOND.to_le_bytes());
        bytes.extend(2u16.to_le_bytes());
        bytes.extend(16u16.to_le_bytes());
        bytes.extend(b"data");
        bytes.extend(length.to_le_bytes());
        bytes.resize(bytes.len() + usize::try_from(length).unwrap(), 0);
        bytes
    }
    #[test]
    fn prepared_pcm_refuses_truncation_wrong_format_and_unbounded_duration() {
        let valid = pcm();
        assert_eq!(pcm_duration(&valid).unwrap(), 1000);
        assert!(pcm_duration(&valid[..valid.len() - 1]).is_err());
        let mut wrong = valid.clone();
        wrong[20] = 3;
        assert!(pcm_duration(&wrong).is_err());
        let mut huge = valid;
        huge.resize(usize::try_from(PCM_BYTES_PER_SECOND).unwrap() * 6 + 44, 0);
        let samples = PCM_BYTES_PER_SECOND * 6;
        huge[4..8].copy_from_slice(&(samples + 36).to_le_bytes());
        huge[40..44].copy_from_slice(&samples.to_le_bytes());
        assert!(pcm_duration(&huge).is_err());
    }
    #[test]
    fn fixed_materials_refuse_unknown_event_and_callback_identity_is_opaque() {
        assert!(selected("read private token aloud").is_err());
        for (_, _, event) in PRESETS {
            assert!(selected(key(event).unwrap()).is_ok());
        }
        let id = callback_id("private conversation and path").unwrap();
        assert_eq!(id.len(), 64);
        assert!(!id.contains("private"));
        assert!(callback_id("").is_err());
    }
}
