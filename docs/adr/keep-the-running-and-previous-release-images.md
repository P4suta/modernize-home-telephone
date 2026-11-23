---
date: 2026-11-23
---

# Keep the running and previous release images

## Status

Proposed.

## Context

Updates never removed release images, and six releases accumulated on the Pi within one day.
Its rootless Podman storage can't shift IDs, so each image that ran under `keep-id` also keeps a mapped layer copy.
`rollback` needs the target release's images in the engine store.

## Decision

After a committed update or rollback, keep the running and replaced releases.
Remove other release source tags that no container uses.
A removal failure leaves the committed release running.
The lab and rehearsal keep every release.

## Alternatives

Keeping every release allows direct rollback to any backup but grows until manual pruning.
Keeping releases that backups reference needs a backup inventory the manager lacks.

## Consequences

Rolling back further requires loading that release's package first.
Each engine store holds one installation, because another installation without containers would lose its images.

## Confirmation

A fault test covers selection, and the deployment rehearsal removes a synthetic tag through the real engine.
Reconsider the count when direct rollback beyond one release becomes necessary.
