# Settings workflow contract

The private workspace package separates settings review from rendering and transport.
Its core has no runtime dependencies.
The Svelte adapter uses [reactive modules](https://svelte.dev/docs/svelte/svelte-js-files).
The console keeps device validation and HTTP effects in its adapters.

`SettingsDraft.capture` validates unknown input, rejects duplicate bindings, and copies the baseline.
Each field carries an opaque identity supplied by its consumer.
Include meaning, constraints, permissions, and documentation in that identity.
The console adapter includes those properties and excludes captured values.
Public edits preserve ordered selections.
An unchanged secret is absent from the outgoing changes.
A secret replacement must contain one nonempty string.
Draft values can be temporarily empty.
An empty secret replacement fails review.
Clearing a secret requires a separate device contract.

`review` requires a changed draft and the current revision.
Review items contain public before-and-after values or a secret marker.
`submit` rejects a different draft instance or revision.
The returned submission exposes the intentional outgoing values to the transport adapter.
Treat that payload as sensitive data.
The library neither persists nor logs it.

`rebase` requires an explicit consumer action.
It preserves edits when each edited binding retains its identity, access, and value kind.
Changed or removed bindings require manual review.
`reconcile` compares submitted public values with a fresh readback supplied by the adapter.
Secrets remain unverified even when public values match.
A comparison establishes matching values, not successful device activation.

The Svelte adapter permits one submission at a time.
An interrupted or unverified submission blocks another review.
Call `interrupt` when transport fails and `reset` after an explicit successful refresh.
`reset` performs no network operation and supplies no evidence of secret verification.
Keep server validation and revision checks authoritative.

The [JSON Schema annotations](https://json-schema.org/understanding-json-schema/reference/annotations) distinguish defaults from value insertion and describe write-only values.
Form renderers can supply controls through their existing extension points.
Use the [documented extension point](https://x0k.dev/svelte-jsonschema-form/guides/custom-components/) to replace form components.
The package supplies draft and submission semantics rather than a replacement form renderer.

The console and a synthetic service-configuration consumer exercise the built exports.
Synthetic reuse establishes portability, not independent demand.
The package remains private and unversioned.
The build checks declarations and exports with `svelte-package` and `publint --pack false --strict`.
Registry packing and publication require a separate release decision.
