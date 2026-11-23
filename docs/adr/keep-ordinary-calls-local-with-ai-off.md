---
date: 2026-11-14
---

# Keep ordinary calls local with optional reception off

## Status

Proposed.

## Context

The requested installation rings the telephone and publishes private incoming-call notifications.
The installation has no AI credentials and sends no audio to AI providers.
The current default screens unknown callers and records connected calls.

## Decision

Add an explicit `disabled` AI provider.
With AI off, route unblocked incoming calls to the telephone without starting media inference or recordings.
End unanswered calls without entering voicemail.
Refuse provider sessions even when credentials are present.
Finish existing recording jobs without cloud inference.

## Alternatives

Missing credentials trigger screening failure and fallback after answering the line.
That behavior changes ordinary ringing and leaves recording active.
Per-number rules can't cover unknown callers.

## Consequences

AI screening, speech, and voicemail require a different provider configuration.
Existing call metadata and notification delivery remain available.

## Confirmation

Test ringing, blocked callers, unanswered calls, recording refusal, and provider isolation with synthetic credentials and HTTP observers.
Verify handset calls and notifications on hardware.
Reconsider when AI reception is explicitly requested.
