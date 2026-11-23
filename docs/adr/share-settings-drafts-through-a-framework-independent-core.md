---
date: 2026-11-16
---

# Share settings drafts through a framework-independent core

## Status

Proposed.

## Context

The console requires review before sending settings.
Form libraries validate inputs but don't establish device revision or readback semantics.
The current rebase comparison omits access and constraint changes.

## Decision

Move immutable drafts and explicit review transitions into a private workspace package.
Keep device validation and network effects in console adapters.
Expose a thin [Svelte reactive module](https://svelte.dev/docs/svelte/svelte-js-files) and package its declarations with [svelte-package](https://svelte.dev/docs/kit/packaging).
Bind review to the captured revision and require reconciliation after submission.
Hide secret values from review data and refuse inaccessible edits.

## Alternatives

Keeping the console module avoids a package boundary but prevents independent consumer checks.
Adopting a form library handles validation but still requires revision and readback rules.
A Svelte-only core couples rules to components.

## Consequences

The package adds build and compatibility checks.
Synthetic reuse establishes portability without establishing independent demand.
Server validation remains authoritative.

## Confirmation

Verify the console and a synthetic settings consumer using built package exports.
Test immutable inputs, changed metadata, secret edits, stale review, interrupted writes, and readback outcomes.
Run mutation tests over the extracted implementation.
Reconsider publication when an independent consumer confirms the contract.
