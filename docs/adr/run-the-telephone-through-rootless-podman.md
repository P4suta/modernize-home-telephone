---
date: 2026-11-14
---

# Run the telephone through rootless Podman

## Status

Proposed.

## Context

The target Pi runs existing monitoring services through rootless Podman 5.4.2.
The telephone manager invokes Docker and requires its system service.

## Decision

Select Docker or Podman explicitly at the process boundary.
For Podman, preserve the service account with [`keep-id`](https://docs.podman.io/en/v5.4.2/markdown/podman-run.1.html#userns-mode) and supervise recovery through a systemd user unit.
Package the console binary and static assets in the app image.
Start the optional console from its own private configuration.
Keep image verification, installation identity, backups, and recovery admission checks.

## Alternatives

Installing Docker would preserve the current manager but introduce a second runtime and privileged daemon.
Independent Quadlet definitions would fit the host but duplicate the manager's recovery lifecycle.

## Consequences

Podman needs its own log options and user namespace mapping.
Select the runtime for each administration command.
Existing monitoring containers remain independent.

## Confirmation

Require a Debian ARM64 rootless rehearsal covering image identity, private files, startup, restart, and restoration.
Verify private access and actual calls on the Pi.
Reconsider if runtime differences prevent the checked recovery behavior.
