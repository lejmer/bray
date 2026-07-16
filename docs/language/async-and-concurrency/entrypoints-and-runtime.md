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
3. transfers that frame into the runtime's root task,
4. drives the root task to a terminal outcome,
5. resolves every root-owned task through ordinary structured scope cleanup,
6. maps normal result, recoverable entry failure, cancellation, and panic to the product runtime contract.

Before shutdown, the host drains the mandatory cleanup-report sink. Suppressed cleanup incidents that are not owned by a returned
`PanicReport` are therefore observable to host diagnostics even though `RunResult.Cancelled` has no source payload.

The root task is a task boundary but does not produce a source-visible `Task<T>` handle. Its normal result forms remain the
entrypoint forms defined by executable products.

An executable product selects exactly one conforming async runtime implementation and runtime ABI version when it contains an async
entrypoint or reachable task start. Libraries do not select runtimes and do not expose runtime implementation types in public
signatures.

The runtime selection declares which execution requirements it can provide. Product validation checks every reachable deferred
requirement carried by public compiled interfaces and selected implementations before emission. Runtime implementation choice does
not change the source semantics of `Future<T>`, `Task<T>`, `RunResult<T>`, cancellation, or structured cleanup.

Executables with no reachable async execution need no async runtime. Runtime worker, reactor, blocking-lane, and compute-lane
resources can be initialized lazily. A conforming product can select a single-thread runtime when its reachable contracts do not
require migratable parallel execution.

Test products form an independent root task for each async test and apply the same runtime, cleanup, and outcome rules.

## Navigation

- [Language index](../index.md)
- [Async and concurrency index](../async-and-concurrency.md)
- Previous: [Execution requirements](execution-requirements.md)
- Next: [Cross-run memory model](cross-run-memory-model.md)
