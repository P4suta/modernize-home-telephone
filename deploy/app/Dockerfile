FROM node:26.13.0-trixie-slim@sha256:5adad64f9a1fca2d51972caa65983634c8a774eb300d72b20f7f2a0f60368072 AS web
RUN npm install --global pnpm@12.15.0
WORKDIR /build/web
COPY web/ ./
RUN pnpm install --frozen-lockfile --ignore-scripts && pnpm build

FROM rust:1.99.1-trixie@sha256:edc584c5d2f915bcc5b08049987e7a492ccb8ab188444daf0a870b3ef82a3b2e AS build
WORKDIR /build
ENV CARGO_BUILD_JOBS=2
COPY Cargo.toml Cargo.lock ./
COPY crates/ ./crates/
COPY xtask/ ./xtask/
RUN --mount=type=cache,id=homephone-cargo-registry,sharing=locked,target=/usr/local/cargo/registry --mount=type=cache,id=homephone-rust-1.99.1-arm64,sharing=locked,target=/build/target cargo build --release --locked -p phone-server && cp target/release/phone-server /phone-server

FROM build AS admin
RUN rustup target add aarch64-unknown-linux-musl
RUN --mount=type=cache,id=homephone-cargo-registry,sharing=locked,target=/usr/local/cargo/registry --mount=type=cache,id=homephone-rust-1.99.1-arm64,sharing=locked,target=/build/target cargo build --release --locked -p xtask --target aarch64-unknown-linux-musl && cp target/aarch64-unknown-linux-musl/release/xtask /homephone

FROM scratch AS artifacts
COPY --from=admin --chmod=755 /homephone /homephone

FROM debian:trixie-slim@sha256:a72b4433de1cf7d6bae0c4cee74d0547f58601a643b3375f070c51600d63b6b2 AS runtime
COPY --from=build /etc/ssl/certs/ca-certificates.crt /etc/ssl/certs/ca-certificates.crt
COPY deploy/apt-trust.conf /etc/apt/apt.conf.d/90homephone-trust
COPY deploy/debian.sources /etc/apt/sources.list.d/debian.sources
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates=20250419+deb13u1 libssl3t64=3.5.7-1~deb13u3 && rm -rf /var/lib/apt/lists/*
WORKDIR /opt/homephone
COPY --from=build /phone-server /usr/local/bin/phone-server
COPY --from=web /build/web/dist/ web/dist/
USER 10001:10001
ENTRYPOINT ["/usr/local/bin/phone-server"]
