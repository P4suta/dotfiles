use anyhow::{Result, ensure};
use serde_json::{Value, json};
use std::path::Path;

#[test]
fn machine_local_chat_peers_override_the_public_default() -> Result<()> {
    let scope = tempfile::tempdir()?;
    let config = scope.path().join("chezmoi.toml");
    let home = scope.path().join("home");
    std::fs::create_dir(&home)?;
    std::fs::write(
        &config,
        "[data]\nprofile = 'windows'\n[data.domyjob.chat]\npeers = ['fixture-peer-one', 'fixture-peer-two']\n",
    )?;
    let source = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap();
    let output = dotfiles::profiles::chezmoi(source, scope.path(), &config, &home)
        .args(["data", "--format", "json"])
        .output()?;
    ensure!(
        output.status.success(),
        "chezmoi data: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let data: Value = serde_json::from_slice(&output.stdout)?;
    assert_eq!(
        data["domyjob"]["chat"]["peers"],
        json!(["fixture-peer-one", "fixture-peer-two"])
    );
    scope.close()?;
    Ok(())
}

fn chat(state: &Path, actor: &str, arguments: &[&str]) -> Result<Value> {
    let mut command = platform::tool::Tool::Domyjob.command();
    command
        .current_dir(state.parent().unwrap())
        .env("DOMYJOB_STATE", state)
        .env_remove("DOMYJOB_CHAT_AGENT")
        .env_remove("DOMYJOB_CHAT_TURN")
        .args(["chat", "--json"]);
    if !actor.is_empty() {
        command.args(["--as", actor]);
    }
    let output = command.args(arguments).output()?;
    ensure!(
        output.status.success(),
        "domyjob chat {arguments:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    Ok(serde_json::from_slice(&output.stdout)?)
}

fn send(state: &Path, actor: &str, message: &Value) -> Result<()> {
    chat(
        state,
        actor,
        &["send", "fixture-room", &message.to_string()],
    )?;
    Ok(())
}

#[test]
#[ignore = "requires the installed domyjob chat CLI; uses only an isolated local state"]
fn isolated_agents_exchange_claims_releases_and_room_status() -> Result<()> {
    let scope = tempfile::tempdir()?;
    let state = scope.path().join("state");
    for agent in ["coordinator", "worker-one", "worker-two"] {
        chat(
            &state,
            "",
            &[
                "agent",
                "join",
                agent,
                "--tool",
                "codex",
                "--description",
                "isolated fixture",
            ],
        )?;
    }
    chat(
        &state,
        "coordinator",
        &["room", "create", "fixture-room", "worker-one", "worker-two"],
    )?;
    let first = json!({"kind": "claim", "id": "build-1", "resource": "cargo-target", "owner": "worker-one@local", "expires_at": "2099-01-01T00:00:00Z"});
    send(&state, "worker-one", &first)?;
    send(
        &state,
        "coordinator",
        &json!({"kind": "acknowledge", "id": "build-1"}),
    )?;
    send(
        &state,
        "worker-one",
        &json!({"kind": "release", "id": "build-1", "owner": "worker-one@local"}),
    )?;
    let expired = json!({"kind": "claim", "id": "build-2", "resource": "cargo-target", "owner": "worker-one@local", "expires_at": "2000-01-01T00:00:00Z"});
    send(&state, "worker-one", &expired)?;
    let takeover = json!({"kind": "claim", "id": "build-3", "resource": "cargo-target", "owner": "worker-two@local", "expires_at": "2099-01-01T00:00:00Z", "replaces": "build-2"});
    send(&state, "worker-two", &takeover)?;
    send(
        &state,
        "coordinator",
        &json!({"kind": "acknowledge", "id": "build-3"}),
    )?;
    let completed = json!({"kind": "status", "owner": "worker-two@local", "status": "completed"});
    send(&state, "worker-two", &completed)?;
    send(
        &state,
        "worker-two",
        &json!({"kind": "release", "id": "build-3", "owner": "worker-two@local"}),
    )?;
    let thread = chat(&state, "coordinator", &["thread", "fixture-room"])?;
    let events = thread["events"].as_array().unwrap();
    let messages = events
        .iter()
        .filter_map(|event| {
            event["text"]
                .as_str()
                .and_then(|text| serde_json::from_str::<Value>(text).ok())
                .map(|body| (event["from"].as_str().unwrap(), body))
        })
        .collect::<Vec<_>>();
    assert!(
        messages
            .iter()
            .any(|(author, body)| *author == "worker-one@local" && *body == first)
    );
    assert!(
        messages
            .iter()
            .any(|(author, body)| *author == "worker-two@local" && *body == takeover)
    );
    assert!(
        messages
            .iter()
            .any(|(author, body)| *author == "worker-two@local" && *body == completed)
    );
    assert_eq!(
        messages
            .iter()
            .filter(|(_, body)| body["kind"] == "release")
            .count(),
        2
    );
    scope.close()?;
    Ok(())
}
