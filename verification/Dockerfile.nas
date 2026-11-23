FROM rust:1.99.1-trixie@sha256:8a4e90c7946b74626488b1446f712c0fe0ed1a596cbd3d871684bc9c5433ae07 AS build
WORKDIR /build
ENV CARGO_BUILD_JOBS=2
COPY Cargo.toml Cargo.lock ./
COPY crates/ ./crates/
COPY xtask/ ./xtask/
COPY console/src/lib/parameters.json console/src/lib/settings.json console/src/lib/firmware-parameters.json ./console/src/lib/
RUN --mount=type=cache,id=homephone-cargo-registry,sharing=locked,target=/usr/local/cargo/registry --mount=type=cache,id=homephone-rust-1.99.1-arm64,sharing=locked,target=/build/target cargo build --release --locked -p phone-server --no-default-features --features nas-lab --bin nas-lab --bin phone-server && cp target/release/nas-lab /nas-lab && cp target/release/phone-server /phone-server

FROM debian:trixie-slim@sha256:a72b4433de1cf7d6bae0c4cee74d0547f58601a643b3375f070c51600d63b6b2
COPY --from=build /etc/ssl/certs/ca-certificates.crt /etc/ssl/certs/ca-certificates.crt
COPY deploy/apt-trust.conf /etc/apt/apt.conf.d/90homephone-trust
COPY deploy/debian.sources /etc/apt/sources.list.d/debian.sources
RUN apt-get update && apt-get install -y --no-install-recommends samba=2:4.22.11+dfsg-0+deb13u1 smbclient=2:4.22.11+dfsg-0+deb13u1 util-linux=2.41.5-0+deb13u1 libssl3t64=3.5.7-1~deb13u3 && useradd --no-create-home --shell /usr/sbin/nologin recorder && rm -rf /var/lib/apt/lists/*
COPY --from=build /nas-lab /usr/local/bin/nas-lab
COPY --from=build /phone-server /usr/local/bin/phone-server
ENTRYPOINT ["/usr/bin/timeout", "--signal=TERM", "--kill-after=10s", "900s", "/usr/local/bin/nas-lab"]
