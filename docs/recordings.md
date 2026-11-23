# Record calls on a NAS

Explicit call recording captures answered inbound and outbound handset calls while AI reception remains off.
The NAS plugin uses encrypted Server Message Block (SMB) 3 sessions and requires an existing writable share.
The app doesn't mount the share over its local data directory.
Ordinary calls continue when NAS credentials, network access, or storage become unavailable.

## Configure capture and storage

Use absolute installation paths and a dedicated NAS directory.
Keep the archive identity stable when restoring an installation.
The example addresses and credentials are documentation placeholders.

```toml
[ai]
provider = "disabled"

[storage]
retention_days = 0
recording_limit_bytes = 536870912

[recording]
enabled = true
max_seconds = 7200
gateway_spool_dir = "/srv/telephone/spool"

[recording.nas]
server = "nas.example.ts.net"
share = "home"
directory = "telephone-recordings"
archive_id = "36b82de3-b456-44b3-bd31-d7c2fe71f460"
port = 445
username_env = "HOMEPHONE_NAS_USERNAME"
password_env = "HOMEPHONE_NAS_PASSWORD"
```

NAS recording requires an absolute `data_dir`, because the SMB adapter accepts only absolute local paths.
Installations write an absolute data directory, and the configuration checker refuses a relative one when `recording.nas` is present.

Provide literal credential assignments in the installation's private `config/secrets.env` file.
Keep its existing telephone and notification credentials.
The adapter accepts nonblank usernames of 1 through 128 ASCII bytes containing letters, digits, spaces, underscores, dots, hyphens, or backslashes.
Credential staging and SMB execution share the validator and preserve spaces as part of one argument.
Passwords travel through the SMB client's standard input rather than process arguments.
Passwords must contain 1 through 127 bytes, because the pinned Samba client reads at most 127 bytes from its password descriptor.
The [Samba client documentation](https://www.samba.org/samba/docs/current/man-html/smbclient.1.html) defines password descriptors and encrypted client protection.

The recording limit accepts 1 through 7200 seconds.
Reaching the limit ends recording while the conversation continues.
Rusterisk captures mixed call audio with a duration limit through the [Asterisk REST Interface bridge recording operation](https://docs.asterisk.org/Latest_API/API_Documentation/Asterisk_REST_Interface/Bridges_REST_API/#record).
Capture validates mono, 8000 Hz, 16-bit pulse-code modulation audio in Waveform Audio File Format (WAV) before publication.
Two hours require approximately 115.2 MB before the WAV header.

## Keep recordings until manual deletion

NAS storage requires `retention_days = 0`.
The settings screen displays manual retention and uses the quota for temporary local storage.
Labeled numeric controls explain the accepted limits, following the [W3C form instruction guidance](https://www.w3.org/WAI/tutorials/forms/instructions/).
Keyboard-operated [HTML popovers](https://html.spec.whatwg.org/multipage/popover.html#the-popover-attribute) provide capacity details.
The local quota never removes NAS recordings or their history.
The NAS's available capacity determines the archive capacity.

Before starting a recording, the app reserves space for the gateway recording, capture, transfer verification, and playback, plus completed recordings awaiting capture.
The calculation checks the smaller free space of the data and gateway spool file systems and the configured local quota with overflow refused.
The example's 512 MiB quota accommodates four copies at the duration limit.
When the reservation can't fit, a new call proceeds without recording.
Existing queued audio remains available for later transfer.

The background worker uploads a temporary object, renames it, and downloads the published object through a separate SMB session.
It verifies the byte count and SHA-256 digest before committing archive ownership and removing the local source.
An outage, rejected write, changed object, or failed database commit retains the local recording.
Durable retry state survives app restart.
The history screen announces archive retries through a [live status region](https://www.w3.org/WAI/WCAG22/Understanding/status-messages.html).
Retries use a bounded exponential delay of 5 through 320 seconds.

## Play, delete, and recover

History playback uses the authenticated recording endpoint and supports HTTP byte ranges for seeking.
Overlapping range requests for the same recording share one verified temporary file.
Only one distinct archived recording can occupy the playback slot at a time.
NAS unavailability produces a playback error that permits a retry.
Playback bounds the download by the byte count recorded at upload, so a later shorter recording limit keeps earlier recordings playable.
The app removes temporary playback audio after the last response releases it.

Deleting a completed call removes its history and queues a durable remote deletion.
Deletion completes after NAS access returns, and a late upload commit can't undo the deletion.
Deleting from the NAS outside the app leaves history without playable audio.

Installation backups preserve archive receipts and private configuration, but they don't copy NAS audio.
Back up the NAS audio and preserve `archive_id` when restoring the app.
A call deleted between the two backups leaves a receipt in the app backup without audio in a later NAS backup.
Take the NAS backup immediately after the app backup without deleting calls in between, or keep NAS snapshot versions that retain every object an app backup references.
Changing the server, share, directory, or archive identity requires migrating the existing objects before using the destination.
Recordings start with newly answered calls after recording becomes active.
The app records calls routed through its Session Initiation Protocol (SIP) path.
Analog fallback calls bypass recording.

The [assurance contracts](assurance.md) describe the isolated SMB fault cases and the verified boundary.

## Stage credentials and verify storage

The app executable can merge NAS credentials from the configured environment variables into a fresh private file.
It preserves existing telephone secrets, refuses an existing output path, and checks that NAS secret names remain independent.
Run credential staging on a trusted administration machine with the private installation configuration and its existing `secrets.env` file.

```sh
phone-server --config ./private/homephone.toml export-nas-credentials --output ./private/with-nas.env
```

The runtime image includes the SMB client and the bounded process launcher, and the host package provides neither `phone-server` nor `smbclient`.
Run the storage check inside the installation's app container, which mounts the configuration and credentials at their host paths.
Stage the configuration and credentials in the installation's `config` directory before running it, and keep the installation's recording configuration disabled until the check passes.

```sh
podman exec homephone-INSTANCE-app phone-server --config /srv/telephone/config/homephone.toml check-recording-storage
```

Replace `INSTANCE` with the installation identity without hyphens, and use `docker exec` for a Docker installation.

The storage check creates a short synthetic audio file, verifies remote readback, and removes its dedicated probe object.
It reports a cleanup failure as an error.
The operation opens neither the call database nor the telephone listener.
