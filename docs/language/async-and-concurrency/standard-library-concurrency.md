# Standard-library concurrency and parallelism

Concurrency and parallelism facilities above the compiler-known computation, task, and run-outcome boundary are ordinary
standard-library declarations. They live under `std`, participate in ordinary visibility and `using` rules, and are not recognized
by name by the language.

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
async func first<T, const N: usize>(pos computations: [Future<T>; N]) -> RunResult<T>
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

struct Thread<T> {}

impl Thread<T>
{
    consume func join() -> RunResult<T>
        requires(blocking_execution());

    consume func cancel() -> RunResult<T>
        requires(blocking_execution());

    finalize()
        requires(blocking_execution());
}

func start<State, T>(pos entry: Entry<State, T>, pos state: State) -> Thread<T>;
func cancellation_requested() -> bool;
func checkpoint();
async func run<State, T>(pos entry: Entry<State, T>, pos state: State) -> T;
```

`start` evaluates the non-capturing synchronous entry callable and explicit state in the creating run, transfers the state to a new
operating-system thread, and returns an ordinary standard-library `Thread<T>`. `join` blocks the calling thread until completion.
`cancel` cooperatively requests cancellation and then blocks until completion. The owner's ordinary synchronous finalization
requests cancellation and joins when ownership otherwise ends.

After waiting, automatic thread-owner finalization treats an unobserved `Completed(T)` by the payload rules below, accepts
`Cancelled`, propagates an unobserved thread panic on ordinary exit, and records it as a suppressed child-run panic when another
panic is already active. During cancellation through the async bridge, that suppressed report is delivered through the host
cleanup-report sink.

The generic bodies infer open independent-run requirements for `State`, the entry callable, and `T`: state and entry dependencies
must survive transfer into the native-thread root, and the completed `T` must survive publication back to the thread owner. These
requirements are exported and instantiated at each concrete use. The entry callable is non-capturing so its callable identity has
no hidden local capture, but its explicit state and any declaration dependency still undergo the same check. Its callable contract
records both execution facts because the native-thread root establishes both before invoking it; an entry implementation that needs
either fact therefore remains assignable without charging the creating run.

Implicit `Thread<T>` finalization on normal scope exit is valid only when the current context establishes
`blocking_execution()` and an unobserved `Completed(T)` can be resolved synchronously and infallibly. If `T` has asynchronous or
fallible finalization, source must consume the owner with `join()` or `cancel()` and explicitly preserve or handle the completed
payload. During panic, synchronous infallible payload cleanup proceeds normally; payloads needing asynchronous finalization cannot
be owned by the public thread owner whose possible implicit cleanup path cannot drive them, so public `start` rejects that
instantiation.

Thread cancellation is observed through `std.thread.cancellation_requested()`. `std.thread.checkpoint()` terminates the thread run
with `RunResult.Cancelled` when a request is pending and otherwise returns normally. Native or noncooperative work can delay
cancellation indefinitely. The safe contracts of `Entry`, `Thread`, and `start` carry every ownership, dependency, affinity, and
cross-run visibility rule required by the transferred state.

`run` is the async bridge. It creates an operating-system thread without blocking a cooperative runtime worker, suspends the current
task until the thread completes, and propagates the thread's normal result, cancellation, or panic into the current task. Starting
the returned computation creates the ordinary `Task<T>` observation boundary. Its implementation uses a private ordinary
async-finalizable bridge owner rather than the public synchronously finalized `Thread<T>`, so `run` can preserve a `T` whose
lifecycle requires an async context.

If cancellation of the current task is observed while `run` is waiting, the bridge requests native-thread cancellation exactly
once, enters shielded cleanup, and waits for the thread to terminate before allowing current-task cancellation to continue. The
current task remains cancelled even if the thread races to normal completion; any completed `T` is lifecycle-resolved in the
shielded async context. A thread panic encountered during that cancellation is recorded as a suppressed panic, while type-erased
payload-finalization failures become cleanup incidents. A noncooperative thread can therefore delay task cancellation indefinitely.
If no current-task cancellation is active, a thread `Cancelled` outcome cancels the awaiting computation and a thread `Panicked`
outcome panics it.

Synchronous programs can use `start` and `Thread<T>` without selecting the async runtime. `std.thread.Thread<T>` is an ordinary
standard-library owner rather than a compiler-known type, and there is no thread-spawn syntax.

Because these facilities can be declared with ordinary callable types, structs, methods, generics, explicit state, lifecycle
declarations, `Future<T>`, `Task<T>`, and `RunResult<T>`, their public semantics are expressible in Bray. Parking threads, waking
tasks, and creating native threads remain private trusted implementation operations rather than pretending to be portable Bray code.

## Child processes

`std.process.Process<T>` is the ordinary standard-library owner for a child process whose selected completion protocol can produce
a value of type `T`.

The minimum typed-process surface is semantically equivalent to:

```bray
struct Program<Input, T> {}
struct Process<T> {}
struct ProcessFailure {}

union ProcessError
{
    Creation(pos failure: ProcessFailure);
    Transport(pos failure: ProcessFailure);
    Protocol(pos failure: ProcessFailure);
    Decoding(pos failure: ProcessFailure);
    ForcedTermination(pos failure: ProcessFailure);
    Reaping(pos failure: ProcessFailure);
}

impl Process<T>
{
    consume async func join() -> Result<RunResult<T>, ProcessError>;
    consume async func cancel() -> Result<RunResult<T>, ProcessError>;
}

async func start<Input, T>(
    pos program: Program<Input, T>,
    pos input: Input,
) -> Result<Process<T>, ProcessError>;

func run_blocking<Input, T>(
    pos program: Program<Input, T>,
    pos input: Input,
) -> Result<RunResult<T>, ProcessError>
    requires(blocking_execution());
```

`Program<Input, T>` is a standard-library description of a separately built executable product and its versioned input/output
protocol. It is not an arbitrary callable moved into another address space. The protocol declares serialization, executable
identity, target compatibility, panic-report compatibility, cancellation transport, and output decoding. Checking the generic
standard-library bodies infers the portable dependency requirements for the encoded input and decoded output.

`start` performs process creation and protocol negotiation without blocking a cooperative worker. `Result.Error` represents
creation, transport, protocol, or decoding failure in the observing process. `Result.Ok(process)` transfers the sole source-level
child-process ownership obligation to the returned `Process<T>`.

`join` waits without requesting cancellation. `cancel` requests cooperative process cancellation and then waits. Their outer
`Result` represents observer-side infrastructure failure. Their inner `RunResult<T>` represents the child run:

- `Completed(value)` is a normally decoded child value,
- `Panicked(report)` is a panic report received through a compatible Bray child protocol,
- `Cancelled` is confirmed cooperative child cancellation.

A consuming `join` or `cancel` returns only after the operating-system child has terminated and been reaped, including on
`Result.Error`. Protocol or decoding failure cannot discard ownership of a still-running process. If cooperative cancellation
fails, the selected `Program` termination policy decides whether to continue waiting or escalate to a target-supported forced
termination. Forced termination is reported by the outer process-error layer and is not mislabeled `RunResult.Cancelled`. An
operation whose policy cannot establish terminal ownership resolution does not return.

The layers remain intentionally distinct:

```bray
let process = try await std.process.start(program, input);
let child_run = try await process.join();
let value = try child_run;
```

The first two `try` expressions propagate recoverable `ProcessError` values to compatible lexical `Result` boundaries. The final
`try` forwards child panic or cancellation to the current run. Source can instead match either layer to apply a different policy.

An arbitrary external executable uses a standard-library program adapter whose completion value is an explicit process-exit type.
A signal, nonzero status, malformed output, Bray panic, and cooperative cancellation are not silently conflated. Only a conforming
Bray protocol can construct `RunResult.Panicked(PanicReport)`.

`Process<T>` is asynchronously finalizable. If ownership ends unresolved, cleanup requests cancellation, waits or escalates
according to the program's statically selected termination policy, reaps the operating-system child, and resolves any decoded
payload. A safe default cannot leak a zombie process or silently leave a child running. A policy permitting operating-system
handoff must be selected explicitly and changes the operation from child-process ownership to an external-service registration
contract.

On a normal scope exit, implicit process finalization is valid only when its selected policy and every possible decoded `T` can be
resolved infallibly in that async context. Otherwise source must explicitly consume the process with `join()` or `cancel()` and
handle the outer infrastructure result. During panic or cancellation, cleanup still resolves the child; fallible cleanup outcomes
become owned cleanup incidents under the ordinary abnormal-exit rules.

`run_blocking` provides the synchronous composition path. It creates, waits for, observes, and reaps the child as one call and never
publishes a `Process<T>` owner. It is available without selecting an async runtime, but cannot make progress on a cooperative async
lane because its contract requires `blocking_execution()`.

Process isolation prevents direct sharing of Bray borrows or memory capabilities. Values cross the boundary only through the
checked protocol. Shared-memory or inherited-handle facilities require separate standard-library types whose contracts expose the
corresponding synchronization, lifetime, authority, and cleanup obligations.

## Parallel algorithms and resource budgets

`std.parallel` provides ordinary Bray algorithms over explicit execution resources. Parallelism is not inferred from an ordinary
loop and no algorithm creates an unbounded private worker pool.

The minimum resource-policy surface is semantically equivalent to:

```bray
union Domain
{
    Tasks;
    Threads;
    Processes;
}

struct Budget {}

union BudgetError
{
    Zero;
    Unavailable(pos domain: Domain);
    Insufficient(pos requested: usize, pos available: usize);
}

impl Budget
{
    func domain() -> Domain;
    func maximum_active() -> usize;
    mut func split(pos maximum_active: usize) -> Result<Budget, BudgetError>;
}

func budget(pos domain: Domain, pos maximum_active: usize) -> Result<Budget, BudgetError>;
```

`Budget` is an owned synchronized resource authority, not a copyable integer hint. A parallel algorithm borrows or consumes it and
holds one internal permit for each active child run until that child reaches terminal observation. Nested algorithms use the same
budget or an explicitly split child budget, so copying a numeric limit cannot accidentally multiply process-wide parallelism.
Construction reserves permits from the product host's configured root authority for that domain. Independent budgets therefore
cannot collectively exceed the product limit. Destroying a budget releases its reservation. Construction rejects zero,
target-unavailable domains, and requests the remaining root authority cannot satisfy. Product capacity is selected configuration,
not an unbounded ambient machine query.

`split` reserves part of a mutable parent budget and returns a child budget carrying a dependency on that parent. The parent cannot
end while the child exists, and the reserved capacity returns to the parent when the child is destroyed. A failed split leaves the
parent unchanged.

A parallel operation receives or owns a `Budget` and states its cancellation, failure, ordering, and deterministic-result policy
in its ordinary callable contract or explicit policy value. Implementations use runtime tasks, native threads, or child processes
according to the budget's explicit domain:

- task parallelism uses `Future<T>`, `Task<T>`, and structured task cleanup,
- thread parallelism uses `std.thread.Thread<T>` or its async bridge,
- process parallelism uses `std.process.Process<T>` and explicit value protocols.

The algorithm's ordinary generic dependency contract proves which values can be borrowed within its scope, transferred to another
thread, or encoded for another process. Scoped task or thread parallelism can borrow caller storage only while the algorithm
statically owns every child and joins it before returning. Process parallelism cannot borrow caller memory; it transfers encoded
values or uses an explicit shared-memory contract.

Cancellation is hierarchical. Cancelling a parallel operation broadcasts cancellation to all owned runs before waiting for any of
them. A policy can collect terminal outcomes, stop after a failure, or reduce successful values, but it cannot silently discard a
panic, cancellation, recoverable error, or payload lifecycle obligation.

These algorithms need no new syntax or compiler-known types. Arrays and collections, generics, non-capturing callables, `Future<T>`,
`Task<T>`, `RunResult<T>`, `std.thread.Thread<T>`, `std.process.Process<T>`, channels, synchronization, and private trusted ABI
wrappers are sufficient to implement their public semantics in Bray.

## Navigation

- [Language index](../index.md)
- [Async and concurrency index](../async-and-concurrency.md)
- Previous: [Capability transfer across run boundaries](capability-transfer-across-run-boundaries.md)
- Next: [Low-level runtime](low-level-runtime.md)
