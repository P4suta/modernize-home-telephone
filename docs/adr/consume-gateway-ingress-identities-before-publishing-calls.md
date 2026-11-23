---
date: 2026-11-15
---

# Consume gateway channel generations before publishing calls

## Status

Proposed.

## Context

A retired gateway channel can create another call, ringing leg, and notification.
Cleanup removes live resources.
History remains deletable.

## Decision

Use a SHA-256 fingerprint of channel ID and canonical creation instant.
Atomically insert it with the initial call, event, outbox, and root resource in one [transaction](https://www.sqlite.org/lang_transaction.html).
Reject consumed identities before creating ringing legs.
Keep fingerprints across cleanup, deletion, retention, and restart.

## Alternatives

Memory loses restart protection.
History-linked state loses deletion protection.
Live Asterisk REST Interface (ARI) reads can't atomically consume admission.
Caller and time deduplication merges legitimate calls.

## Consequences

Each admitted root retains 32 fingerprint bytes plus SQLite overhead.
Fingerprints contain no caller number, audio, or transcript.
Schema 2 requires a pre-upgrade database snapshot for rollback.

## Confirmation

Test replay, deletion, restart, atomic write failure, and valid later generations.
Run the software gate before installation and confirm cancellation on hardware when testing resumes.
Protection starts with post-upgrade admissions.
Reconsider if gateway identity semantics change or fingerprints exceed one million entries.
