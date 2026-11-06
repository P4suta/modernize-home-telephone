# Third-party materials

## Project source

Authored project source and documentation use `MIT OR Apache-2.0` under the root license files.
The root licenses grant no rights to manufacturer manuals, product trademarks, dependencies, operating system packages, or bundled recordings.
Grandstream, HT813, au, Raspberry Pi, Asterisk, and other product names identify their respective products.
The project claims no manufacturer affiliation or certified interoperability.

## Manufacturer documentation

The [source manifest](.agents/skills/grandstream-ht813/references/sources.json) records publisher URLs, retrieval dates, editions, sizes, and checksums.
The authored skill and operational references remain in the public source distribution.
Preserved publisher originals remain local and are absent from public source archives, container build contexts, and maintained package source inventories.

The HT813 Administration Guide's copyright page requires Grandstream's written permission for reproduction.
The [publisher's PDF](https://www.grandstream.com/hubfs/Product_Documentation/HT813_Administration_Guide.pdf) retains the governing notice.
Possession of a downloaded manual establishes no project license for redistribution.
Keep original notices and bytes in a private archive.
The [publication guide](docs/publication.md) explains why the existing local Git history must remain private.

## Runtime and development dependencies

`Cargo.lock` and `web/pnpm-lock.yaml` identify the resolved Rust and JavaScript dependency versions.
Dependency licenses remain in effect.
The runtime images also contain Debian packages under their package-specific terms.

The Asterisk build uses the pinned source archive and bundled dependencies declared in `deploy/asterisk/Dockerfile`.
Deployment packages include the corresponding Asterisk source archive under `gateway-source/asterisk.tar.gz`.
The archive retains the upstream source notices.
The gateway image includes `LICENSE-asterisk-core-ja-1.6.1` and `CREDITS-asterisk-core-ja-1.6.1` for Japanese Asterisk sounds under `/usr/share/licenses/asterisk-ja`.
The voicemail announcement derives from the pinned Japanese sound archive.
Generated lab tones come from the project's waveform generator.

Source publication doesn't establish license completeness for a future binary or container release.
Review the exact distributed dependency artifacts and corresponding-source obligations before publishing compiled packages.
