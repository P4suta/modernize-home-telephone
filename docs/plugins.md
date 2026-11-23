# Optional call plugins

The host owns line admission, call transitions, gateway channels and bridges, persistent events, and delivery checkpoints.
Optional implementations live in independent Rust crates.
Each plugin must remain independent of the server, administration tool, and other plugins.
The fast gate checks the dependency graph and compiles every host feature combination with warnings refused.
It also runs the server tests without optional features, and tests that need AI reception declare the `plugin-ai` feature.
Without that feature, the default AI provider is `disabled`.

The `plugin-ai` Cargo feature selects `phone-plugin-ai`.
Select `openai` for `ai.provider` and provide its credentials to enable reception.
The `plugin-ntfy` feature selects `phone-plugin-ntfy`.
Add `notifications.ntfy` and its publisher token to enable incoming-call publication.

Both implementations are available in the standard image.
Production initialization keeps AI off.
Explicit block rules still apply, and unanswered calls end without voicemail.

```toml
[ai]
provider = "disabled"
```

Build without optional implementations, or select the required plugins.
The minimal build retains shared media contract types.

```sh
mise x -- cargo build --locked -p phone-server --no-default-features
mise x -- cargo build --locked -p phone-server --no-default-features --features plugin-ntfy
```

Use `disabled` for `ai.provider` when the AI implementation is absent.
Omit `notifications.ntfy` when the ntfy implementation is absent.
The configuration checker rejects malformed settings.
During call-service startup, unavailable ntfy credentials leave its publisher off and produce a diagnostic without preventing ordinary calls.
Notification requests run outside call admission and keep their bounded timeout, retry, and durable acknowledgement rules.
The [notification guide](notifications.md) describes the payload and delivery limits.

The plugin APIs are statically linked Rust interfaces with shared process memory.
Independent processes require an authenticated protocol and coordinated cancellation and recovery.
The [architecture decision](adr/compile-optional-features-as-independent-rust-plugins.md) records the boundary.
