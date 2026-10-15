# BRA-558 implementation record

## Baseline and contract

Start at reviewed BRA-554 commit `5b4831a0` on
`feature/bra-558-implement-host-incident-consumption-and-report-output-in`.
[PR #609](https://github.com/lejmer/bray/pull/609) is ready and BRA-554 is In Review.
Its [independent review](https://github.com/lejmer/bray/pull/609#pullrequestreview-5534605036)
has no findings. BRA-502 remains In Review in PR #608. Step 5 completes only after this issue.

Move ordinary report output, typed incident disposal and host sink policy to Bray.
Preserve a single moved owner, complete messages, source/cause, origin/ordinal, concrete cleanup,
backing and provider/attachment dependencies. Formatting, payload destruction and backing release
are independent mandatory operations. No terminal allocation, recursive chain disposal, scheduler
retention by synchronous sinks, new language features or duplicate semantic analysis.

## Design and replacements

Use the existing admitted panic record pool and typed native outcomes. The implemented typed
producer is static finalizer resolution. Its descriptor callbacks return only panic reports, never
nested typed descriptors. Keep formatting, destruction and backing-release outcomes in three fixed
stack slots, run every operation, then consume the three reports with the existing iterative
consumer. This requires no mixed queue or additional record admission. Remove Rust typed disposal
and sink sequencing as their source consumers take over. Keep sink synchronization in its existing
Rust owner and move ordering, batch detachment and drain state transitions into Bray.

Stream ordinary output from retained message bytes through bounded stack storage and existing
standard-error operations. Optional captured history may copy explicitly. Its observation boundary
borrows the report and cannot overwrite an established primary with a suppressed incident.

Terminal generated callbacks execute body observation and resolution before returning to entry.
This retains attachment and optional bridge services through typed result destruction and produces
body-first output. Returning native report-owner callers retain their existing ownership contract.
A delayed sink alone cannot repair terminal ordering because exact-thread providers may retire
before the caller observes the body. Do not defer reports past those dependencies.

Reuse validated type cleanup and provider binding. Invalid cleanup-storage release remains an
internal invariant failure: that operation has no user callback or allocator failure. Real callback
failures use distinct explicit outcomes. Do not reinterpret unknown object bytes as a formatter.

## Obligation map and acceptance

| Obligation | Existing mechanism | Required evidence |
| --- | --- | --- |
| Full panic owner and message | Protected report, pool, message copy/release | Ordinary and captured long messages, source/cause, denied allocation |
| Typed payload and backing | NativeCleanupIncident, generated concrete destroyer, BRA-502 lease | Large/overaligned payload, producer exit, exactly-once destroy/release |
| Multiple disposal failures | Explicit native outcomes and admitted outgoing records | Formatter, destructor and release failures all retained in order |
| Bounded traversal | Existing iterative panic consumer and flat chains | Deep callback-produced panic chains on small native stack, no terminal admission |
| Sink ownership | CleanupReportSink and segmented records | Reentrant/concurrent transfers, detached callbacks, no scheduler retention |
| Entry output ordering | Existing host Execute/Observe/Resolve operations and callback result resolver | Root/foreign body before TLS cleanup, typed error destruction while services live |
| Native contracts | Validated runtime roles and source opaque protocol | Forged headers/owners rejected, source/imported consumers equivalent |
| Optional history | Existing test-host capture | Primary preserved, ordered suppressed observations, provider released after original disposal |
| Structural cost | Native artifact/link map workflow | No migrated Rust consumer exports, report layout and successful-path metadata, failure-only allocations |

Keep independently loaded-provider formation in BRA-555, full attachment/static migration in
BRA-557, and test protocol migration in BRA-559. Their scope cannot excuse early disposal of a
current incident dependency. BRA-619 follows this shared incident transport.

## Work and validation policy

Batch coherent edits before compiling. Run focused contract/native tests, then mandatory style and
format. Audit every changed behavior against sibling paths and remove replaced mechanisms.
Publish a stacked draft PR and obtain independent published review before readying it. Record exact
results here. Reconsider the design if it requires another pool, terminal allocation, whole-message
copies for ordinary output, weaker authority or duplicate downstream checks.

## Implementation findings and initial evidence

The source consumer keeps explicit formatter, destructor and backing-release outcomes in stack
storage. Callback projections read the witnessed descriptor itself. They cannot pair a witness with
an unrelated address. Outcome materialization invokes no callbacks, and the two output pointers
come from simultaneous exclusive borrows. The source-only production check passes. Native imported
checking also catches C ABI and package-import requirements absent from that fixture.

Static finalizer errors may depend on their own receiver. `run_static_cleanup` now consumes each
phase's incidents before receiver destruction or detachment. The shared regression observes receiver
state through finalizer and destructor failure disposal. The native static-cleanup fixture adds
receiver-borrowing errors for both product and thread storage.

The observer must remain installed through the last attachment acquisition. Retirement now leaves
first, restores the outer provider only when a row survives, and otherwise verifies that no outer
provider was retained. During native cleanup TLS is empty. Observation uses the existing unique
closing row for the exact native thread under the host lease, without permitting a new entry or
creating another TLS registry. Nested entry still uses the current attachment identity.

Initial runtime verification passes 164 tests, including all three explicit disposal failures,
producer-thread exit, 20,000 ordinary records and nested destructor-produced reports on a small
stack. LLVM passes 93 tests, ABI 18 and test protocol 19. The default adapter target has zero tests,
so selected-host and adapter capture tests require their feature-enabled run before acceptance.
These are intermediate results, not completion evidence. Native artifact acceptance and final
published review remain required.


## Review matrix and replacement audit

| Behavior category | Repository search and owning implementation | Disposition |
| --- | --- | --- |
| Primary bytes and entry errors | `report_panic`, `substrate_report_primary`, `entry_failure_reporting`, `copy_to`, standard-error writes | Remove Rust formatting and arbitrary error-byte reads. One Bray output module streams bytes and uses a shared first-failure selector. Explicit test history is the only whole-message copy. |
| Concrete incident disposal | `NativeCleanupIncident*Callback`, `dispose`, static finalizer mapping | All native descriptor callbacks use explicit outcomes. Source disposal runs formatter, destroyer and backing release, then the existing iterative report consumer. Rust panic payload adaptation remains for Rust producers only. |
| Sink mutation and traversal | `next_ordinal`, `pending_count`, `report_segment`, `CleanupDrain`, `draining` | Bray owns append/ordinal/batch/drain-state transitions. The existing Rust mutex protects each transition and callbacks run outside it. No second sink or record pool. |
| Terminal result resolution | `ResolveRootTerminal`, `resolve_callback_outcome`, `entry_result_succeeded`, root/callback translation | Reuse validated MIR operations inside the synchronous continuation. Resolve results before attachment retirement. Finalize and destroy have separate outcome slots. Async returning-owner paths retain their existing contract. |
| Receiver-dependent cleanup | `run_static_cleanup`, `collect_finalizer_incidents`, product/thread callers | Consume both sync and async finalizer incidents before owner destruction. Consume destructor incidents before detachment. One shared cleanup implementation serves product and thread storage. |
| Optional capture and serialization | `record_outcome`, `record_report`, `TestHostResult`, `TestInvocationResult`, codec and JSON | Keep the first primary, then ordered suppression. Transfer independent history through the existing child result and JSON representation. No parallel capture protocol. |
| Observer lifetime | `NativeHostCallbacks`, context registration, `Scope`, attachment exit/reuse | Retain observer through the final callback and clear both callbacks on every row-retirement path. Nested scopes restore the outer pair. Closing lookup uses the existing exact-thread row and lease. |
| Role ownership and native graph | runtime catalog, export macros, adapter exports, archive support exports, link-map forbidden symbols | Remove migrated Rust output/destruction exports. Add source-owned incident/sink roles to the existing catalog. Native smoke checks actual linked implementations. |

The concrete typed producer in this issue is static finalization, including asynchronous finalizers.
Selected-scope emission still has BRA-619's retained MissingRole regression. Its producer acceptance
remains Step 6 and must exercise this same consumer. This is a producer limitation, not a second
consumer interpretation. Independent providers and fully Bray-owned test-host transport retain
their existing downstream owners.

Feature-enabled verification passes 169 runtime and 8 adapter tests in
`bra-558-capture-tests.log`. The last-attachment test retains a long Unicode history after original
report disposal, and the empty-cleanup test verifies callback removal before row reuse. Native
artifact smoke passes in `bra-558-native-final.log`, including generated ordinary and observed
products and foreign entry. A final extension replaces the removed Rust origin-format assertion
with captured native segment output under allocation denial. The extended native smoke passes in the same final log.


## Acceptance evidence and cost

All 38 production host-contract tests pass, including forged incident headers, aliased outcome
destinations, source consumer bodies and the real bridge declarations. The generated native
static-cleanup fixture passes for product and thread storage with receiver-borrowing errors.
The retained static-cleanup-panic fixture passes synchronous and asynchronous finalizer panics,
owner destructor panics and typed-error destructor panic, with its exact cleanup counter intact.
The new synchronous entry-error fixture returns status 1 and observes the primary, finalizer panic
and destructor panic in that order. Async `Result.Error` also returns status 1 and the opaque error
message. These focused generated executables and logs are under the shared target's
`bra-620-authority/bra-558-*` paths. The final rerun uses the current compiler, matching BRA-558 native archive and freshly generated bootstrap standard-library bundle.

The full native artifact smoke proves bounded copying of all 16,385 message bytes in 33 chunks,
once-only release, opaque error output and stable cleanup origin output with allocation denied.
It also passes archive ownership, bootstrap/synchronous link maps, ordinary/observed/timed native
products and the generated foreign-entry driver. The report and outcome layouts remain 136 and
152 bytes on x86_64. Startup remains six allocations, now 704 bytes versus BRA-554's 688 bytes.
The added optional observer accounts for that attachment metadata increase. Cleanup-bundle
admission remains nine allocations and 148,280 bytes for 131,074 payload bytes at alignment 4,096.
No new allocation is added to ordinary terminal output or disposal. Independent captured history
allocates only when a test host explicitly selects observation.

The last full native run measured 100,000 synchronous entries in 0.2028659 elapsed seconds and
100,000 foreign entries in 0.209 C-clock seconds. Earlier runs in this task varied from 0.1568319
to 0.2077606 seconds for synchronous entry. This is local instrumentation under changing build
load, not an isolated performance comparison or a Rust/C++ parity claim. The final numerical
performance and size owners retain their comparative gates.

The mechanical audit separates approximately 1,180 net production/tooling lines from 518 net
test/fixture lines before the final formatting pass. Growth pays for the source consumer and
explicit callback contracts, observer lifetime, independent capture and generated terminal
sequencing. Searches removed the previous Rust byte formatter, typed disposal loop, sink
mutation policy, metadata formatter and adapter exports. The role catalog remains one declarative
inventory despite its existing size warning. There is no new compiler analysis or second protocol
registry. Unknown run states and panic causes are invariant failures, not substituted observations.

The capture audit found that a static cleanup error reused the entry-error observer and could
inherit the test entry's type. Typed observation now carries its actual descriptor digest and source
through the existing borrowed observer callback. Independent results retain both through codec and
JSON. The generated fallback formatter no longer reports a second entry error because source already
reports the descriptor. Actual formatter callbacks and their failure outcomes still run normally.
Optional source serialization now uses one helper shared by returned errors, explicit failures and
panics. There is no additional observer registry or persistent attachment field.

Final verification passes 170 feature-enabled runtime tests, 9 adapter tests and 19 protocol tests in bra-558-capture-final.log. The focused production contract check passes after the typed-observation correction. Mutating cargo xtask style and cargo xtask format pass. The final native artifact smoke passes in bra-558-native-current.log after the capture correction and the move to report/consumer.bray. All four generated executable fixtures pass again against the matching retained runtime archive in bra-558-runtime. Synchronous entry cleanup output matches the exact three-line primary/finalizer/destructor order. Publication and independent PR review remain pending.


## Published review repair

[PR #610](https://github.com/lejmer/bray/pull/610) contains implementation checkpoint `812b4122`.
[Review 5536829559](https://github.com/lejmer/bray/pull/610#pullrequestreview-5536829559)
identified callback-time allocation in optional capture before mandatory original disposal.

The repair retains fallibly allocated owned bytes and scalar type/source metadata during cleanup.
Shared protocol strings and digest formatting are constructed only after cleanup. Message buffer
and history growth use the same fallible reservation helper. Observation returns its status through
the existing native callback and records capture failure for final protocol publication. Copy
failure also returns normally. Capture does not borrow session state while invoking the provider.
The audit includes cancellation insertion and typed identities, not only message allocation.
No source runtime ABI or ordinary output implementation changes for this repair.

Review repair verification passes 171 runtime, 11 adapter and 19 protocol tests in bra-558-capture-review.log. It covers denied message/history allocation, failed provider copy, and actual source consumption of ordinary and typed reports after observer rejection, with all four original owners released once. Mutating style, format and the native artifact smoke pass in bra-558-review-style.log, bra-558-review-format.log and bra-558-review-native.log. The latter retains six startup allocations and 704 bytes. Published re-review remains pending.
