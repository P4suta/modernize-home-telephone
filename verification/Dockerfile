FROM rust:1.99.1-trixie@sha256:edc584c5d2f915bcc5b08049987e7a492ccb8ab188444daf0a870b3ef82a3b2e

ENV CARGO_BUILD_JOBS=2
RUN cargo install --locked cargo-mutants --version 27.1.0 && cargo install --locked kani-verifier --version 0.68.0
RUN cargo kani setup
RUN rustup toolchain install nightly-2026-10-01 --profile minimal --component miri,rust-src && cargo install --locked cargo-fuzz --version 0.13.2
WORKDIR /workspace
