---
name: domyjob
description: >-
  Use the personal domyjob client for persistent remote commands and cross-platform project checks.
  Use when a command must run on another machine or survive the client disconnecting.
---

# domyjob

The project-owned [domyjob skill](https://github.com/P4suta/domyjob/blob/main/skills/domyjob/SKILL.md) is the current command manual.
Read its local copy before driving another machine, then follow `multi-machine` for synchronization and host boundaries.
From the source checkout, use `mise x -- cargo run --locked -p domyjob -- COMMAND` or its verified built binary.
For an installed client, verify its version and relevant `--help` rather than assuming old selectors or short job names exist.

Use `run` to send the current checkout as a bounded snapshot and `on` for a command in the remote home directory.
Pass an explicit program after `--` and preserve the remote command's exit status.
Retain the returned machine and job identity so a long job can be inspected without resubmission.
Do not retry an uncertain submission as a new job when the existing submission identity can resolve it.
Remote agent dispatch and sending messages require authorization for those actions; ordinary remote build permission does not imply it.
Do not bypass domyjob with a raw SSH command when a snapshot, job identity, environment boundary, or project instruction depends on domyjob.
