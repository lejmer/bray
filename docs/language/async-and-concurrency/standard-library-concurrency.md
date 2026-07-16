# Standard-library concurrency

Concurrency facilities above the compiler-known computation and task boundary are ordinary standard-library declarations. They live
under `std`, participate in ordinary visibility and `using` rules, and are not recognized by name by the language.

The standard library is written in Bray over the compiler-known types and private trusted runtime ABI declarations. Its public
surface does not expose those private ABI declarations.

## Task utilities

`std.task` provides ordinary functions including:

```bray
func cancellation_requested() -> bool;
async func checkpoint();
async func yield_now();
```

`cancellation_requested()` returns the current task's request state and returns `false` when called outside task execution.
`checkpoint()` observes cancellation and otherwise permits continued execution. `yield_now()` additionally allows another ready
task to run. Their implementation uses private runtime operations.

## Channels and synchronization

The minimum public channel surface is semantically equivalent to:

```bray
union SendError<T>
{
    Closed(pos value: T);
}

union ReceiveResult<T>
{
    Received(pos value: T);
    Closed;
}

struct Sender<T> {}

impl Sender<T>
{
    func duplicate() -> Sender<T>;
    async func send(pos value: T) -> Result<unit, SendError<T>>;
    func close();
}

struct Receiver<T> {}

impl Receiver<T>
{
    async func receive() -> ReceiveResult<T>;
    func close();
}

func bounded<T, const N: usize>() -> (Sender<T>, Receiver<T>)
    with(N > 0);
```

The protected fields shown empty here are standard-library implementation details, not compiler-protected representations. Bounded
channel construction, asynchronous send, asynchronous receive, closure, and explicit sender duplication are implemented with
ordinary ownership, unions, atomics, synchronization types, async functions, and a runtime-backed event primitive.

Cancellation-safe channel operations register a waiter before suspension, withdraw an uncommitted waiter during cancellation, and
transfer an owned message only at the operation's atomic commit point. Before commit, the sending frame still owns the value and
normal cancellation cleanup resolves it.

`send` produces `SendError.Closed(value)` without consuming the message into channel storage when the receiver side is closed.
`receive` returns queued values before returning `ReceiveResult.Closed`; it returns `Closed` only when no queued value remains and
every sender is closed or destroyed. `close` is idempotent. Destroying the final endpoint performs the corresponding close operation.

`std.sync` provides ordinary synchronization types such as mutexes, events, and higher-level atomic wrappers. The safe contracts of
those types establish the cross-run visibility edges they expose.

## Concurrent combinators

The minimum first-completion surface is:

```bray
async func first<T, const N: usize>(pos computations: [Async<T>; N]) -> RunResult<T>
    with(N > 0);
```

It can be expressed for a fixed array of computations by:

1. creating a bounded result channel,
2. starting each input computation,
3. starting an explicit-state watcher that joins each task and sends its `RunResult<T>`,
4. awaiting the first channel result,
5. relying on structured scope exit to cancel and join the losing watchers and tasks.

This uses only ordinary Bray arrays, const generics, async functions or non-capturing async lambdas, channels, `start()`, `join()`, and
scope cleanup. Heterogeneous operations map their outputs into an ordinary user-defined common union. No race or select expression
is required.

`first` returns the first terminal `RunResult<T>` committed to its result channel. Simultaneous readiness has no language-defined
winner beyond that atomic commit. A first panic or cancellation is returned as its corresponding run-result variant; it is not
skipped in search of a completed value. All non-winning computations are cancelled and resolved before `first` returns.

An implementation can use private runtime event facilities to reduce helper-task or allocation overhead without changing the public
ordinary Bray contract.

## Operating-system threads

The minimum operating-system-thread surface is semantically equivalent to:

```bray
callable Entry<State, T> = func(pos state: State) -> T;

struct Handle<T> {}

impl Handle<T>
{
    consume func join() -> RunResult<T>;
    consume func cancel() -> RunResult<T>;
}

func start<State, T>(pos entry: Entry<State, T>, pos state: State) -> Handle<T>;
func cancellation_requested() -> bool;
func checkpoint();
async func run<State, T>(pos entry: Entry<State, T>, pos state: State) -> T;
```

`start` evaluates the non-capturing synchronous entry callable and explicit state in the creating run, transfers the state to a new
operating-system thread, and returns an ordinary standard-library `Handle<T>`. `join` blocks the calling thread until completion.
`cancel` cooperatively requests cancellation and then blocks until completion. The handle's ordinary synchronous finalization
requests cancellation and joins when ownership otherwise ends.

Thread cancellation is observed through `std.thread.cancellation_requested()`. `std.thread.checkpoint()` terminates the thread run
with `RunResult.Cancelled` when a request is pending and otherwise returns normally. Native or noncooperative work can delay
cancellation indefinitely. The safe contracts of `Entry`, `Handle`, and `start` carry every ownership, dependency, affinity, and
cross-run visibility rule required by the transferred state.

`run` is the async bridge. It creates an operating-system thread without blocking a cooperative runtime worker, suspends the current
task until the standard thread handle completes, and propagates the thread's normal result or panic into the current task. Starting
the returned computation creates the ordinary `Task<T>` observation boundary.

Synchronous programs can use `start` and `Handle<T>` without selecting the async runtime. There is no compiler-known `Thread<T>` type
or thread-spawn syntax.

Because these facilities can be declared with ordinary callable types, structs, methods, generics, explicit state, lifecycle
declarations, `Async<T>`, `Task<T>`, and `RunResult<T>`, their public semantics are expressible in Bray. Parking threads, waking tasks,
and creating native threads remain private trusted implementation operations rather than pretending to be portable Bray code.

## Navigation

- [Language index](../index.md)
- [Async and concurrency index](../async-and-concurrency.md)
- Previous: [Capability transfer across run boundaries](capability-transfer-across-run-boundaries.md)
- Next: [Low-level runtime](low-level-runtime.md)
