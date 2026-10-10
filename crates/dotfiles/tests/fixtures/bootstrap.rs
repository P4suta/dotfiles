fn main() {
    for name in [
        "HOME",
        "CARGO_HOME",
        "MISE_CARGO_HOME",
        "MISE_GLOBAL_CONFIG_FILE",
        "MISE_SYSTEM_CONFIG_FILE",
    ] {
        println!(
            "{name}={}",
            std::env::var(name).expect("bootstrap environment")
        );
    }
    println!("RUSTUP_TOOLCHAIN={}", env!("RUSTUP_TOOLCHAIN"));
}
