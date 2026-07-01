# Async model

## Overview

Bray treats asynchronous execution as part of the core ownership and control-flow model.

The core async forms are:

- async functions,
- async computations,
- `await` expressions,
- `async` block expressions,
- `spawn` expressions,
- `spawn detached` expressions,
- task handles,
- task observation through `catch`.

An async computation is an owned value. It can own or borrow state, hold capabilities, carry effects, and carry finalization
obligations.

Spawned work is structured by default. A non-detached spawned task belongs to the nearest enclosing `async` block until the task is
completed, cancelled, or transferred to another owner.

Detached work is explicit. `spawn detached` creates a task whose lifetime is represented by the returned task handle and whose
captured state must be detached-safe.

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

If the task completed normally with a value of type `T`, `catch task.join()` produces `RunResult.Completed(value = value)`.

If the task panicked, `catch task.join()` produces `RunResult.Panicked(report = report)`.

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
        yield Result.Error(error = HashError.cancelled);
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
        yield Result.Error(error = HashError.cancelled);
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

## Cancellation

Cancellation is an exit path for an incomplete async computation or task.

Cancelling an async computation destroys owned captured state and releases held capabilities according to the computation's
contract.

Cancelling a task consumes the task handle and resolves the task obligation.

Cancellation of a task observed through `catch task.join()` produces `RunResult.Cancelled`.

Cancellation does not produce the task's ordinary result type.

Cancellation must satisfy all ownership, destruction, finalization, borrow, capability, and effect obligations of the cancelled
state.

---

## Low-Level Runtime

Low-level async runtime machinery is part of the trusted substrate.

Executors, reactors, wakers, completion queues, foreign async callbacks, device async integration, and custom scheduling primitives
are implemented through trusted capabilities and exposed through safe async contracts.

---

## Finalization TODOs

- TODO: Define thread run-boundaries, including thread creation, thread handles, join and cancel behavior, ownership transfer,
  capture restrictions, panic reporting through `RunResult<T>`, and how threads differ from tasks.
- TODO: Define the cross-task and cross-thread memory model, including data-race prevention, shared-state synchronization, atomic
  operation contracts, cancellation interaction, capability transfer, and trusted escape hatches.
