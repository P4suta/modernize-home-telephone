# Accept the physical telephone installation

The software lab checks app behavior using Asterisk, mock AI, browser WebRTC, and SIPp.
The HT813, BL3000HM analog interface, actual telephone, Raspberry Pi boot, paid AI service, and browser push service need separate observations.
Record device models, installed firmware, configuration backup locations, call outcomes, and remaining faults in private installation records.

## Connect and identify the gateway

Use the [preserved installation guide](https://documentation.grandstream.com/knowledge-base/ht813-quick-installation-guide/) and the [setup procedure](../.agents/skills/grandstream-ht813/references/setup-and-routing.md).
Connect the BL3000HM telephone cable to HT813 `FXO`, the telephone to `FXS`, and the HT813 `WAN` port to the private network.
Keep the original direct telephone connection recoverable.
The [BL3000HM restrictions](https://www.aterm.jp/function/bl3000hm/guide/phn_hikarir_estrictions.html) prohibit branching the telephone connection.

Record the effective firmware and save the initial configuration before provisioning.
The preserved parameter template has internal version `1.0.17.3`, despite a download filename containing `1.0.19.6`.
The renderer accepts only the qualified internal version and refuses other versions.
The restriction doesn't recommend a firmware downgrade.
Another firmware requires a reviewed matching template before expanding the accepted version set.

```sh
./homephone provision --directory ./telephone --firmware 1.0.17.3 --mac 001122334455 --server 192.0.2.10 --output ./cfg001122334455.xml
```

Use the observed MAC address and Pi local network address in place of the documentation values.
The generated private XML changes a minimal set of Session Initiation Protocol (SIP) account and routing fields for `fxo` and `handset`.
It doesn't reset the gateway or select unmeasured Japanese caller identification, impedance, disconnect detection, or ring timing values.
Deliver the XML using the firmware's documented provisioning mechanism and verify the effective settings afterward.

## Exercise normal and failed calls

Check both port registrations before making an ordinary incoming test call.
Observe the caller number, AI announcement, intelligible audio in both directions, intended handset ringing, and browser takeover.
End separate calls from each side, verify that `FXO` returns to idle, and place another call.
Exercise handset and browser outgoing calls to an ordinary test destination and verify `DTMF` with a destination that reports the received digits.

Check voicemail playback, saved recording, transcript, and summary with the paid provider configured.
The default policy disconnects both spam and suspected spam, so inspect false positives during acceptance.
Status reports whether the AI configuration contains credentials.
Paid provider readiness requires an observed connection.
Observe actual Web Push delivery through the browser's push service.

While the line is idle, test app loss, SIP service loss, and HT813 power loss as separate conditions.
The documented `Life Line Mode` behavior and `*00` analog access code depend on effective settings and upstream power.
The [fallback reference](../.agents/skills/grandstream-ht813/references/setup-and-routing.md#handset-fallback) identifies the manufacturer passages.
Verify fallback independently for a stopped app and lost SIP registration.

Boot the actual Pi after a normal shutdown and verify systemd startup, readiness, a new call, and recording access.
Perform any power interruption test on a disposable installation with a preserved backup.
The software journal rehearsal doesn't simulate SD card controller behavior, storage write caches, or electrical faults.
Finally, reconnect the original telephone directly to the BL3000HM and verify the recovery path.
