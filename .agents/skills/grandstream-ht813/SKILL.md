---
name: grandstream-ht813
description: >-
  Configure, provision, and troubleshoot Grandstream HT813 gateways, including analog wiring and call routing.
  Use for HT813 operation rather than AI telephony app development.
---

# Operate the `HT813` gateway

Operate the HT813 as the analog interface for the home telephone project.
Keep app development separate from gateway configuration.
Connect the gateway to the project's Session Initiation Protocol (SIP) endpoint.

## Establish the installation

Inspect available device status and configuration before requesting missing facts.
Establish the HT813 firmware, home gateway and telephone models, SIP destination, and desired handset ringing behavior.
Distinguish a registered SIP account from a direct SIP peer before selecting settings.
Treat the BL3000HM combination as awaiting hardware acceptance until call tests have recorded results.

## Select the procedure

Read [setup and routing](references/setup-and-routing.md) for wiring, management access, SIP registration, direct peering, caller identification, and handset fallback.
Read [provisioning and troubleshooting](references/provisioning-and-troubleshooting.md) for firmware checks, configuration backups, XML provisioning, and call failures.
Load the reference relevant to the requested operation.
Use the project's [hardware acceptance procedure](../../../docs/hardware-acceptance.md) for measured BL3000HM integration and the [Pi operations guide](../../../docs/raspberry-pi.md) for the administration executable.

Use the [preserved source catalog](references/source-catalog.md) to locate full manuals, release notes, and parameter templates.
When a private archive exists, read the local source before following a publisher URL.
Public source distributions keep publisher links and provenance without redistributing manufacturer originals.
Match the saved edition to the device and use the publisher URL when the operation needs newer information.
Keep saved source bytes intact and record interpretations in the authored references.

## Apply and verify settings

Use the observed Web UI or the documented XML provisioning mechanism.
Check the HT813 documentation and firmware template before generating P values or relying on a setting's default.
Don't invent HTTP endpoints or copy another HT8xx model's parameters.
Keep device addresses, MAC addresses, credentials, backups, and captured call content outside the skill.

Record the current settings, proposed changes, expected behavior, and recovery path before applying a configuration change.
Use a small configuration change to isolate its effect.
After applying the change, confirm effective settings and run the affected acceptance checks.
Separate documented capabilities, installation observations, and untested assumptions in the result.
Report remaining hardware checks without claiming that documentation review established interoperability.
