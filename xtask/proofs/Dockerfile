# Linux verification host for `xtask proofs` where Kani has no native build.
FROM docker.io/library/rust:1.99.0-bookworm@sha256:59037199c44290f2befcdd58dcc540164763fc296950255aaefeef096a1866b0

RUN cargo install --locked kani-verifier --version 0.68.0 \
    && cargo kani setup \
    && cargo install --locked cargo-mutants --version 27.1.0 \
    && rm -rf /usr/local/cargo/registry

# Miri interprets the pure core's tests for the host and for Windows on the nightly that Kani 0.68.0 pins.
# Each run builds its sysroots, because a sysroot built here doesn't match the settings of a run.
RUN rustup toolchain install nightly-2026-08-21 --profile minimal --component miri,rust-src
