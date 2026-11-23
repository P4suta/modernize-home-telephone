# Gateway administration console

The console provides Japanese pages for device status, basic settings, advanced settings, telephone ports, and telephone line ports.
The interface follows the [approved Figma design](https://www.figma.com/design/QlUy4jmX9BFnL6NEMjK1r7/HT813).
SvelteKit prerenders the interface, and Axum serves the resulting static assets.
Production needs no Node runtime.
Fonts follow the operating system.

## Configuration and authentication

Build the interface with `mise x -- pnpm --dir console build`.
Validate a configuration with `mise x -- cargo run --locked -p phone-server --bin ht813-console -- check-config --config <path>`.
The configuration has the following shape.

```toml
listen = "127.0.0.1:8790"
public_origin = "http://127.0.0.1:8790"
device_origin = "http://192.0.2.13"
assets = "console/dist"
```

The example device address belongs to the documentation range.
Keep actual addresses and configuration outside maintained source.
The listener must use loopback.
Remote origins require HTTPS and a Tailscale identity supplied by Serve.
The tailnet policy authorizes access to the management port without an app user list.
[Serve removes client-supplied identity headers](https://tailscale.com/docs/features/tailscale-serve#identity-headers) before supplying `Tailscale-User-Login`.
The HT813 requires its device credentials.

Enter the device password in the login form.
Each console session has its own native cookie jar.
The server retains credentials in memory and never sends native tokens or existing secret values to the browser.
A browser session uses an HttpOnly, SameSite=Strict cookie, with Secure enabled for HTTPS.
The server refuses sessions after 24 hours without API activity and admits at most eight active sessions.
No browser storage retains draft passwords.

## Reading and saving

Cached pages and drafts remain available during navigation.
An explicit refresh reads the device again.
Read authentication can recover once after native session expiry.
Requests don't automatically retry failed writes.

A review token binds the browser session, page, and revision.
Before submission, the server obtains a fresh native form and compares its private control state with the reviewed baseline.
The adapter preserves hidden values, native choices, `disabled` controls, and the selected Update or Apply action.
Any submitted write requires readback before another submission.

Readback compares the native parameter identity, control kind, and public values.
A secret change still requires independent authentication or call verification.
A combined public and secret edit remains uncertain when any public value fails readback.
An HTTP response and matching stored values establish neither runtime activation nor successful telephony.

The qualified HT813 1.0.19.6 profile checks three external resource hashes and normalized inline fingerprints.
Only decimal peer ports, generated option values, and explicit reset-role flags may vary inside reviewed script patterns.
The adapter materializes generated fields, validates Session Initiation Protocol (SIP) port separation and Transport Layer Security (TLS) version ordering, and enables reviewed password focus behavior.
Changed web passwords must fit the device's Latin-1 login encoding.
Single-select labels may share one value only when their `disabled` states agree.
The [HTML form entry-list algorithm](https://html.spec.whatwg.org/multipage/form-control-infrastructure.html#constructing-the-entry-list) determines submission order, including a submitter before trailing hidden controls.
The [read-only input rule](https://html.spec.whatwg.org/multipage/input.html#the-readonly-attribute) preserves an unchanged password while explicit edits use the reviewed focus behavior.
Unsupported JavaScript transformations prevent writes.
Unverified form metadata permits a read-only view when parsing remains safe.
The console hides unknown parameters and refuses edits.
Secret values in unexpected choice controls remain private.
The adapter never executes manufacturer HTML in the browser.

Native status extraction preserves the [relationship between column headers and data](https://www.w3.org/WAI/tutorials/tables/two-headers/).
The parser separates SIP registration, hook state, and the native `DND` column across distinct tables and spacer columns.

## Command-line operations

The interactive CLI follows the [terminal design](https://www.figma.com/design/QlUy4jmX9BFnL6NEMjK1r7/HT813?node-id=57-1569).
Readable results, progress, and recovery actions follow the [Command Line Interface Guidelines](https://clig.dev/#output).
The terminal supplies the font and foreground.
Headings, spacing, and explicit state markers provide hierarchy, and results remain in normal terminal scrollback.
The [Rust terminal detector](https://doc.rust-lang.org/std/io/trait.IsTerminal.html) selects human output for terminal standard output and JSON for redirected output.
Use `--human` to select readable output or `--json` to request structured output explicitly.
The CLI suppresses emphasis when a nonempty `NO_COLOR` variable is present, extending the [NO_COLOR convention](https://no-color.org/) to its bold styling.
Use `--no-progress` to suppress progress animation.
`TERM=dumb` also suppresses animation and emphasis.

The CLI uses the console API with its own authenticated session.
It never reads browser cookies or supplies tailnet identity headers.
Use the actual HTTPS console origin.
The CLI admits plain HTTP only for loopback development fixtures.
The client uses no redirects, system proxies, or automatic retries.
The CLI supports macOS and Linux private file permissions.

Build with `mise x -- cargo build --locked -p phone-server --bin ht813-console`.
Create a private working directory before login.
The default prompt hides the administrator password.
The optional `--password-file` reads one private line, while `--password-stdin` admits a nonterminal pipe from a secret manager.
No password argument is available.
The qualified firmware's login and administrator password controls limit input to 30 characters.
The manufacturer's [password change procedure](https://documentation.grandstream.com/knowledge-base/ht813-administration-guide/#changing-admin-level-password) specifies 1 to 30 characters, while a separate general note says 25.
Native validation follows the observed control.
Session, plan, change, and secret files must belong to the current account and deny group and other access.
The reader refuses symbolic links and nonregular files.

```sh
mkdir -m 700 private-console
mise x -- ./target/debug/ht813-console login --origin https://gateway.example.invalid --session private-console/session.json
mise x -- ./target/debug/ht813-console get --session private-console/session.json --page fxs --parameter P47
mise x -- ./target/debug/ht813-console docs --session private-console/session.json --parameter P47
```

Create a change document with permissions 0600.
Public values use native parameter identifiers and arrays of strings.
Secret controls require absolute private file paths.
The example uses a documentation address.

```toml
[values]
P47 = ["192.0.2.10"]

[secrets]
P34 = "/absolute/private/gateway-password"
```

Planning reads the page and invokes server validation without submitting the native form.
The saved plan captures public values and binds resolved secret values without retaining plaintext secrets.
The review identifies secret edits without displaying their contents.
The plan binds the session, origin, page, revision, operation, and inputs.
The server still fetches and qualifies the native form before submission.

```sh
mise x -- ./target/debug/ht813-console plan --session private-console/session.json --page fxs --changes private-console/changes.toml --mode apply --output private-console/fxs-plan.json
mise x -- ./target/debug/ht813-console apply --session private-console/session.json --plan private-console/fxs-plan.json
mise x -- ./target/debug/ht813-console reconcile --session private-console/session.json --plan private-console/fxs-plan.json
mise x -- ./target/debug/ht813-console logout --session private-console/session.json
```

The plan's mode selects native storage only with `save` or native storage and activation with `apply`.
The submission command claims the plan before its first request.
Concurrent processes and renamed copies share the same claim.
A submitted plan stays consumed after interruption, refusal, or an ambiguous result.
Read current values with `reconcile` and create another reviewed plan when another attempt is necessary.

Results go to standard output, and diagnostics and progress go to standard error.
Human output distinguishes confirmed public readback, independent secret verification, and uncertain submission.
Private values remain redacted in both output modes.
Visible text escapes terminal controls and bidirectional formatting controls.
Submission exits zero only for confirmed public readback.
Exit 2 requires independent secret verification, and exit 3 denotes an uncertain submission.
Other errors exit 1.
Reconciliation reports current public-value agreement and preserves the need for secret verification.
It establishes neither runtime activation nor successful calls.
Logout revokes the session and removes its credential file.
Private claim markers remain for submitted plans.
A console server restart requires another login.

## Parameter reference

The searchable reference covers all 598 definitions in the preserved template, including commented definitions.
Ten supplemental explanations cover active controls observed on HT813 1.0.19.6 but absent from that template.
A source discriminator keeps observed controls separate from template references and prevents invented source lines or template values.
The maintained inventory requires one explanation for every field across the four captured settings forms.
Japanese labels preserve negative settings, codec priority, port identity, units, and source parameter numbers.
Each inline explanation shares its content with the documentation page.

The [configuration archive](https://www.grandstream.com/hubfs/Support%20Tools/config-template-10212026.zip) contains the member named `ht813_config_1.0.19.6.txt`.
The member's header identifies 1.0.17.3.
The reference records source lines and distinguishes documented factory values from template assignments.
Template assignments include examples and must never become automatic defaults.
Parameter 8467 has no prefix in the preserved source.
The observed firmware identifies that control as `P8467`, and both searches resolve to its explanation.
Conflicting descriptions and numeric domains keep the affected setting read-only.

The [manufacturer guide](https://documentation.grandstream.com/knowledge-base/ht813-administration-guide/) supplies operational context.
The [audio profile](https://www.rfc-editor.org/rfc/rfc3551.html#section-4.5.14) identifies pulse code modulation with A-law as `PCMA` and with μ-law as `PCMU`.
The interface explains Session Initiation Protocol (SIP) signaling, Real-time Transport Protocol (RTP) audio, codec negotiation, Network Address Translation (NAT), timing, and the consequences of changing a setting.
The reference identifies missing factory values.

## Verification and device acceptance

The `just check` command includes Rust tests, frontend lint, strict types, unit tests, static builds, generated API comparisons, source checksums, and complete parameter coverage.
The `just security` command scans both JavaScript workspaces and Rust dependencies and checks maintained source for secrets.
The `just console-browser` command tests synthetic authentication, drafts, help, confirmation, expiry, stale review, uncertain results, accessibility, layout, Content Security Policy (CSP), caches, and Lighthouse.
Lighthouse requires at least 95 for performance and 100 for accessibility and Best Practices.
Every applicable Search Engine Optimization (SEO) audit must pass except deliberate indexing refusal for the private console.

The `just proof` command checks the production submission guard with Kani.
The `just console-mutants` command checks the pure draft and validation functions with Stryker.
The bounded runner rejects survivors, skipped outcomes, timeouts, empty campaigns, and missing production files.
The `just release-check` command also requires the existing call lab, browser audio, deployment recovery, Miri, fuzzing, Rust mutation, and telephone frontend mutation checks.

Synthetic fixtures have no configurable device origin and can't contact the real gateway.
Enable the fixture through the `console-fixture` feature.
A passing software suite doesn't establish compatibility with an unobserved firmware form.
Device acceptance requires authenticated form inspection and controlled verification on the actual firmware.
The offline inspection command accepts a saved HTML page, its native document URL, and a directory containing the three script basenames.
Run `mise x -- cargo run --locked -p phone-server --bin ht813-console -- inspect-form --html <page> --document <native-url> --resources <directory> --output <private-path>`.
The command refuses an unsupported save or apply operation and writes both complete entry lists to a newly created private file.
Submission data contains session tokens and must remain outside source control.
Compare both entry lists with browser `FormData` in an offline replay of the captured page and its exact resources.
Remove browser-injected extension tags only from a separate private replay copy.
The live adapter refuses unrecognized external scripts.
Offline comparison establishes entry values and ordering for the captured forms.
Device persistence and call behavior require separate observations.
Keep the live console stopped until the required software checks pass.
Hardware power loss, analog interoperability, and paid providers keep their separate acceptance procedures.
