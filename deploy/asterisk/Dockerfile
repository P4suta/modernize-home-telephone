FROM rust:1.99.1-trixie@sha256:edc584c5d2f915bcc5b08049987e7a492ccb8ab188444daf0a870b3ef82a3b2e AS trust

FROM scratch AS source-inputs
ADD --checksum=sha256:3bd5ee040509a3d3cd9b1ba9520c18e6ec0a7e7981ca68c457dcd36ba3c54d94 https://downloads.asterisk.org/pub/telephony/asterisk/releases/asterisk-22.11.0.tar.gz /asterisk.tar.gz
ADD --checksum=sha256:8b418608ce935df77b5c5784aea5a751673ff1a5ae608c690928a524532f0207 https://downloads.asterisk.org/pub/telephony/sounds/releases/asterisk-core-sounds-ja-ulaw-1.6.1.tar.gz /japanese.tar.gz
ADD --checksum=sha256:1f207f8f972208582558af13d2d02cbac00d8179cbb067bfdd3ba40c216df2a9 https://deb.debian.org/debian/pool/main/libe/libedit/libedit-dev_3.1-20250104-1_arm64.deb /libedit-dev_3.1-20250104-1_arm64.deb
ADD --checksum=sha256:f395dbcace893d5b72c748bf227b43b0aaa92273e35b49f6275bd8c9df85c750 https://deb.debian.org/debian/pool/main/n/nspr/libnspr4-dev_4.36-1_arm64.deb /libnspr4-dev_4.36-1_arm64.deb
ADD --checksum=sha256:c313f9b2f1b0fead31c3f308b6f0b92aa1523e2427231329abfd02fea831fe2c https://deb.debian.org/debian/pool/main/libm/libmd/libmd-dev_1.1.0-2+b1_arm64.deb /libmd-dev_1.1.0-2+b1_arm64.deb
ADD --checksum=sha256:899988c6e8f565dbee63839c5e7ac889119b855b2a2cd6a52f561c4d022901a7 https://deb.debian.org/debian/pool/main/libs/libsrtp2/libsrtp2-1_2.7.0-3_arm64.deb /libsrtp2-1_2.7.0-3_arm64.deb
ADD --checksum=sha256:edb6b04791bccdb37bedb570255d224b1e9c69efa6084e4dac61b3b4904329a2 https://deb.debian.org/debian/pool/main/s/speexdsp/libspeexdsp1_1.2.1-3_arm64.deb /libspeexdsp1_1.2.1-3_arm64.deb
ADD --checksum=sha256:a66b9c960acef8ed9653223cbd00b155a92ebb758ea13a788c41609246c23bc7 https://deb.debian.org/debian/pool/main/libx/libxml2/libxml2_2.12.7+dfsg+really2.9.14-2.1+deb13u3_arm64.deb /libxml2_2.12.7+dfsg+really2.9.14-2.1+deb13u3_arm64.deb

FROM trust AS build
COPY deploy/apt-trust.conf /etc/apt/apt.conf.d/90homephone-trust
COPY deploy/debian.sources /etc/apt/sources.list.d/debian.sources
COPY --from=source-inputs /libedit-dev_3.1-20250104-1_arm64.deb /libnspr4-dev_4.36-1_arm64.deb /libmd-dev_1.1.0-2+b1_arm64.deb /libsrtp2-1_2.7.0-3_arm64.deb /tmp/debs/
RUN apt-get update && mkdir -p /var/cache/apt/archives && cp /tmp/debs/*.deb /var/cache/apt/archives/ && mv /var/cache/apt/archives/libnspr4-dev_4.36-1_arm64.deb /var/cache/apt/archives/libnspr4-dev_2%3a4.36-1_arm64.deb && apt-get install -y --no-remove --no-install-recommends libedit-dev=3.1-20250104-1 libjansson-dev=2.14-2+b3 libmd-dev=1.1.0-2+b1 libnspr4-dev=2:4.36-1 libsrtp2-1=2.7.0-3 libsrtp2-dev=2.7.0-3 && rm -rf /var/lib/apt/lists/* /tmp/debs
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
COPY --from=source-inputs /libsrtp2-1_2.7.0-3_arm64.deb /libspeexdsp1_1.2.1-3_arm64.deb /libxml2_2.12.7+dfsg+really2.9.14-2.1+deb13u3_arm64.deb /tmp/debs/
RUN apt-get update && mkdir -p /var/cache/apt/archives && cp /tmp/debs/*.deb /var/cache/apt/archives/ && apt-get install -y --no-remove --no-install-recommends ca-certificates=20250419+deb13u1 libedit2=3.1-20250104-1 libjansson4=2.14-2+b3 libsrtp2-1=2.7.0-3 libuuid1=2.41.5-0+deb13u1 libssl3t64=3.5.7-1~deb13u3 libsqlite3-0=3.46.1-7+deb13u2 libxml2=2.12.7+dfsg+really2.9.14-2.1+deb13u3 libspeexdsp1=1.2.1-3 libcurl4t64=8.14.1-2+deb13u7 libxslt1.1=1.1.35-1.2+deb13u3 && rm -rf /var/lib/apt/lists/* /tmp/debs
COPY --from=build /image/usr/ /usr/
COPY --from=build /image/etc/ /etc/
COPY --from=build /image/var/lib/ /var/lib/
COPY --from=build /assets/hold.wav /var/lib/asterisk/moh/hold.wav
COPY --from=build /assets/caller.wav /var/lib/asterisk/sounds/homephone/caller.wav
USER 10001:10001
ENTRYPOINT ["/usr/sbin/asterisk"]
CMD ["-f"]
