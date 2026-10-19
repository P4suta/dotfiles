use std::fs;
use std::io::{self, Write as _};
use std::path::PathBuf;

const STATE: &str = "DOTFILES_TEST_PROOF_IMAGE_STATE";
const FAILURE: i32 = 9;
const UNEXPECTED: i32 = 2;
const CONFIG: &str = "sha256:f27f9c00a3aca2c219642d1500610eade3ddcb6b873ea8847852ab156663d32f";
const FOREIGN_ID: &str = "cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc";
const MEMORY: i64 = 8_589_934_592;

fn run() -> io::Result<i32> {
    let arguments: Vec<_> = std::env::args().skip(1).collect();
    let mut fields = arguments.iter().map(String::as_str);
    let scenario = fields.next().ok_or_else(|| io::Error::other("scenario"))?;
    let operation = fields.next().ok_or_else(|| io::Error::other("operation"))?;
    let id = fields.next().ok_or_else(|| io::Error::other("ID"))?;
    let name = fields.next().ok_or_else(|| io::Error::other("name"))?;
    let volume = fields.next().ok_or_else(|| io::Error::other("volume"))?;
    let state = PathBuf::from(std::env::var_os(STATE).ok_or_else(|| io::Error::other("state"))?);
    let mut calls = fs::OpenOptions::new()
        .append(true)
        .create(true)
        .open(state.with_extension("calls"))?;
    writeln!(calls, "{operation} {id}")?;
    match operation {
        "absent" | "create" | "limits" => Ok(0),
        "bootstrap" => {
            fs::write(&state, b"running")?;
            Ok(if scenario == "bootstrap-failure" {
                FAILURE
            } else {
                0
            })
        }
        "capture" | "inspect" => {
            let built = state.with_extension("built").is_file();
            let id = if scenario == "foreign" && built {
                FOREIGN_ID
            } else {
                id
            };
            let memory = if scenario == "bad-limits" { 0 } else { MEMORY };
            let swap = if scenario == "negative-swap" {
                -1
            } else {
                MEMORY
            };
            let running = fs::read(&state)? == b"running";
            println!(
                "{{\"id\":\"{id}\",\"name\":\"{name}\",\"created\":\"2026-10-14T00:00:00Z\",\"image\":\"{CONFIG}\",\"state_volume\":\"{volume}\",\"running\":{running},\"cpu_period\":100000,\"cpu_quota\":200000,\"memory\":{memory},\"memory_swap\":{swap},\"restart_policy\":\"no\"}}"
            );
            Ok(0)
        }
        "build" => {
            fs::write(state.with_extension("built"), b"built")?;
            println!("fixture: original build finished");
            Ok(if scenario == "build-failure" {
                FAILURE
            } else {
                0
            })
        }
        "stop" | "kill" => {
            if scenario == "false-stop" {
                return Ok(0);
            }
            if scenario == "no-ack" || (scenario == "stop-failure" && operation == "stop") {
                eprintln!("fixture: owned stop acknowledgement unavailable");
                return Ok(FAILURE);
            }
            fs::write(&state, b"stopped")?;
            Ok(0)
        }
        "loaded" => {
            let platform = if scenario == "bad-image" {
                "linux/arm64"
            } else {
                "linux/amd64"
            };
            println!("sha256:{id} {platform}");
            Ok(0)
        }
        _ => Ok(UNEXPECTED),
    }
}

fn main() {
    std::process::exit(run().unwrap_or(UNEXPECTED));
}
