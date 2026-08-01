# Standard-library concurrency and parallelism

Concurrency and parallelism facilities above the compiler-known computation, task, and run-outcome boundary are ordinary
standard-library declarations. They live under `std`, participate in ordinary visibility and `using` rules, and are not recognized
by name by the language.

The standard library is written in Bray over the compiler-known types and private trusted runtime ABI declarations. Its public
surface does not expose those private ABI declarations.

General stream, path, filesystem, process-context, raw child-process, clock, and entropy semantics are defined by
[I/O and platform services](../io-and-platform-services.md). This chapter defines the additional concurrency, typed child-process,
and structured ownership contracts built over that surface.

## Implementation boundary

The concurrency and parallelism implementation is divided into three layers:

| Layer | Required role |
| --- | --- |
| Ordinary safe Bray | Public owners, state machines, protocols, policies, combinators, budgets, and parallel algorithms |
| Trusted Bray | Raw internal representations, atomics-based synchronization internals, runtime-event integration, callback context handling, and safe wrappers around platform handles |
| Product or platform boundary | Operations unavailable in the Bray abstract machine, such as creating native threads or processes, waiting on operating-system events, signalling or reaping processes, polling platform reactors, and acquiring virtual memory |

The public standard-library layer is Bray source. In particular, channel semantics, task combinators, thread and process ownership,
typed process protocols, codecs, termination policy, budgets, cancellation composition, and parallel algorithms are not delegated
to C, Rust, or another foreign library.

Portable target-independent internals should also be trusted Bray. This includes ready queues, waiter lists, permit accounting,
reference management, protocol framing, timer data structures, scheduler policy, and lifecycle state machines when the required
atomic, memory, and runtime operations are available through Bray contracts.

The bottom layer is necessarily target-specific because Bray source cannot by itself ask an operating system to create a native
thread, create or reap a process, wait on a kernel object, or poll a platform event facility. A target can supply those mechanisms
through:

- direct private Bray `extern` declarations for stable platform or system-library symbols,
- compiler-lowered target operations or private runtime ABI roles,
- a separately built runtime artifact whose implementation is itself Bray,
- a narrow native shim when the platform ABI cannot be represented cleanly or safely as direct declarations.

An `extern` declaration does not imply a C implementation. It states that the body is supplied by another linked artifact. That
artifact can be compiled Bray, compiler-generated runtime code, a platform library, or code written in another language. A
genuinely foreign ABI uses the ordinary FFI and `foreign_call` rules.

A native shim is mechanism only. It can normalize awkward platform macros, calling conventions, signal or unwind trampolines, or
unstable structure layouts, but it must not own Bray-level channel behavior, structured cancellation, task ownership, process
protocol policy, codecs, budgets, or parallel algorithms. Replacing direct platform bindings with a shim must not change the public
Bray contracts.

## Run and task utilities

`std.run` provides the universal logical-run surface:

```bray
func cancellation_requested() -> bool;
func checkpoint();
```

`cancellation_requested()` reports the current run's request state. `checkpoint()` enters cancellation when a request is pending
and otherwise returns normally. Executable roots, tasks, native threads, and conforming child-process roots all use this state.

`std.task` provides async-domain conveniences including:

```bray
func cancellation_requested() -> bool;
async func checkpoint();
async func yield_now();
```

The task helpers delegate to `std.run`. `yield_now()` additionally allows another ready task to run. Native-thread helpers under
`std.thread` likewise delegate to the same logical state. Their implementations use private runtime operations.

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

struct Sender<T>
{
    internal state: SenderState<T>;
}

impl Sender<T>
{
    func duplicate() -> Sender<T>;
    async func send(pos value: T) -> Result<unit, SendError<T>>;
    func close();
}

struct Receiver<T>
{
    internal state: ReceiverState<T>;
}

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

The internal state types in these conceptual signatures are implementation placeholders, not public contracts. The owner types have
no public primary construction. Bounded channel construction, asynchronous send, asynchronous receive, closure, and explicit sender
duplication are implemented with ordinary ownership, unions, atomics, synchronization types, async functions, and a runtime-backed
event primitive.

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

struct Thread<T>
{
    internal state: ThreadState<T>;
}

struct ThreadFailure
{
    internal state: ThreadFailureState;
}

union ThreadError
{
    Capacity;
    Creation(pos failure: ThreadFailure);
}

impl Thread<T>
{
    consume func join() -> RunResult<T>
        requires(blocking_execution());

    consume func cancel() -> RunResult<T>
        requires(blocking_execution());

    finalize()
        requires(blocking_execution());
}

func start<State, T>(
    pos entry: Entry<State, T>,
    pos state: State,
) -> Result<Thread<T>, ThreadError>;

func cancellation_requested() -> bool;
func checkpoint();

async func run<State, T>(
    pos entry: Entry<State, T>,
    pos state: State,
) -> Result<T, ThreadError>;
```

`start` evaluates the non-capturing synchronous entry callable and explicit state in the creating run, transfers the state to a new
operating-system thread, and returns an ordinary standard-library `Thread<T>`. Capacity exhaustion and operating-system creation
failure are recoverable `ThreadError` values. Once creation succeeds, the safe wrapper invariant makes `join` and `cancel`
operationally infallible: failure of the trusted substrate to observe a successfully created thread is a catastrophic contract bug,
not an ordinary resource error. `join` blocks the calling thread until completion. `cancel` cooperatively requests cancellation and
then blocks until completion. The owner's ordinary synchronous finalization requests cancellation and joins when ownership
otherwise ends. `Thread<T>` and `ThreadFailure` have no public primary construction; their internal state can be created only by
the standard-library implementation.

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

Thread cancellation is observed through `std.thread.cancellation_requested()` or `std.run.cancellation_requested()`.
`std.thread.checkpoint()` and `std.run.checkpoint()` terminate the thread run with `RunResult.Cancelled` when a request is pending
and otherwise return normally. Native or noncooperative work can delay cancellation indefinitely. The safe contracts of `Entry`,
`Thread`, and `start` carry every ownership, dependency, affinity, and cross-run visibility rule required by the transferred state.

`run` is the async bridge. It creates an operating-system thread without blocking a cooperative runtime worker and suspends the
current task until the thread completes. Creation failure produces `Result.Error(ThreadError)`. Normal thread completion produces
`Result.Ok(value)`. A child cancellation or panic is forwarded into the current async computation rather than nested inside the
outer `Result`. Starting the returned computation creates the ordinary `Task<Result<T, ThreadError>>` observation boundary. Its
implementation uses a private ordinary async-finalizable bridge owner rather than the public synchronously finalized `Thread<T>`,
so `run` can preserve a `T` whose lifecycle requires an async context.

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
tasks, and creating native threads remain private trusted implementation operations rather than pretending to be portable Bray
code. Thread-entry trampolines and terminal-state publication can be trusted Bray exported through the platform callback ABI; only
the native thread creation, wait, and wake mechanism must cross the platform boundary.

## Child processes

`std.process.Process<T>` is the ordinary standard-library owner for a child process whose selected completion protocol can produce
a value of type `T`. Process binding is explicit and does not depend on compiler reflection, synthesized serialization, or a
compiler-recognized entrypoint shape.

The minimum typed-process surface is semantically equivalent to:

```bray
struct Executable
{
    internal state: ExecutableState;
}

struct Codec<T>
{
    internal state: CodecState<T>;
}

struct TerminationPolicy
{
    internal state: TerminationPolicyState;
}

struct Program<Input, T>
{
    internal state: ProgramState<Input, T>;
}

struct Process<T>
{
    internal state: ProcessState<T>;
}

struct ProcessFailure
{
    internal state: ProcessFailureState;
}

struct CodecFailure
{
    internal state: CodecFailureState;
}

union ProcessError
{
    Creation(pos failure: ProcessFailure);
    Transport(pos failure: ProcessFailure);
    Protocol(pos failure: ProcessFailure);
    Encoding(pos failure: CodecFailure);
    Decoding(pos failure: CodecFailure);
    ForcedTermination(pos failure: ProcessFailure);
    Reaping(pos failure: ProcessFailure);
}

impl Process<T>
{
    consume async func join() -> Result<RunResult<T>, ProcessError>;
    consume async func cancel() -> Result<RunResult<T>, ProcessError>;
    async finalize() -> Result<unit, ProcessError>;
}

callable Worker<Input, T> = async func(pos input: Input) -> T;
callable Encoder<T> = func(pos value: &T) -> Result<Bytes, CodecFailure>;
callable Decoder<T> = func(pos bytes: Bytes) -> Result<T, CodecFailure>;

func executable_from_path(pos path: Path, pos digest: Digest) -> Result<Executable, ProcessError>;
func executable_from_dependency(pos dependency: ProductDependency) -> Result<Executable, ProcessError>;

func wait_for_cooperative_exit() -> TerminationPolicy;
func force_after(pos grace: Duration) -> Result<TerminationPolicy, ProcessError>;

func codec<T>(
    pos encode: Encoder<T>,
    pos decode: Decoder<T>,
    pos fingerprint: ProtocolFingerprint,
) -> Codec<T>;

func program<Input, T>(
    pos executable: Executable,
    pos input_codec: Codec<Input>,
    pos output_codec: Codec<T>,
    pos termination: TerminationPolicy,
) -> Result<Program<Input, T>, ProcessError>;

async func serve<Input, T>(
    pos worker: Worker<Input, T>,
    pos input_codec: Codec<Input>,
    pos output_codec: Codec<T>,
) -> Result<unit, ProcessError>;

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

`Executable` is an authenticated descriptor obtained either from an explicit path plus product digest or from the current
product's declared dependency manifest. It is not discovered by name or compiler reflection. `Codec<T>` owns ordinary encoder and
decoder callable witnesses plus a schema-and-protocol fingerprint. `TerminationPolicy` selects cooperative wait and permitted
target-specific escalation. `Program<Input, T>` stores those four values and validates their target, protocol, panic-report, and
cancellation compatibility. None of these owner types has public primary construction.

The callable aliases and supporting path, bytes, digest, duration, dependency, and fingerprint types are ordinary standard-library
declarations. A `ProductDependency` is obtained from ordinary product-manifest lookup rather than constructed from an unchecked
string. `wait_for_cooperative_exit()` waits indefinitely after cancellation; `force_after(grace)` constructs a policy only when the
target supports the required termination and reaping behavior. A codec is explicit: the compiler synthesizes no serialization
implementation and does not inspect `T` to invent one. Checking the generic bodies infers the portable dependency requirements for
encoded input and decoded output.

A conforming child executable uses an ordinary async entrypoint that directly awaits `std.process.serve(worker, input_codec,
output_codec)`. `serve` performs the authenticated protocol handshake, decodes the input, registers the private host terminal
reporter, and directly awaits `worker(input)` in the child executable root. Normal completion encodes `T`. If the worker or child
root panics or is cancelled, the product host reporter sends the compatible panic or cancellation outcome. This is ordinary Bray
library composition over private ABI operations; it needs no language directive, compiler-known process type, callable reflection,
or synthesized child-entry code. An input-decoding or output-encoding error is reported through the protocol as observer-side
`ProcessError`, not as a child `Completed(T)`.

The parent handshake authenticates the executable digest and the input, output, panic-report, cancellation, and protocol
fingerprints before transferring the input. A mismatch is `ProcessError.Protocol`.

`start` performs input encoding, process creation, and protocol negotiation without blocking a cooperative worker. `Result.Error`
represents creation, transport, protocol, or encoding failure in the observing process. `Result.Ok(process)` transfers the sole
source-level child-process ownership obligation to the returned `Process<T>`.

`join` waits without requesting cancellation. `cancel` requests cooperative process cancellation and then waits. Their outer
`Result` represents observer-side infrastructure failure. Their inner `RunResult<T>` represents the child run:

- `Completed(value)` is a normally decoded child value,
- `Panicked(report)` is a panic report received through a compatible Bray child protocol,
- `Cancelled` is confirmed cooperative child cancellation.

A consuming `join` or `cancel` returns only after the operating-system child has terminated and been reaped, including on
`Result.Error`. Protocol or decoding failure cannot discard ownership of a still-running process. Received terminal payload bytes
remain raw protocol storage until termination, reaping, framing, authentication, and all other infrastructure checks that can
produce `ProcessError` have completed. Only then may the decoder construct and commit `T` or `PanicReport`. A decoder's partial
values are ordinarily lifecycle-resolved on error, and after a decoded terminal payload is committed there is no later outer
`ProcessError` path.

If cooperative cancellation fails, the selected `Program` termination policy decides whether to continue waiting or escalate to a
target-supported forced termination. Forced termination is reported by the outer process-error layer and is not mislabeled
`RunResult.Cancelled`. An operation whose policy cannot establish terminal ownership resolution does not return.

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

`Process<T>` has an explicitly fallible async finalizer. If ownership ends unresolved, cleanup requests cancellation, waits or
escalates according to the stored termination policy, reaps the operating-system child, and resolves any terminal payload. Because
that cleanup can return `ProcessError`, implicit unresolved `Process<T>` finalization is rejected on normal scope exit. Source must
consume the process with `join()` or `cancel()` and handle the outer infrastructure result. During panic or cancellation, shielded
cleanup attempts the same policy; failure becomes an owned cleanup incident under the ordinary abnormal-exit rules.

Operating-system handoff or detached service registration is a separate standard-library API and owner type. It is never a
`Process<T>` finalization policy, because successful handoff ends the child-process ownership contract rather than resolving it.

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
struct TaskDomain {}
struct ThreadDomain {}
struct ProcessDomain {}

struct Budget<Domain>
{
    internal state: BudgetState<Domain>;
}

union BudgetError
{
    Zero;
    Insufficient(pos requested: usize, pos available: usize);
}

impl Budget<Domain>
{
    func maximum_active() -> usize;
    mut func split(pos maximum_active: usize) -> Result<Budget<Domain>, BudgetError>;
}

func task_budget(pos maximum_active: usize) -> Result<Budget<TaskDomain>, BudgetError>;
func thread_budget(pos maximum_active: usize) -> Result<Budget<ThreadDomain>, BudgetError>;
func process_budget(pos maximum_active: usize) -> Result<Budget<ProcessDomain>, BudgetError>;
```

`Budget<Domain>` is an owned synchronized algorithm bound, not a copyable integer hint and not global machine or product authority.
A parallel algorithm borrows or consumes it and holds one internal library permit for each active child until terminal observation.
Its domain parameter prevents passing a task budget to a thread or process algorithm. The state is internal and there is no public
primary construction.

Independent budgets can collectively request more work than the machine or product can supply. Runtime task limits bound
simultaneously executing lanes and queue excess ready tasks without changing the infallible source contract of
`Future<T>.start()`. Native-thread and child-process hard limits are enforced through their recoverable creation errors.
`Future<T>.start()` does not consult or charge a `Budget<TaskDomain>`; the standard-library parallel algorithm acquires its own
permit before calling `start()`. Thread and process algorithms likewise acquire a permit before invoking their fallible creation
operations and release it only after terminal observation.

`split` reserves part of a mutable parent budget and returns a child budget carrying a dependency on that parent. The parent cannot
end while the child exists, and the reserved capacity returns to the parent when the child is destroyed. A failed split leaves the
parent unchanged.

A parallel operation receives or owns a domain-specific budget and states its cancellation, failure, ordering, and
deterministic-result policy in its ordinary callable contract or explicit policy value. Implementations use runtime tasks, native
threads, or child processes according to the budget's explicit domain:

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

A thread-domain algorithm handles `ThreadError` as its recoverable creation layer. Its async bridge can return that error while
forwarding a successfully created child's later panic or cancellation into the algorithm run. Process-domain algorithms preserve
the distinct `ProcessError` and `RunResult<T>` layers.

These algorithms need no new syntax or compiler-known types. Arrays and collections, generics, non-capturing callables, `Future<T>`,
`Task<T>`, `RunResult<T>`, `std.thread.Thread<T>`, `std.process.Process<T>`, channels, synchronization, and private trusted ABI
wrappers are sufficient to implement their public semantics in Bray. No foreign-language implementation is required for these
algorithms.

## Navigation

- [Language index](../index.md)
- [Async and concurrency index](../async-and-concurrency.md)
- Previous: [Capability transfer across run boundaries](capability-transfer-across-run-boundaries.md)
- Next: [Low-level runtime](low-level-runtime.md)
