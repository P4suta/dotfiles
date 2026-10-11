# Linux verification host for `xtask proofs` where Kani has no native build.
FROM docker.io/library/rust:1.99.0-bookworm@sha256:114c7a4425406451c2866b6aafe69fe29b1b298832db1277d411ac73c82d04d6

RUN cargo install --locked kani-verifier --version 0.68.0 \
    && cargo kani setup \
    && rm -rf /usr/local/cargo/registry
