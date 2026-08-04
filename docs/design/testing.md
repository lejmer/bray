# Testing Standard Library And Runner

This document defines the compiler, toolchain, runtime, and standard-library contracts that make Bray tests discoverable and
executable. The design supports sequential and bounded parallel execution through the same protocol. Capture, cancellation,
timeouts, cleanup, and deterministic reporting are part of the foundation rather than later execution modes.

The language rules for test products and test declarations are defined by the
[language specification](../language/modules-and-packages/test-products-and-entries.md).

## Principles

- Test discovery consumes checked compiler facts and never reparses source.
- A test product is compiled once into a native host and an immutable test catalog.
- Test output uses dedicated capture streams. The runner protocol never shares stdout or stderr with test code.
- Sequential and parallel execution use the same invocation and outcome contracts.
- Scheduling can be concurrent while discovery and final report order remain deterministic.
- Cancellation and timeout outcomes are distinct and both complete ordinary language cleanup before publication.
- User-facing prose is rendered by `bray-messages`. Catalogs, protocol messages, and reports carry typed data.
- Durations and live event order are volatile observations, not reproducible compiler facts.

## Ownership

The testing system crosses compiler and toolchain boundaries without giving any one layer unrelated responsibilities:

- `bray-compilation` derives checked test entries and test catalog facts from the selected test product.
- A dedicated test-protocol crate owns catalog, invocation, event, outcome, report, and wire-format types. It does not own process
  launch, scheduling policy, terminal rendering, or compiler test helpers.
- `bray-lowering`, code generation, and emission produce the native test host and publish its matching catalog.
- `bray-runtime-interface` and `bray-runtime` provide root-run execution, cancellation, panic capture, cleanup completion, and
  per-run standard-stream routing required by the generated host.
- `bray-platform` provides process, pipe, clock, wait, and hard-termination mechanisms used at the host boundary.
- Bray Tack owns workspace selection, metadata filtering, global resource budgets, host process orchestration, report aggregation,
  terminal presentation, JSON output, and command exit status.
- `bray-messages` owns localized test progress, outcome, assertion, and infrastructure-failure text.
- `std.testing` provides the small source-level testing surface. Ordinary compiler test fixtures remain in `bray-testing` and are
  unrelated to the production runner protocol.

## Test Catalog

Every successfully checked test product publishes one immutable test catalog beside its native host. The catalog is a bounded,
versioned compiler artifact with deterministic encoding. It is not an importable package interface and does not make test
declarations externally visible.

The catalog header records:

- catalog format version,
- package, product, target, and build-configuration identity,
- native host artifact identity and digest,
- runtime ABI requirements,
- ordered entry count.

Each entry records:

- stable test identity,
- fully qualified declaration path,
- source anchor for navigation and reporting,
- generated host entry identity,
- synchronous or asynchronous invocation kind,
- `unit` or `Result<unit, E>` result shape and recoverable error type identity,
- execution constraint,
- required runtime capabilities,
- optional compiler-known display metadata needed for structured failures.

Catalog order is the canonical ascending order of fully qualified test identity, with source identity and source position as stable
tie breakers. Discovery, filtering, sharding, and final reporting preserve this order. A catalog contains no rendered English and
no volatile value such as a duration, process identity, temporary path, or completion timestamp.

Test filters operate only on catalog metadata. Bray Tack can select packages, products, modules, qualified names, exact test
identities, and deterministic shards without starting the host or loading source text. An empty selection is a valid runner result
unless command policy explicitly requires at least one match.

## Execution Constraints

Every catalog entry carries one of these constraints:

- `parallel`: the entry can run concurrently subject to the active resource budget,
- `serial`: the entry runs alone with respect to other entries in the same test command.

`@test(serial)` selects the serial constraint for a function test entry. Bare `@test` selects `parallel`. Module-level `@test`
remains argumentless because it controls source contribution rather than entry scheduling. The parser publishes the optional
constraint as typed directive metadata, and semantic validation rejects any other argument or use of `serial` on a module.
A command-wide sequential mode does not rewrite entry metadata; it sets the global scheduler concurrency budget to one.

Serial entries form deterministic global barriers across all selected products. Bray Tack owns the global admission permit. It
finishes every admitted invocation, grants the serial entry the only permit, waits for its terminal outcome, and only then grants
later permits. A product host cannot begin an entry until Tack sends that entry's start command. This gives serial tests predictable
command-wide exclusion without defining a language-level execution order.

## Runner Protocol

Bray Tack starts each selected native test host with dedicated protocol input and output channels that are distinct from the child
process's stdout and stderr handles. Runtime-provided per-test stream sinks convert test writes into test-identified protocol events.
The raw process streams are not a capture transport and any unframed host write is an infrastructure failure. Protocol messages use
a bounded, canonical, length-prefixed binary encoding so arbitrary user output cannot be interpreted as control data.

The connection begins with a handshake that establishes:

- protocol version,
- catalog and host identity,
- supported capabilities,
- maximum frame and capture sizes,
- runner and host resource limits.

Incompatible versions, identities, or required capabilities fail before any test starts. Unknown optional fields can be skipped only
when the enclosing protocol version permits them. All lengths, counts, and nesting depths are validated before allocation.

The runner first sends one immutable selection plan containing:

- selected test identities in canonical order,
- per-test timeout policy,
- capture policy and byte limits,
- command cancellation identity,
- deterministic resource budget.

Bray Tack then sends one start command per entry after acquiring the global admission permit. The command carries the test identity
and timeout deadline. A host can execute multiple admitted entries up to the plan's host-local limit, but it never selects or admits
work independently. The host emits typed events for invocation start, captured stdout or stderr chunks, terminal outcome, cleanup
completion, host diagnostics, and host shutdown. Every event carries its test identity and a per-test monotonic sequence number.
Completion can arrive in any order. The final report is assembled in catalog order.

Live events are observational and can reflect actual completion order. They are not stored as reproducible facts. JSON reports
separate canonical result data from explicitly volatile timing and live-progress fields.

## Invocation Lifecycle

Each selected test executes behind its own panic-catching root-run boundary. Synchronous and asynchronous entries differ only in
how the generated host drives the declared callable:

- a synchronous test runs directly as the root run,
- an asynchronous test creates its future and drives it through the selected runtime as the root task.

Both forms use the same terminal outcomes:

- `passed`,
- `failed` with a recoverable `Result.Error` value,
- `assertion_failed` with structured assertion metadata,
- `panicked` with a `PanicReport`,
- `timed_out`,
- `cancelled`,
- `infrastructure_failed` when the host or protocol cannot complete the invocation contract.

A recoverable error report always carries the concrete error type identity. It carries a rendered value only when checked formatting
support exists for that type. Test validity does not depend on the error type implementing a formatting contract.

No terminal outcome is published until the root scope has resolved child tasks, lifecycle members, finalization, destruction,
cleanup incidents, and suppressed panics according to ordinary language rules. Cleanup failure enriches or replaces the pending
outcome according to the existing panic and cancellation contracts; it is never silently discarded.

## Cancellation And Timeouts

Command cancellation and per-test timeout use the ordinary run-cancellation mechanism. A timeout is initiated by a host timer and
reported as `timed_out`; an explicit runner or user request is reported as `cancelled`. Both request cooperative cancellation, wake
cancellation-aware waits, and allow cleanup to finish.

The runner can cancel the complete plan or an individual invocation. The host acknowledges each accepted request and publishes the
eventual terminal outcome after cleanup. Ctrl-C first requests ordinary cancellation. A second interrupt or an expired host-shutdown
grace period permits Bray Tack to terminate the host process and report an infrastructure failure for entries that never produced a
terminal outcome.

Hard process termination is a containment fallback, not a language cancellation path. It cannot be reported as successful cleanup.

## Capture

Every invocation receives distinct stdout and stderr capture sinks. Child tasks and standard-library thread or process helpers that
inherit the test root context inherit those sinks. Concurrent tests can therefore write without cross-test leakage.

Capture is bounded per stream and per invocation. Exceeding a bound preserves the retained prefix, records the number of discarded
bytes when known, and marks the stream as truncated. Capture limits do not change the test outcome.

The default human report shows captured output for unsuccessful tests and suppresses it for successful tests. Command options can
show all captured output or disable capture when direct inherited streams are explicitly desired. Machine reports always state the
capture policy and truncation state.

## Scheduling And Budgets

The scheduler accepts a positive concurrency limit and explicit budgets for active host processes, active test runs, capture bytes,
and protocol memory. Sequential execution is the same scheduler with concurrency one. Parallel execution never means unbounded
execution.

Bray Tack considers ready parallel entries in global package, product, and catalog order, acquires one global permit, and sends the
corresponding host a start command. Product hosts enforce their local capacity but do not reorder or independently admit entries.
Scheduling order is deterministic even though operating-system and runtime interleaving are not.

The scheduler stops admitting work after command cancellation. Fail-fast policy can stop admission after the first unsuccessful
outcome but still waits for already admitted work to finish or cancel cleanly. Resource exhaustion is a structured infrastructure
outcome rather than a panic.

## Fixtures And Assertions

Fixtures use ordinary Bray values and lifecycle semantics. Setup is ordinary function or constructor code. Per-test fixture values
are local to the test root, and their finalizers and destructors run before the outcome is published. Shared fixtures use ordinary
owned synchronization and must not bypass language ownership or cleanup rules. The runner does not introduce hidden parameter
injection or a second object-lifecycle system.

The complete public `std.testing` declaration surface is:

```bray
module std.testing;

func fail(pos message: string) -> never;
```

`fail` evaluates and consumes its message once, constructs an explicit structured test failure at the call source, and terminates the
current test root without a normal continuation. A generated test host reports it as `explicit_failure`, not as an unclassified
panic. Calling it outside a test root panics with the same owned failure payload so it can never return. The operation requires no
I/O capability and does not write the message to a stream.

No public fixture base type, hidden parameter injection, current-runner context, or runner-control API is defined. Ordinary values,
constructors, lifecycle declarations, and shared synchronized owners are the fixture surface. A focused utility can be added only
when a concrete fixture behavior cannot be expressed by those language mechanisms.

The built-in `assert(...)` expression remains the primary assertion surface. Its lowered failure carries a source anchor, optional
user message, and compiler-known assertion identity as structured data. Comparison operands can be attached only when their values
have checked formatting support and evaluation has already occurred; reporting never reevaluates an expression.

`std.testing` does not duplicate operators into an assertion family. `assert(actual == expected)` continues to use ordinary Bray
equality and type checking.

## Reports And Exit Status

The machine report uses this logical schema. The wire codec and JSON projection preserve these distinctions rather than flattening
them into strings:

```text
TestCommandReport {
    format: 1,
    selection: TestSelectionSummary,
    products: [TestProductReport],
    summary: TestOutcomeCounts,
    duration: VolatileDuration?,
}

TestSelectionSummary {
    discovered: integer,
    selected: integer,
    filtered_out: integer,
    filters: [TestFilter],
    execution: sequential | parallel { maximum_concurrency: integer },
    capture: TestCapturePolicy,
}

TestProductReport {
    package: PackageIdentity,
    product: ProductIdentity,
    target: TargetIdentity,
    catalog_digest: Digest,
    tests: [TestResult],
    host_failures: [TestInfrastructureFailure],
    shutdown: clean | failed(TestInfrastructureFailure),
    duration: VolatileDuration?,
}

TestResult {
    identity: TestIdentity,
    declaration_path: DeclarationPath,
    source: SourceAnchor,
    constraint: parallel | serial,
    outcome: TestOutcome,
    stdout: CapturedStream,
    stderr: CapturedStream,
    cleanup_incidents: [CleanupIncident],
    suppressed_panics: [PanicReport],
    duration: VolatileDuration?,
}

TestOutcome =
      passed
    | returned_error { error_type: TypeIdentity, value: FormattedValue? }
    | explicit_failure { message: string, source: SourceAnchor }
    | assertion_failure { assertion: AssertionFailure }
    | panicked { report: PanicReport }
    | timed_out { limit: Duration }
    | cancelled { source: CancellationSource }
    | infrastructure_failed { failure: TestInfrastructureFailure };

CapturedStream {
    policy: captured | inherited | discarded,
    bytes: byte_string?,
    truncated: bool,
    discarded_byte_count: integer?,
}

VolatileDuration {
    nanoseconds: integer,
}
```

`AssertionFailure`, `CleanupIncident`, `PanicReport`, `CancellationSource`, `TestFilter`, and `TestInfrastructureFailure` are closed
typed records with their own stable category and payload variants. Optional fields above are absent only when their associated fact
is unavailable or the selected policy does not produce it. Unknown outcome variants are never treated as passes. The report format
version governs both the binary protocol record and its JSON field contract.

Human output is a localized projection rendered through `bray-messages`. JSON output serializes the typed report directly and does
not contain pre-rendered diagnostic or failure prose.

The command succeeds only when every selected test passes and every selected host shuts down cleanly. Failed assertions,
recoverable errors, panics, cancellations, timeouts, missing outcomes, protocol failures, and cleanup or host failures produce a
nonzero command status. An empty valid selection succeeds unless the user requested a filter that must match.

## Publication And Validation

The emitted native host and catalog are one publication unit. Publication validates matching package, product, target,
configuration, host digest, runtime contract, and catalog version before making either artifact visible. Bray Tack rejects partial,
stale, or mismatched output.

Conformance coverage must prove:

- metadata-only discovery and filtering,
- synchronous and asynchronous `unit` and `Result` outcomes,
- structured assertion, panic, cancellation, timeout, and infrastructure outcomes,
- per-test stream isolation under concurrent execution,
- cleanup completion before outcome publication,
- serial barriers and bounded parallel admission,
- deterministic reports under deliberately varied completion order,
- capture and protocol resource limits,
- graceful and forced host shutdown,
- equivalent human and JSON facts without embedded English in machine data.
