---
name: orbstack
description: >-
  Run containers and Linux machines on the owner's Mac through OrbStack, the Docker engine the Mac profile installs.
  Use for Docker or Linux-machine work on macOS, such as the containerized proof gate, not for container work on other hosts.
metadata:
  tools: "tools.toml"
---

# Containers and Linux machines on macOS with OrbStack

OrbStack provides the Docker engine and the `docker` command on the Mac, so container commands from a project run unchanged there.
Confirm with `docker context show` that the active context names `orbstack` before you blame a project for a container failure.
Use the bounded-tool rule in `formal-assurance`: prefer the project's bounded container entrypoint, such as `just proofs`, or use `dotfiles bounded run`.

OrbStack shares the Mac's memory and CPU with the host and with every other agent.
Check running containers and machines with `docker ps` and `orb list` before you start a large build, and follow `resource-coordination` for long proofs, fuzzing, and mutation campaigns.
A container that fills the disk image fills the Mac's disk, so remove only the images and containers your own work created.

A Linux machine created with `orb create` shares the Mac's home directory, so a write inside it changes host files.
Deleting a machine, resetting OrbStack, or switching the Docker context away affects every project on the Mac, so do those only after the owner approves.
For work on a real Linux host, use `domyjob` on that host instead of an OrbStack machine.

Read `orb --help` and `docker --help` for the installed release, and the [OrbStack documentation](https://docs.orbstack.dev/) for behavior they don't describe.
