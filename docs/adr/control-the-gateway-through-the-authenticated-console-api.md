---
date: 2026-11-15
---

# Control the gateway through the authenticated console API

## Status

Proposed.

## Context

Desktop automation interrupts other work.
A headless client needs the browser's submission protections.

## Decision

Add a CLI with an independent [cookie session](https://www.rfc-editor.org/rfc/rfc6265.html#section-4.1).
Keep native authentication, validation, and serialization in the server.
Use HTTPS without extracting browser state or supplying identity headers.

Validate plans through the server.
Bind each plan to its session, origin, page, revision, mode, values, and secret inputs using a [hash-based message authentication code](https://docs.rs/hmac/0.12.1/hmac/).
Exclude plaintext secrets from plans and output.
Claim plans with [exclusive file creation](https://doc.rust-lang.org/std/fs/struct.OpenOptions.html#method.create_new) before submission.
Never retry writes automatically.
Preserve readback and independent secret verification.

## Alternatives

A direct native client can share Rust code but needs distributed write serialization.
[MCP](https://modelcontextprotocol.io/specification/latest/basic/transports) adds a protocol adapter before clients need tool discovery.
Sharing browser storage couples automation to private state.

## Consequences

Server restarts require authentication again.
Session files require filesystem protection.

## Confirmation

Test redaction, changed inputs, stale revisions, replay, logout, and ambiguous outcomes with a synthetic device.
Run the complete software gate before installation.
Reconsider when clients need streamed events.
