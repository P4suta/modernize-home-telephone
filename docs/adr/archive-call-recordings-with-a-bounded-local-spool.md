---
date: 2026-11-15
---

# Archive call recordings with a bounded local spool

## Status

Proposed.

## Context

Handset calls need recordings with AI off.
The Raspberry Pi has finite storage.
The NAS provides Server Message Block (SMB).

## Decision

Reserve local capacity before [bounded capture](https://docs.asterisk.org/Latest_API/API_Documentation/Asterisk_REST_Interface/Bridges_REST_API/#record), independently of AI.
Keep decisions in `phone-core` and calls available when recording fails.
Use an optional Rust plugin around [Samba's client](https://www.samba.org/samba/docs/current/man-html/smbclient.1.html).
Verify published audio through a separate SMB session before removing local files.
Persist transfer and deletion intents in SQLite.
Preserve a separate archive identity across installation restore.
Keep NAS audio and history until manual deletion.
Bound playback temporary storage and preserve authenticated range requests.

## Alternatives

Direct SMB mounts couple capture to remote stalls.
Local retention exhausts Pi storage.
Object storage requires another NAS service.

## Consequences

Outages suspend new recordings when local capacity fills.
Verification adds network reads.
Local metadata backups exclude archived audio.

## Confirmation

Test capacity, corrupt copies, interruption, deletion recovery, and playback.
Run the release gate and deployment rehearsal before deployment.
Confirm playable handset audio on the NAS.
Reconsider when SMB can't meet bounded interruption requirements.
