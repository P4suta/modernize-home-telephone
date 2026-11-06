---
date: 2026-11-06
---

# Verify runtime and proofs on native runners

## Status

Proposed.

## Context

The first hosted campaign spent an hour compiling ARM64 code through emulation before detecting a moved Asterisk archive.
[GitHub provides standard ARM64 runners](https://docs.github.com/en/actions/reference/runners/github-hosted-runners), while [Kani supports Linux x86_64 installation](https://model-checking.github.io/kani/install-guide.html).

## Decision

Verify checksum-protected downloads before compilation.
Run the ARM64 lab, browser, deployment, and web mutation checks on an ARM64 runner.
Run the existing AMD64 proof, Miri, fuzz, and Rust mutation containers on an AMD64 runner.
Check native reports through the existing source-bound evidence gate on the same commit.
Keep local image IDs distinct from registry manifest digests.

## Alternatives

A single emulated runner preserves simpler orchestration but delays failures and spends time translating instructions.
Cross-compilation alone can't exercise the ARM64 runtime.

## Consequences

One report transfer adds coordination and storage.
The runtime job follows verification so deployment backups remain on their originating runner.
Artifact retention is one day.
GitHub and Docker remain external trust boundaries.
No software result certifies physical hardware.

## Confirmation

Require valid downloads, rejecting checksum controls, both native campaigns, and the complete evidence inventory.
Reconsider the split if verifier target support changes or source-bound aggregation can't preserve report integrity.
