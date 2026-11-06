---
date: 2026-11-06
---

# Capture recordings before cloud inference

## Status

Proposed.
The code uses the decision pending architectural acceptance.

## Context

External inference can stall after Asterisk finishes recording.
A serial worker delays local capture and quota enforcement behind stalled requests.

## Decision

Persist independent capture and inference jobs.
Capture validates bounded `WAV` files, synchronizes publication, and commits availability before gateway cleanup.
Inference updates available recordings through conditional writes.
Quota enforcement excludes active and quarantined ownership.

## Alternatives

One worker simplifies scheduling but couples capture to provider completion.
Memory-only capture loses audio after process failure.

## Consequences

Two job lifecycles and a cleanup queue increase database complexity.
Synchronization assumes storage honors requested barriers.
Pi power-loss behavior remains unverified.

## Confirmation

The [provider and cancellation regressions](../../crates/phone-server/tests/postcall_faults.rs) require available audio despite stalled inference and removal of partial downloads.
Reconsider when capture no longer depends on retrieving gateway files.
