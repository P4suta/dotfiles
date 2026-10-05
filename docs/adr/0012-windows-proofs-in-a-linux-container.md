# Run the Kani proofs on Windows in a pinned Linux container

Status: accepted.

## Context

The pre-push `contracts` hook runs `xtask proofs`, which uses Kani 0.68.0 to check the production policy harnesses and their refusing counterexamples.
Kani publishes no native Windows build, and `verify` refused every host but macOS and Linux.
A Windows machine had no way to push.
The hosted required workflow runs the proofs on macOS only, which contradicts the Linux CI proof gate that [local skill operations](0003-local-skill-operations.md) describes.

## Decision

On Windows, `xtask proofs` runs the same command inside a Linux container through Docker, where the native Linux path runs unchanged.
The container reads the checkout through a read-only bind mount, so the proofs run on the exact source under push and leave it unchanged.
Kani writes temporary files beside the source it compiles.
The container copies the checkout into its own filesystem, without Git metadata, dependencies, or build output, and runs the gate there.
Docker builds the image locally from `xtask/proofs/Dockerfile`, which pins the Rust base image by digest and installs the Kani version from `mise.toml`.
The image tag comes from the definition's content, so a changed definition builds a new image.
Build output lives in a Docker volume keyed by the checkout path, so separate checkouts never share compiled proof models.
A repository test refuses a Kani or Rust version in the definition that differs from the `mise.toml` pins.
Without a running Docker engine for Linux containers, the gate fails with an explicit error.

Kani keeps running natively on macOS and Linux, and the hosted required checks stay unchanged, because GitHub's Windows runners have no Linux containers.

## Alternatives

Rewriting the proofs for Verus, which runs natively on Windows, lost for now: every harness would need a new deductive proof for a host limitation.
Keeping Kani on macOS and Linux and adding Verus on Windows lost because two proof sets for one contract drift apart.
Running Kani in a separate Windows Subsystem for Linux (WSL) distribution lost to the Docker engine that the Windows host already runs.
Skipping the proofs on Windows and relying on the hosted macOS job lost because it removes local proof evidence before publication.

## Consequences

Windows pushes require Docker Desktop with Linux containers running.
The first run builds the image and downloads Kani's toolchain bundle, and later runs reuse the image and the per-checkout build volume.
Changing the Kani pin means updating `mise.toml` and the Dockerfile together.
