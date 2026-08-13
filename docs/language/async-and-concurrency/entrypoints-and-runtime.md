# Entrypoints and runtime selection

An executable can use an async entrypoint:

```bray
@entrypoint
async func main() -> Result<unit, AppError>
{
    ...
}
```

The function body is the root lexical task scope. The compiler does not insert a source-level async block or require a source-level
runtime value.

For an async entrypoint, the compiler emits a host entry stub that:

1. validates and initializes the executable product's selected runtime implementation,
2. invokes the async entrypoint to create its root `Future<T>`,
3. transfers that frame into the runtime's host-owned root task on the distinguished main-thread lane,
4. drives the root body to an outcome candidate,
5. lets the generated root frame complete root lexical task broadcast and ordinary lifecycle resolution,
6. observes the final `RunResult<T>`-equivalent terminal record,
7. maps normal result, recoverable entry failure, cancellation, and panic to the product runtime contract,
8. drains cleanup reports and shuts down runtime infrastructure only after no source run can use it.

Before shutdown, the host drains the mandatory cleanup-report sink. Suppressed cleanup incidents that are not owned by a returned
`PanicReport` are therefore observable to host diagnostics even though `RunResult.Cancelled` has no source payload.

The root task is a task boundary but does not produce a source-visible `Task<T>` handle. Its normal result forms remain the
entrypoint forms defined by executable products. The host's terminal observation is equivalent to observing `RunResult<T>`, but it
is not a source-level join and cannot resume the root continuation. The host does not search for or re-resolve source-owned tasks,
threads, or processes after publication. Those obligations were already handled by the generated root frame's checked lexical
cleanup.

The root task remains on the distinguished main-thread lane for its lifetime. That lane establishes
`main_thread_execution()` and does not automatically establish `blocking_execution()` or `compute_execution()`. Explicit child
tasks can execute on other compatible lanes, including in parallel.

An executable product selects exactly one conforming async runtime implementation and runtime ABI version when it contains an async
entrypoint or reachable task start. Libraries do not select runtimes and do not expose runtime implementation types in public
signatures.

The runtime selection declares which execution requirements it can provide. Product validation checks every reachable deferred
requirement carried by public compiled interfaces and selected implementations before emission. Runtime implementation choice does
not change the source semantics of `Future<T>`, `Task<T>`, `RunResult<T>`, cancellation, or structured cleanup.

Executables with no reachable async execution need no async runtime. Runtime worker, reactor, main-thread-lane, blocking-lane, and
compute-lane resources can be initialized lazily. A conforming product can select a single-thread runtime when its reachable
contracts do not require migratable parallel execution.

Test products form an independent root task for each async test and apply the same runtime, cleanup, and outcome rules.

## Navigation

- [Language index](../index.md)
- [Async and concurrency index](../async-and-concurrency.md)
- Previous: [Execution roots and product shutdown](execution-roots-and-product-shutdown.md)
- Next: [Cross-run memory model](cross-run-memory-model.md)
