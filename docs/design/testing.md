# Testing standard library and runner

Test products compile checked entries into a native host and an immutable catalog. Discovery uses that catalog without
reparsing source or executing tests. Sequential and bounded parallel runs share the same protocol and lifecycle model.
[Language rules](../language/modules-and-packages/test-products-and-entries.md) own test declaration and outcome
semantics.

## Ownership

| Component                  | Responsibility                                                         |
|----------------------------|------------------------------------------------------------------------|
| Compilation                | Checked entries and their catalog identities                           |
| Test protocol              | Shared catalog, invocation, event, outcome, and report data            |
| Lowering, codegen, emitter | Generated host and matching published artifacts                        |
| Runtime test-host adapter  | Root execution, cancellation, capture, and cleanup completion          |
| Platform                   | Processes, pipes, clocks, waiting, and hard termination                |
| Bray Tack                  | Workspace selection, filtering, global budgets, admission, and reports |
| `bray-messages`            | Localized presentation                                                 |
| `std.testing`              | Ordinary source-level testing helpers                                  |

Compiler fixture helpers in `bray-testing` are separate from this production protocol. Ordinary runtime adapters have no
test protocol dependency. Integration-test products consume their library's emitted public contract as a dependency.

## Catalog and publication

A catalog identifies entries, their source origins, generated host entries, result shapes, execution constraints, and
runtime requirements. It is a versioned compiler artifact tied to its product and host, not an importable package
interface. Stable entry identity determines discovery, filtering, sharding, and final report order.

Host, catalog, and complete build provenance publish as one unit. Provenance covers source and project inputs as well as
compiler, toolchain, library, runtime, and protocol identities. Retained execution validates this evidence and pins the
selected generation through host shutdown. Once resolved, retained and freshly built hosts use the same runner path.

## Protocol and admission

Dedicated bounded protocol channels remain separate from test stdout and stderr. A handshake establishes host and
catalog identity, compatibility, capabilities, and limits before execution. Structured framing prevents arbitrary test
output from becoming control data, and decoding validates resource bounds before allocation.

Tack sends an immutable selection plan and owns command-wide admission. Hosts execute admitted entries within local
capacity but do not independently select work. Serial entries form global barriers across products. Parallel and
sequential execution use the same scheduler with different limits.

Per-test event sequences preserve attribution while allowing concurrent completion. Final reports use catalog order.
Timing and live event order are explicit observations, separate from reproducible artifact and selection data.

## Invocation and cleanup

Each entry has its own panic-catching root. The generated host invokes synchronous entries directly and drives async
entries through the selected runtime. Outcomes retain typed recoverable errors, assertions, panics, cancellation,
timeouts, and infrastructure causes without making formatting support a condition for test validity.

Terminal publication follows child resolution, finalization, destruction, and incident handling. Fixtures are ordinary
values with ordinary lifecycle semantics. Shared fixtures use explicit synchronization rather than hidden injection or a
second lifecycle system. Assertion reporting uses already evaluated values and never reevaluates an expression to
explain failure.

Cancellation and timeout request ordinary cooperative cancellation and permit cleanup to finish. Host hard termination
is a containment fallback and reports unresolved work as infrastructure failure. It cannot claim successful cleanup.

## Capture and reporting

Each root receives independent bounded stream sinks inherited by its child work. Stream synchronization follows the same
operation boundary as ordinary standard streams, while capture storage owns its own synchronization. Truncation records
loss explicitly and does not redefine the test outcome.

The protocol and machine report preserve typed outcomes, capture policy, cleanup incidents, and host shutdown state.
Human output is a localized projection. Command policy controls filtering, fail-fast admission, display, and exit status
while already admitted work still has to resolve.

## Related documents

- [Bray Tack](bray-tack.md)
- [Emission](emitter.md)
- [Async runtime](async-runtime.md)
- [Testing helpers](../language/modules-and-packages/test-products-and-entries.md#standard-library-helpers)
- [Compiler test contributor guidance](../contributing/testing.md)
