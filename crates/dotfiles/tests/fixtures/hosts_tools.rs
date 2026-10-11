//! Stands in for `tailscale` and `domyjob`, chosen by the executable's name.

fn main() {
    let program = std::env::current_exe().unwrap();
    let name = program.file_stem().unwrap().to_str().unwrap().to_owned();
    let args: Vec<String> = std::env::args().skip(1).collect();
    if name == "tailscale" {
        println!("{}", std::env::var("HOSTS_FIXTURE_STATUS").unwrap());
        return;
    }
    let alias = &args[1];
    match args[0].as_str() {
        "doctor" if std::env::var("HOSTS_FIXTURE_DOCTOR").is_ok_and(|value| value == "fail") => {
            eprintln!("domyjob: SSH could not reach the machine");
            std::process::exit(1);
        }
        "doctor" => println!("{alias}: ready"),
        "on" if args.iter().any(|argument| {
            argument.contains("df -Pk") || argument.contains("AvailableFreeSpace")
        }) =>
        {
            println!("{alias}:space")
        }
        "on" => println!("{alias}:probe"),
        "run" => println!("{alias}:job"),
        "status" if alias.ends_with(":probe") => println!("{alias} finished succeeded"),
        "status" if alias.ends_with(":space") => println!("{alias} finished succeeded"),
        "status" => {
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
            let stages = std::env::var("HOSTS_FIXTURE_STAGES")
                .unwrap_or_else(|_| "{\"code\":0,\"real\":0,\"applied\":{\"drifted_targets\":[],\"verification_error\":null}}".to_owned());
            println!("DOTFILES_HOST_STAGES {stages}");
        }
        _ => println!("{alias}:job finished succeeded"),
    }
}
