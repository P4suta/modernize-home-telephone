---
date: 2026-11-24
---

# Name callers from a local Public Phonebook copy

## Status

Proposed.

## Context

Notifications and history show only caller numbers.
[Public Phonebook](https://www.dial.page) publishes a keyless directory archive; its search routes serve only its own site.

## Decision

Sync the verified archive on demand into a rebuildable local index.
Look up each inbound caller once at admission and store the label on the call.
Apply the Public Phonebook rule: one distinct name shows its categories, and several names show their count.
A missing, slow, or damaged index leaves the call unlabeled.

## Alternatives

A hosted lookup sends every caller number off the home network during ringing.
A manual contact list covers only numbers the household enters.

## Consequences

Labels reflect the installed snapshot, and a match names a published contact point rather than the actual caller.
The [phonebook guide](../phonebook.md) records validation, storage, and size.

## Confirmation

Native tests and Kani proofs cover normalization, labels, archive validation, replacement, and bounded lookup.
Reconsider if the archive format changes or labels mislead the household.
