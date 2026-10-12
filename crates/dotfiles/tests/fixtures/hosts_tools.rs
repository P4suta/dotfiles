//! Stands in for `tailscale` and `domyjob`, chosen by the executable's name.

fn barrier(variable: &str, alias: &str) {
    if let Ok(path) = std::env::var(variable) {
        let path = std::path::Path::new(&path);
        std::fs::write(path.with_extension("ready"), alias).unwrap();
        const WAIT_LIMIT: std::time::Duration = std::time::Duration::from_secs(20);
        const POLL_INTERVAL: std::time::Duration = std::time::Duration::from_millis(10);
        let start = std::time::Instant::now();
        while !path.is_file() {
            assert!(start.elapsed() < WAIT_LIMIT, "fixture barrier timed out");
            std::thread::sleep(POLL_INTERVAL);
        }
    }
}

fn main() {
    let program = std::env::current_exe().unwrap();
    let name = program.file_stem().unwrap().to_str().unwrap().to_owned();
    let args: Vec<String> = std::env::args().skip(1).collect();
    if name == "tailscale" {
        println!("{}", std::env::var("HOSTS_FIXTURE_STATUS").unwrap());
        return;
    }
    let alias = &args[1];
    let interruption = std::env::var("HOSTS_FIXTURE_INTERRUPTION").unwrap_or_default();
    let interruption = if std::env::var("HOSTS_FIXTURE_INTERRUPTION_HOST")
        .is_ok_and(|host| alias.split(':').next() != Some(host.as_str()))
    {
        ""
    } else {
        interruption.as_str()
    };
    match args[0].as_str() {
        "doctor" if std::env::var("HOSTS_FIXTURE_DOCTOR").is_ok_and(|value| value == "fail") => {
            eprintln!("domyjob: SSH could not reach the machine");
            std::process::exit(1);
        }
        "doctor" => {
            barrier("HOSTS_FIXTURE_PROBE_BARRIER", alias);
            println!("{alias}: ready");
        }
        "on" if args.iter().any(|argument| {
            argument.contains("df -Pk") || argument.contains("AvailableFreeSpace")
        }) =>
        {
            println!("{alias}:space")
        }
        "on" => println!("{alias}:probe"),
        "run" => {
            let job = args
                .windows(2)
                .find_map(|pair| (pair[0] == "--submission").then_some(pair[1].as_str()))
                .unwrap_or("job");
            if let Ok(path) = std::env::var("HOSTS_FIXTURE_EXECUTIONS") {
                use std::io::{Read as _, Write as _};
                let mut file = std::fs::OpenOptions::new()
                    .read(true)
                    .create(true)
                    .append(true)
                    .open(&path)
                    .unwrap();
                file.lock().unwrap();
                let mut executions = String::new();
                file.read_to_string(&mut executions).unwrap();
                let bundle = std::fs::read("head.bundle").unwrap();
                let saved = std::path::Path::new(&path).with_extension(format!("{job}.bundle"));
                let program =
                    &args[args.iter().position(|argument| argument == "--").unwrap() + 1..];
                let request = std::path::Path::new(&path).with_extension(format!("{job}.program"));
                if executions
                    .lines()
                    .any(|line| line == format!("{alias}:{job}"))
                {
                    assert_eq!(std::fs::read(saved).unwrap(), bundle);
                    assert_eq!(
                        std::fs::read_to_string(request).unwrap(),
                        format!("{program:?}")
                    );
                } else {
                    std::fs::write(saved, bundle).unwrap();
                    std::fs::write(request, format!("{program:?}")).unwrap();
                    writeln!(file, "{alias}:{job}").unwrap();
                }
            }
            if interruption == "submission" {
                eprintln!("SSH acknowledgement connection timed out");
                std::process::exit(1);
            }
            println!("{alias}:{job}");
        }
        "status" if alias.ends_with(":probe") => println!("{alias} finished succeeded"),
        "status" if alias.ends_with(":space") => println!("{alias} finished succeeded"),
        "status" => {
            match interruption {
                "status" => {
                    eprintln!("SSH status connection timed out");
                    std::process::exit(1);
                }
                "unknown" => {
                    println!("{alias} unknown");
                    return;
                }
                "finished-unknown" => {
                    println!("{alias} finished unknown");
                    return;
                }
                "supervisor" | "supervisor-malformed" => {
                    println!(
                        "{}:unrelated finished succeeded",
                        alias.split(':').next().unwrap()
                    );
                    println!("{alias} finished killed");
                    return;
                }
                "contradictory" => {
                    println!("{alias} finished failed (1)");
                    return;
                }
                "no-report-0" => {
                    println!("{alias} finished succeeded");
                    return;
                }
                "no-report-1" => {
                    println!("{alias} finished failed (1)");
                    return;
                }
                _ => {}
            }
            barrier("HOSTS_FIXTURE_BARRIER", alias);
            let stages = std::env::var("HOSTS_FIXTURE_STAGES")
                .unwrap_or_else(|_| "{\"code\":0,\"real\":0,\"applied\":{\"drifted_targets\":[],\"verification_error\":null}}".to_owned());
            if stages.starts_with("{\"code\":0,\"real\":0,") {
                println!("{alias} finished succeeded");
            } else {
                println!("{alias} finished failed (1)");
            }
        }
        "logs" if alias.ends_with(":probe") => {}
        "logs" if alias.ends_with(":space") => {
            let free =
                std::env::var("HOSTS_FIXTURE_SPACE").unwrap_or_else(|_| "100000000000".into());
            if alias.starts_with("pc:") {
                println!("{free}");
            } else {
                println!("/dev/fixture 0 0 {free} 0% /fixture");
            }
        }
        "logs" => {
            if interruption.starts_with("no-report-") {
                println!("bootstrap exited before reporting stages");
                return;
            }
            if interruption == "logs" {
                eprintln!("SSH logs connection timed out");
                std::process::exit(1);
            }
            if interruption == "malformed" || interruption == "supervisor-malformed" {
                println!("DOTFILES_HOST_STAGES malformed");
                return;
            }
            let stages = std::env::var("HOSTS_FIXTURE_STAGES")
                .unwrap_or_else(|_| "{\"code\":0,\"real\":0,\"applied\":{\"drifted_targets\":[],\"verification_error\":null}}".to_owned());
            let stages = if stages.contains("\"workspace\":") {
                stages
            } else {
                let root = if alias.starts_with("pc:") {
                    "C:/state"
                } else {
                    "/state"
                };
                format!(
                    "{},\"workspace\":\"{root}/jobs/{}/workspace\"}}",
                    stages.strip_suffix('}').unwrap(),
                    alias.split_once(':').unwrap().1
                )
            };
            println!("DOTFILES_HOST_STAGES {stages}");
        }
        _ => println!("{alias}:job finished succeeded"),
    }
}
