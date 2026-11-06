# Project contracts

Keep telephone decisions in `phone-core` and external effects in `phone-server` or `xtask` adapters.
Use Japanese for interface text and telephone announcements, and English for source prose and documentation.
Use the pinned tools through `mise x --`.

Run `just check` for formatting, semantic lints, strict types, native tests, generated API contracts, preserved source checksums, and the fault inventory.
Run `just security` for dependency and secret checks.
Run `just lab`, `just browser`, and `just deployment` for the ARM64 Asterisk lab, real WebRTC and SIPp audio, and deployment recovery.
Automation must refuse production installations and paid AI providers.

Run the proof, Miri, fuzz, Rust mutation, and TypeScript mutation commands through the bounded verification runner.
Use `just evidence` to reject missing or stale receipts and `just release-check` for the complete local gate.
A timeout, skipped case, retry, survivor, empty campaign, or missing harness must not count as successful verification.
Keep proof inputs, assumptions, negative controls, and limitations in [the assurance contracts](docs/assurance.md).

Add a failing regression before repairing a defect.
For Rust fault tests, maintain the invariant, previous coverage gap, and executable enforcement in `verification/faults.toml`.
Review the test fixture and evidence collector as possible causes of a false result.
Keep generated files and real credentials outside authored source changes.
Preserve upstream documentation bytes and consult the [HT813 skill](.agents/skills/grandstream-ht813/SKILL.md) before device operations.

Hardware acceptance, real power loss, and paid provider calls require separate observed evidence.
Never infer BL3000HM interoperability or Raspberry Pi boot behavior from the software lab.
Create README and license files from `repository.toml` with `dotfiles repository generate`.
