---
date: 2026-11-06
---

# Restore verification output ownership

## Status

Proposed.

## Context

Native Linux verification succeeded, but report upload failed because a root-owned private directory refused the runner's user.
[Docker bind mounts](https://docs.docker.com/engine/storage/bind-mounts/) expose the same host files to the container.

## Decision

Create output parents as the caller.
After execution, restore the caller's ownership of verification outputs and the current driver directory.
Use a separate restricted container with the same immutable image, no network, and fixed generated paths.
Preserve private file modes and attempt cleanup after verification or ownership failures.

## Alternatives

Running every verifier under the caller's user avoids the ownership transfer but requires new writable Cargo, Kani, and Miri cache arrangements.
Making reports readable by every user weakens their existing privacy boundary.

## Consequences

Each verification command adds a bounded metadata operation.
The Docker engine and operating-system ownership semantics remain external trust assumptions.

## Confirmation

Require process-boundary fault tests, captured ownership and cleanup ordering, caught adapter mutations, and successful native artifact transfer.
Reconsider the transfer when the verifier images support the caller's user without privileged cache setup.
