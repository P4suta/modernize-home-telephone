# Incoming call notifications

The telephone service can publish Japanese incoming call notifications to ntfy.
Publication stays off until the configuration includes ntfy and a publisher token.
The [Raspberry Pi monitoring service](https://github.com/P4suta/raspberrypi-bootstrap/blob/main/docs/monitoring.md) already supplies a private authenticated server and the `home-status` iPhone subscription.
Use that service without changing its running configuration.

## Configuration

Add the following section to the private telephone configuration.
The server value is an HTTPS origin, including a port when required.
It excludes the topic path, credentials, query parameters, and fragments.

```toml
[notifications.ntfy]
server = "https://notify.example.ts.net:10443"
topic = "home-status"
token_env = "HOMEPHONE_NTFY_TOKEN"
```

The topic accepts 1 to 64 ASCII letters, digits, underscores, or hyphens.
Only lab configurations permit HTTP, and only with a literal loopback address.
The optional `token_env` defaults to `HOMEPHONE_NTFY_TOKEN`.
Supply the publisher token through that environment variable or a private `secrets.env` beside the configuration file.
Environment values take precedence over the secret file.
Tokens must contain 1 to 4096 visible ASCII characters without spaces.
Keep the files outside maintained sources with permissions restricted to the current account.

Validate the configuration without starting the service or sending a notification.

```sh
mise x -- cargo run --locked -p phone-server -- --config /private/path/config.toml check-config
```

The existing publisher has write access to `home-status`.
The iPhone reader has read-only access to that topic.
Reuse the subscribed topic without granting the telephone service administrative or subscription permissions.

## Published content and delivery

The [JSON publication API](https://docs.ntfy.sh/publish/#publish-as-json) receives the topic, Japanese title, caller number, original call time in Japan Standard Time, priority 4, a telephone tag, and the configured app origin.
The server sends the publisher token only through the Authorization header and refuses redirects.
The response must identify the selected topic and notification sequence within a 16 KiB body limit.
Existing numbers accept ASCII digits and an optional leading plus sign, with a 32-byte limit.
Unavailable or invalid numbers display `非通知・番号取得不可`.
No transcript, summary, recording, or diagnostic text enters the payload.

Only the original inbound `incoming` event in the offered phase can publish.
The event time must be at or after the call start and no later than publication.
The publication window includes the first 120 seconds after the original call start.
Outbound, ringing, unknown, expired, and future events publish nothing.
The window limits new publication after an outage, while ntfy can still deliver an already cached message later.

The durable outbox retries failed publication with exponential backoff and at most eight attempts.
SQLite records successful ntfy publication per call and successful webhook and browser push processing per event.
The worker attempts webhook and browser push processing even when ntfy fails.
Successful checkpoints survive retries and restarts.
Notification checkpoints follow call deletion and history retention.
Requests time out after five seconds and run outside call processing.

An uncertain retry keeps the same [ntfy sequence](https://docs.ntfy.sh/publish/#updating-notifications).
Supported clients replace that notification, while the ntfy server still stores separate messages.
A lost HTTP acknowledgement can still cause duplicate delivery.
The private service uses the [iOS wake-up relay](https://docs.ntfy.sh/config/#ios-instant-notifications), and the app retrieves content through Tailscale.
An unavailable Pi, home connection, or tailnet can prevent delivery.

## Verification

The core tests and Kani harness cover event selection and the full integer domain of the publication window.
Native HTTP and SQLite tests cover private payloads, authentication, invalid responses, independent retries, restart persistence, checkpoint failures, deletion, and expiry.
The maintained `just check`, `just security`, and `just release-check` commands include the affected contracts.
Automated tests use loopback fixtures.
Tests never contact a real notification server.
Actual caller identification and locked-screen receipt require separate observations on the telephone installation and phone.
