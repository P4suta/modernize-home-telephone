# Security policy

## Report a vulnerability

Report vulnerabilities in the current default branch.
The project has no supported binary release series yet.
Use the repository's Security tab and Report a vulnerability when private vulnerability reporting is available.
GitHub describes the private channel in its [reporting guide](https://docs.github.com/en/code-security/how-tos/report-and-fix-vulnerabilities/configure-vulnerability-reporting/configure-for-a-repository).

If the private reporting channel is unavailable, open an issue requesting a private security channel without describing the vulnerability.
Keep exploit instructions, credentials, call content, telephone numbers, and affected private infrastructure out of public issues.
Include the affected commit, minimal synthetic reproducer, observed impact, and relevant redacted logs in the private report.
Investigate only installations that you own or have explicit permission to test.

## Relevant boundaries

Reports may concern authentication bypass, unexpected telephone origination, stolen line ownership, command or configuration injection, arbitrary file access, credential disclosure, recording access, or incomplete deletion and recovery.
Paid provider requests, notification endpoints, gateway signaling, and the browser audio path cross separate trust boundaries.
AI transcripts and replies remain untrusted input and must not grant administrative capabilities.

Use the mock lab for a reproducer.
Automated verification refuses production installations and paid providers.
The [assurance contracts](docs/assurance.md) state the implemented checks and their limits.
Passing software checks establishes no physical gateway or carrier behavior.

## Operate a private installation

Keep the web service behind the documented loopback and Tailscale access boundary.
Keep Session Initiation Protocol (SIP), Real-time Transport Protocol (RTP), administration interfaces, backups, and recordings off the public internet.
Backups contain credentials and call content and need private storage.
Use the [Pi operations guide](docs/raspberry-pi.md) for installation, credential storage, maintenance, and recovery.

Hardware acceptance, real storage power loss, external notification delivery, and paid AI behavior require separate observations.
The [hardware acceptance procedure](docs/hardware-acceptance.md) records the required boundaries and handset fallback.
