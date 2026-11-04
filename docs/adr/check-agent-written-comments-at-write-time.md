---
status: proposed
date: 2026-10-29
---
# Check agent-written comments at write time

## Context

[The lint hook](lint-agent-writes-and-script-invocations.md) runs after every agent file mutation but selects linters only for shell, Python, JavaScript, workflow, TOML, Dockerfile, and justfile files.
A Rust, Markdown, or other source edit therefore runs no check, and the comment policy that OComment enforces reaches the agent only through the global Lefthook pre-commit stage.
A doc comment that wraps one sentence across lines survives until a commit, after which the agent has already moved on.

## Decision

Every file type selects OComment as its last linter, including the unknown type, because OComment chooses languages from its own table and skips the rest.
The adapter runs `ocomment check` through the same bounded runner, passes the repository's `.ocomment.toml` when one exists, and blocks on any reported comment with the reproducing command.
An unknown file that is not a regular file within the inspection limit is admitted without being read, and only shell files are read for the Bash portability check.
The pure rule module keeps the language linters first and the comment check last, so the proof over every file type checks both facts.

## Alternatives

Registering `ocomment hook claude-code` as a separate Claude Code hook would duplicate the runner policy and leave Codex and OpenCode without the check.
Adding a file type per language OComment knows would copy its language table into the rules and drift from it.

## Consequences

Agents receive comment findings in the same turn as the edit, with the `ocomment fix --tidy` next action in the report.
Unknown file types no longer admit an empty outcome set; they admit OComment's silence.
Repositories without `.ocomment.toml` receive OComment's default policy, which is the same policy the pre-commit stage applies.
The hook test writes a wrapped doc comment, expects a blocking finding, rewrites it to one sentence per line, and expects admission.
