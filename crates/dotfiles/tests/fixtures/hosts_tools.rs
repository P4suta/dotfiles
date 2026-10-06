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
        _ => println!("{alias}:job finished succeeded"),
    }
}
