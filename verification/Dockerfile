FROM rust:1.99.1-trixie@sha256:8a4e90c7946b74626488b1446f712c0fe0ed1a596cbd3d871684bc9c5433ae07

ENV CARGO_BUILD_JOBS=2
RUN cargo install --locked cargo-mutants --version 27.1.0 && cargo install --locked kani-verifier --version 0.68.0
RUN cargo kani setup
RUN rustup toolchain install nightly-2026-10-01 --profile minimal --component miri,rust-src && cargo install --locked cargo-fuzz --version 0.13.2
WORKDIR /workspace
