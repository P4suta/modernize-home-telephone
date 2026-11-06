FROM rust:1.99.1-trixie@sha256:edc584c5d2f915bcc5b08049987e7a492ccb8ab188444daf0a870b3ef82a3b2e AS trust

FROM scratch AS source-inputs
ADD --checksum=sha256:3bd5ee040509a3d3cd9b1ba9520c18e6ec0a7e7981ca68c457dcd36ba3c54d94 https://downloads.asterisk.org/pub/telephony/asterisk/releases/asterisk-22.11.0.tar.gz /asterisk.tar.gz
ADD --checksum=sha256:8b418608ce935df77b5c5784aea5a751673ff1a5ae608c690928a524532f0207 https://downloads.asterisk.org/pub/telephony/sounds/releases/asterisk-core-sounds-ja-ulaw-1.6.1.tar.gz /japanese.tar.gz

FROM trust AS build
COPY deploy/apt-trust.conf /etc/apt/apt.conf.d/90homephone-trust
COPY deploy/debian.sources /etc/apt/sources.list.d/debian.sources
RUN apt-get update && apt-get install -y --no-install-recommends libedit-dev=3.1-20250104-1 libjansson-dev=2.14-2+b3 libsrtp2-dev=2.7.0-3 && rm -rf /var/lib/apt/lists/*
COPY --from=source-inputs /asterisk.tar.gz /tmp/asterisk.tar.gz
WORKDIR /usr/src/asterisk
RUN tar --extract --gzip --file=/tmp/asterisk.tar.gz --strip-components=1 && ./configure --prefix=/usr --sysconfdir=/etc --localstatedir=/var --libdir=/usr/lib --with-pjproject-bundled --with-ssl --with-srtp && make menuselect.makeopts && menuselect/menuselect --disable BUILD_NATIVE --disable-category MENUSELECT_CORE_SOUNDS --disable-category MENUSELECT_MOH menuselect.makeopts && make -j2 && make DESTDIR=/image install
COPY --from=source-inputs /japanese.tar.gz /tmp/japanese.tar.gz
RUN mkdir -p /image/var/lib/asterisk/sounds/ja /image/var/lib/asterisk/sounds/homephone /image/usr/share/licenses/asterisk-ja && tar --extract --gzip --file=/tmp/japanese.tar.gz --directory=/image/var/lib/asterisk/sounds/ja && cp /image/var/lib/asterisk/sounds/ja/vm-intro.ulaw /image/var/lib/asterisk/sounds/homephone/voicemail.ulaw && cp /image/var/lib/asterisk/sounds/ja/LICENSE-asterisk-core-ja-1.6.1 /image/var/lib/asterisk/sounds/ja/CREDITS-asterisk-core-ja-1.6.1 /image/usr/share/licenses/asterisk-ja/
WORKDIR /build
COPY Cargo.toml Cargo.lock ./
COPY crates ./crates
COPY xtask ./xtask
ENV CARGO_BUILD_JOBS=2
RUN --mount=type=cache,id=homephone-cargo-registry,sharing=locked,target=/usr/local/cargo/registry --mount=type=cache,id=homephone-rust-1.99.1-arm64,sharing=locked,target=/build/target cargo run --locked -p xtask -- assets --output /assets

FROM build AS source-archive
RUN mkdir /corresponding-source && tar --create --gzip --file=/corresponding-source/asterisk.tar.gz --exclude='*.o' --exclude='*.a' --exclude='*.so' --exclude='*.so.*' --exclude='*.lo' --exclude='*.la' --exclude='.git' --directory=/usr/src asterisk

FROM scratch AS corresponding-source
COPY --from=source-archive /corresponding-source/ /

FROM debian:trixie-slim@sha256:a72b4433de1cf7d6bae0c4cee74d0547f58601a643b3375f070c51600d63b6b2 AS runtime
COPY --from=trust /etc/ssl/certs/ca-certificates.crt /etc/ssl/certs/ca-certificates.crt
COPY deploy/apt-trust.conf /etc/apt/apt.conf.d/90homephone-trust
COPY deploy/debian.sources /etc/apt/sources.list.d/debian.sources
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates=20250419+deb13u1 libedit2=3.1-20250104-1 libjansson4=2.14-2+b3 libsrtp2-1=2.7.0-3 libuuid1=2.41.5-0+deb13u1 libssl3t64=3.5.7-1~deb13u3 libsqlite3-0=3.46.1-7+deb13u2 libxml2=2.12.7+dfsg+really2.9.14-2.1+deb13u3 libspeexdsp1=1.2.1-3 libcurl4t64=8.14.1-2+deb13u7 libxslt1.1=1.1.35-1.2+deb13u3 && rm -rf /var/lib/apt/lists/*
COPY --from=build /image/usr/ /usr/
COPY --from=build /image/etc/ /etc/
COPY --from=build /image/var/lib/ /var/lib/
COPY --from=build /assets/hold.wav /var/lib/asterisk/moh/hold.wav
COPY --from=build /assets/caller.wav /var/lib/asterisk/sounds/homephone/caller.wav
USER 10001:10001
ENTRYPOINT ["/usr/sbin/asterisk"]
CMD ["-f"]
