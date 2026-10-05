# Linux verification host for `xtask proofs` where Kani has no native build.
# The toolchain matches mise.toml, and the repository check refuses a drifted Kani version.
FROM docker.io/library/rust:1.99.0-bookworm@sha256:59037199c44290f2befcdd58dcc540164763fc296950255aaefeef096a1866b0

RUN cargo install --locked kani-verifier --version 0.68.0 \
    && cargo kani setup \
    && rm -rf /usr/local/cargo/registry
