---
name: domyjob
description: >-
  Use the personal domyjob client for persistent remote commands and cross-platform project checks.
  Use when a command must run on another machine or survive the client disconnecting.
---

# Remote jobs

The project's [domyjob skill](https://github.com/P4suta/domyjob/blob/main/skills/domyjob/SKILL.md) holds the current command manual.
Read its local copy before driving another machine, then follow `multi-machine` for synchronization and host boundaries.
From the source checkout, run `mise x -- cargo run --locked -p domyjob -- COMMAND` or its verified built binary.
For an installed client, check its version and the relevant `--help` instead of assuming old selectors or short job names.

Use `run` to send the current checkout as a bounded snapshot, and `on` for a command in the remote `HOME`.
Pass an explicit program after `--`, and keep the remote command's exit status.
Keep the returned machine and job identity, so a long job stays inspectable without resubmission.
Resolve an uncertain submission through its identity instead of retrying it as a new job.
Remote assistant dispatch and message sending each need their own authorization, beyond remote build permission.
Never bypass domyjob with a raw SSH command when a snapshot, job identity, environment boundary, or project instruction depends on domyjob.
