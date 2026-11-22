---
date: 2026-11-22
---

# Run the telephone gateway with Rusterisk

## Status

Proposed.

## Context

The telephone controller uses the Asterisk REST Interface (ARI) for calls, bridges, recordings, and external audio.
The [call policy decision](keep-call-policy-pure-and-own-effects-at-the-boundaries.md) places routing in `phone-core`.

## Decision

Run [Rusterisk](https://github.com/P4suta/Rusterisk/tree/v0.1.0) as the managed gateway.
Authenticate browser and handset endpoints using the Session Initiation Protocol (SIP).
Replace an endpoint's previous contact when a newly authenticated client registers.
Simulate calls over loopback.
Preserve the Asterisk configuration for rollback and mount the recording spool.
Keep WebSocket signaling on loopback and bind browser media to the local network address.
Package the patched gateway source.
Keep audio active when delivery of a control report fails.

## Alternatives

Staying with Asterisk would preserve its configuration and local channel driver.
Embedding SIP and media in the telephone app would couple call policy and gateway lifetimes.

## Consequences

The gateway image needs the pinned Rusterisk source and uncompressed wave files.
Existing installations keep their credentials and recording paths across an update.

## Confirmation

Run the generated configuration, SIP authentication, browser audio, recording, and deployment recovery checks.
Reconsider the gateway choice if an authenticated handset or browser call fails to complete.
Observe the physical line independently using the [hardware acceptance procedure](../hardware-acceptance.md).
