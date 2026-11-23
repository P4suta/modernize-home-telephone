---
date: 2026-11-14
---

# Publish incoming calls to the private ntfy service

## Status

Proposed.

## Context

The [Pi monitoring service](https://github.com/P4suta/raspberrypi-bootstrap/blob/main/docs/adr/collect-metrics-and-deliver-notifications-on-the-pi.md) already supplies authenticated ntfy and an iPhone subscription.

## Decision

Add optional authenticated ntfy publication to the durable notification worker.
Keep inbound event selection and the 120-second publication window in phone-core.
Send a validated number, original call time, and app link using the [JSON publication API](https://docs.ntfy.sh/publish/#publish-as-json).
Persist successful delivery checkpoints and use the call identity as a [notification sequence](https://docs.ntfy.sh/publish/#updating-notifications) during uncertain retries.
Attempt ntfy and the existing notification channels independently.

## Alternatives

Direct ntfy.sh publication sends caller numbers to a hosted service.
The existing webhook requires translation and separate authentication.
Reusing the private service preserves phone subscriptions.

## Consequences

The Pi, home connection, and Tailscale remain delivery dependencies.
The existing [iOS relay](https://docs.ntfy.sh/config/#ios-instant-notifications) carries wake-up metadata while the app retrieves content privately.
A lost acknowledgement can create another server message even when supported clients replace the same sequence.

## Confirmation

Verify configuration, private payloads, expiry, independent retries, and persistent checkpoints with local HTTP fixtures.
Prove the production eligibility decision and complete the maintained checks before enabling publication.
Reconsider if notifications must survive a home outage or the receiving client can't retrieve private content reliably.
