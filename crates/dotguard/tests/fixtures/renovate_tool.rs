use std::path::Path;

const FIXTURE_ID: u32 = 1000;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let executable = std::env::current_exe().unwrap();
    match executable.file_stem().unwrap().to_str().unwrap() {
        "id" => println!("{FIXTURE_ID}"),
        "jq" => {
            let status =
                std::process::Command::new(std::env::var_os("RENOVATE_FIXTURE_JQ").unwrap())
                    .args(&args)
                    .status()
                    .unwrap();
            std::process::exit(status.code().unwrap_or(1));
        }
        "docker" => docker(&args),
        "dotguard-renovate" => {
            assert!(!args.is_empty());
            let status =
                std::process::Command::new(std::env::var_os("RENOVATE_FIXTURE_BACKEND").unwrap())
                    .arg("different-source-build")
                    .args(&args[1..])
                    .status()
                    .unwrap();
            std::process::exit(status.code().unwrap_or(1));
        }
        name => panic!("unexpected fixture executable: {name}"),
    }
}

fn docker(args: &[String]) {
    let image = args
        .iter()
        .position(|arg| arg.starts_with("renovate/renovate:"))
        .unwrap();
    std::fs::write(
        std::env::var_os("RENOVATE_FIXTURE_ARGS").unwrap(),
        args[image + 1..].join("\n"),
    )
    .unwrap();
    if args[image + 1..].iter().any(|arg| !arg.starts_with("--")) {
        eprintln!("FATAL: repositories list not supported when platform=local");
        std::process::exit(1);
    }
    let snapshot = Path::new(
        args.iter()
            .find_map(|arg| arg.strip_suffix(":/repo:ro"))
            .unwrap(),
    );
    assert!(
        snapshot.join(".git").is_dir(),
        "snapshot has its own Git directory"
    );
    assert!(
        snapshot.join(".git/index").is_file(),
        "snapshot has its own index"
    );
    assert_eq!(
        std::fs::read_to_string(snapshot.join("Cargo.toml")).unwrap(),
        std::env::var("RENOVATE_FIXTURE_MANIFEST").unwrap(),
    );
    if std::env::var_os("RENOVATE_FIXTURE_FAIL").is_some() {
        eprintln!("fixture: Renovate failed before producing a report");
        std::process::exit(1);
    }
    if let Some(directory) = args.iter().find_map(|arg| arg.strip_suffix(":/report:rw")) {
        let name = args
            .iter()
            .find_map(|arg| arg.strip_prefix("RENOVATE_REPORT_PATH=/report/"))
            .unwrap();
        let report = std::env::var_os("RENOVATE_FIXTURE_REPORT").map_or_else(
            || br#"{"repositories":{"local":{"packageFiles":{}}}}"#.to_vec(),
            |path| std::fs::read(path).unwrap(),
        );
        std::fs::write(Path::new(directory).join(name), report).unwrap();
    }
}
