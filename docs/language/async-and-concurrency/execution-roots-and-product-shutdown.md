# Execution roots and product shutdown

Every executing Bray operation belongs to exactly one **run**. A run is the dynamic ownership and control-flow domain
that ultimately completes normally, panics, or is cancelled.

Ordinary synchronous calls and direct awaits remain in the current run. The following operations create child runs:

- `Future<T>.start()` creates a runtime-scheduled child task,
- `std.thread.start(...)` creates a child operating-system thread,
- `std.process.start(...)` creates a child operating-system process.

The corresponding owned observation values are `Task<T>`, `std.thread.Thread<T>`, and `std.process.Process<T>`. The
standard-library types are not compiler-known. Every child run has exactly one source-level owner until its terminal
outcome and payload lifecycle are resolved.

## Executable root

The host operating system or embedding environment creates the executable's process and initial operating-system thread.
Those two roots are not represented by a source-visible `std.process.Process<T>` or `std.thread.Thread<T>` value: no
Bray owner inside the same process created them and no such owner can join itself.

The product host owns one root run:

```text
host process
└── main operating-system thread
    └── executable root run
        ├── runtime-scheduled child tasks
        ├── explicitly created child threads
        └── explicitly created child processes
```

A synchronous entrypoint executes directly as the root run on the main thread.

An async entrypoint invocation creates `Future<T>`. The compiler-generated host stub transfers that frame into a
host-owned root task and drives it to terminal completion. The root task is a task run boundary but has no
source-visible `Task<T>` because the product host, rather than Bray source, owns its resolution obligation.

Runtime worker, reactor, blocking-lane, and compute-lane threads are product infrastructure. They are not source-visible
`std.thread.Thread<T>` children and cannot be joined, detached, or retained by source.

Product and runtime configuration can impose hard limits on runtime tasks, native threads, and child processes. Ordinary
`std.parallel.Budget<Domain>` values are algorithm-local concurrency bounds. They do not represent or reserve the host's
hard-limit authority. Runtime task limits bound simultaneously executing lanes and can queue ready tasks. Native-thread
and process limits surface through their ordinary recoverable creation errors. A parallel algorithm remains subject to
both its library permit and those underlying rules.

## Main-thread execution

The process's initial thread establishes `main_thread_execution()`.

A synchronous executable root additionally establishes `blocking_execution()` and `compute_execution()`.

An async executable root is driven on the runtime's distinguished main-thread lane. That lane establishes
`main_thread_execution()` but does not establish `blocking_execution()` or `compute_execution()` merely because it uses
an operating-system thread. Directly awaited computations therefore remain on the main-thread lane and must satisfy its
progress contract. CPU-bound or blocking work is moved to a compatible runtime lane with `start()` or composed through
an ordinary standard-library thread, process, or parallel algorithm.

The root task remains on the main-thread lane for its lifetime. Child tasks without a live exact-thread dependency can
migrate among compatible runtime workers. A child retaining main-thread-affine state is pinned to the main-thread lane
through the ordinary dependency and affinity rules.

`main_thread_execution()` is an execution-context requirement, not ownership of a `MainThread` handle. Standard-library
or user APIs that must execute on the initial thread state that requirement in `requires(...)`.

`std.thread.Id` is a nonforgeable observational value type with no public primary construction. `std.thread` provides
these public thread-identity declarations:

```bray
func current_id() -> Id;
func main_id() -> Id;
func is_main() -> bool;
```

These declarations belong to `std.thread`. Their values do not grant execution authority, establish
`main_thread_execution()`, keep a thread alive, or permit joining it. Two `Id` values are equal exactly when they
identify the same operating-system thread lifetime in the same process. Reuse of a target-native thread number for a
later thread does not make the identities equal.

## Current-process facilities

The current process is observed through the `std.process` identity, arguments, environment, and
startup-working-directory declarations defined by [I/O and platform services](../io-and-platform-services.md). These
operations do not manufacture an owning `std.process.Process<T>` for the current process and do not grant child-process
creation, termination, raw-handle, or shared-memory authority.

Normal process termination occurs only after the executable root run and its owned lifecycle obligations resolve. A safe
ordinary process-exit operation cannot silently bypass structured cleanup. A platform may expose an explicitly aborting
operation through a trusted standard-library contract, but that operation is catastrophic termination and does not claim
to run source lifecycle code.

## Root outcome

The product host observes the root run as if it had the following outcome:

```bray
RunResult<T>
```

This observation is a host operation rather than a source-level `Task<T>.join()`.

For an entrypoint returning `Result<unit, E>`, the root outcome is `RunResult<Result<unit, E>>`. The product contract
maps:

- `RunResult.Completed(Result.Ok(unit))` to successful completion,
- `RunResult.Completed(Result.Error(error))` to recoverable executable failure,
- `RunResult.Panicked(report)` to panic termination and panic reporting,
- `RunResult.Cancelled` to the product's interrupted or cancelled termination policy.

Returning `Result.Error(error)` transfers the error owner to the product host. The host reports the entry failure,
broadcasts cancellation to child runs owned by the error, and resolves its finalization and destruction before releasing
the root's result storage. Asynchronous cleanup runs under a cancellation shield. Cleanup failures become owned cleanup
incidents, and the terminal boundary applies the abandonment fallback while preserving represented-part obligations.
Runtime services and provider products remain available until this cleanup completes.

An `i32` normal result supplies the numeric exit result. The host never resumes the root continuation after observing a
terminal outcome.

A product-host shutdown request, such as an embedding cancellation request or a target signal mapped by product policy,
requests cooperative cancellation of the root run and wakes a suspended root task. The root observes it at the same
language-defined cancellation points as any other run. Noncooperative synchronous or foreign work can delay graceful
product shutdown. An unmaskable operating-system termination is catastrophic host termination and is outside the source
lifecycle guarantee.

Source in the root run uses the ordinary propagation rules. A synchronous main can forward a child-thread outcome:

```bray
let value = try thread.join();
```

An async main can separate recoverable process infrastructure failure from the child run outcome:

```bray
let child_run = try await process.join();
let value = try child_run;
```

The first `try` propagates `Result.Error` through main's lexical `Result` boundary. The second forwards child panic or
cancellation into the executable root run. As an alternative to the second line, `catch (try child_run)` can recover the
forwarded panic as `Result<T, PanicReport>` but does not catch cancellation.

## Structured product shutdown

The root body first produces an outcome candidate. Before that outcome becomes terminal, the compiler-generated root
frame executes the root lexical cleanup plan:

1. Broadcast cancellation to every root-scope owned unresolved task.
2. Perform ordinary reverse lifecycle resolution, including task observation and the checked finalizers of ordinary
   standard-library thread, process, budget, and synchronization owners.
3. Complete shielded abnormal cleanup and attach or transfer every suppressed cleanup incident.
4. Replace the outcome candidate with a panic outcome when cleanup panics according to the ordinary
   primary-and-suppressed panic rules.
5. Commit and publish the final root terminal record only after root lexical cleanup is complete.

The product host does not rediscover source owners, inspect standard-library type names, or perform a second source
lifecycle pass. It observes the already-final root terminal record, takes ownership of any `Completed(T)` payload, maps
or reports the outcome, and resolves that payload under the product contract.

After root terminal observation, the host closes new source entries, foreign entries, callbacks, and native-thread
attachments for the active
[teardown set](../declarations/static-storage-declarations.md#entry-closure-and-product-cleanup). It waits for every
in-flight entry and external root that can reach a domain in that set. A dependency owned by a static scheduled for
cleanup is an internal lifecycle edge, not an external root. Each eligible attached native thread then cleans its
thread-local static domain on that exact thread and detaches. The host cleans product-static domains after their
consumer domains and before any retained provider domain.

The mandatory cleanup-report sink, scheduler, required execution lanes, allocator, platform services, loader, and
retained provider products remain available throughout static cleanup. The host drains static cleanup incidents, shuts
down runtime infrastructure, resolves host-owned process resources, and returns control to the embedding environment
only afterward.

The root run ends at terminal publication. Product shutdown follows terminal observation and is not part of the root
run.

Entry closure does not invalidate live product or exact-thread dependencies. A root outside the teardown set blocks
every reached provider from cleanup and unload. A consumer static inside the set releases its provider edge during
dependency-ordered cleanup, so the host does not wait for that edge before cleaning the consumer. A dynamically loaded
product unloads only after external roots have resolved and every scheduled consumer domain has released its internal
edge.

The host does not silently detach source-owned work during shutdown. A long-lived child can outlive an inner lexical
block only by moving its owning value to a valid enclosing source owner. It cannot outlive the executable root unless an
external process has explicitly ceased to be a child of the Bray product under a separately specified operating-system
handoff contract.

## Navigation

- [Language index](../index.md)
- [Async and concurrency index](../async-and-concurrency.md)
- Previous: [Execution requirements](execution-requirements.md)
- Next: [Entrypoints and runtime selection](entrypoints-and-runtime.md)
