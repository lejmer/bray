# Execution requirements

`blocking_execution()` and `compute_execution()` are ambient compiler-provided predicate declarations. They have no package path,
require no `using`, and can appear in ordinary `requires(...)` clauses.

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
```

`blocking_execution()` means the current execution lane permits operations that can block its operating-system thread.

`compute_execution()` means the current execution lane permits sustained CPU-bound work that would violate the progress and
latency contract of a cooperative lane.

These predicates describe execution-context facts. Source cannot establish them with an assertion, trusted boundary, witness,
ordinary predicate implementation, or user-defined value. They are established only by a language-defined execution root or a
selected runtime lane:

- a synchronous executable entrypoint root establishes both predicates for its host thread;
- every `std.thread` native entry root establishes both predicates for that dedicated operating-system thread;
- an async runtime lane establishes exactly the predicates advertised for that lane;
- a test root follows its synchronous or async product entry contract;
- a foreign callback establishes neither predicate unless its trusted ABI contract explicitly supplies a compatible execution root.

Ordinary synchronous calls inherit the current context's facts. Entering an ordinary synchronous function does not create them.

For a synchronous callable, these requirements are checked at the call as ordinary preconditions.

For an async callable, invocation does not execute the body, so these two requirements are deferred into the produced `Future<T>`
instead of being required merely to construct it. Other invocation-time preconditions remain checked at invocation according to
ordinary call rules.

Direct await checks deferred execution requirements against the current lane before execution begins. `start()` selects a lane that
satisfies them. The produced `Task<T>` preserves the requirements for product validation and runtime inspection.

A computation can require both predicates. A runtime can satisfy both with one lane or with a lane whose contract includes both
facts. Target and product validation rejects a reachable started computation when no selected runtime lane can satisfy its
requirements.

Absence of `compute_execution()` does not prove that a body is cheap. The compiler can diagnose evident unbounded or long-running
paths, but arbitrary computational cost is not decidable. Incorrect trusted or foreign contracts can also violate these guarantees
and are contract bugs at the trust boundary.

## Navigation

- [Language index](../index.md)
- [Async and concurrency index](../async-and-concurrency.md)
- Previous: [Cancellation](cancellation.md)
- Next: [Entrypoints and runtime selection](entrypoints-and-runtime.md)
