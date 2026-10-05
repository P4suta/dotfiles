//! What `~/.local/bin/git` refuses to run.
//!
//! Two families of rule live here:
//!
//! * **force**: operations that discard committed history or uncommitted work with no undo.
//!   `push --force`, `reset --hard`, `clean -f`, `branch -D`, `stash drop`, `reflog expire`, `filter-branch`.
//! * **no-verify**: operations that switch off signing or hook enforcement for one command, such as `--no-verify`, `--no-gpg-sign`, and their `-c key=value` forms.
//!
//! This layer only handles commands that resolve `git` through `PATH`, so a direct path or a different `PATH` bypasses it.
//! `dotguard pre-push` runs from `core.hooksPath` and enforces the rule for every push.
//!
//! Rules left out:
//! * `rebase` and `commit --amend`: rewriting unpushed history counts as ordinary work, and pre-push catches pushed history.
//! * `restore` and `checkout -- <path>`: no flag marks the destruction, so a rule would refuse the whole command.
//! * `rm -f`: common in scripts, and the index still holds what it removes.

use crate::bypass::Category;
use crate::realgit;

pub struct Denial {
    pub category: Category,
    /// One line, for the audit log.
    pub reason: String,
    /// What to do instead.
    /// The hint for the human, which can span lines.
    pub hint: String,
}

impl Denial {
    fn new(category: Category, reason: impl Into<String>, hint: impl Into<String>) -> Self {
        Self {
            category,
            reason: reason.into(),
            hint: hint.into(),
        }
    }
}

/// Global options that consume the following argv element as their value.
/// The list omits `--exec-path`, because its value stays optional and the bare form prints a path and exits.
const GLOBAL_TAKES_VALUE: [&str; 8] = [
    "-c",
    "-C",
    "--git-dir",
    "--work-tree",
    "--namespace",
    "--config-env",
    "--super-prefix",
    "--attr-source",
];

/// Known git subcommands.
const BUILTINS: [&str; 46] = [
    "add",
    "am",
    "apply",
    "archive",
    "bisect",
    "blame",
    "branch",
    "checkout",
    "cherry-pick",
    "clean",
    "clone",
    "commit",
    "config",
    "describe",
    "diff",
    "fetch",
    "filter-branch",
    "fsck",
    "gc",
    "grep",
    "init",
    "log",
    "ls-files",
    "merge",
    "mv",
    "notes",
    "pull",
    "push",
    "rebase",
    "reflog",
    "remote",
    "reset",
    "restore",
    "revert",
    "rm",
    "shortlog",
    "show",
    "stash",
    "status",
    "submodule",
    "switch",
    "tag",
    "update-ref",
    "worktree",
    "verify-commit",
    "help",
];

/// Decide whether this git invocation may proceed.
pub fn inspect(argv: &[String]) -> Option<Denial> {
    let mut i = 0;
    // Collect `-c` settings first, because the subcommand decides whether one overrides policy.
    let mut settings: Vec<String> = Vec::new();

    while i < argv.len() {
        let arg = argv[i].as_str();
        if !arg.starts_with('-') {
            break;
        }
        if arg == "--" {
            i += 1;
            break;
        }
        if let Some(rest) = arg.strip_prefix("-c") {
            // Both `-c key=value` and the attached `-ckey=value` form.
            if rest.is_empty() {
                i += 1;
                if let Some(v) = argv.get(i) {
                    settings.push(v.clone());
                }
            } else {
                settings.push(rest.to_owned());
            }
        } else if arg.starts_with("--config-env") {
            // `--config-env=key=ENVVAR` hides the value in the environment, so the key decides alone, recorded as `key=` for an unseen value.
            let setting = arg.strip_prefix("--config-env=").map_or_else(
                || {
                    i += 1;
                    argv.get(i).map(String::as_str).unwrap_or_default()
                },
                |s| s,
            );
            if let Some(key) = setting.split('=').next()
                && is_policy_key(key)
            {
                settings.push(format!("{key}="));
            }
        } else if GLOBAL_TAKES_VALUE.contains(&arg) {
            i += 1;
        }
        i += 1;
    }

    let sub = argv.get(i)?;
    let rest = &argv[i + 1..];

    for setting in &settings {
        if let Some(denial) = config_override(setting, sub) {
            return Some(denial);
        }
    }

    if let Some(denial) = subcommand(sub, rest) {
        return Some(denial);
    }

    // An `alias.*` entry can expand to anything, including `push --force`.
    // Resolving every token would put a `git config` call in front of `git status`, so only tokens unknown to git get a lookup.
    if !BUILTINS.contains(&sub.as_str())
        && let Some(expansion) = realgit::capture(&["config", "--get", &format!("alias.{sub}")])
    {
        let words: Vec<String> = expansion.split_whitespace().map(str::to_owned).collect();
        // `!sh -c ...` aliases run an arbitrary shell, beyond the reach of this policy.
        if let Some(first) = words.first()
            && !first.starts_with('!')
            && let Some(denial) = subcommand(first, &words[1..])
        {
            return Some(Denial::new(
                denial.category,
                format!("alias {sub} -> {}", expansion.trim()),
                denial.hint,
            ));
        }
    }

    None
}

/// The per-subcommand rules.
#[allow(clippy::too_many_lines)]
fn subcommand(sub: &str, args: &[String]) -> Option<Denial> {
    let flags = flags_of(args);
    let has = |f: &str| flags.iter().any(|a| a == f);
    if has("--no-verify")
        || (sub == "commit"
            && flags
                .iter()
                .any(|arg| short_cluster(arg).is_some_and(|cluster| cluster.contains('n'))))
    {
        return Some(Denial::new(
            Category::NoVerify,
            "--no-verify",
            "Hook verification cannot be skipped.",
        ));
    }
    match sub {
        "push" => {
            for a in &flags {
                let deny = |reason: &str, hint: &str| Some(Denial::new(Category::Force, reason, hint));
                match a.as_str() {
                    "-f" | "--force" => {
                        return deny(
                            "git push --force",
                            "A force push replaces history other clones already have.\n\
                             If you rebased on purpose:  ALLOW_FORCE=1 git push --force-with-lease\n\
                             --force-with-lease at least refuses when someone else has pushed since your last fetch.",
                        );
                    }
                    "--force-if-includes" => {
                        return deny("git push --force-if-includes", "Still a force push. ALLOW_FORCE=1 to proceed.");
                    }
                    "--mirror" => {
                        return deny(
                            "git push --mirror",
                            "--mirror makes the remote match this clone exactly, deleting every ref you do not have.",
                        );
                    }
                    "-d" | "--delete" => {
                        return deny(
                            "git push --delete",
                            "Deleting a remote ref is not recoverable from here.\n\
                             Delete merged branches through the forge, where the ref is still in the reflog.",
                        );
                    }
                    other if other.starts_with("--force-with-lease") => {
                        return deny(
                            "git push --force-with-lease",
                            "Safer than --force and still a rewrite of published history.\n\
                             ALLOW_FORCE=1 git push --force-with-lease",
                        );
                    }
                    _ => {}
                }
            }
// Refspecs: `+src:dst` forces that one ref, `:dst` deletes it.
            for a in args.iter().filter(|a| !a.starts_with('-')) {
                if let Some(stripped) = a.strip_prefix('+')
                    && stripped.contains(':')
                {
                    return Some(Denial::new(
                        Category::Force,
                        format!("git push refspec {a}"),
                        "A leading '+' on a refspec is a force push for that ref.".to_owned(),
                    ));
                }
                if a.starts_with(':') && a.len() > 1 {
                    return Some(Denial::new(
                        Category::Force,
                        format!("git push refspec {a}"),
                        "A refspec with an empty source deletes the remote ref.".to_owned(),
                    ));
                }
            }
            None
        }

        "reset" if has("--hard") => Some(Denial::new(
            Category::Force,
            "git reset --hard",
            "--hard discards every uncommitted change with no reflog entry to get it back.\n\
             git stash --include-untracked   keeps the work and gives you a clean tree\n\
             git reset --keep <commit>       moves the branch but refuses to clobber local edits",
        )),

        "clean" if flags.iter().any(|a| short_cluster(a).is_some_and(|c| c.contains('f'))) || has("--force") => {
            Some(Denial::new(
                Category::Force,
                "git clean -f",
                "Untracked files are not in git; once removed there is nothing to restore from.\n\
                 git clean -nd                    list exactly what would go\n\
                 git stash push --include-untracked   keep it instead of deleting it",
            ))
        }

        "checkout" | "switch"
            if has("--force") || has("--discard-changes") || flags.iter().any(|a| short_cluster(a).is_some_and(|c| c.contains('f'))) =>
        {
            Some(Denial::new(
                Category::Force,
                format!("git {sub} --force"),
                "--force here means 'overwrite my uncommitted changes'.\n\
                 git stash   first, then switch; the changes come back with `git stash pop`."
                    .to_owned(),
            ))
        }

        "branch" => {
            let force_delete = flags.iter().any(|a| short_cluster(a).is_some_and(|c| c.contains('D')))
                || ((has("-d") || has("--delete")) && (has("-f") || has("--force")));
            let force_move = flags.iter().any(|a| short_cluster(a).is_some_and(|c| c.contains('M')))
                || ((has("-m") || has("--move")) && (has("-f") || has("--force")));
            if force_delete {
                Some(Denial::new(
                    Category::Force,
                    "git branch -D",
                    "-D deletes a branch whose commits are not merged anywhere.\n\
                     git branch -d <name>   deletes it only if it is merged — if that refuses,\n\
                     the commits really would become unreachable.",
                ))
            } else if force_move {
                Some(Denial::new(
                    Category::Force,
                    "git branch -M",
                    "-M overwrites an existing branch of the target name.",
                ))
            } else {
                None
            }
        }

        "tag" => {
            if has("-d") || has("--delete") {
                Some(Denial::new(Category::Force, "git tag --delete", "A deleted tag is gone from this clone's history of releases."))
            } else if has("-f") || has("--force") {
                Some(Denial::new(
                    Category::Force,
                    "git tag --force",
                    "--force moves an existing tag. Anyone who fetched the old one keeps it, and the two disagree forever.",
                ))
            } else {
                None
            }
        }

        "stash" => match first_word(args) {
            Some("drop") => Some(Denial::new(
                Category::Force,
                "git stash drop",
                "The stash has no reflog you are likely to find in time.\n\
                 git stash show -p stash@{0}   look at it first",
            )),
            Some("clear") => Some(Denial::new(Category::Force, "git stash clear", "This drops every stash entry at once.")),
            _ => None,
        },

        "update-ref" if has("-d") || has("--delete") => Some(Denial::new(
            Category::Force,
            "git update-ref -d",
            "Deleting a ref by hand bypasses every safety net the porcelain commands have.",
        )),

        "reflog" => match first_word(args) {
            Some("expire" | "delete") => Some(Denial::new(
                Category::Force,
                format!("git reflog {}", first_word(args).unwrap_or("expire")),
                "The reflog is what makes `reset --hard` and a bad rebase recoverable. Expiring it is the one\n\
                 operation that turns a recoverable mistake into a permanent one."
                    .to_owned(),
            )),
            _ => None,
        },

        "gc" if flags.iter().any(|a| a.starts_with("--prune=") && matches!(&a["--prune=".len()..], "now" | "all")) => {
            Some(Denial::new(
                Category::Force,
                "git gc --prune=now",
                "This deletes unreachable objects immediately, including anything a reflog entry would have found.",
            ))
        }

        "filter-branch" => Some(Denial::new(
            Category::Force,
            "git filter-branch",
            "Rewrites every commit it touches. git-filter-repo is the maintained replacement and is no less destructive.",
        )),

        "worktree" if first_word(args) == Some("remove") && (has("-f") || has("--force")) => Some(Denial::new(
            Category::Force,
            "git worktree remove --force",
            "--force removes a worktree that still has uncommitted changes in it.",
        )),

        _ if has("--no-gpg-sign") => Some(Denial::new(
            Category::NoVerify,
            "--no-gpg-sign",
            "Every commit on this machine is signed; an unsigned one is refused by the repository ruleset anyway.",
        )),

        _ => None,
    }
}

/// Subcommands that actually run a hook.
const RUNS_HOOKS: [&str; 12] = [
    "commit",
    "merge",
    "rebase",
    "push",
    "pull",
    "am",
    "cherry-pick",
    "revert",
    "checkout",
    "switch",
    "applypatch",
    "clone",
];

/// Subcommands that create a signable object.
const CREATES_SIGNED_OBJECTS: [&str; 7] = [
    "commit",
    "tag",
    "merge",
    "rebase",
    "cherry-pick",
    "revert",
    "am",
];

/// Whether a `-c key=value` sets something the signing policy owns.
fn is_policy_key(key: &str) -> bool {
    matches!(
        key,
        "commit.gpgsign" | "tag.gpgsign" | "core.hooksPath" | "gpg.format" | "gpg.ssh.program"
    )
}

/// `-c key=value` overrides that would change signing or hooks for one command.
///
/// The check depends on the subcommand.
/// Editors, agents, and status-line tools run read-only queries such as
///
/// ```text
/// git -c core.hooksPath=/dev/null -c core.askPass= … rev-parse HEAD
/// ```
///
/// so their polling skips your hooks.
/// `rev-parse`, `status`, `remote get-url`, and `ls-files` run no hooks, so refusing them breaks the tool for no gain.
fn config_override(setting: &str, sub: &str) -> Option<Denial> {
    let (key, value) = setting.split_once('=')?;
    let offends = match key {
        // `-c commit.gpgsign=false` acts as `--no-gpg-sign`.
        "commit.gpgsign" | "tag.gpgsign" => {
            CREATES_SIGNED_OBJECTS.contains(&sub)
                && matches!(value, "false" | "0" | "no" | "off" | "")
        }
        // A per-invocation hooksPath change can replace every gate in ~/.config/git/hooks.
        "core.hooksPath" => RUNS_HOOKS.contains(&sub),
        // The SSH agent signs every commit, so any other format names a key this machine lacks.
        "gpg.format" => CREATES_SIGNED_OBJECTS.contains(&sub) && value != "ssh",
        "gpg.ssh.program" => CREATES_SIGNED_OBJECTS.contains(&sub) && value.is_empty(),
        _ => false,
    };
    offends.then(|| {
        Denial::new(
            Category::NoVerify,
            format!("-c {setting}"),
            format!("`{key}={value}` overrides signing or hook enforcement for this one command."),
        )
    })
}

/// argv up to `--`, because everything after it forms a pathspec and a file may carry the name `--force`.
fn flags_of(args: &[String]) -> Vec<String> {
    args.iter()
        .take_while(|a| a.as_str() != "--")
        .cloned()
        .collect()
}

/// The first non-flag argument, a subcommand word like `drop` or `expire`.
fn first_word(args: &[String]) -> Option<&str> {
    args.iter()
        .take_while(|a| a.as_str() != "--")
        .find(|a| !a.starts_with('-'))
        .map(String::as_str)
}

/// The letters of a clustered short-option group, so `-fdx` yields `fdx`, or `None` for long options and bare `-` or `--`.
fn short_cluster(arg: &str) -> Option<&str> {
    let rest = arg.strip_prefix('-')?;
    (!rest.is_empty() && !rest.starts_with('-') && rest.chars().all(|c| c.is_ascii_alphabetic()))
        .then_some(rest)
}

#[cfg(test)]
mod tests {
    use super::inspect;

    fn argv(line: &str) -> Vec<String> {
        line.split_whitespace().map(str::to_owned).collect()
    }
    fn refused(line: &str) -> bool {
        inspect(&argv(line)).is_some()
    }

    #[test]
    fn destructive_commands_are_refused() {
        for line in [
            "push --force",
            "push -f origin main",
            "push --force-with-lease",
            "push --force-if-includes",
            "push --mirror",
            "push --delete origin topic",
            "push origin :topic",
            "push origin +main:main",
            "reset --hard HEAD~1",
            "clean -fdx",
            "clean --force",
            "checkout -f main",
            "switch --discard-changes main",
            "branch -D topic",
            "branch -M main",
            "tag -d v1",
            "tag -f v1",
            "stash drop",
            "stash clear",
            "update-ref -d refs/heads/topic",
            "reflog expire --expire=now --all",
            "gc --prune=now",
            "filter-branch --tree-filter true HEAD",
            "worktree remove --force w",
            "commit --no-verify -m x",
            "commit -n -m x",
            "commit -an -m x",
            "push --no-verify origin main",
            "commit --no-gpg-sign -m x",
        ] {
            assert!(refused(line), "should refuse: git {line}");
        }
    }

    #[test]
    fn ordinary_commands_are_not() {
        for line in [
            "status",
            "commit -m x",
            "push",
            "push origin main",
            "push --tags",
            "add --force ignored.txt", // Here `-f` adds an ignored file.
            "branch -d merged-topic",
            "clean -nd",
            "checkout main",
            "switch main",
            "rebase -i main",
            "commit --amend --no-edit",
            "stash list",
            "stash pop",
            "tag v1",
            "gc --auto",
            "log --oneline",
            "rm -f path", // reachable from the index in every case that matters
        ] {
            assert!(!refused(line), "should allow: git {line}");
        }
    }

    /// Editors and agents run read-only queries with `core.hooksPath=/dev/null`, and refusing those protects nothing.
    #[test]
    fn read_only_queries_may_disable_hooks() {
        for line in [
            "-c core.hooksPath=/dev/null rev-parse HEAD",
            "-c core.hooksPath=/dev/null -c core.askPass= remote get-url origin",
            "-c core.hooksPath=/dev/null --no-optional-locks status --porcelain",
            "-c core.hooksPath=/dev/null ls-files --recurse-submodules",
            "-c commit.gpgsign=false config --get user.name",
            "-c gpg.format=openpgp log --show-signature -1",
        ] {
            assert!(!refused(line), "should allow: git {line}");
        }
    }

    #[test]
    fn the_same_overrides_are_refused_where_they_would_bite() {
        for line in [
            "-c core.hooksPath=/dev/null commit -m x",
            "-c core.hooksPath= push origin main",
            "-c core.hooksPath=/tmp/other push origin main",
            "-c commit.gpgsign=false commit -m x",
            "-c tag.gpgsign=no tag v1",
            "-c gpg.format=openpgp commit -m x",
            "-ccommit.gpgsign=false commit -m x",
        ] {
            assert!(refused(line), "should refuse: git {line}");
        }
    }

    #[test]
    fn a_pathspec_named_like_a_flag_is_not_a_flag() {
        assert!(!refused("clean -n -- --force"));
    }
}
