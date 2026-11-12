FROM rust:1.99.0-bookworm@sha256:114c7a4425406451c2866b6aafe69fe29b1b298832db1277d411ac73c82d04d6 AS tools
ENV CARGO_BUILD_JOBS=2
RUN rustup toolchain install nightly-2026-08-21 --profile minimal --component miri,rust-src
RUN cargo install --locked cargo-mutants --version 27.1.0 \
    && cargo install --locked cargo-fuzz --version 0.13.2

FROM docker.io/library/rust:1.99.1-bookworm@sha256:9181b56c2a21ab1d5e73cffe04fc762426d30aa7fac3c91141dd033f49a7fc8b
ENV CARGO_BUILD_JOBS=2
COPY --from=tools /usr/local/cargo/bin/cargo-mutants /usr/local/cargo/bin/cargo-fuzz /usr/local/cargo/bin/
COPY --from=tools /usr/local/rustup/toolchains /usr/local/rustup/toolchains
WORKDIR /work
