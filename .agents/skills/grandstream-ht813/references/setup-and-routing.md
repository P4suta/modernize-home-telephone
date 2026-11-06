# Setup and routing

## Source context

This reference uses the official source snapshots recorded in the [local catalog](source-catalog.md).
Grandstream updated its installation guide on 2025-01-03 and its User Guide and Administration Guide on 2025-06-23.
The guides include settings introduced by different firmware releases.
Match version-specific settings to the installed release.
Consult [firmware checks](provisioning-and-troubleshooting.md#firmware-and-parameter-evidence) before diagnosing Japanese caller identification.

## Wiring and management access

Connect the BL3000HM telephone port to the HT813 `FXO` port using the existing telephone cable or wall extension.
Connect the analog telephone to the `FXS` port.
Connect the `WAN` port to the private home network.
Use the unit's specified power adapter.
The HT813 `WAN` connection uses the Dynamic Host Configuration Protocol (DHCP) as a client.
Its `LAN` port provides a different installation path with a DHCP server.
Grandstream explains these port roles in the [Quick Installation Guide, Ports Description](https://documentation.grandstream.com/knowledge-base/ht813-quick-installation-guide/).

Keep the original direct connection recoverable by reconnecting the telephone to the BL3000HM cable.
Don't split that telephone port between the HT813 and another telephone.
The manufacturer's [BL3000HM service restrictions](https://www.aterm.jp/function/bl3000hm/guide/phn_hikarir_estrictions.html) prohibit branched telephone connections and explain that the port emulates an analog line.
The same guide warns that some communications equipment may be incompatible.
The BL3000HM requires power to provide telephone service.
HT813 fallback still depends on the upstream gateway's power.

Get the assigned address from the router's DHCP leases or the telephone attached to `FXS`.
Use the interactive voice response menu on that telephone to hear the address by dialing `***` and then `02`.
The [Quick Installation Guide, `Configure via IVR Menu`](https://documentation.grandstream.com/knowledge-base/ht813-quick-installation-guide/) describes this operation.
Open the observed address from a computer on the same private network.
Use the administrator credentials supplied for the unit, change the initial password, and record its firmware and port status.
Save the initial configuration using the [backup procedure](provisioning-and-troubleshooting.md#backup-and-recovery).

If management access fails, inspect the actual interface and access policy before changing the network layout.
The [Administration Guide, Basic Settings](https://documentation.grandstream.com/knowledge-base/ht813-administration-guide/) documents `WAN Side Web/SSH Access`.
Its `Auto` setting permits access from private addresses.
Keep management access within the private network when choosing an access policy.

## Registered connection

Use the Session Initiation Protocol (SIP) service's actual server address and account credentials for the `FXO` account.
Configure the `FXS` account when the telephone also needs SIP service.
Grandstream's [User Guide, Register a SIP Account](https://documentation.grandstream.com/knowledge-base/ht813-user-guide/) identifies the account fields and the registration status under `Status` and `Port Status`.
Verify registration before testing calls, then check audio in both directions.

Route incoming analog calls to the receiving extension or inbound route using `Unconditional Call Forward to VoIP` under `Basic Settings`.
Match its destination to the receiving SIP system instead of copying example extension numbers.
Grandstream's [Connecting UCM6XXX with HT813, Method 1](https://documentation.grandstream.com/knowledge-base/connecting-ucm6xxxx-with-ht813/) demonstrates the registered connection and forwarding destination.
Use its HT813 behavior as a reference without requiring that product or adding a telephone exchange to the project.

## Direct peer

For an existing SIP endpoint that supports direct peering, use Grandstream's [Peering IP Phone with HT813, HT813 Configuration](https://documentation.grandstream.com/knowledge-base/peering-ip-phone-with-ht813/).
The example disables registration, permits outgoing calls without registration, and uses one-stage dialing.
Its default `FXO` SIP port is `5062`.
Confirm the effective port on the unit.
Replace the example addresses and identities with the actual peer's values.
Keep both endpoints reachable on the private network and match their ports.
Keep the existing ringing settings until you verify the desired behavior and caller identification.
The example excludes two-stage dialing for direct peering.
For a peer without registration, test its fallback behavior.

## Ringing and Japanese caller identification

Decide whether incoming analog calls should ring the telephone attached to `FXS` or go to the SIP destination for screening.
Use `PSTN Ring Thru FXS` for handset ringing and `Number of Rings` for forwarding timing.
The [HT813 interconnection guide, Channel Dialing](https://documentation.grandstream.com/knowledge-base/connecting-ucm6xxxx-with-ht813/) shows why caller identification can require more ringing time.
Validate number delivery before shortening the answer delay.

The [Administration Guide, `FXS` and `FXO` Port Settings](https://documentation.grandstream.com/knowledge-base/ht813-administration-guide/) includes `NTT JAPAN` caller identification and Japanese termination options.
Verify the `FXO` settings for receiving the number and the `FXS` settings for presenting it to the telephone.
Confirm what the upstream service actually delivers before selecting a scheme.
Check number decoding on `FXO` before investigating transmission to the SIP endpoint.
Don't treat the presence of a Japanese option as a tested BL3000HM preset.

## Handset fallback

The [User Guide, `PSTN Pass Through`](https://documentation.grandstream.com/knowledge-base/ht813-user-guide/) describes receiving analog calls on `FXS` and using the default `*00` access code for analog line dialing.
Confirm the configured access code before relying on it.
The [Administration Guide, Advanced Settings](https://documentation.grandstream.com/knowledge-base/ht813-administration-guide/) describes `Life Line Mode` with `Auto`, `Always Connected`, and `Always Disconnected` choices.
In `Auto` mode, the documented fallback covers HT813 power loss or loss of SIP registration.
Verify the selected mode and both failure cases while keeping the BL3000HM powered.
Distinguish a stopped app, a failed SIP server, and detected registration loss.

## Hardware acceptance

Use an ordinary test call to the home number and record whether `FXS` rings and the intended SIP destination receives it.
Check the displayed caller number and intelligible audio in both directions.
End calls from each side in separate trials, confirm that `FXO` returns to idle, and place another call.
Test the intended handset dialing path by calling an ordinary test destination.

While idle, test the configured behavior after SIP service loss and after removing power from the HT813 alone.
Record the observed transition instead of assuming automatic fallback from the setting name.
Restore the normal configuration and confirm another call.
Finally, verify that reconnecting the original telephone directly restores the original calling path.
