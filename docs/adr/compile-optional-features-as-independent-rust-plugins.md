---
date: 2026-11-14
---

# Compile optional features as independent Rust plugins

## Status

Proposed.

## Context

Ordinary calls must remain available without AI reception or ntfy.
The call controller owns the line, media resources, and recovery admission.
The server crate coupled optional protocols to host dependencies.

## Decision

Put AI protocols and ntfy publication in independent Rust crates.
Select each implementation through Cargo features and explicit runtime configuration.
Keep typed call transitions, durable events, credentials, and delivery checkpoints in the host adapters.
Plugin code can't depend on the server or administration tool.
Require a host build without either implementation.

## Alternatives

Separate processes provide stronger crash isolation but require authenticated interprocess communication, cancellation, and coordinated recovery.
Sharing SQLite ownership state across processes would invalidate current in-memory admission checks.
Use process isolation when an installation needs untrusted plugins or independent runtime replacement.

## Consequences

Plugins remain statically linked and share process memory.
Network failures remain bounded and asynchronous.
AI is off by default in production initialization.

## Confirmation

Check dependency direction and builds without plugins.
Test optional failures without losing ordinary calls, and keep delivery and media fault tests.
Reconsider when the installation needs crash isolation.
