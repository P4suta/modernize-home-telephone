# Documentation

The software is experimental and has no published binary release series.
The mock lab exercises software behavior without certifying an HT813, BL3000HM, carrier connection, or physical Raspberry Pi installation.
Start with the [contribution guide](../CONTRIBUTING.md) for prerequisites and verification commands.

Use the [local lab](local-lab.md) to exercise calls without an analog line or paid provider.
Use the [Raspberry Pi operations guide](raspberry-pi.md) to create an installation, supervise boot, and preserve or recover its data.
Use the [gateway console guide](ht813-console.md) for Japanese device administration and the complete parameter reference.
The [optional plugin guide](plugins.md) explains builds and operation with independently selected features.
The [notification guide](notifications.md) describes private incoming-call publication.
The [recording guide](recordings.md) describes handset recording, NAS storage, and manual retention.
The [hardware acceptance procedure](hardware-acceptance.md) separates documented HT813 capabilities from the measurements still needed on the BL3000HM connection.

The [assurance contracts](assurance.md) define the proof domains, fault injection, mutation campaigns, and source-bound result gates.
The [call policy decision](adr/keep-call-policy-pure-and-own-effects-at-the-boundaries.md) defines the app boundary.
The [gateway decision](adr/run-the-telephone-gateway-with-rusterisk.md) describes Rusterisk deployment and rollback.
The [recording durability decision](adr/capture-recordings-before-cloud-inference.md) records why local capture has an independent worker.
The [source distribution decision](adr/distribute-authored-sources-without-vendor-originals.md) keeps manufacturer originals outside public artifacts.
The [native verification decision](adr/verify-runtime-and-proofs-on-native-runners.md) assigns runtime and proof checks to their supported processor architectures.
The [verification ownership decision](adr/restore-verification-output-ownership.md) lets the caller read private reports after container execution.

The [HT813 source catalog](../.agents/skills/grandstream-ht813/references/source-catalog.md) locates the preserved manuals and parameter templates.
The [OpenAPI contract](../api/openapi.json) describes the implemented HTTP endpoints.

The [security policy](../SECURITY.md) describes private reporting and installation boundaries.
The [third-party notices](../THIRD_PARTY_NOTICES.md) distinguish authored code from upstream materials.
The [publication guide](publication.md) describes the history-free public source candidate and hosted prerequisites.
