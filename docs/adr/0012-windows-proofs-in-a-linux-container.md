# Run the Kani proofs on Windows in a pinned Linux container

Status: Accepted.

## Context

The pre-push `contracts` hook runs `xtask proofs`, which verifies the production policy harnesses and their rejecting counterexamples with Kani 0.68.0.
Kani publishes no native Windows build, and `verify` refused every host other than macOS and Linux.
A Windows machine therefore could not push any change by itself, even when every other gate passed.
The hosted required workflow runs the proofs on macOS only, so ADR 0003's description of a Linux CI proof gate does not match the workflow.

## Decision

On Windows, `xtask proofs` runs the same `xtask proofs` command inside a Linux container through Docker, where the native Linux path executes unchanged.
The container sees the checkout through a read-only bind mount, so the proofs run against the exact source being pushed and cannot change it.
Kani writes temporary files beside the source it compiles, so the container copies the checkout without Git metadata, dependencies, or build output into its own filesystem and runs the gate there.
The image is built locally from `xtask/proofs/Dockerfile`, which pins the Rust base image by digest and installs the same Kani version as `mise.toml`.
The image tag is derived from the definition's content, so a changed definition builds a new image instead of reusing a stale one.
Build output lives in a Docker volume keyed by the checkout path, so separate checkouts never reuse each other's compiled proof models.
A repository test refuses a Kani or Rust version in the definition that differs from the `mise.toml` pins.
Without a running Docker engine for Linux containers, the gate fails with an explicit error.

macOS and Linux keep running Kani natively, and the hosted required checks are unchanged, because GitHub's Windows runners cannot run Linux containers.

## Alternatives

Rewriting the proofs for Verus, which runs natively on Windows, was rejected for now: every harness would need a new deductive proof, and the verifier change is unrelated to the host limitation.
Keeping Kani on macOS and Linux while adding Verus on Windows was rejected because two proof sets for one contract can drift silently.
Running Kani in a separate WSL distribution was rejected in favor of the Docker engine that the Windows host already runs, which needs no extra distribution to maintain.
Skipping the proofs on Windows and relying on the hosted macOS job was rejected because it would remove local proof evidence before publication.

## Consequences

Windows pushes require Docker Desktop with Linux containers running.
The first run builds the image and downloads Kani's toolchain bundle; later runs reuse the image and the per-checkout build volume.
Changing the Kani pin requires updating `mise.toml` and the Dockerfile together.
