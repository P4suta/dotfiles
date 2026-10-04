//! The readiness report: `dotguard doctor`.
//!
//! A diagnostic, not a gate: exit 0 always, unless `--strict` — a doctor that refuses to finish its report is useless precisely when you need it.
//! Ported from `dot_local/bin/executable_dotfiles-doctor`, output line for output line, because the output *is* the interface people read.

use crate::realgit;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

const OK: &str = "\x1b[1;32m✓\x1b[0m";
const WARN: &str = "\x1b[1;33m!\x1b[0m";
const BAD: &str = "\x1b[1;31m✗\x1b[0m";
const SECTION: &str = "\x1b[1;36m";

pub fn run(args: &[String]) -> i32 {
    let strict = args.first().map(String::as_str) == Some("--strict");
    let mut d = Doctor::default();

    system_tier(&mut d);
    modern_terminal(&mut d);
    native_toolchain(&mut d);
    language_toolchains(&mut d);
    agents_and_editors(&mut d);
    network(&mut d);
    git_signing(&mut d);
    repository_guards(&mut d);
    apple_platform(&mut d);
    macos_integration(&mut d);
    scheduled_maintenance(&mut d);

    println!();
    if d.missing == 0 {
        ok(
            "summary",
            &format!("core environment is ready ({} warning(s))", d.warnings),
        );
        return 0;
    }
    d.bad(
        "summary",
        &format!(
            "{} required component(s) missing; {} warning(s)",
            d.missing, d.warnings
        ),
    );
    i32::from(strict)
}

#[derive(Default)]
struct Doctor {
    missing: usize,
    warnings: usize,
}

impl Doctor {
    fn warn(&mut self, label: &str, detail: &str) {
        println!("{WARN} {label:<24} {detail}");
        self.warnings += 1;
    }
    fn bad(&mut self, label: &str, detail: &str) {
        println!("{BAD} {label:<24} {detail}");
        self.missing += 1;
    }

    fn command(&mut self, name: &str, label: &str, required: bool) {
        match which_doctored(name) {
            Some(p) => ok(label, &p.display().to_string()),
            None if required => self.bad(label, "missing"),
            None => self.warn(label, "not installed (optional)"),
        }
    }

    fn file(&mut self, path: &Path, label: &str) {
        if path.is_file() && path.metadata().is_ok_and(|m| m.len() > 0) {
            ok(label, &path.display().to_string());
        } else {
            self.bad(label, &format!("missing: {}", path.display()));
        }
    }

    fn app(&mut self, bundle: &str, label: &str, required: bool) {
        let dir = PathBuf::from("/Applications").join(bundle);
        if dir.is_dir() {
            ok(label, &dir.display().to_string());
        } else if required {
            self.bad(label, &format!("missing: {}", dir.display()));
        } else {
            self.warn(label, "not installed (optional)");
        }
    }

    fn agent(&mut self, label: &str) {
        let uid = whoami_uid();
        let up = Command::new("/bin/launchctl")
            .arg("print")
            .arg(format!("gui/{uid}/{label}"))
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .is_ok_and(|s| s.success());
        if up {
            ok(label, "loaded");
        } else {
            self.warn(label, "not loaded (launchctl bootstrap; see just apply)");
        }
    }
}

fn home() -> PathBuf {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_default()
}

fn whoami_uid() -> String {
    Command::new("/usr/bin/id")
        .arg("-u")
        .output()
        .ok()
        .map_or_else(
            || "0".into(),
            |o| String::from_utf8_lossy(&o.stdout).trim().to_owned(),
        )
}

/// The PATH the doctor resolves commands against: the hook-style prepend of shims, ~/.local/bin and Homebrew, plus the opam switch — the OCaml Platform lives there rather than in a mise shim, so without it `ocaml`, `dune` and `utop` all read as missing even on a healthy machine.
/// Asked of `opam env` rather than hardcoded, because the switch name is data.
fn doctored_path() -> String {
    let h = home();
    let mut dirs: Vec<PathBuf> = vec![
        h.join(".local/share/mise/shims"),
        h.join(".local/bin"),
        PathBuf::from("/opt/homebrew/bin"),
        PathBuf::from("/opt/homebrew/sbin"),
    ];
    if let Ok(p) = std::env::var("PATH") {
        dirs.extend(std::env::split_paths(&p));
    }
    // The active switch's bin directory, asked for directly rather than parsed out of `opam env` shell syntax.
    if let Ok(out) = Command::new("opam").args(["var", "bin"]).output()
        && out.status.success()
    {
        let p = PathBuf::from(String::from_utf8_lossy(&out.stdout).trim());
        if p.is_dir() {
            dirs.push(p);
        }
    }
    std::env::join_paths(dirs).map_or_else(|_| String::new(), |s| s.to_string_lossy().into_owned())
}

fn which_doctored(name: &str) -> Option<PathBuf> {
    std::env::split_paths(&doctored_path())
        .map(|d| d.join(name))
        .find(|p| p.is_file())
}

/// The SSH agent selection from `~/.config/shell/ssh-agent.sh`: a local session uses the 1Password app-group socket; a remote session preserves the socket sshd forwarded.
fn ssh_auth_sock() -> Option<String> {
    if let Some(sock) = std::env::var_os("DOTFILES_SSH_AUTH_SOCK")
        && Path::new(&sock).exists()
    {
        return Some(sock.to_string_lossy().into_owned());
    }
    let app_group = home().join("Library/Group Containers/2BUA8C4S2C.com.1password/t/agent.sock");
    if std::env::var_os("SSH_CONNECTION").is_none() && app_group.exists() {
        return Some(app_group.display().to_string());
    }
    std::env::var("SSH_AUTH_SOCK").ok()
}

// -- sections ---------------------------------------------------------------

fn system_tier(d: &mut Doctor) {
    section("System tier (Homebrew)");
    d.command("brew", "Homebrew", true);
    if which_doctored("brew").is_some() {
        let brewfile = home().join(".config/homebrew/Brewfile");
        if brewfile.is_file() && brewfile.metadata().is_ok_and(|m| m.len() > 0) {
            let satisfied = Command::new("brew")
                .args(["bundle", "check", "--no-upgrade", "--file"])
                .arg(&brewfile)
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status()
                .is_ok_and(|s| s.success());
            if satisfied {
                ok("Brewfile", "satisfied");
            } else {
                d.warn(
                    "Brewfile",
                    "packages missing; run: brew bundle install --no-upgrade --file ~/.config/homebrew/Brewfile",
                );
            }
        } else {
            d.bad("Brewfile", &format!("missing: {}", brewfile.display()));
        }
    }
}

fn modern_terminal(d: &mut Doctor) {
    section("Modern terminal");
    d.command("chezmoi", "chezmoi", true);
    d.command("mise", "mise", true);
    d.command("nu", "Nushell", true);
    d.command("starship", "Starship", true);
    d.command("herdr", "Herdr", true);
    d.command("atuin", "Atuin", true);
    d.command("zoxide", "Zoxide", true);
    d.app("Ghostty.app", "Ghostty", true);
    d.file(
        &home().join(".config/nushell/generated/mise.nu"),
        "mise.nu integration",
    );

    // Some tools follow the macOS platform convention instead of XDG and read ~/Library/Application Support/<tool>.
    // Ask each tool where it actually looks and resolve the answer physically: a symlink can be present and still point somewhere wrong, and getting that wrong fails in the worst possible way — the tool starts fine, writes fresh defaults at the platform path, and every setting here quietly does not apply.
    let check_platform_config = |d: &mut Doctor,
                                 label: &str,
                                 reported: Option<String>,
                                 leaf: &str| {
        let Some(reported) = reported.filter(|s| !s.is_empty()) else {
            d.bad(
                &format!("{label} config path"),
                "the tool reported no config directory",
            );
            return;
        };
        let expected = home().join(".config").join(leaf);
        #[allow(
            clippy::disallowed_methods,
            reason = "Unix paths have no verbatim spelling, and the doctor compares a resolved binary with PATH resolution"
        )]
        let resolved = std::fs::canonicalize(&reported).ok();
        if resolved.as_deref() == Some(expected.as_path()) {
            ok(
                &format!("{label} config path"),
                &expected
                    .display()
                    .to_string()
                    .replacen(&home().display().to_string(), "~", 1),
            );
        } else {
            d.bad(
                &format!("{label} config path"),
                &format!(
                    "reads {reported}, which resolves to {}, not {}",
                    resolved.map_or_else(|| "nowhere".into(), |r| r.display().to_string()),
                    expected.display()
                ),
            );
        }
    };

    let nu_dir = which_doctored("nu").and_then(|_| {
        Command::new("nu")
            .args(["-c", "print $nu.default-config-dir"])
            .output()
            .ok()
            .filter(|o| o.status.success())
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_owned())
    });
    check_platform_config(d, "Nushell", nu_dir, "nushell");

    let lg_dir = which_doctored("lazygit").and_then(|_| {
        Command::new("lazygit")
            .arg("--print-config-dir")
            .output()
            .ok()
            .filter(|o| o.status.success())
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_owned())
    });
    check_platform_config(d, "lazygit", lg_dir, "lazygit");

    // Ask whether the family Ghostty is *configured* with actually resolves, rather than globbing a filename in ~/Library/Fonts: a glob passes for a font that is installed but not the one in use.
    // A family Ghostty cannot resolve silently falls back, and the terminal quietly stops being the one that was configured.
    let config = std::fs::read_to_string(home().join(".config/ghostty/config")).unwrap_or_default();
    let configured_font = parse_font_family(&config);
    match configured_font {
        None => d.bad(
            "terminal font",
            "~/.config/ghostty/config declares no font-family",
        ),
        Some(font) if which_doctored("ghostty").is_none() => {
            d.warn(
                "terminal font",
                &format!("cannot verify {font}; the ghostty CLI is not on PATH"),
            );
        }
        Some(font) => {
            let listed = Command::new("ghostty")
                .arg("+list-fonts")
                .output()
                .ok()
                .map(|o| String::from_utf8_lossy(&o.stdout).into_owned())
                .unwrap_or_default();
            if listed.lines().any(|l| l.trim() == font) {
                ok("terminal font", &font);
            } else {
                d.bad(
                    "terminal font",
                    &format!(
                        "Ghostty cannot resolve '{font}'; it is falling back to a system font"
                    ),
                );
            }
        }
    }
}

fn native_toolchain(d: &mut Doctor) {
    section("Native toolchain");
    // The cause worth catching is SDK/linker skew: `xcrun` resolves to the HIGHEST installed SDK, which after a partial Command Line Tools update can be newer than the `ld` that shipped with them, and then nothing on the machine can link at all.
    let probe_dir = std::env::temp_dir().join(format!("doctor-probe.{}", std::process::id()));
    let _ = std::fs::create_dir_all(&probe_dir);
    let src = probe_dir.join("probe.c");
    let bin = probe_dir.join("probe");
    let _ = std::fs::write(&src, "int main(void){return 0;}\n");
    let cc = Command::new("cc").arg(&src).arg("-o").arg(&bin).output();
    match cc {
        Ok(o) if o.status.success() => {
            let sdk = Command::new("xcrun")
                .arg("--show-sdk-version")
                .output()
                .ok()
                .and_then(|o| {
                    o.status
                        .success()
                        .then(|| String::from_utf8_lossy(&o.stdout).trim().to_owned())
                })
                .filter(|s| !s.is_empty())
                .unwrap_or_else(|| "unknown".into());
            ok("C toolchain", &format!("links against SDK {sdk}"));
        }
        Ok(o) => {
            let err = String::from_utf8_lossy(&o.stderr);
            if err.contains("unknown architecture") {
                d.bad(
                    "C toolchain",
                    "ld cannot parse the selected SDK — the Command Line Tools are older than it",
                );
                let sdk_path = Command::new("xcrun")
                    .arg("--show-sdk-path")
                    .output()
                    .ok()
                    .filter(|o| o.status.success())
                    .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_owned())
                    .unwrap_or_default();
                let linker = Command::new("ld").arg("-v").output().map_or_else(
                    |_| "unknown".into(),
                    |o| {
                        String::from_utf8_lossy(&o.stderr)
                            .lines()
                            .next()
                            .unwrap_or_default()
                            .to_owned()
                    },
                );
                let update = Command::new("softwareupdate")
                    .arg("--list")
                    .output()
                    .ok()
                    .map(|o| String::from_utf8_lossy(&o.stdout).into_owned())
                    .and_then(|list| clt_update_label(&list))
                    .unwrap_or_default();
                println!("    selected SDK : {sdk_path}");
                println!("    linker       : {linker}");
                if update.is_empty() {
                    println!(
                        "    fix          : update the Command Line Tools (softwareupdate --list)"
                    );
                } else {
                    println!("    fix          : sudo softwareupdate --install \"{update}\"");
                }
                println!(
                    "    stopgap      : SDKROOT=/Library/Developer/CommandLineTools/SDKs/MacOSX.sdk"
                );
            } else {
                let first = err.lines().next().unwrap_or("unknown error").to_owned();
                d.bad("C toolchain", &format!("cc cannot link: {first}"));
            }
        }
        Err(_) => d.bad("C toolchain", "cc cannot link: no compiler"),
    }
    let _ = std::fs::remove_dir_all(&probe_dir);
}

fn language_toolchains(d: &mut Doctor) {
    section("Language toolchains");
    d.command("java", "Java 25", true);
    d.command("rustc", "Rust", true);
    d.command("go", "Go", true);
    d.command("opam", "opam", true);
    d.command("ocaml", "OCaml", true);
    d.command("gleam", "Gleam", true);
    d.command("erl", "Erlang", true);
    d.command("ghc", "GHC", true);
    d.command("cabal", "cabal-install", true);
    d.command("stack", "Stack", true);
    // The wrapper, not `haskell-language-server`: upstream's bindist ships only the wrapper plus one binary per GHC version, and the wrapper is what every editor integration launches anyway.
    d.command("haskell-language-server-wrapper", "HLS", true);
    d.command("bun", "Bun", true);
    d.command("uv", "uv", true);
}

fn agents_and_editors(d: &mut Doctor) {
    section("Agents and editors");
    d.command("codex", "Codex", false);
    d.command("claude", "Claude Code", false);
    d.command("opencode", "OpenCode", false);
    // OrbStack generates its CLI shims on first launch, so "app present, docker missing" is a distinct and actionable state from "not installed".
    match which_doctored("docker") {
        Some(p) => ok("Docker (OrbStack)", &p.display().to_string()),
        None if Path::new("/Applications/OrbStack.app").is_dir() => d.warn(
            "Docker (OrbStack)",
            "OrbStack.app is installed but has never been launched; open it once to get the docker CLI",
        ),
        None => d.warn("Docker (OrbStack)", "not installed (optional)"),
    }
    d.command("code", "VS Code", false);
    d.command("zed", "Zed", false);
    d.command("op", "1Password CLI", false);
    d.app("1Password.app", "1Password app", true);
}

fn network(d: &mut Doctor) {
    section("Network");
    d.app("Tailscale.app", "Tailscale", false);
    if let Some(ts) = which_doctored("tailscale") {
        // `tailscale status` exits non-zero when logged out, which is the state worth distinguishing: the app being installed says nothing about tailnet membership.
        let up = Command::new(&ts)
            .arg("status")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .is_ok_and(|s| s.success());
        if up {
            let name = Command::new(&ts)
                .args(["status", "--json"])
                .output()
                .ok()
                .and_then(|o| {
                    let json = String::from_utf8_lossy(&o.stdout).into_owned();
                    json_field(&json, "DNSName").filter(|s| !s.is_empty())
                })
                .unwrap_or_else(|| "connected".into());
            ok("tailnet", &name);
        } else {
            d.warn(
                "tailnet",
                "installed but not joined; run: tailscale up --ssh",
            );
        }
    } else {
        d.warn(
            "tailscale CLI",
            "not on PATH (just refresh links it from the app bundle)",
        );
    }
}

fn git_signing(d: &mut Doctor) {
    section("Git signing");
    let agent_socket = home()
        .join("Library/Group Containers/2BUA8C4S2C.com.1password/t/agent.sock")
        .display()
        .to_string();
    let agent_keys = ssh_auth_sock().and_then(|sock| {
        Command::new("ssh-add")
            .env("SSH_AUTH_SOCK", &sock)
            .arg("-L")
            .output()
            .ok()
            .filter(|o| o.status.success())
            .map(|o| String::from_utf8_lossy(&o.stdout).into_owned())
            .filter(|s| !s.is_empty())
    });
    match agent_keys {
        Some(_) => match ssh_auth_sock() {
            Some(s) if s == agent_socket => {
                ok("SSH signing agent", "1Password (app group socket)");
            }
            Some(s) => ok(
                "SSH signing agent",
                &format!("forwarded/custom agent ({s})"),
            ),
            None => ok("SSH signing agent", "forwarded/custom agent (unset)"),
        },
        None if Path::new(&agent_socket).exists() => {
            d.warn(
                "SSH signing agent",
                "1Password is locked or exposes no keys",
            );
        }
        None => d.bad(
            "SSH signing agent",
            "enable 1Password → Settings → Developer → Use the SSH agent",
        ),
    }

    if realgit::capture(&["config", "--get", "commit.gpgsign"]).as_deref() == Some("true\n") {
        ok("commit signing", "enabled");
    } else {
        d.bad(
            "commit signing",
            "~/.gitconfig has no signing block — re-run 'just apply' with the agent unlocked",
        );
    }
    let instead = realgit::capture(&["config", "--get-all", "url.git@github.com:.insteadOf"])
        .unwrap_or_default();
    if instead.lines().any(|l| l.trim() == "https://github.com/") {
        ok("github https→ssh", "insteadOf rewrite active");
    } else {
        d.bad("github https→ssh", "insteadOf rewrite missing");
    }
}

fn repository_guards(d: &mut Doctor) {
    section("Repository guards");
    let dotguard_bin = home().join(".local/bin/dotguard");
    if dotguard_bin.is_file() {
        let version = Command::new(&dotguard_bin)
            .arg("--version")
            .output()
            .ok()
            .filter(|o| o.status.success())
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_owned())
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| "installed".into());
        ok("dotguard", &version);
    } else {
        d.bad(
            "dotguard",
            "not built — run 'just apply' in the dotfiles repo",
        );
    }

    // The wrapper only enforces anything if it is the git that PATH resolves to; being shadowed by Homebrew's or Apple's git is silent.
    let resolved_git = which_doctored("git").map(|p| p.display().to_string());
    if resolved_git.as_deref() == Some(home().join(".local/bin/git").display().to_string().as_str())
    {
        ok("git wrapper", "the wrapper precedes the real git on PATH");
    } else {
        d.bad(
            "git wrapper",
            &format!(
                "PATH resolves git to {} — ~/.local/bin must come first",
                resolved_git.unwrap_or_else(|| "nothing".into())
            ),
        );
    }

    // core.hooksPath is what makes any of the hooks global rather than per-repo.
    let hooks_path = realgit::capture(&["config", "--get", "core.hooksPath"])
        .map(|s| s.trim().replace('~', &home().display().to_string()));
    let hooks_ok = hooks_path.as_ref().is_some_and(|p| {
        let base = Path::new(p);
        ["pre-commit", "commit-msg", "pre-push"]
            .iter()
            .all(|h| base.join(h).is_file())
    });
    if let (Some(p), true) = (hooks_path, hooks_ok) {
        ok("global git hooks", &p);
    } else {
        d.bad(
            "global git hooks",
            "core.hooksPath is unset, missing, or incomplete",
        );
    }

    // Every refusal and every waiver is appended to the bypass log; a waiver you cannot remember making is the one worth reading.
    let bypass_log = home().join(".local/state/git-bypass.log");
    if bypass_log.is_file() {
        let text = std::fs::read_to_string(&bypass_log).unwrap_or_default();
        let waived = count_marker(&text, "BYPASS");
        let refused = count_marker(&text, "REJECT");
        if waived > 0 {
            let tilde_path =
                bypass_log
                    .display()
                    .to_string()
                    .replacen(&home().display().to_string(), "~", 1);
            d.warn(
                "guard bypass log",
                &format!("{waived} waived, {refused} refused — review {tilde_path}"),
            );
        } else {
            ok(
                "guard bypass log",
                &format!("{refused} refused, none waived"),
            );
        }
    } else {
        ok("guard bypass log", "nothing refused or waived yet");
    }
}

fn apple_platform(d: &mut Doctor) {
    section("Apple platform development");
    // `xcode-select -p` pointing at CommandLineTools is the state that matters: swiftc exists, so a Swift package builds, but there is no iOS SDK, no simulator and no xcodebuild — and the error says none of that.
    let xcode_path = Command::new("xcode-select")
        .arg("-p")
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_owned())
        .unwrap_or_default();
    match classify_xcode_path(&xcode_path) {
        Xcode::Full => {
            let version = Command::new("xcodebuild")
                .arg("-version")
                .output()
                .ok()
                .map(|o| {
                    String::from_utf8_lossy(&o.stdout)
                        .lines()
                        .next()
                        .unwrap_or_default()
                        .to_owned()
                })
                .filter(|s| !s.is_empty())
                .unwrap_or_else(|| xcode_path.clone());
            ok("Xcode", &version);
// Since Xcode 16 the simulator runtimes are a separate download; an Xcode with none installed has no device to build for and says nothing about why.
            let runtimes = Command::new("xcrun")
                .args(["simctl", "list", "runtimes"])
                .output()
                .ok()
                .map(|o| String::from_utf8_lossy(&o.stdout).into_owned())
                .unwrap_or_default();
            let ios = runtimes.lines().filter(|l| l.starts_with("iOS ")).count();
            if ios > 0 {
                ok("iOS simulator runtime", &format!("{ios} installed"));
            } else {
                d.warn("iOS simulator runtime", "none installed; run: xcodebuild -downloadPlatform iOS");
            }
        }
        Xcode::CommandLineToolsOnly => d.warn(
            "Xcode",
            "only the Command Line Tools are selected — no iOS SDK, simulators or xcodebuild. Install Xcode, then: sudo xcode-select -s /Applications/Xcode.app",
        ),
        Xcode::None => d.warn("Xcode", "no developer directory selected (xcode-select -p)"),
    }
    d.command("xcodes", "xcodes (version manager)", false);
    d.command("swiftlint", "SwiftLint", false);
    d.command("xcbeautify", "xcbeautify", false);
    d.command("fastlane", "fastlane", false);
    d.command("mas", "mas (App Store CLI)", false);
    // Apple's formatter ships inside the toolchain rather than as its own binary, so `which swift-format` finds nothing even when it is perfectly available.
    let swift_format = Command::new("swift")
        .args(["format", "--version"])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|s| s.success());
    if swift_format {
        ok("swift format", "bundled with the Swift toolchain");
    } else {
        d.warn("swift format", "not available from this toolchain");
    }
}

fn macos_integration(d: &mut Doctor) {
    section("macOS integration");
    // /etc/pam.d/sudo_local is Apple's supported hook and survives OS updates, which is why `dotfiles-touchid` writes there rather than editing /etc/pam.d/sudo — that file is replaced by the next system update and the setting silently reverts.
    let sudo_local = std::fs::read_to_string("/etc/pam.d/sudo_local").unwrap_or_default();
    let enabled = sudo_local
        .lines()
        .any(|l| l.starts_with("auth") && l.contains("pam_tid.so"));
    if enabled {
        ok("Touch ID for sudo", "/etc/pam.d/sudo_local");
    } else if Path::new("/usr/lib/pam/pam_tid.so.2").exists() {
        d.warn(
            "Touch ID for sudo",
            "available but not enabled; run dotfiles-touchid in a real terminal (it needs a tty for the sudo prompt)",
        );
    } else {
        d.warn(
            "Touch ID for sudo",
            "pam_tid.so is not present on this macOS",
        );
    }
}

fn scheduled_maintenance(d: &mut Doctor) {
    section("Scheduled maintenance (launchd)");
    for agent in [
        "dev.dotfiles.tools-upgrade",
        "dev.dotfiles.desktop-refresh",
        "dev.dotfiles.session-reaper",
        "dev.dotfiles.dev-reaper",
        "dev.dotfiles.dev-cleanup",
    ] {
        d.agent(agent);
    }
}

fn ok(label: &str, detail: &str) {
    println!("{OK} {label:<24} {detail}");
}

fn section(title: &str) {
    println!("\n{SECTION}{title}\x1b[0m");
}

// -- small parsers ----------------------------------------------------------

/// The `font-family = "…"` line of a Ghostty config, first one wins.
fn parse_font_family(config: &str) -> Option<String> {
    config
        .lines()
        .find_map(|l| l.strip_prefix("font-family = "))
        .map(|v| v.trim_matches('"').to_owned())
        .filter(|s| !s.is_empty())
}

enum Xcode {
    Full,
    CommandLineToolsOnly,
    None,
}

fn classify_xcode_path(path: &str) -> Xcode {
    if path.contains("/Xcode") && path.contains(".app") {
        Xcode::Full
    } else if path.ends_with("/CommandLineTools") {
        Xcode::CommandLineToolsOnly
    } else {
        Xcode::None
    }
}

/// The Command Line Tools label out of `softwareupdate --list`, if any.
fn clt_update_label(list: &str) -> Option<String> {
    list.lines().find_map(|l| {
        let t = l.trim();
        t.strip_prefix("* Label: ")
            .filter(|label| label.starts_with("Command Line Tools"))
            .map(str::to_owned)
    })
}

/// Count log lines carrying `\t<marker>\t`.
fn count_marker(text: &str, marker: &str) -> usize {
    text.lines()
        .filter(|l| l.split('\t').any(|f| f == marker))
        .count()
}

/// One string field out of flat JSON, without a parser: tailscale's `"DNSName": "machine.tailnet."` is the only consumer.
fn json_field(json: &str, field: &str) -> Option<String> {
    let needle = format!("\"{field}\":");
    let start = json.find(&needle)? + needle.len();
    let rest = &json[start..];
    let rest = rest.trim_start();
    let rest = rest.strip_prefix('"')?;
    let end = rest.find('"')?;
    Some(rest[..end].to_owned())
}

#[cfg(test)]
mod tests {
    use super::{
        Xcode, classify_xcode_path, clt_update_label, count_marker, json_field, parse_font_family,
    };

    #[test]
    fn the_first_font_family_line_wins() {
        let cfg = "theme = dark\nfont-family = \"UDEV Gothic NFLG\"\nfont-family = \"Other\"\n";
        assert_eq!(parse_font_family(cfg).as_deref(), Some("UDEV Gothic NFLG"));
        assert_eq!(parse_font_family("theme = dark"), None);
        assert_eq!(parse_font_family("font-family = \"\""), None);
    }

    #[test]
    fn xcode_paths_classify_into_three_states() {
        assert!(matches!(
            classify_xcode_path("/Applications/Xcode.app/Contents/Developer"),
            Xcode::Full
        ));
        assert!(matches!(
            classify_xcode_path("/Library/Developer/CommandLineTools"),
            Xcode::CommandLineToolsOnly
        ));
        assert!(matches!(classify_xcode_path(""), Xcode::None));
    }

    #[test]
    fn bypass_log_lines_are_counted_by_marker() {
        let log = "t1\tBYPASS\tprose\tcwd=x\treason=y\n\
                   t2\tREJECT\tforce\tcwd=x\treason=y\n\
                   t3\tBYPASS\tforeign\tcwd=x\n";
        assert_eq!(count_marker(log, "BYPASS"), 2);
        assert_eq!(count_marker(log, "REJECT"), 1);
        assert_eq!(count_marker("", "BYPASS"), 0);
    }

    #[test]
    fn tailscale_dns_names_come_out_of_flat_json() {
        let json = r#"{"Self": {"DNSName": "box.tail-scale.ts.net.", "Online": true}}"#;
        assert_eq!(
            json_field(json, "DNSName").as_deref(),
            Some("box.tail-scale.ts.net.")
        );
        assert_eq!(json_field(json, "Missing"), None);
    }

    #[test]
    fn the_clt_label_is_found_in_softwareupdate_output() {
        let list = "Software Update found the following new or updated software:\n   * Label: Command Line Tools for Xcode-26.4\n";
        assert_eq!(
            clt_update_label(list).as_deref(),
            Some("Command Line Tools for Xcode-26.4")
        );
        assert_eq!(clt_update_label("   * Label: Safari"), None);
    }
}
