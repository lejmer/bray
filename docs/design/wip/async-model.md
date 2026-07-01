# Async model

## Overview

Bray treats asynchronous execution and run-boundary observation as part of the core ownership and control-flow model.

The core async and run-boundary forms are:

- async functions,
- async computations,
- `await` expressions,
- `async` block expressions,
- `spawn` expressions,
- `spawn detached` expressions,
- `spawn thread` expressions,
- task handles,
- thread handles,
- task and thread observation through `catch`.

An async computation is an owned value. It can own or borrow state, hold capabilities, carry effects, and carry finalization
obligations.

Spawned work is structured by default. A non-detached spawned task belongs to the nearest enclosing `async` block until the task is
completed, cancelled, or transferred to another owner.

Detached work is explicit. `spawn detached` creates a task whose lifetime is represented by the returned task handle and whose
captured state must be detached-safe.

Thread work is explicit. `spawn thread` creates a thread whose lifetime is represented by the returned thread handle and whose
captured state must be valid for thread execution.

---

## Async functions

An async function uses the `async` modifier:

```bray
async func fetch(pos url: Url) -> Result<Response, FetchError>
{
    ...
}
```

Calling an async function evaluates the call arguments and creates an async computation value.

Calling an async function does not create a task.

Calling an async function does not by itself run the function body to completion.

The async computation's declared result type is the async function's declared result type.

The async computation owns or borrows the state supplied to the call according to the function signature, argument forms, body,
suspension points, capability requirements, and lifetime contract.

An async computation is not copyable.

Moving an async computation transfers responsibility for completing, spawning, cancelling, or otherwise resolving it.

Destroying an incomplete async computation cancels it, destroys owned captured state, and releases held capabilities according to
the async computation's contract.

---

## Captured State

Suspension captures every live value, borrow, capability, effect, fact dependency, and finalization obligation needed to resume the
async computation.

A value moved into an async computation is owned by that computation until the value is returned, moved elsewhere, destroyed, or
transferred into a spawned task.

A borrow captured by an async computation remains active for the computation's lifetime.

A mutable borrow captured by an async computation remains exclusive for the computation's lifetime.

A scoped capability captured by an async computation remains held for the computation's lifetime.

An async computation cannot outlive a borrowed value or scoped capability it captures.

The compiler tracks captured state across suspension, movement, await, spawn, cancellation, destruction, and task transfer.

---

## Await Expressions

The await expression is:

```bray
await expression
```

The operand is evaluated exactly once.

The operand must produce an async computation.

`await` consumes the async computation and drives it to completion in the current execution flow.

If the async computation completes normally with a value of type `T`, the await expression has type `T`.

If the async computation panics, the panic propagates to the nearest panic-catching boundary.

Awaiting an async computation directly is not task observation and does not produce `RunResult<T>`.

If the current async computation or async block is cancelled while an await is active, the awaited computation is cancelled according
to its cancellation contract.

`await` requires async execution capability.

Async function bodies have async execution capability.

`async` block expressions have async execution capability.

`try await expression` means `try (await expression)`.

---

## Async Block Expressions

An async block expression is a block expression with the `async` modifier:

```bray
async
{
    ...
}
```

An async block expression introduces an ordinary block scope.

An async block expression also introduces a structured async ownership boundary for tasks spawned inside the block.

An async block expression is not a task.

An async block expression is not detached work.

An async block expression is evaluated as part of the current execution flow.

An async block expression can use `await`.

An async block expression can use non-detached `spawn`.

An async block expression can produce `unit`, `never`, or another value type according to ordinary block-expression rules.

In value-producing context, an async block expression receives its value through `yield`.

Local bindings introduced inside an async block are scoped to that block.

Owned values whose ownership remains in the async block are destroyed when the async block exits.

Tasks owned by the async block are task obligations. Every non-panic source-level path that leaves the async block must complete,
cancel, or transfer each task obligation owned by the block.

If a panic reaches an async block while the block owns live task obligations, the async block cancels those tasks before the panic
continues outward.

---

## Spawn Expressions

The non-detached spawn expression is:

```bray
spawn expression
```

The operand is evaluated exactly once.

The operand must produce an async computation.

If the async computation's declared result type is `T`, the spawn expression produces `Task<T>`.

`Task<T>` is the compiler-known linear task handle type for a spawned asynchronous computation whose ordinary result type is `T`.

`spawn` consumes the async computation and schedules it as a task.

`spawn` requires async execution capability.

A non-detached `spawn` expression is valid only inside an `async` block.

The spawned task initially belongs to the nearest enclosing `async` block.

The nearest enclosing `async` block owns the task obligation until the task is joined, cancelled, or transferred.

The spawned task owns all state moved into the async computation.

The spawned task borrows all state borrowed by the async computation.

The spawned task holds all scoped capabilities held by the async computation.

While the spawned task is live, captured borrows and capabilities remain active.

---

## Detached Spawn Expressions

The detached spawn expression is:

```bray
spawn detached expression
```

The operand is evaluated exactly once.

The operand must produce an async computation.

If the async computation's declared result type is `T`, the detached spawn expression produces `Task<T>`.

`spawn detached` consumes the async computation and schedules it as a task whose lifetime is represented by the returned task
handle.

`spawn detached` requires async execution capability.

`spawn detached` is valid only when the spawned computation is detached-safe.

A detached-safe computation captures only:

- owned values,
- copied values,
- shared state whose lifetime is independent of the current block,
- capabilities whose contract permits transfer to detached execution.

A detached task cannot capture:

- borrows of local storage from the current block,
- borrows of local storage from an enclosing block whose lifetime is not carried by the task handle,
- scoped capabilities that must be released by the current block,
- finalization obligations that must be completed before the current block exits.

The returned task handle is still a linear obligation. It must be joined, cancelled, or transferred before the owning scope exits.

---

## Task Handles

`Task<T>` is an owned task handle.

`Task<T>` is not copyable.

Moving a `Task<T>` transfers the task obligation.

A task handle can be joined, cancelled, or moved to transfer the task obligation to another owner.

The join operation consumes the task handle.

Task joins are observed with `catch`:

```bray
let result: RunResult<T> = catch task.join();
```

A task join expression is valid only as the operand of `catch`.

If the task completed normally with a value of type `T`, `catch task.join()` produces `RunResult.Completed(value)`.

If the task panicked, `catch task.join()` produces `RunResult.Panicked(report)`.

If the task was cancelled before normal completion, `catch task.join()` produces `RunResult.Cancelled`.

Joining a task resolves the task obligation regardless of which `RunResult` variant is produced.

The cancellation operation consumes the task handle:

```bray
task.cancel();
```

Cancelling a task drives cancellation to completion according to the task contract.

Cancelling a task destroys owned captured state and releases captured capabilities according to ordinary destruction and
finalization rules.

Cancelling a task resolves the task obligation.

---

## Task Obligation Checking

The compiler tracks live task obligations in the same control-flow state as ownership, initialization, movement, destruction,
finalization, borrow, and capability state.

Each `spawn` creates one live task obligation.

Each `spawn detached` creates one live task obligation owned by the returned task handle.

The obligation is resolved when the task is joined or cancelled.

The obligation is transferred when the task handle is moved to another valid owner.

Every non-panic source-level path that exits an `async` block must leave no unresolved task obligation owned by that block.

Source-level exits that require resolved task obligations include:

- natural block completion,
- `yield`,
- `return`,
- `continue`,
- nullable propagation,
- result propagation,
- run-result propagation.

An explicit `panic` expression inside an async block cancels task obligations owned by that block before the panic propagates.

An exit path can transfer a task obligation as part of the exit by yielding or returning the task handle.

After a task handle is transferred, the old owner no longer has the task obligation.

At control-flow merge points, task-obligation state must be coherent. A task handle cannot be joined, cancelled, transferred, or
used on a path where that handle is no longer live.

```bray
async
{
    let task = spawn hash_file(file);

    if cancelled
    {
        task.cancel();
        yield Result.Error(HashError.cancelled);
    }

    let result = try catch task.join();
    yield result;
}
```

This is invalid because the `yield` path leaves the async block while `task` is still owned by the block:

```bray
async
{
    let task = spawn hash_file(file);

    if cancelled
    {
        yield Result.Error(HashError.cancelled);
    }

    let result = try catch task.join();
    yield result;
}
```

---

## Transfers And Escapes

Moving a task handle transfers the task obligation to the destination owner.

Task-handle destinations include:

- local bindings,
- fields,
- variant payloads,
- array elements,
- tuple elements,
- function arguments,
- returned values,
- values supplied by `yield`.

A task handle can be transferred only when the destination owner can legally own the task and every state item captured by the
task.

A task that captures only owned values and detached-safe capabilities can escape the creating async block through ordinary
ownership transfer.

A task that captures a borrow can escape only to an owner whose lifetime is proven not to outlive the borrowed storage.

A task that captures a scoped capability can escape only to an owner whose contract assumes responsibility for releasing that
capability before the capability's source scope exits.

A task that captures local storage from its creating async block cannot escape that async block.

A declaration that returns, stores, or otherwise exposes a task handle with non-local dependencies preserves those dependencies
through the task handle's inferred lifetime and capability dependency contract.

If the destination type or declaration contract does not preserve the task handle's capture requirements, the transfer is rejected.

```bray
func start(pos data: &Data) -> Task<Result<Hash, HashError>>
{
    return async
    {
        let task = spawn hash(data);
        yield task;
    };
}
```

This is invalid unless the returned task handle's dependency contract preserves the borrow of `data`.

```bray
func start(pos data: Data) -> Task<Result<Hash, HashError>>
{
    return async
    {
        let task = spawn hash(data);
        yield task;
    };
}
```

This is valid because the task owns `data`.

---

## Thread Spawn Expressions

The thread spawn expression is:

```bray
spawn thread expression
```

The operand is evaluated exactly once.

The operand must produce a synchronous callable value that can be called with no runtime arguments.

If the callable value's result type is `T`, the thread spawn expression produces `Thread<T>`.

`Thread<T>` is the compiler-known linear thread handle type for a spawned synchronous thread whose ordinary result type is `T`.

`spawn thread` consumes the callable value and schedules it as a thread outside the current execution flow.

The callable value's captured state becomes captured thread state.

The thread handle carries the dependency contract of the captured thread state and the thread obligation represented by the
handle.

`spawn thread` is valid in synchronous and asynchronous execution contexts when thread creation is available for the target.

The source evaluation order guarantees operand evaluation and thread-handle creation.

Thread entry scheduling is governed by the runtime and target thread model.

The created thread can start before or after the creating execution flow continues past the `spawn thread` expression.

Inter-thread visibility and synchronization are governed by the cross-task and cross-thread memory model.

The thread entry callable must be synchronous.

An async callable produces an async computation and is spawned as a task through `spawn` or `spawn detached`.

Example:

```bray
let handle: Thread<Hash> = spawn thread lambda () -> Hash
{
    return hash(data);
};
```

---

## Thread Captures

Thread captures are the captured state of the callable value supplied to `spawn thread`.

A thread entry can capture:

- owned values whose type contract permits transfer to a thread,
- copied values whose type contract permits use by a thread,
- borrows whose lifetime is preserved by the thread handle's dependency contract and whose access contract permits thread use,
- capabilities whose contract permits transfer to or use by a thread,
- finalization obligations whose contract can be completed by the thread or preserved by the thread handle.

The thread handle preserves every captured-state dependency until the thread is joined, cancelled, or transferred to another owner.

A thread that captures a borrow can escape only to an owner whose lifetime is proven not to outlive the borrowed storage.

A thread that captures a scoped capability can escape only to an owner whose contract assumes responsibility for resolving that
capability before the capability's source scope exits.

A thread that captures local storage from its creating scope can escape that scope only when the destination preserves the captured
dependency contract.

If the destination type or declaration contract does not preserve the thread handle's capture requirements, the transfer is
rejected.

---

## Thread Handles

`Thread<T>` is an owned thread handle.

`Thread<T>` is not copyable.

Moving a `Thread<T>` transfers the thread obligation.

A thread handle can be joined, cancelled when its contract permits cancellation, or moved to transfer the thread obligation to
another owner.

The join operation consumes the thread handle.

Thread joins are observed with `catch`:

```bray
let result: RunResult<T> = catch thread.join();
```

A thread join expression is valid only as the operand of `catch`.

If the thread completed normally with a value of type `T`, `catch thread.join()` produces `RunResult.Completed(value)`.

If the thread panicked, `catch thread.join()` produces `RunResult.Panicked(report)`.

If the thread was cancelled before normal completion, `catch thread.join()` produces `RunResult.Cancelled`.

Joining a thread resolves the thread obligation regardless of which `RunResult` variant is produced.

Thread cancellation is valid when the thread handle's contract permits cancellation.

The cancellation operation consumes the thread handle:

```bray
thread.cancel();
```

Thread cancellation reaches completion through cancellation points and operation contracts declared by the thread entry and the
values it uses.

Cancelling a thread destroys owned captured state and releases captured capabilities according to ordinary destruction and
finalization rules.

Cancelling a thread resolves the thread obligation.

---

## Thread Obligation Checking

The compiler tracks live thread obligations in the same control-flow state as ownership, initialization, movement, destruction,
finalization, borrow, capability, and task-obligation state.

Each `spawn thread` creates one live thread obligation owned by the returned thread handle.

The obligation is resolved when the thread is joined or cancelled.

The obligation is transferred when the thread handle is moved to another valid owner.

Every non-panic source-level path that exits a scope owning a thread handle must leave no unresolved thread obligation owned by
that scope.

An exit path can transfer a thread obligation as part of the exit by yielding, returning, assigning, or otherwise moving the thread
handle into a valid destination.

After a thread handle is transferred, the old owner no longer has the thread obligation.

At control-flow merge points, thread-obligation state must be coherent. A thread handle cannot be joined, cancelled, transferred,
or used on a path where that handle is no longer live.

---

## Tasks And Threads

Tasks and threads are both run boundaries.

Both are represented by linear owned handles after spawning.

Both are observed through `catch handle.join()` as `RunResult<T>`.

Both record panic at the run boundary and report it as `RunResult.Panicked(report)`.

Both report completed cancellation as `RunResult.Cancelled`.

A task runs an async computation.

A thread runs a synchronous callable value.

Non-detached task spawning is structured by the nearest enclosing `async` block.

Thread spawning is represented directly by the returned `Thread<T>` handle.

Task spawning with `spawn` requires async execution capability.

Thread spawning with `spawn thread` requires thread creation availability for the target.

Await drives an async computation in the current execution flow.

Join observes a task or thread run boundary from its handle.

---

## Cancellation

Cancellation is an exit path for an incomplete async computation, task, or thread.

Cancelling an async computation destroys owned captured state and releases held capabilities according to the computation's
contract.

Cancelling a task consumes the task handle and resolves the task obligation.

Cancelling a thread consumes the thread handle and resolves the thread obligation.

Cancellation of a task observed through `catch task.join()` produces `RunResult.Cancelled`.

Cancellation of a thread observed through `catch thread.join()` produces `RunResult.Cancelled`.

Cancellation does not produce the task's or thread's ordinary result type.

Cancellation must satisfy all ownership, destruction, finalization, borrow, capability, and effect obligations of the cancelled
state.

---

## Cross-Task And Cross-Thread Memory Model

Tasks and threads are concurrent runs when their execution can overlap with another live run.

An async computation that is only awaited in the current execution flow is not a concurrent run by itself.

A task becomes a concurrent run when it is spawned.

A thread becomes a concurrent run when it is spawned.

Within one run, ordinary expression evaluation order defines the order of memory effects.

Between concurrent runs, memory effects are ordered only by language-defined synchronization edges.

A synchronization edge is created by:

- transferring captured state into a spawned task or thread before that run can observe the state,
- joining a task or thread,
- completing task or thread cancellation,
- operations whose type or declaration contract explicitly synchronizes access,
- atomic operations according to their ordering contract,
- trusted runtime declarations whose safe contract establishes synchronization.

Moving a task handle or thread handle transfers the obligation represented by that handle.

Moving a handle does not by itself create a synchronization edge with the run owned by the handle.

Joining a task or thread creates a completion edge from the joined run to the continuation after the `catch handle.join()`
expression.

Completing cancellation creates a completion edge from the cancelled run's cleanup path to the continuation after the cancellation
operation.

---

## Data-Race Prevention

A data race is a pair of potentially overlapping accesses from different concurrent runs where:

- both accesses can reach the same storage or overlapping storage,
- at least one access mutates, initializes, reinitializes, moves, destroys, finalizes, replaces an active variant, changes nullable
  state, writes through raw memory, or performs another operation whose contract modifies storage,
- the accesses are not ordered by a synchronization edge,
- and the accesses are not both mediated by a valid atomic or synchronization contract for the reached storage.

A Bray program with a data race is invalid.

Safe Bray source must be data-race-free by construction.

If the compiler cannot prove that potentially overlapping concurrent accesses are disjoint, ordered, immutable, or mediated by a
valid synchronization contract, the program is rejected.

Moving owned state into a spawned task or thread transfers exclusive ownership of that state to the spawned run.

After the move, the creating run has no access path that owns the moved state.

Copying a value into a spawned task or thread is valid only when the copied value's type contract permits use in that run boundary.

Copyability does not imply cross-thread sharing, detached execution safety, atomic access, or synchronized interior mutation.

Capturing a shared borrow into a concurrent run keeps the shared borrow active for the lifetime carried by the task or thread
handle.

While that shared borrow is active, incompatible mutation, movement, destruction, finalization, reinitialization, or variant
replacement of the reached storage remains suspended in every run.

Capturing a mutable borrow into a concurrent run transfers exclusive mutation authority to that run for the lifetime carried by the
task or thread handle.

No other run can observe, mutate, move, destroy, finalize, reinitialize, or otherwise incompatibly access the reached storage until
the mutable borrow is resolved.

Raw pointers do not create an exception to the data-race rule.

Raw memory access across concurrent runs requires the ordinary raw-memory trusted facts plus a synchronization or atomic contract
covering the reached storage.

---

## Shared State And Synchronization

Shared mutable state is valid only when access is mediated by a type or declaration contract that defines the synchronization
behavior for that state.

A synchronization contract must identify:

- the storage or resource it protects,
- the access capability granted while synchronization is held,
- whether the access is shared observation, exclusive mutation, transfer, or atomic mutation,
- the lifetime of the granted capability,
- the operations that acquire and release the synchronization,
- the synchronization edges established by those operations,
- the cancellation, panic, destruction, and finalization behavior while the synchronization is held.

Scoped synchronization fits the ordinary `enter` and `exit` lifecycle model.

The `enter` declaration produces a scoped capability that grants access to the protected storage.

The matching `exit` declaration releases that scoped capability and establishes the release behavior declared by the type.

The scoped capability cannot escape its valid scope unless its type contract explicitly preserves the protected storage,
synchronization state, and dependency contract.

Library synchronization declarations are ordinary declarations.

They become meaningful to the compiler through their language-defined or recognized standard-library contracts, not through special
call syntax.

---

## Atomic Operation Contracts

An atomic operation is an operation whose declaration contract states that it atomically observes or mutates a specific storage
location.

Atomic operations can be compiler-known declarations, compiler-provided declarations, or recognized standard-library declarations.

An atomic operation contract must state:

- the storage reached by the operation,
- the element type, size, alignment, initialization, and target-availability requirements,
- whether the operation reads, writes, or read-modify-writes,
- the value produced by the operation, if any,
- the memory ordering used by the operation,
- the failure ordering for compare-exchange style operations,
- the trusted facts and trusted implementation capabilities required when the operation reaches raw memory or target intrinsics,
- the panic, cancellation, destruction, finalization, and fact-invalidation behavior.

The required atomic ordering meanings are:

- relaxed ordering: the operation is atomic but creates no synchronization edge,
- acquire ordering: later effects in the acquiring run can observe effects published by a matching release edge,
- release ordering: earlier effects in the releasing run are published to a matching acquire edge,
- acquire-release ordering: the operation has both acquire and release behavior,
- sequentially consistent ordering: the operation participates in one language-defined total order of sequentially consistent
  atomic operations in addition to its acquire or release behavior.

A compare-exchange failure ordering cannot include release behavior because the failing operation does not write the target storage.

Atomic access to one storage location does not make ordinary non-atomic access to the same storage valid while concurrent access can
overlap.

Atomic access to one field, element, or raw location does not authorize concurrent movement, destruction, finalization,
reinitialization, or variant replacement of the owning value unless the owning type contract explicitly permits that operation.

---

## Cancellation And Memory Visibility

Cancellation requests are observed by tasks and threads through cancellation points and operation contracts.

A cancellation request does not grant direct access to the cancelled run's captured storage.

Cancellation does not interrupt an atomic operation at a partial state.

Cancellation does not interrupt a non-cancellable operation at an arbitrary source point.

When cancellation completes, all destruction, finalization, capability release, and synchronization behavior required by the
cancelled state has completed or has been transferred according to the cancelled run's contract.

After cancellation completes, the cancelling run observes the completion edge produced by cancellation.

If cancellation runs cleanup while a scoped synchronization capability is held, cleanup must release that capability according to
the same `exit`, destruction, and finalization rules that apply to ordinary scope exit.

---

## Capability Transfer Across Run Boundaries

Every value captured by a spawned task or thread carries its dependency contract across that run boundary.

A capability can cross a run boundary only when its contract permits use in that kind of run.

A capability contract can permit:

- use inside an async computation,
- transfer into a structured task,
- transfer into a detached task,
- transfer into a thread,
- copying into a run boundary,
- movement into a run boundary,
- sharing across concurrent runs through a synchronization contract.

If a capability is tied to the creating run, creating scope, stack storage, local resource scope, target thread, executor, or
foreign callback context, the capability cannot cross a run boundary unless its contract preserves that dependency through the task
or thread handle.

Detached tasks require captured capabilities whose contracts permit detached execution and whose lifetime and release obligations
are independent of the creating async block or are preserved by the returned task handle's dependency contract.

Threads require captured capabilities whose contracts permit thread execution.

When a task or thread handle is transferred, the handle's dependency contract and obligation transfer with it.

The destination must preserve every lifetime, capability, synchronization, cancellation, destruction, and finalization requirement
carried by the handle.

---

## Low-Level Runtime

Low-level async runtime machinery is part of the trusted substrate.

Executors, reactors, wakers, completion queues, foreign async callbacks, device async integration, and custom scheduling primitives
are implemented through trusted capabilities and exposed through safe async contracts.

Trusted runtime declarations that affect scheduling, cross-run memory visibility, synchronization, cancellation, or foreign
callbacks must expose a safe contract that states the ownership effects, borrow effects, synchronization edges, cancellation
behavior, panic behavior, trusted facts, fact invalidation, and capability requirements visible to callers.

Trusted runtime declarations can use raw memory, unchecked aliasing, target intrinsics, device memory, or foreign calls only through
the trusted capabilities defined by the Trust Model and Raw Memory Model.

Foreign or device operations that can access Bray-owned storage must either be represented by a synchronization contract or be
treated by their declaration contract as affecting every reachable storage item they can touch.

A trusted declaration cannot expose an ordinary safe API that permits data races, dangling borrows, unsynchronized shared mutation,
invalid raw memory access, leaked scoped capabilities, or unresolved run obligations.
