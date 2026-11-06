# Preserved source documents

## Local reference policy

Read the working procedures first, then inspect the saved source for the relevant setting.
The [source manifest](sources.json) records publisher URLs, edition markers, the retrieval interval, file sizes, and checksums.
Private originals preserve their publisher notices and source bytes.
Public source distributions contain the manifest, checksums, and authored procedures without the originals.
Publisher links identify the saved edition's source.
A current response may have changed since retrieval.
Keep interpretations and corrections in the authored Markdown files.

The saved HTML includes page markup and inline content.
External images, styles, scripts, and remote services remain outside the snapshot.
Use local text inspection for HTML and the saved PDFs for figures when their edition applies.

## Manual editions

The [Administration Guide web snapshot](https://documentation.grandstream.com/knowledge-base/ht813-administration-guide/) includes changes through firmware 1.0.19.6.
The [User Guide web snapshot](https://documentation.grandstream.com/knowledge-base/ht813-user-guide/) supplies the corresponding operation reference.
The [Administration Guide PDF](https://www.grandstream.com/hubfs/Product_Documentation/HT813_Administration_Guide.pdf) and [User Guide PDF](https://www.grandstream.com/hubfs/Product_Documentation/HT813_User_Guide.pdf) identify firmware 1.0.13.3.
Match each setting to the device's release instead of assuming that the web and PDF editions agree.
Use the [release notes](https://firmware.grandstream.com/Release_Note_HT813_1.0.19.6.pdf) for the 1.0.19.6 changes and the [installation guide](https://documentation.grandstream.com/knowledge-base/ht813-quick-installation-guide/) for wiring.

## Routing and provisioning

The [direct peer example](https://documentation.grandstream.com/knowledge-base/peering-ip-phone-with-ht813/) and [registered connection example](https://documentation.grandstream.com/knowledge-base/connecting-ucm6xxxx-with-ht813/) describe different connection modes.
Use the [device provisioning guide](https://documentation.grandstream.com/knowledge-base/sip-device-provisioning-guide/) for the general delivery mechanism.
The [home gateway restrictions](https://www.aterm.jp/function/bl3000hm/guide/phn_hikarir_estrictions.html) document the upstream telephone port.

The [configuration template](https://www.grandstream.com/hubfs/Support%20Tools/config-template-10212026.zip) comes from the publisher's archive member named for 1.0.19.6.
Its internal header identifies 1.0.17.3.
The [template change log](https://www.grandstream.com/hubfs/Support%20Tools/config-template-10212026.zip) records changes since 1.0.17.3.
Keep the discrepancy visible and confirm the required field using the change log, model guide, and device.
Use the template as a parameter reference and generate only the intended settings.
Don't upload the complete factory template to an installed unit.

## Inspect without network access

When the private archive is available, search the saved HTML from the project root with Pandoc 3.10.1 and `rg`.
The converter reads the saved file without retrieving a publisher page.

```sh
mise x pandoc@3.10.1 -- pandoc --from=html --to=plain --wrap=none .agents/skills/grandstream-ht813/references/upstream/ht813-administration-guide.html | rg -n -m 2 -F 'PSTN Ring Thru FXS'
```

For a PDF, limit extraction to the relevant pages and search the text.

```sh
pdftotext -f 2 -l 2 .agents/skills/grandstream-ht813/references/upstream/ht813-administration-guide.pdf - | rg -n -F 'Version 1.0.13.3'
```

From the project root, require the complete private archive and verify the saved bytes and qualified provisioning fields.

```sh
mise x -- cargo xtask sources
mise x -- cargo test --locked -p xtask --features vendor-archive --test provisioning
```

## Freshness and updates

The [firmware index snapshot](https://www.grandstream.com/support/firmware) and [tools snapshot](https://www.grandstream.com/support/tools) describe availability at retrieval.
Use the URLs in the manifest when an operation requires newer information or the saved edition lacks the required setting.
If the publisher site is unavailable, report the saved edition and any unresolved version question.
Don't treat a saved listing as a current release check.

When refreshing a source, save the publisher response and update its provenance and checksum together.
Check edition changes and affected procedures before replacing a saved source.
