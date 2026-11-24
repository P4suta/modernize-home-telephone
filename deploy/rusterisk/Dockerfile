FROM rust:1.100.0-bookworm@sha256:ade39a710ef85f3506cffcfec3e8a24ba090badef1dff2fb798f71d878833fea AS gateway
ADD --checksum=sha256:0f2c3c737a3a0482e1df28e875203ec95bcd8aa8453c5368b9ee488d82e22a70 https://codeload.github.com/P4suta/Rusterisk/tar.gz/7f4ec5ab85e638a2b962a200366b86aa4587dd0c /tmp/rusterisk.tar.gz
ADD --checksum=sha256:0c8447e7bba9701467b5ac6155d157aa9e309e71af4981f78bd4b9176107f553 https://github.com/Kitware/CMake/releases/download/v4.5.0/cmake-4.5.0-linux-aarch64.tar.gz /tmp/cmake.tar.gz
RUN mkdir /opt/cmake && tar -xzf /tmp/cmake.tar.gz --strip-components=1 -C /opt/cmake
ENV PATH="/opt/cmake/bin:${PATH}"
WORKDIR /src
RUN tar -xzf /tmp/rusterisk.tar.gz --strip-components=1 -C /src
COPY deploy/rusterisk/bind-browser-media-to-the-lan.patch /tmp/browser-media.patch
COPY deploy/rusterisk/keep-rtp-active-on-control-send-failure.patch /tmp/control-report.patch
COPY deploy/rusterisk/replace-registered-contacts.patch /tmp/contact.patch
COPY deploy/rusterisk/telephone_contact_replacement_test.rs /src/crates/sip/tests/support/telephone_contact_replacement_test.rs
COPY deploy/rusterisk/buffer-recording-storage-stalls.patch /tmp/recording.patch
COPY deploy/rusterisk/recording_backlog_test.rs /src/crates/server/src/storage/recording_backlog_test.rs
COPY deploy/rusterisk/browser_media_binding_test.rs /src/crates/server/src/browser_media_binding_test.rs
COPY deploy/rusterisk/control_report_delivery_test.rs /src/crates/server/src/rtp/control_report_delivery_test.rs
RUN git apply /tmp/browser-media.patch /tmp/control-report.patch /tmp/contact.patch /tmp/recording.patch && tar --sort=name --mtime=@0 --owner=0 --group=0 --numeric-owner -czf /tmp/rusterisk-build-source.tar.gz .
ENV CARGO_BUILD_JOBS=2
RUN --mount=type=cache,id=homephone-rusterisk-registry,target=/usr/local/cargo/registry,sharing=locked set -eu; \
    for name in call_profiles::browser_media_binding rtp::control_tests::failed_control_delivery_preserves_rtp storage::recording::tests::recording_finish_preserves_a_second_of_buffered_audio storage::recording::tests::an_overflow_is_terminal_even_when_audio_capacity_becomes_available; do \
        cargo test --release --locked -p rusterisk-server --lib "$name" -- --exact > /tmp/media-test.log || { cat /tmp/media-test.log; exit 1; }; \
        cat /tmp/media-test.log; \
        grep -Fqx "test $name ... ok" /tmp/media-test.log; \
    done; \
    cargo test --release --locked -p rusterisk-sip --test registration_bindings > /tmp/registration-test.log || { cat /tmp/registration-test.log; exit 1; }; \
    cat /tmp/registration-test.log; \
    grep -Fqx 'test authenticated_registration_replaces_only_previous_contacts ... ok' /tmp/registration-test.log
RUN --mount=type=cache,id=homephone-rusterisk-registry,target=/usr/local/cargo/registry,sharing=locked CARGO_PROFILE_RELEASE_LTO=fat CARGO_PROFILE_RELEASE_CODEGEN_UNITS=1 CARGO_PROFILE_RELEASE_OPT_LEVEL=s cargo build --locked --release -p rusterisk-server --bin rusterisk && cp target/release/rusterisk /usr/local/bin/rusterisk

FROM rust:1.99.1-trixie@sha256:edc584c5d2f915bcc5b08049987e7a492ccb8ab188444daf0a870b3ef82a3b2e AS assets
WORKDIR /build
COPY Cargo.toml Cargo.lock ./
COPY crates ./crates
COPY xtask ./xtask
ENV CARGO_BUILD_JOBS=2
RUN --mount=type=cache,id=homephone-cargo-registry,sharing=locked,target=/usr/local/cargo/registry cargo run --locked -p xtask -- assets --output /assets
ADD --checksum=sha256:f7bd2b21d72e3538f0f5d7331dbd3dbd56a9cdb339a789daabe689f5a33e484f https://downloads.asterisk.org/pub/telephony/sounds/releases/asterisk-core-sounds-ja-wav-1.6.1.tar.gz /tmp/japanese.tar.gz
RUN mkdir /japanese && tar -xzf /tmp/japanese.tar.gz -C /japanese

FROM scratch AS corresponding-source
COPY --from=gateway /tmp/rusterisk-build-source.tar.gz /rusterisk.tar.gz

FROM debian:trixie-slim@sha256:a72b4433de1cf7d6bae0c4cee74d0547f58601a643b3375f070c51600d63b6b2 AS runtime
COPY --from=gateway /usr/local/bin/rusterisk /usr/local/bin/rusterisk
COPY --from=gateway /src/LICENSE-MIT /src/LICENSE-APACHE /usr/share/licenses/rusterisk/
COPY --from=assets /etc/ssl/certs/ca-certificates.crt /etc/ssl/certs/ca-certificates.crt
COPY --from=assets /assets/caller.wav /assets/hold.wav /usr/share/homephone/
COPY --from=assets /japanese/vm-intro.wav /usr/share/homephone/voicemail.wav
COPY --from=assets /japanese/LICENSE-asterisk-core-ja-1.6.1 /japanese/CREDITS-asterisk-core-ja-1.6.1 /usr/share/licenses/asterisk-ja/
COPY --chmod=755 deploy/rusterisk/start /usr/local/bin/homephone-gateway
USER 10001:10001
ENTRYPOINT ["/usr/local/bin/homephone-gateway"]
