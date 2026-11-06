# Assurance contracts

## Establish the verdict

Use `mise x -- just check` for the deterministic development gate and `mise x -- just release-check` for the complete local verification sequence.
The release sequence builds the ARM64 lab, exercises browser and SIPp calls, rehearses deployment recovery, and runs the bounded proof, interpreter, fuzz, and mutation campaigns.
`just evidence` checks the source and report identities of retained results.
Changing an implementation, harness, verification recipe, or relevant build input invalidates its receipt.
A retained receipt applies only to its recorded sources.

The manual [hosted workflow](../.github/workflows/software-assurance.yml) runs the same complete inventory across native AMD64 verification and native ARM64 runtime jobs on one commit.
The runtime job downloads the verification reports and applies the existing evidence gate to their source identities and artifact hashes.
Deployment backups stay on the runtime runner.
The transferred verification reports expire after one day.

The gate requires nonempty expected inventories.
The gate checks proof harness names, browser case names, and Rust fault records using their maintained source records.
Browser results must contain one passed execution per named case, no skipped or flaky case, and no retry.
SIPp must finish one dialog with no failed or remaining call.
Mutation campaigns must have no survivor or timeout.
The mutation report distinguishes compilation errors from killed mutants.
A resource limit stop leaves the campaign inconclusive and produces no successful receipt.

Before expensive builds, the hosted workflow executes the Docker source-only target and its corruption controls.
Run the same boundary tests with `mise x -- cargo test --locked -p xtask --features upstream-archives --test upstream_archives`.
The source-only stage preserves the pinned Asterisk and Japanese announcement bytes without compiling either runtime.
Asterisk uses its official release archive because the ordinary download directory retires superseded versions.
The tests fetch the actual archives and independently compare exported file hashes with the Dockerfile's declared digests.
For each archive, a local HTTP fixture must build with valid bytes and fail with a checksum mismatch for corrupt bytes.
Removing only the checksum instruction in the test copy must permit corrupt bytes.
Each instruction and attempt uses distinct content and a distinct digest because Docker may reuse checksum-addressed downloads even with its build cache turned off.
The rejecting control must identify the injected bytes by their actual digest.
Docker's checksum enforcement and SHA256 collision resistance remain external trust assumptions rather than Rust proof claims.

## Prove the pure production boundary

The [Kani inventory](../verification/contracts.toml) imports executable production functions from `phone-core`.
It covers call ownership, every call and update action, terminal state refusal, number validation, path traversal refusal, routing, retention, playback marks, deadlines, input lengths, and streamed byte budgets.
Enums use their complete generated domains.
Integer properties quantify the declared Rust integer domains, including zero, exact limits, integer extrema, checked overflow, and clock regression where applicable.
The proof target uses 64-bit integer and pointer semantics, matching the intended ARM64 runtime's integer widths.
It doesn't establish equivalence of arbitrary platform libraries or operating systems.

The path proof admits arbitrary byte content and lengths through 257 bytes, with the public parser accepting at most 256 bytes.
The dial-number proof admits arbitrary byte content through 33 bytes.
An independently checked length predicate covers the entire `usize` domain, and an Abstract Syntax Tree (AST) gate binds the public rejection guards to that predicate.
The composition accounts for inputs larger than the finite backing arrays without assuming that every input already fits.
The AST gate also checks the proof inventory, permitted assumptions, public parser wrapper, observation erasure, and loop decrease form.
The path scanner's loop contracts observe an arbitrary byte position while the production caller supplies a fixed observation parameter that can't affect the returned decision.

Kani 0.68.0 runs with loop contracts and an explicit C Bounded Model Checker (CBMC) object-bit configuration.
The positive gate requires the exact expected harness set, successful assertions, and satisfiable coverage checks.
Three negative controls must fail for identified properties rather than for compilation or tool errors.
The first asserts lost ownership after acquiring a line.
The second copies the core and replaces path validation with unconditional acceptance, requiring a loop invariant failure.
The third expands the accepted ASCII alphabet and requires the independent path specification to fail.
The copied implementations stay inside the verification run directory.
The controls preserve the source files under development.

The native verifier checks supported Rust semantics under the recorded harness assumptions.
It doesn't prove the effects of Asterisk, SQLite, HTTP, file synchronization, cloud inference, or the analog telephone interface.
Those boundaries use the production adapters in native fault tests and real service rehearsals because their external state and operating-system behavior are outside the core proof semantics.
Reconsider the boundary when a deterministic policy or adapter decision fits the executable core without changing its meaning.
Prove that decision before reporting the complete gate as passing.

## Inject failures at observable boundaries

The [fault catalog](../verification/faults.toml) records each covered invariant, the previous gap that hid the defect, and its executable enforcement.
The checker discovers Rust tests in fault files, checks named contract records through the Rust syntax tree, and rejects missing, duplicated, empty, or unresolvable records.
Adding a test alone can't close a cataloged fault without explaining the coverage gap and the prevention mechanism.
The catalog's completeness check applies to its declared Rust test inventory.

Admission tests reject a SQLite insert before a call row exists and observe cleanup of the original external channel.
Readiness tests keep REST responsive while the event stream or reconciliation fails.
An external-media test delays the exact Stasis event, rejects premature bridge attachment, and sends an unrelated channel event first.
The production event consumer updates readiness independently of the serialized controller, so waiting for Stasis can't block delivery of the event needed to continue.
[Asterisk's `applications` resource](https://docs.asterisk.org/Latest_API/API_Documentation/Asterisk_REST_Interface/Applications_REST_API/) supplies the ownership boundary used during reconciliation.

Provider-response tests inject authentication, rate-limit, server, schema, fixed-length overflow, and streamed overflow faults.
A causal barrier stalls one transcription request while another call's recording must become durable through the independent capture worker.
A cancelled download must leave pending metadata, remove its partial file, preserve the gateway copy, and permit a later valid download.
The capture worker must revalidate and replace a corrupt cached file before committing availability or source cleanup.
Successful background work clears only the fault fields it owns and preserves the cause of an unrelated call failure.

Media tests negotiate incorrect codecs, frame sizes, and packet times, overflow the pre-readiness queue, pause output with `MEDIA_XOFF`, resume with `MEDIA_XON`, and deliver audio after cancellation.
The fixtures exchange real WebSocket messages and use causal barriers instead of sleeping until a favorable schedule appears.
Readers for JSON over HTTP and gateway WebSockets enforce transport bounds before field-level parsing.
The tests exercise exact limits and the first rejected byte.
Channel readiness also bounds identifier length and live inventory growth.

Retention tests include more than the history page limit and more than one expiry batch.
They require quota accounting across the complete recording inventory while protecting an active or quarantined owner.
Deletion tests let a worker finish after history deletion and require conditional writes to prevent resurrection.
Notification tests require stable event identities during retries and verify webhook authentication, push encryption, and push signatures.
Actual delivery through a browser vendor's push service remains separate from those cryptographic checks.

## Test the measurement and collection code

Native HTTP fixtures share a bounded reader that consumes the complete declared request before replying or closing a connection.
A header-only recovery fixture caused a Linux connection reset when the body arrived after the headers.
Deterministic reader tests split a complete request at every two-part transport boundary and require the complete body with no unread bytes.
The fixture contract also accepts exactly 4096 bytes and rejects truncated, ambiguous, unsupported, and larger messages.
The fixture results establish the behavior of the shared test helper.

Browser audio checks compare a known signal with a different frequency and an absolute power floor.
The fixtures use a 400 Hz browser microphone, a 500 Hz virtual caller, and 1000 Hz mock read aloud audio.
The browser test checks the saved recording for the corresponding signals after capture completes.
The 730 Hz comparison is a null measurement, not a speech classifier.
Independent unit tests reject silence, an unrelated frequency, and insufficient samples.

The browser microphone fixture carries a continuous tone.
Outgoing Real-time Transport Protocol (RTP) echo must contain that tone before muting, omit the tone across successive sampling windows while muted or held, and contain the tone after resumption.
A fixture with natural silent intervals would weaken the absence measurement.
The waveform regression requires every microphone sample to keep its amplitude throughout the fixture.

The browser inventory also covers settings validation and persistence, number-rule precedence, manual answering, voicemail completion, recording playback and seeking, transcript retry, confirmed deletion, and command refusal.
Tests of server-sent events interrupt browser networking while independent API requests complete and delete calls, then require a fresh convergent history without duplicates.
Session Initiation Protocol (SIP) tests close a proxied live signaling connection, reconnect explicitly, and answer another incoming call.
[Playwright's offline emulation](https://github.com/microsoft/playwright/blob/v1.64.1/docs/src/api/class-browsercontext.md) leaves established WebRTC media unaffected.
Signaling recovery supplies no evidence of RTP recovery after a network outage.

Background recording updates publish through the same committed-event channel as call transitions.
A paused-clock HTTP regression requires updates and deletions before the polling timer can fire and requires failed database commits to publish no event.
Browser lifecycle regressions close offline streams and reject callbacks from retired streams.
Gateway readiness arrives through watched status events even while call history is idle.
A paused-clock HTTP regression toggles maintenance and requires both readiness changes before polling.
The browser validates each status event and clears old readiness after a transport error or an offline event.
Those adapter tests complement the core proofs without proving browser or network behavior for every possible schedule.

Preserved vendor files have Git attributes that turn off text conversion.
A real index and checkout round trip checks byte preservation, while the source inventory binds the attributes to each artifact identity.
The checksum catalog remains authoritative for the preserved original bytes.

A short outgoing call exposed an assumption in the original measurement: a fixed 4096-sample window produced no observation for a valid shorter recording.
The measurement now uses an available window up to 4096 samples, requires at least 800 samples, and retains the same power and null-ratio thresholds.
Unit cases span short and long recordings.
The measurement establishes the presence of the specified fixture signal under its tested conditions.
Speech intelligibility and acoustic quality require their own measurements.

Evidence collectors also have negative tests.
They reject empty or duplicated inventories, stale sources, changed reports, missing browser cases, retries, failed proof assertions, and unreachable proof coverage.
Deployment evidence checks the recorded backup manifest hashes, verifies each backup's current contents, and compares the restored recording hash.
Resealing a changed backup preserves its invalidity for the earlier result.
The evidence contract assumes cooperative control of the checkout and local evidence files.
Adversarial signing requires a separate trust boundary.

## Challenge tests with independent tools

`just miri` runs the core tests under the pinned Miri toolchain with strict provenance.
The interpreter complements the logical harnesses by checking the exercised execution paths for invalid memory behavior.
Passing Miri doesn't extend its tested paths into an all-input proof.
The core forbids unsafe Rust and has no third-party runtime dependencies.

`just fuzz` runs two targets with two fixed seeds each and 10000 executions per seed.
The total campaign is 40000 executions with an input length limit of 512 and AddressSanitizer enabled.
The targets compare path and number parsing with independent specifications and exercise sequences of state and ownership operations.
The campaign copies versioned seed corpora into a private run directory and replays their inputs before mutation.
Successful parser results must preserve their input bytes and round trip through the public parser.
Execution count and seeds determine the campaign's scope.
Elapsed machine time doesn't determine success.
The container deadline is only a resource stop condition.

`just mutants` selects every production Rust file in the core and excludes only the proof harness file.
The gate requires every generated mutant to trigger a test failure or a compilation refusal, with both categories retained in the report.
`just web-mutants` uses Stryker on the frontend's pure timeline reducer with per-test coverage and the pinned TypeScript checker.
The TypeScript mutation result covers only the timeline reducer.
The Kani broken-parser and broken-alphabet controls also challenge macro implementation changes.

`just security` checks the locked Rust and JavaScript dependencies and scans maintained source for credentials with redacted output.
The gate validates the exact 14-entry source checksum inventory even when a public checkout has no manufacturer originals.
When originals exist, every file must match and the complete archive must be present without symlinks.
Manufacturer originals never enter the maintained source inventory or secret scan, and the scanner has no manufacturer-path exception.
The generated OpenAPI document and TypeScript declarations must match the server schema, and runtime schemas remain tied to those declarations.
These checks reduce identified dependency, schema, and secret risks without claiming a complete security audit.

## Exercise deployment and preserve limits

The public distribution boundary uses the existing validated relative-path type for checksum names.
Filesystem metadata, SHA-256 implementation, and Git archive behavior remain external trust assumptions.
Native tests exercise absent and complete archives, each missing or modified member, duplicate and malformed catalogs, file and directory symlinks, and the real Git archive interface.
Mutation testing for publication covers the changed inventory and archive verification functions.
The telephone-core campaign uses its own source inventory.
The publication checks don't prove publisher permission or whole-filesystem behavior.
Reconsider the boundary if archives need remote acquisition, untrusted manifests, or redistribution permission.

The real deployment rehearsal uses ARM64 app and Asterisk images, a wrong executable as a failed candidate, an uncommitted recovery journal, and a fresh restored installation.
The rehearsal returns the original installation to service.
Native update-port tests inject paired failures, so a failed recovery can't erase the error that triggered it.
Filesystem tests reject symlinks, parent traversal, deep empty directory trees, occupied destinations, and incomplete publication.
Package validation requires the executable, both image archives, licenses, Asterisk source archive, and the maintained source identity before Docker import.

Recovery must retire the rollback journal and synchronize its parent before sending the HTTP request that reopens call admission.
A real HTTP observer checks the ordering, and a failed retirement must leave quiescence enabled without sending a request.
Standalone backup also refuses a pending journal so it can't bypass interrupted-update recovery.
File and directory synchronization expresses the required persistence order, but actual power-loss behavior still depends on the kernel, filesystem, device controller, and storage medium.

Verification containers have explicit CPU, memory, swap, process, and time limits.
Each run isolates compiler and proof outputs to prevent reuse of stale generated models.
Hardware-free results establish software behavior under the stated service and filesystem assumptions.
The [hardware acceptance procedure](hardware-acceptance.md) remains the boundary for BL3000HM interoperability, HT813 fallback, actual Pi startup, storage power loss, paid AI behavior, and external push delivery.

## Follow ownership across failures

An external channel can exist before the database accepts its call row.
The admission regression injects rejection at that exact commit boundary instead of failing a later operation with a valid row already present.
The controller records the resource before publishing the incoming call and retains a cleanup route that doesn't depend on retrieving the rejected row.
An initial admission failure closes the original channel and disables readiness until reconciliation succeeds.
An offered call also has a five-second deadline, preventing a call with unfinished admission from owning the line indefinitely.
The deadline proof covers every phase and integer boundary.
The adapter test supplies the database failure and observes the channel deletion.

Logical call completion and physical resource release have different lifetimes.
A failed gateway hangup can leave an external channel present after the history row says that the call ended.
The quarantine contract retains ownership during that interval, refuses another owner, and protects the recording from deletion or retention work.
Cleanup retries then release the resource before a later call can own the line.
Tests that checked only the final history phase would miss the remaining gateway resource.
The regression checks ownership and attempted reassignment while deletion is still failing.

Finished-call deletion also competes with background processing.
A worker can download or transcribe audio before a user deletes the history and then attempt its final metadata write afterward.
The deletion contract delays the worker at the relevant boundary and requires the late write to leave the row absent.
Conditional updates bind completion to the still-existing job and call state.
The cleanup queue retains the gateway recording identifier independently of the history row, allowing source deletion after history removal.
The tests exercise those two lifetimes rather than treating absence from the history query as proof that every external copy vanished.

Retention has a similar inventory boundary.
The history endpoint returns a bounded page for display, while quota decisions need the complete durable recording usage.
The large-history fixture places available recordings after more than ten thousand older rows and exceeds one purge batch.
It verifies total usage and file-to-row consistency across successive expiry batches.
The fixture keeps an owned recording outside every purge decision.
The independent retention theorem checks the decision for arbitrary numeric limits, while the database fixture checks that the adapter supplies complete usage and preserves each surviving file.

## Keep readiness evidence specific

A successful channel creation response establishes an identifier before Asterisk necessarily delivers the corresponding `StasisStart` event.
The earlier gateway fixture returned success for bridge attachment immediately, so the test omitted the ordering constraint of the real server.
The delayed-event fixture now makes premature attachment return a conflict.
An unrelated Stasis event can't release the readiness wait, and disconnect resets the live inventory.
The real Asterisk browser suite supplies a complementary observation of the full event, channel, bridge, and media path.
The native exact-identity contract avoids depending on a favorable scheduler delay to expose the race.

Administrative readiness also needs a narrower meaning than an HTTP server accepting requests.
The candidate gate checks an initialized event consumer and completed gateway reconciliation, including a candidate intentionally paused for an update.
A responsive endpoint of the Asterisk REST Interface (ARI) must fail readiness when its event WebSocket is missing.
Recording workers start paused and remain paused until reconciliation establishes the resource inventory.
A candidate must complete reconciliation before it captures or deletes recordings.
Fault fixtures preserve the responsive REST endpoint while independently breaking each missing prerequisite.

AI status has a different evidence source.
The status field reports whether the provider configuration has credentials, and the interface labels that field as configuration.
The field makes no claim about an established paid provider connection.
The mock lab exercises request framing, audio transfer, tool argument validation, and routing decisions without supplying the provider's actual model behavior.
Provider authentication errors and malformed replies have their own injected tests and error states.
A real account, model response, Japanese speech quality, and network latency remain observations for provider acceptance.

## Preserve restart decisions

The update port models quiescence, backup, stop, candidate startup, health, commit, and recovery.
A rollback journal adds a persistence lifetime beyond those logical phases.
Admission must remain paused while an uncommitted rollback instruction exists.
Otherwise, a later startup can replay an earlier snapshot after new calls have committed.
The common resumption path now removes that instruction and synchronizes its parent before it sends the administrative request to accept calls.
The ordering test observes the filesystem at HTTP receipt, and a conflicting journal directory must prevent the request entirely.
The test covers the adapter boundary omitted by a state-machine test that only recorded a successful recovery followed by resumption.

An interrupted operation also affects the next administrative command.
Standalone backup shared resumption behavior without the pending-journal guard used by replacement.
The guard now runs before configuration reads in backup and also protects installation and replacement.
An operator must complete recovery through `boot` before starting another protected operation.
The regression supplies a pending journal and omits the rest of the installation, requiring the specific recovery refusal before any configuration error.
That arrangement establishes which boundary rejected the operation and prevents an unrelated missing file from making the test appear successful.

Docker image references have an independent restart lifetime.
A source identifier alone can't distinguish two builds that might produce different image contents, and reusing a tag could replace the reference retained for rollback.
Each build has its own identifier alongside the source identity.
The manager fingerprints image configuration and filesystem identity and checks the intended Linux ARM64 target before startup.
A native contract requires different references for different builds of the same source.
Package validation then binds the supplied maintained source tree and required archives to the declared release before importing images.

Paired fault injection examines the error-reporting path during failed recovery.
An original backup or candidate error can coincide with failed resumption, restoration, startup, health, or stop.
Returning only the second error would erase the operation that initiated recovery, while returning only the first would conceal the failed recovery.
The update tests inject both faults in the same run and require the returned error chain to preserve both causes.
The real rehearsal additionally checks service readiness and the sample recording after rejecting a candidate.
Neither an informative error string nor a successful rollback state alone establishes that the restored service can answer another call.
