# Execution requirements

`blocking_execution()`, `compute_execution()`, and `main_thread_execution()` are ambient compiler-provided predicate declarations.
They have no package path, require no `using`, and can appear in ordinary `requires(...)` clauses.

```bray
func read_blocking(pos file: &File) -> Data
    requires(blocking_execution())
{
    ...
}

async func read(pos file: &File) -> Data
    requires(blocking_execution())
{
    return read_blocking(file);
}

func dispatch_application_event(pos event: Event)
    requires(main_thread_execution())
{
    ...
}
```

`blocking_execution()` means the current execution lane permits operations that can block its operating-system thread.

`compute_execution()` means the current execution lane permits sustained CPU-bound work that would violate the progress and
latency contract of a cooperative lane.

`main_thread_execution()` means the current execution lane is the executable process's distinguished initial operating-system
thread.

These predicates describe execution-context requirements. Source cannot establish them with an assertion, trusted boundary, witness,
ordinary predicate implementation, or user-defined value. They are established only by a language-defined execution root or a
selected runtime lane:

- a synchronous executable entrypoint root establishes all three predicates for its host thread.
- an async executable entrypoint root establishes `main_thread_execution()` on its distinguished cooperative main-thread lane.
- every `std.thread` native entry root establishes `blocking_execution()` and `compute_execution()` for that dedicated
  operating-system thread, but not `main_thread_execution()`.
- an async runtime lane establishes exactly the predicates advertised for that lane.
- a test root follows its synchronous or async product entry contract.
- a foreign callback establishes none of the predicates unless its trusted ABI contract explicitly supplies a compatible execution
  root.

Ordinary synchronous calls inherit the current context's conditions. Entering an ordinary synchronous function does not create them.

For a synchronous callable, these requirements are checked at the call as ordinary preconditions.

For an async callable, invocation does not execute the body, so these execution requirements are deferred into the produced `Future<T>`
instead of being required merely to construct it. Other invocation-time preconditions remain checked at invocation according to
ordinary call rules.

Direct await checks deferred execution requirements against the current lane before execution begins. `start()` selects a lane that
satisfies them. The produced `Task<T>` preserves the requirements for product validation and runtime inspection.

A computation can require any compatible combination of the predicates. A runtime can satisfy multiple requirements with one lane
whose contract includes all required conditions. Target and product validation rejects a reachable started computation when no selected
runtime lane can satisfy its requirements.

The distinguished async main-thread lane is intentionally not assumed to permit blocking or sustained compute work. Being backed by
an operating-system thread does not waive the runtime's progress contract. A direct await on that lane is rejected when its
computation requires either unavailable condition. Starting the computation routes it to another compatible lane when one exists.

Absence of `compute_execution()` does not prove that a body is cheap. The compiler can diagnose evident unbounded or long-running
paths, but arbitrary computational cost is not decidable. Incorrect trusted or foreign contracts can also violate these guarantees
and are contract bugs at the trust boundary.

## Navigation

- [Language index](../index.md)
- [Async and concurrency index](../async-and-concurrency.md)
- Previous: [Cancellation](cancellation.md)
- Next: [Execution roots and product shutdown](execution-roots-and-product-shutdown.md)
