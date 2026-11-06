---
date: 2026-11-06
---

# Distribute authored sources without vendor originals

## Status

Proposed.
Publication and publisher permission remain unconfirmed.

## Context

The [HT813 Administration Guide](https://www.grandstream.com/hubfs/Product_Documentation/HT813_Administration_Guide.pdf), copyright page, requires written permission for reproduction.
The local history contains preserved manufacturer originals with separate terms.

## Decision

Keep original bytes locally and publish authored procedures, publisher URLs, and checksums.
Exclude originals from source identities, packages, container contexts, and Git archives.
Create the publication candidate from a history-free archive instead of pushing the existing history.
Permit an absent archive during software checks, but reject a partial, modified, or symlinked archive.

## Alternatives

Publishing existing history requires publisher permission.
Deleting local originals loses offline reference material.
Downloading mutable pages during every build couples software verification to publisher availability and changing bytes.

## Consequences

Public contributors use publisher links unless they keep matching originals privately.
Hardware qualification still requires the matching manual edition.

## Confirmation

Native tests exercise Git archive exclusion, source identities, checksum failures, and symlinks.
Verify a fresh public source checkout without the originals.
Reconsider archive distribution after the publishers grant permission.
