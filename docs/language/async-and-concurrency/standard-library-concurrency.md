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

Checking these generic declarations infers an open dependency template for `T`. Publishing a value into channel storage requires
that the synchronized shared owner preserve the value's dependencies; transferring an endpoint to another run additionally
instantiates the template for that destination. A concrete use is rejected when `T` carries a creating-run borrow, incompatible
thread affinity, unsynchronized mutation authority, or a lifecycle obligation that the receiver cannot resolve. This is ordinary
generic dependency-contract inference, not a channel-specific trait bound or compiler-recognized `std` declaration.

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

This uses only ordinary Bray arrays, const generics, async functions or non-capturing async lambdas, channels, `start()`, `join()`,
and scope cleanup. Heterogeneous operations map their outputs into an ordinary user-defined common union. No race or select
expression is required.

`first` returns the first terminal `RunResult<T>` committed to its result channel. Simultaneous readiness has no language-defined
winner beyond that atomic commit. A first panic or cancellation is returned as its corresponding run-result variant; it is not
skipped in search of a completed value. All non-winning computations are cancelled and resolved before `first` returns.

An implementation can use private runtime event facilities to reduce helper-task or allocation overhead without changing the public
ordinary Bray contract.

## Operating-system threads

The minimum operating-system-thread surface is semantically equivalent to:

```bray
callable Entry<State, T> = func(pos state: State) -> T
    requires(
        blocking_execution(),
        compute_execution(),
    );

struct Handle<T> {}

impl Handle<T>
{
    consume func join() -> RunResult<T>
        requires(blocking_execution());

    consume func cancel() -> RunResult<T>
        requires(blocking_execution());

    finalize()
        requires(blocking_execution());
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

After waiting, automatic handle finalization treats an unobserved `Completed(T)` by the payload rules below, accepts `Cancelled`,
propagates an unobserved thread panic on ordinary exit, and records it as a suppressed child-run panic when another panic is already
active. During cancellation through the async bridge, that suppressed report is delivered through the host cleanup-report sink.

The generic bodies infer open independent-run requirements for `State`, the entry callable, and `T`: state and entry dependencies
must survive transfer into the native-thread root, and the completed `T` must survive publication back to the handle owner. These
requirements are exported and instantiated at each concrete use. The entry callable is non-capturing so its callable identity has
no hidden local capture, but its explicit state and any declaration dependency still undergo the same check. Its callable contract
records both execution facts because the native-thread root establishes both before invoking it; an entry implementation that needs
either fact therefore remains assignable without charging the creating run.

Implicit `Handle<T>` finalization on normal scope exit is valid only when the current context establishes
`blocking_execution()` and an unobserved `Completed(T)` can be resolved synchronously and infallibly. If `T` has asynchronous or
fallible finalization, source must consume the handle with `join()` or `cancel()` and explicitly preserve or handle the completed
payload. During panic, synchronous infallible payload cleanup proceeds normally; payloads needing asynchronous finalization cannot
be owned by the public handle whose possible implicit cleanup path cannot drive them, so public `start` rejects that instantiation.

Thread cancellation is observed through `std.thread.cancellation_requested()`. `std.thread.checkpoint()` terminates the thread run
with `RunResult.Cancelled` when a request is pending and otherwise returns normally. Native or noncooperative work can delay
cancellation indefinitely. The safe contracts of `Entry`, `Handle`, and `start` carry every ownership, dependency, affinity, and
cross-run visibility rule required by the transferred state.

`run` is the async bridge. It creates an operating-system thread without blocking a cooperative runtime worker, suspends the current
task until the thread completes, and propagates the thread's normal result, cancellation, or panic into the current task. Starting
the returned computation creates the ordinary `Task<T>` observation boundary. Its implementation uses a private ordinary
async-finalizable bridge owner rather than the public synchronously finalized `Handle<T>`, so `run` can preserve a `T` whose
lifecycle requires an async context.

If cancellation of the current task is observed while `run` is waiting, the bridge requests native-thread cancellation exactly
once, enters shielded cleanup, and waits for the thread to terminate before allowing current-task cancellation to continue. The
current task remains cancelled even if the thread races to normal completion; any completed `T` is lifecycle-resolved in the
shielded async context. A thread panic encountered during that cancellation is recorded as a suppressed panic, while type-erased
payload-finalization failures become cleanup incidents. A noncooperative thread can therefore delay task cancellation indefinitely.
If no current-task cancellation is active, a thread `Cancelled` outcome cancels the awaiting computation and a thread `Panicked`
outcome panics it.

Synchronous programs can use `start` and `Handle<T>` without selecting the async runtime. There is no compiler-known `Thread<T>` type
or thread-spawn syntax.

Because these facilities can be declared with ordinary callable types, structs, methods, generics, explicit state, lifecycle
declarations, `Async<T>`, `Task<T>`, and `RunResult<T>`, their public semantics are expressible in Bray. Parking threads, waking
tasks, and creating native threads remain private trusted implementation operations rather than pretending to be portable Bray code.

## Navigation

- [Language index](../index.md)
- [Async and concurrency index](../async-and-concurrency.md)
- Previous: [Capability transfer across run boundaries](capability-transfer-across-run-boundaries.md)
- Next: [Low-level runtime](low-level-runtime.md)
