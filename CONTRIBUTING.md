# Contributing

## Start with the software lab

The project implements call screening, browser calling, recording, history, and recovery around an HT813 analog gateway.
Physical HT813 interoperability and Raspberry Pi startup still need the [hardware acceptance procedure](docs/hardware-acceptance.md).
Use the deterministic mock provider for development.
Normal checks need no telephone line, vendor archive, cloud credentials, or owner-specific tools.

Install [mise](https://mise.jdx.dev/installing-mise.html), then install the pinned project tools and frontend dependencies.

```sh
mise install
mise x -- pnpm --dir web install --frozen-lockfile --ignore-scripts
mise x -- just check
mise x -- just security
```

Rust compilation needs the operating system's C toolchain, `pkg-config`, and OpenSSL development headers.
On Debian or Ubuntu, install `build-essential`, `pkg-config`, and `libssl-dev`.
The [local lab guide](docs/local-lab.md) describes the Docker engine and browser requirements for real Asterisk, WebRTC, and SIPp checks.
Linux ARM64 containers require native ARM64 execution or registered emulation on another architecture.

## Change and verify behavior

Keep telephone decisions in `phone-core` and external effects in `phone-server` or `xtask`.
Add a failing behavioral regression before repairing a defect.
Document Rust fault regressions in `verification/faults.toml` with the invariant, previous coverage gap, and executable enforcement.
Inspect fixtures and evidence collectors when a test gives a misleading result.

Run `just check` and `just security` after a change.
Regenerate changed API contracts with `just api` and commit their corresponding source change.
For browser or deployment behavior, rebuild with `just lab` before `just browser` or `just deployment`.
Install the pinned Chromium binary with the following command before the first browser run.

```sh
mise x -- pnpm --dir web exec playwright install --with-deps chromium
```

The [assurance contracts](docs/assurance.md) define the proof domains, fixed fuzz campaigns, mutation scope, and evidence completeness rules.
Run affected heavy checks through the bounded project commands.
Run `just release-check` for the complete software inventory before a release.
The manual Software assurance workflow runs that same gate on its selected commit without publishing a release.
Timeouts, survivors, retries, skips, and incomplete reports never establish a pass.

Renovate checks the Debian trixie ARM64 repositories for package updates.
Docker builds use the fixed snapshot in `deploy/debian.sources`.
Verify package pin changes with the matching snapshot date through `just release-check`.

Publisher originals remain private local references.
Ordinary checks validate the checksum inventory and verify every original when the complete archive exists.
Run `mise x -- cargo xtask sources` to require all originals.
Check provisioning field names in the archived firmware template with `mise x -- cargo test --locked -p xtask --features vendor-archive --test provisioning`.
The public source distribution contains no originals.
Use the publisher URLs in the [source catalog](.agents/skills/grandstream-ht813/references/source-catalog.md).

## Submit a reviewable change

Open an issue describing the problem, intended input and result, bounded scope, and acceptance criteria.
Use Conventional Commits and keep a pull request focused on the issue.
Explain the behavior, relevant verification, and remaining boundaries in the pull request template.
Architecture choices belong in concise records under `docs/adr`.
Write documentation and source prose in English, and interface text and telephone announcements in Japanese.

Use synthetic data in issues, tests, and logs.
Keep generated installation directories, tokens, MAC addresses, caller identities, recordings, and private network configuration out of public changes.
Follow [SECURITY.md](SECURITY.md) for vulnerabilities and [CODE_OF_CONDUCT.md](CODE_OF_CONDUCT.md) for participation.
Contributions use `MIT OR Apache-2.0` unless the contributor states separate terms.
Keep third-party notices and license boundaries intact.
