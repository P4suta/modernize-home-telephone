---
date: 2026-11-14
---

# Qualify native form scripts before saving settings

## Status

Proposed.

## Context

Observed gateway forms use scripted validation, generated controls, and password fields that become editable on focus.
The current adapter refuses scripted settings, preventing configuration on the installed firmware.

## Decision

Recognize reviewed script fingerprints and validate their bounded dynamic data.
Verify external script resources during reads and write preflight.
Convert known behavior into typed submission rules without executing manufacturer scripts.
Preserve the [HTML form entry list](https://html.spec.whatwg.org/multipage/form-control-infrastructure.html#constructing-the-entry-list).
The list includes generated controls and the selected submitter.
Keep unsupported behavior read-only.
Bind derived constraints to revision comparison.

## Alternatives

A browser engine would reproduce more native behavior but add a runtime and network sandbox to the Pi.
Allowing scripts by function name would leave their effects unchecked.

## Consequences

Fingerprints establish identity, not semantics.
Source review and independent browser comparisons remain required.
Unknown script changes require another qualification.

## Confirmation

Require rejecting fixtures for unknown resources, changed scripts, stale constraints, and altered controls.
Compare submission data with browser form data using captured pages in an offline fixture.
Run the complete software gate before device writes.
Reconsider when supported behavior exceeds a small reviewed profile.
