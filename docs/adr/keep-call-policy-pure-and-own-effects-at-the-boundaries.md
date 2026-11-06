---
date: 2026-11-06
---

# Keep call policy pure and own effects at the boundaries

## Status

Proposed.
The code uses the decision pending architectural acceptance.

## Context

A household line permits one external owner.
Signaling, database commits, and files don't share one transaction.

## Decision

Keep deterministic decisions in Rust `phone-core` and effects in typed adapters.
Serialize commands and persist channel ownership before relying on external resources.
Use Asterisk for signaling and media through its [REST interface](https://docs.asterisk.org/Latest_API/API_Documentation/Asterisk_REST_Interface/Applications_REST_API/) and [WebSocket media](https://docs.asterisk.org/Configuration/Channel-Drivers/WebSocket/).

## Alternatives

A custom signaling stack removes Asterisk but enlarges protocol obligations.
Circuit modification adds electrical obligations.
A cloud platform adds a service dependency and changes gateway integration.

## Consequences

Deployment needs two containers and an administration executable.
Core proofs establish no carrier, cloud, or electrical behavior.

## Confirmation

The [assurance contracts](../assurance.md) require source-bound Kani results and adapter fault tests.
[Physical acceptance](../hardware-acceptance.md) remains pending.
Reconsider when Asterisk can't expose a required call operation.
