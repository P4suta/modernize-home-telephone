# Exercise the local telephone lab

Install the pinned tools with `mise install` and provide a Docker engine capable of running Linux ARM64 containers with host networking.
OrbStack supplies the engine on the development Mac.
On Linux, use Docker Engine and enable ARM64 emulation when the host architecture differs.
The lab binds HTTP to loopback and uses a deterministic mock AI provider.
The Session Initiation Protocol (SIP) and Asterisk REST Interface (ARI) listeners also use loopback.
It places no calls on a public telephone network.

```sh
mise x -- just lab
```

The command initializes `.local/telephone` when absent, builds the app and Asterisk images, and starts the managed installation.
Open `http://localhost:8787` to simulate incoming calls, enable browser audio, inspect history, and replay recordings.
Grant microphone access before starting browser calls.
The mock classifies configured scenarios after one second of `PCMU` input, independently of WebSocket message partitioning.
The browser automation supplies its own audio fixture.
Install the pinned browser and its operating system dependencies before the first browser run.

```sh
mise x -- pnpm --dir web exec playwright install --with-deps chromium
```

```sh
mise x -- just check
mise x -- just security
mise x -- just browser
mise x -- just deployment
```

The browser command starts a bounded SIPp responder using the User Datagram Protocol (UDP) on loopback port 5070.
The responder echoes Real-time Transport Protocol (RTP) media on port 6000.
The browser suite verifies outgoing calls, incoming takeover, hold, resume, read aloud, spam handling, recording, accessibility, and narrow layout.
SIPp must report exactly one completed dialog and no failed or active call.
The command removes only its uniquely named responder container.

Deployment rehearsal requires an idle mock installation built from the current sources.
It creates a sample recording, replaces the running service, rejects an invalid candidate, recovers an interrupted update, and boots a restored installation with a different identity.
The rehearsal compares recording hashes and returns service to the original lab installation.
Artifacts remain under `.cache/deployment` for inspection.

```sh
mise x -- cargo xtask doctor --directory .local/telephone
mise x -- cargo xtask stop --directory .local/telephone
```

Rebuild the lab after a source change before running browser or deployment verification.
The gate rejects images associated with a different source identity.
Run `boot` to recover an interrupted administrative operation before backup or replacement.
Use [the assurance contracts](assurance.md) for the heavier checks and the meaning of each verdict.
