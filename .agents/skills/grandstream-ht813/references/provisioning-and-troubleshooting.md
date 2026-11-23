# Provisioning and troubleshooting

## Source context

This reference uses the official source snapshots recorded in the [local catalog](source-catalog.md).
Grandstream updated the HT813 Administration Guide on 2025-06-23.
Grandstream updated the general device provisioning guide on 2026-03-12.
That guide includes products outside the HT813 family.
Prefer the HT813 release notes and template for model-specific behavior.

## Firmware and parameter evidence

The [official firmware index, HT813 entry](https://www.grandstream.com/support/firmware) lists general firmware `1.0.19.6` as of the check date.
Compare the installed version with the recorded HT813 entry.
For an upgrade, verify the publisher's current listing through the URL in the source manifest.
The [HT813 release notes, firmware 1.0.19.6, pages 5 and 6](https://firmware.grandstream.com/Release_Note_HT813_1.0.19.6.pdf) date that release to 2025-05-15.
They document a fix for premature call termination with the `NTT JAPAN` caller identification scheme and a change to request the XML configuration first.
Check an older unit's firmware before attributing a Japanese caller identification failure to its wiring.

Read the saved [HT813 parameter template](https://www.grandstream.com/hubfs/Support%20Tools/config-template-10212026.zip) and [change log](https://www.grandstream.com/hubfs/Support%20Tools/config-template-10212026.zip).
The archive member name identifies 1.0.19.6, but the template header identifies 1.0.17.3.
Use the [catalog's version notes](source-catalog.md#routing-and-provisioning) when resolving that discrepancy.
Grandstream's saved [Tools, Configuration Template](https://www.grandstream.com/support/tools) page records the download source.
Check the model and firmware applicability inside the template rather than inferring them from the archive's update date.
For every generated P value, keep the field's template definition and intended value with the working configuration evidence.
If the required mapping is unavailable, use the documented Web UI field and leave XML generation for that field unresolved.
Back up the unit and review the release's upgrade restrictions before changing firmware.

## Backup and recovery

Before a change, save the current configuration and record the fields you plan to change.
The [Administration Guide, configuration download and backup controls](https://documentation.grandstream.com/knowledge-base/ht813-administration-guide/) distinguishes readable text or XML exports from an encrypted backup.
Readable exports omit passwords.
Keep the credentials needed for recovery with the private backup records.
The backup export has a separate restore control.
Keep the matching firmware record, backup, and credentials in private machine-local storage.

To roll back a settings change, restore the recorded field values or use the documented backup restore operation appropriate to the saved file.
Verify status and a call after recovery.
If you lose management access, reconnect the telephone to BL3000HM to restore the original physical path, then diagnose HT813.
Review the documented reset procedure and the saved recovery inputs before a factory reset.
Don't describe an untested backup as proven recovery.

## Configuration delivery

Grandstream's [device provisioning guide, Provisioning Flow](https://documentation.grandstream.com/knowledge-base/sip-device-provisioning-guide/) describes configuration delivery over HTTP or HTTPS and device-specific XML files.
Use HTTPS with certificate validation when configuring a provisioning server.
Create a device-specific configuration containing only the intended changes.
Keep its secrets and device identity outside the skill and source-controlled documentation.

For HT813, form `cfgMAC.xml` using the lowercase MAC address of the unit's `LAN` port in place of `MAC`.
The [Administration Guide, Configuration through a Central Server](https://documentation.grandstream.com/knowledge-base/ht813-administration-guide/) identifies that MAC and the model-specific configuration names.
Check which configuration files the unit processes before assuming that downloaded files merge.
Check automatic provisioning inputs before rebooting a unit whose settings revert.
Keep firmware delivery separate from the requested configuration change.

Serve the configuration through the configured server path, apply it through the documented provisioning or upload mechanism, and inspect the resulting settings.
Use the server's request log to confirm the filename actually fetched.
Check a fresh configuration export for the intended non-secret values and test the affected call behavior.
If a setting needs a reboot, use the model guide's instruction for that setting and verify the value again afterward.

## Diagnose the failing boundary

If an incoming call never reaches the Session Initiation Protocol (SIP) endpoint, check whether the `FXO` port detected ringing.
Inspect the configured forwarding destination and the receiving route, then compare the intended registration or peer mode with the actual state.
If handset ringing differs from the intended behavior, revisit the [ringing policy](setup-and-routing.md#ringing-and-japanese-caller-identification).

If a call connects with audio in only one direction, inspect the SIP session description and media packets at both endpoints.
Identify the missing direction and check the advertised media addresses, reachable ports, negotiated codec, and network filtering.
Registration success alone is insufficient evidence for media delivery.
Change the identified cause and repeat the same call before adjusting analog gain.

If caller identification is missing, compare upstream number delivery, `FXO` decoding, answer timing, and the SIP caller information.
Separate a number missing at `FXO` from a number missing at the SIP endpoint or the telephone on the `FXS` port.
Check the [firmware evidence](#firmware-and-parameter-evidence) before changing Japanese caller identification parameters.
Keep the calling source constant when comparing ring settings, and correlate each physical attempt with its call record and notification.
With only `anonymous` in the stored caller field, intentional number withholding and failed number reception remain unresolved alternatives.
The [HT813 interconnection guide, Channel Dialing](https://documentation.grandstream.com/knowledge-base/connecting-ucm6xxxx-with-ht813/) recommends trying two or four rings when one ring causes caller identification problems.
Verify the number and ringing delay on the installed line before adopting the shorter setting.

When a fax and telephone share `FXS`, establish their connection order and the fax receive mode before interpreting an automatic answer or continued ringing.
For the `MFC-L2860DW`, Brother's [external telephone instructions](https://support.brother.co.jp/j/s/support/html/cv_mfcl2880dw_jpn/doc/html/GUID-E03086F9-E188-4CCF-980D-D6D1C619F4E3_1.html) require the `EXT.` socket and prohibit a parallel branch.
Its [receive-mode guide](https://support.brother.co.jp/j/s/support/html/cv_mfcl2880dw_jpn/doc/html/GUID-E4223EFD-5D6E-41A9-AF19-595FE8663E6E_1.html) explains automatic answering in `F/T=自動切換え` mode.
Its [repeat-ring instructions](https://support.brother.co.jp/j/s/support/html/cv_mfcl2880dw_jpn/doc/html/GUID-171F671F-E6C6-4A1D-98DF-9159DBE93F4D_1.html) state that the fax continues its own ringing for the selected count after the caller disconnects.
Require a person's confirmation of answering and audio in both directions even when SIP reports a connected call.
After a manual-mode comparison, restore the recorded receive mode unless the user chooses a different operating policy.

If `FXO` stays busy after the remote party ends the call, compare the analog line state with the SIP termination trace.
Grandstream's [HT813 interconnection guide, `FXO` Termination](https://documentation.grandstream.com/knowledge-base/connecting-ucm6xxxx-with-ht813/) requires choosing disconnect detection to match the line's signaling.
Get the provider's documented signaling method or measure the disconnect behavior before tuning current loss, tone detection, or polarity detection.
Don't copy North American tone values into a Japanese installation.
After a change, verify release from both sides and the next call.

Keep the firmware, changed fields, sanitized observations, expected behavior, actual result, and recovery result with the task's working evidence.
Mark hardware checks as pending whenever the device or test line is unavailable.
