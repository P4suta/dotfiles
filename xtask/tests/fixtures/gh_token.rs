//! A GitHub CLI login whose token exists only behind `auth token`.

fn main() {
    let arguments: Vec<String> = std::env::args().skip(1).collect();
    assert_eq!(
        arguments,
        [
            "auth",
            "token",
            "--hostname",
            &std::env::var("MISE_CREDENTIAL_HOST").unwrap(),
        ]
    );
    assert_eq!(std::env::var("MISE_CREDENTIAL_PROVIDER").unwrap(), "github");
    std::fs::write(
        std::env::var_os("GH_FIXTURE_LOG").unwrap(),
        arguments.join(" "),
    )
    .unwrap();
    if std::env::var("GH_FIXTURE_LOGIN").unwrap() == "yes" {
        println!("fixture-keyring-token");
    } else {
        std::process::exit(1);
    }
}
