FROM rust:1.99.1-trixie@sha256:edc584c5d2f915bcc5b08049987e7a492ccb8ab188444daf0a870b3ef82a3b2e AS trust
FROM debian:trixie-slim@sha256:a72b4433de1cf7d6bae0c4cee74d0547f58601a643b3375f070c51600d63b6b2
COPY --from=trust /etc/ssl/certs/ca-certificates.crt /etc/ssl/certs/ca-certificates.crt
COPY deploy/apt-trust.conf /etc/apt/apt.conf.d/90homephone-trust
COPY deploy/debian.sources /etc/apt/sources.list.d/debian.sources
ADD --checksum=sha256:103364925efc4e4cdb5129cbc10cb12543969ec9993a0765cdb71f735d54a16f https://deb.debian.org/debian/pool/main/g/gsl/libgslcblas0_2.8+dfsg-5_arm64.deb /tmp/debs/libgslcblas0_2.8+dfsg-5_arm64.deb
ADD --checksum=sha256:28b8b9983b5e21144dc1a7dd37cb3528de1cebb7e020d9c0a544cafcdaf5d170 https://deb.debian.org/debian/pool/main/g/gsl/libgsl28_2.8+dfsg-5_arm64.deb /tmp/debs/libgsl28_2.8+dfsg-5_arm64.deb
ADD --checksum=sha256:1444270448db8119b5cb0086305e717b8a5e5e0ff0be376c5acba2685127c4e6 https://deb.debian.org/debian/pool/main/w/wolfssl/libwolfssl42t64_5.7.2-0.1+deb13u2_arm64.deb /tmp/debs/libwolfssl42t64_5.7.2-0.1+deb13u2_arm64.deb
ADD --checksum=sha256:1ac131653be353a2949ee0e2e00472a7b9e561270067fe859f59842eb63f4b0e https://deb.debian.org/debian/pool/main/s/sip-tester/sip-tester_3.7.3-2_arm64.deb /tmp/debs/sip-tester_3.7.3-2_arm64.deb
RUN apt-get update && apt-get install -y --no-install-recommends /tmp/debs/*.deb && rm -rf /var/lib/apt/lists/* /tmp/debs
USER 10001:10001
ENTRYPOINT ["/usr/bin/sipp"]
