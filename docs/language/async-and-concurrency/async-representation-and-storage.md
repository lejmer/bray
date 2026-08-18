# Async representation and storage

`Future<T>` has protected representation. Source can name its completion type `T`, move the value, and call its
language-defined methods, but cannot name, inspect, construct, project, lay out, or depend on the concrete frame type.

Each async callable has a compiler-generated frame representation containing:

- a control state identifying the next resumable region,
- receiver and argument state not yet consumed by the body,
- locals live across suspension,
- active child computations composed by direct await,
- cleanup state needed for cancellation, panic, and lifecycle resolution,
- implementation metadata required to resume, cancel, move, complete, and destroy the frame.

The source type `Future<T>` is an owned compiler-managed existential over that hidden frame representation. Two async
callables with the same declared result type can have different frame layouts while both invocation expressions have
source type `Future<T>`.

Ordinary control-flow merges, parameters, returns, and homogeneous aggregates can therefore contain `Future<T>` values
from different producers. The compiler uses closed result-place storage when all representations are known and an erased
descriptor plus suitably owned backing storage when they are not. Representation erasure can require dynamic storage.
Direct await of a known producer does not.

The hidden frame identity and its size, alignment, move, resume, cancellation, result-move, and destruction operations
are portable compiled-interface metadata. They are not source declarations, generic arguments, reflection results, or
name-resolution entries.

## Storage rules

Calling an async callable does not semantically require heap allocation. A compiler can place an inactive frame in
caller-provided result storage, an enclosing async frame, stack storage, runtime-managed storage, or another location
that preserves the observable ownership and lifecycle rules.

Direct await does not semantically create a task, enqueue work, or allocate a task control block. A compiler can compose
the child frame directly into the current frame and resume it without scheduler mediation.

Before execution begins, an inactive frame can be relocated through ordinary movement of its `Future<T>` owner. Once
execution has begun, any state whose generated representation requires a stable address remains stable until that state
is completed or destroyed. This stability rule is enforced by generated code and is not exposed through a source-level
pinning type or operation.

Calling `start()` is the independent-execution storage boundary. The runtime obtains stable task-owned storage, moves
the inactive frame into that storage before its first resume, creates the task control state, and returns the
source-level `Task<T>` owner. A runtime can co-allocate the task control state and frame. Allocation strategy is not
observable language behavior.

Recursive async execution whose dynamically suspended depth is not statically bounded requires dynamic storage for the
recursive activations. The compiler can use indirect frames, segmented frame storage, a task arena, tail-recursion
transformation where valid, or another representation. Source never introduces boxing or pinning solely to make async
recursion well formed.

## Cost transparency

Compiler inspection information for an async callable must make these properties available:

- total frame size and alignment for each concrete compiled instantiation,
- values retained across each suspension point,
- the largest retained values and their source locations,
- whether starting requires task-owned dynamic storage,
- dynamic-storage sites introduced for recursion or representation erasure,
- execution requirements and thread-affinity constraints.

This information supports compiler inspection and diagnostics. It is not a source-visible layout guarantee. Optimization
can change it without changing the callable's source contract unless an explicit ABI rule states otherwise.

## Navigation

- [Language index](../index.md)
- [Async and concurrency index](../async-and-concurrency.md)
- Previous: [Async functions and computations](async-functions-and-computations.md)
- Next: [Captured state](captured-state.md)
