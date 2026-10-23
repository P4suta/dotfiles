# Dotfiles

One chezmoi source for Mac, Linux, Windows, and WSL with Rust-driven setup.

## Installation

```sh
git clone https://github.com/P4suta/dotfiles.git
cd dotfiles
mise install
chezmoi init --source .
```

## Usage

```sh
just diff CONFIG_ABSOLUTE_PATH NATIVE_HOME_ABSOLUTE_PATH
just apply CONFIG_ABSOLUTE_PATH NATIVE_HOME_ABSOLUTE_PATH FRESH_BACKUP_DIRECTORY
just check
```

## License

Licensed under either license:

- [Apache License, Version 2.0](LICENSE-APACHE)
- [Massachusetts Institute of Technology (MIT) license](LICENSE-MIT)

Contributions use both licenses unless you state other terms.
