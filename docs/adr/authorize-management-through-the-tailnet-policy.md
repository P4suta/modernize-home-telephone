---
date: 2026-11-14
---

# Manage access through the tailnet policy

## Status

Proposed.

## Context

The existing tailnet policy controls access to the Pi's administration ports.
Repeating user lists in both telephone services creates separate authorization state.

## Decision

Use the tailnet policy to control access to HTTPS management endpoints.
Require [Serve's authenticated identity header](https://tailscale.com/docs/features/tailscale-serve#identity-headers) for interactive remote requests and bind backends to loopback.
Keep the local administration token, Origin checks, device login, Session Initiation Protocol credentials, and Asterisk REST Interface credentials.
Preserve local network recovery access without adding network restrictions.

## Alternatives

Per-service user lists add another revocation boundary but require synchronized policy changes.
Serve app capabilities support finer roles but add no needed distinction for the current administration endpoints.

## Consequences

Serve and the host's local processes remain trusted.
Tagged devices need the local token path rather than an interactive identity.
The tailnet policy must deny family access to administration ports.

## Confirmation

Check missing identity, malformed identity, nonloopback peers, foreign origins, and valid identities without a local user list.
Verify the live policy and endpoint exposure before deployment.
Reconsider when the app needs distinct user roles.
