---
date: 2026-11-14
---

# Serve the gateway console without a Node runtime

## Status

Proposed.

## Context

HT813 firmware couples navigation to device requests.
The [Japanese console design](https://www.figma.com/design/QlUy4jmX9BFnL6NEMjK1r7/HT813?node-id=15-237) requires responsive forms and explicit write confirmation.

## Decision

Prerender a Svelte 5 and SvelteKit 3 shell with the [static adapter](https://svelte.dev/docs/kit/adapter-static) and serve it from Axum.
Use TanStack Query for cached reads and the [native modal dialog](https://html.spec.whatwg.org/multipage/interactive-elements.html#the-dialog-element) for confirmations.
Keep firmware sessions and form tokens in the server, with submission transitions in phone-core.
Preserve [save and apply semantics](https://documentation.grandstream.com/knowledge-base/ht813-administration-guide/).
Require reconciliation after each submitted write.

## Alternatives

A React console could reuse tooling but would keep the current frontend framework.
A style sheet retains slow firmware navigation.
Server rendering needs a JavaScript server.
Static rendering fits the Axum deployment.

## Consequences

Builds need the [SvelteKit-compatible toolchain](https://svelte.dev/docs/kit/migrating-to-sveltekit-3).
The evaluated Bits UI release depends on a helper that requires SvelteKit 2.
Device latency remains visible during retrieval and submission.
Refuse unsupported firmware behavior.

## Confirmation

Test authentication, forms, stale drafts, interrupted writes, keyboard navigation, and narrow viewports with synthetic fixtures.
Verify live firmware before claiming device acceptance.
Reconsider if native forms can't support a reliable adapter.
