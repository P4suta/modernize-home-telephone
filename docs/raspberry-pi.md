# Operate the telephone

## Runtime and package

The deployment target is Raspberry Pi 4 or 5 running a 64-bit Linux operating system with Docker and OpenSSL available.
Use Raspberry Pi OS Lite 64-bit for the intended installation.
Raspberry Pi documents headless network and SSH setup in its [getting started guide](https://www.raspberrypi.com/documentation/computers/getting-started.html).
Docker documents ARM64 support and installation for the underlying Debian release in its [Debian installation guide](https://docs.docker.com/engine/install/debian/).
Physical Pi boot, thermal behavior, and storage durability remain hardware acceptance work.

Build the package on a development machine with the repository and a Docker engine supporting Linux ARM64 builds.
The destination must be absent.

```sh
mise x -- just package .local/package
```

The package contains the statically linked ARM64 `homephone` administration executable, app and gateway image archives, release fingerprints, a checksum manifest, the maintained project sources, and Asterisk corresponding source.
Project code uses the root dual license.
Asterisk, Debian components, and bundled sound recordings keep their respective upstream terms.
Manufacturer originals remain outside the package.
The [third-party notices](../THIRD_PARTY_NOTICES.md) describe the source boundary.
The gateway image retains the Japanese sound license and credits under `/usr/share/licenses/asterisk-ja`.

Copy the package to the Pi and run the following commands from a directory owned by the service user.
Replace the documentation address, origin, and user with the Pi's observed local network address, private HTTPS origin, and allowed tailnet login.
The installation directory must be empty or absent.

```sh
install -m 755 package/homephone ./homephone
./homephone init --production --directory ./telephone --address 192.0.2.10 --origin https://phone.example.ts.net --owner owner@example.com
./homephone install --package ./package --directory ./telephone
```

Initialization creates private configuration, gateway and browser passwords, administration credentials, WebRTC certificate keys, and push signing keys.
Add `OPENAI_API_KEY` to `telephone/config/secrets.env` using an editor.
The file contains literal assignments that the server parses as data.
The parser rejects shell syntax and duplicate assignments.
Keep the generated credentials when changing configuration.

The HTTP listener binds to `127.0.0.1:8787`.
Production access requires HTTPS and an explicitly allowed Tailscale user, or the administration token on the loopback management path.
[Tailscale Serve](https://tailscale.com/docs/reference/tailscale-cli/serve) can provide the private HTTPS endpoint and identity headers.

```sh
tailscale serve --bg http://127.0.0.1:8787
./homephone boot --directory ./telephone
./homephone doctor --directory ./telephone
```

Serve carries HTTP and Session Initiation Protocol (SIP) signaling over WebSocket.
WebRTC audio uses Interactive Connectivity Establishment (ICE) to choose a media path.
The path must reach the Pi's Real-time Transport Protocol (RTP) range, 10000 through 10099, over the User Datagram Protocol (UDP).
The HT813 needs local network access to port 5060 for SIP over UDP and the RTP range.
The Asterisk REST Interface (ARI) remains on loopback port 8088.
Tailnet audio reachability and the local network firewall belong to installation acceptance.

## Supervise boot

Generate a systemd unit using the actual absolute executable path and service user.
The generated unit waits for Docker and the network, starts through the recovery-aware `boot` command, and stops the owned containers through `stop`.

```sh
./homephone unit --directory ./telephone --binary "$(pwd)/homephone" --user "$(id -un)" --output ./homephone.service
sudo install -m 644 ./homephone.service /etc/systemd/system/homephone.service
sudo systemctl daemon-reload
sudo systemctl enable --now homephone.service
```

The service user must already be able to use the Docker engine.
Runtime containers use the installation's user identity, read-only images, bounded memory and CPU, and bounded Docker logs.
The Pi runs prebuilt images and doesn't need the Rust or Node.js toolchain.

## Preserve and recover data

Backups contain credentials, call metadata, recordings, and gateway spool files.
Store backups as private artifacts.
The manager refuses backup during an active call and refuses to overwrite an existing destination.
It quiesces new calls, creates a consistent SQLite snapshot, verifies the file inventory, and resumes after publication.

```sh
./homephone backup --directory ./telephone --output ./backups/before-update
./homephone update --directory ./telephone --package ./candidate-package --backup ./backups/update-transaction
```

Each destination name must identify an absent path.
An update preserves its own backup and journal, stops the old service only after backup succeeds, and commits a candidate only after its event consumer and gateway reconciliation are ready.
A failed candidate restores the previous installation before accepting calls.
The manager removes the journal and synchronizes its parent directory before sending a request to reopen admission.
If journal retirement fails, the service remains quiesced.

```sh
./homephone boot --directory ./telephone
./homephone restore --backup ./backups/before-update --directory ./restored-telephone
./homephone rollback --directory ./telephone --snapshot ./backups/before-update --backup ./backups/before-rollback
```

`boot` recovers an interrupted operation before reopening the line.
Backup, installation, and replacement refuse a pending recovery journal.
A fresh restore receives a new installation identity and refuses an occupied destination.
Only one installation may own the configured HTTP and SIP ports at a time.
Restoring an earlier snapshot intentionally restores its earlier call history.

The default recording policy keeps finished recordings for 30 days with a 4 GiB quota.
Capture and quota enforcement continue independently of slow transcription or summary requests.
Active and quarantined calls remain protected until gateway cleanup releases ownership.
The policy can temporarily exceed the quota while a protected recording remains active.
The [hardware acceptance procedure](hardware-acceptance.md) covers the physical telephone connection and recovery fallback.
