# Standard-library concurrency and parallelism

Concurrency and parallelism facilities above the compiler-known computation, task, and run-outcome boundary are ordinary
standard-library declarations. They live under `std`, participate in ordinary visibility and `using` rules, and are not recognized
by name by the language.

Every public declaration written in this chapter is normative. Its module, name, generic parameters, ownership modifiers, result,
and contracts are exact. Private representation and lowering are not language-visible and cannot change the public contract.

General stream, path, filesystem, process-context, raw child-process, clock, and entropy semantics are defined by
[I/O and platform services](../io-and-platform-services.md). This chapter defines the additional concurrency, typed child-process,
and structured ownership contracts built over that surface.

## One-time initialization

`std.sync.Once<T>` is a non-copyable synchronized owner with no public primary construction. Its public declarations are:

```bray
impl Once<T>
{
    static const func empty() -> Self;
    func get_or_init(pos initializer: func() -> T) -> &T;
    func get_or_try_init<E>(pos initializer: func() -> Result<T, E>) -> Result<&T, E>;
}
```

`empty()` is valid in static-initializer constant evaluation and creates the `empty` state without executing runtime code.

The first successful initializer publishes exactly one completely initialized `T`. Concurrent callers wait for that attempt and
observe its synchronization edge before borrowing the value. Normal completion changes the state to `initialized` and wakes all
waiters.

A returned error from `get_or_try_init`, panic, or cancellation publishes no value, returns the state to `empty`, and wakes waiters.
The active caller alone receives its returned `E`, propagates its panic, or enters its cancellation outcome. Existing waiters do not
inherit that outcome. Each awakened waiter rechecks the cell and, unless its own run is cancelled, competes to start a new attempt
with its own initializer and error type. One eligible caller becomes the next initializer while the others wait again. No priority
among eligible callers is guaranteed. `Once<Result<T, E>>` caches a failure because the `Result` is then the successfully initialized
value.

Waiting is cancellation-aware. A waiter that observes cancellation of its own run withdraws without invoking its initializer or
changing the cell state, then continues that run's cancellation. This outcome is independent of the active attempt's outcome.

Direct or indirect reentry into the same `Once<T>` on its current initialization chain panics before waiting and marks that cell's
owning attempt as failed. Catching the panic inside the initializer cannot make the attempt publishable. A value later returned by
that initializer is lifecycle-resolved, the cell returns to `empty`, and the outer accessor propagates the reentry panic. This rule
prevents a reentrant initialization cycle from becoming an indefinite self-wait.

The returned borrow depends on the `Once<T>` owner. When the owner is a product static, that dependency is product-rooted. When it
is a thread-local static, it also carries the exact native-thread attachment root. Destroying an initialized `Once<T>`
lifecycle-resolves the contained `T` exactly once. Destroying an empty or rolled-back value destroys no `T`.

## Run and task utilities

`std.run` provides the universal logical-run surface:

```bray
func cancellation_requested() -> bool;
func checkpoint();
```

`cancellation_requested()` reports the current run's request state. `checkpoint()` enters cancellation when a request is pending
and otherwise returns normally. Executable roots, tasks, native threads, and conforming child-process roots all use this state.

`std.task` provides these async-domain declarations:

```bray
func cancellation_requested() -> bool;
async func checkpoint();
async func yield_now();
```

The task helpers observe the same logical state as `std.run`. `yield_now()` additionally permits another ready task to execute
before the calling task resumes. Native-thread helpers under `std.thread` observe the same logical run state.

## Atomic storage

`std.atomic` provides the ordinary safe wrapper `Atomic<T>` over protected compiler-provided atomic storage. `atomic(value)` is its only primary constructor. The module exposes `load`, `store`, `exchange`, strong and weak `compare_exchange`, integer `fetch_add`, `fetch_sub`, `fetch_and`, `fetch_or`, and `fetch_xor`, hardware and compiler `fence`, `wait`, `notify_one`, and `notify_all`.

The copyable policy unions `LoadOrder`, `StoreOrder`, `ReadModifyWriteOrder`, `FenceOrder`, and `CompareExchangeOrder` make invalid operation and ordering combinations unrepresentable. Their ordinary exhaustive dispatch invokes compiler-provided primitives with closed ordering constants. These declarations are not recognized by module or declaration name. Exact ordering, target, wait, and synchronization behavior is defined by [Atomic operation contracts](atomic-operation-contracts.md).

## Channels

`std.channel` provides bounded multiple-producer, single-consumer channels. `Sender<T>` and `Receiver<T>` are nonforgeable owner
types with no public primary construction. The public declarations are:

```bray
union SendError<T>
{
    Closed(pos value: T);
}

union TrySendError<T>
{
    Full(pos value: T);
    Closed(pos value: T);
}

union ReceiveResult<T>
{
    Received(pos value: T);
    Closed;
}

union TryReceiveResult<T>
{
    Received(pos value: T);
    Empty;
    Closed;
}

union ChannelError
{
    CapacityOverflow;
}

impl Sender<T>
{
    func duplicate() -> Sender<T>;
    async func send(pos value: T) -> Result<unit, SendError<T>>;
    func try_send(pos value: T) -> Result<unit, TrySendError<T>>;
    consume func close();
}

impl Receiver<T>
{
    async func receive() -> ReceiveResult<T>;
    func try_receive() -> TryReceiveResult<T>;
    consume func close();
}

func bounded<T>(capacity: usize) -> Result<(Sender<T>, Receiver<T>), ChannelError>;
```

Checking these generic declarations infers an open dependency template for `T`. Publishing a value into channel storage requires
that the synchronized shared owner preserve the value's dependencies. Transferring an endpoint to another run additionally
instantiates the template for that destination. A concrete use is rejected when `T` carries a creating-run borrow, incompatible
thread affinity, unsynchronized mutation authority, or a lifecycle obligation that the receiver cannot resolve. This is ordinary
generic dependency-contract inference, not a channel-specific trait bound or compiler-recognized `std` declaration.

`bounded(capacity)` accepts every `usize` capacity. Capacity zero creates a rendezvous channel in which a send commits only when it
is paired with a receive. A positive capacity is the maximum number of committed messages that can await receipt. A capacity whose
storage layout cannot be represented returns `ChannelError.CapacityOverflow`. Allocation failure follows the language allocation
panic contract. `std.channel` has no unbounded channel constructor, so every channel has an explicit backpressure bound.

Cancellation-safe channel operations register a waiter before suspension, withdraw an uncommitted waiter during cancellation, and
transfer an owned message only at the operation's atomic commit point. Before commit, the sending frame still owns the value and
normal cancellation cleanup resolves it.

`send` produces `SendError.Closed(value)` without consuming the message into channel storage when the receiver is closed.
`try_send` returns immediately. It produces `TrySendError.Full(value)` when no commit is possible without suspension and
`TrySendError.Closed(value)` when the receiver is closed. `try_receive` returns immediately and distinguishes a received value, an
open channel with no immediately available value, and terminal closure.

Messages are received in send-commit order. Concurrent sends have no ordering before their atomic commit. Suspended senders and the
single receiver are admitted in waiter-registration order. Cancelling an uncommitted operation removes only that waiter and does
not reorder the remaining waiters. These ordering rules prevent starvation among continuously registered waiters but do not impose
an order on operations that race before registration.

Consuming `Sender.close()` closes that sender endpoint. The receiver reports `Closed` only after every sender endpoint is closed or
destroyed and every committed message has been received. Consuming `Receiver.close()` closes the receive side and causes every
uncommitted send to recover its value as `Closed`. Closing the receiver lifecycle-resolves every queued message before returning.
Destroying an endpoint has the same closure effect as consuming `close()`.

The synchronization declarations under `std.sync` are ordinary standard-library APIs. Their safe contracts establish the
cross-run visibility edges described by [shared state and synchronization](shared-state-and-synchronization.md). Their declaration
catalog is not part of the language-defined channel contract.

## Concurrent combinators

`std.concurrent` provides the two ownership-complete combinators over fixed homogeneous arrays:

```bray
async func first<T, const N: usize>(pos computations: [Future<T>; N]) -> RunResult<T>
    with(N > 0);

async func all<T, const N: usize>(pos computations: [Future<T>; N]) -> [RunResult<T>; N];
```

`first` returns the first terminal `RunResult<T>` selected by its atomic winner commit. Simultaneous readiness has no
language-defined winner before that commit. A first panic or cancellation is returned as its corresponding run-result variant. It is not
skipped in search of a completed value. All non-winning computations are cancelled and resolved before `first` returns.

`all` drives every computation to a terminal outcome and returns results in input-index order. It does not stop after a panic or
cancellation result. The zero-length input returns an empty array without suspending. Cancellation of either combinator cancels and
resolves every owned computation before cancellation leaves the combinator.

Heterogeneous operations map their outputs into an ordinary user-defined common union. A timeout is expressed by including an
ordinary timer future whose result is represented in that union. No race or select expression is required.

## Operating-system threads

The public `std.thread` owner type is `Thread<T>`. `ThreadFailure` is its creation-failure record. Both are nonforgeable and have no
public primary construction. The public operating-system-thread declarations are:

```bray
callable Entry<State, T> = func(pos state: State) -> T
    requires(
        blocking_execution(),
        compute_execution(),
    );

union ThreadError
{
    Capacity;
    Creation(pos failure: ThreadFailure);
}

impl ThreadFailure
{
    func native_code() -> i64?;
}

impl Thread<T>
{
    func id() -> Id;

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

These declarations are target-available only when `target.platform.native_threads` is true.

`start` evaluates the non-capturing synchronous entry callable and explicit state in the creating run, transfers the state to a new
operating-system thread, and returns an ordinary standard-library `Thread<T>`. Capacity exhaustion and operating-system creation
failure are recoverable `ThreadError` values. Once creation succeeds, the safe wrapper invariant makes `join` and `cancel`
operationally infallible: failure of the trusted substrate to observe a successfully created thread is a catastrophic contract bug,
not an ordinary resource error. `join` blocks the calling thread until completion. `cancel` cooperatively requests cancellation and
then blocks until completion. The owner's ordinary synchronous finalization requests cancellation and joins when ownership
otherwise ends. `Thread<T>` and `ThreadFailure` have no public primary construction.

If cancellation of the calling run is observed while consuming `join` or `cancel`, the operation requests child-thread
cancellation, waits for the child to terminate, resolves any completed payload under shielding, and then continues the caller's
cancellation. Consuming the owner can therefore never leave a detached native thread.

`Thread.id()` returns the same observational identity that `std.thread.current_id()` returns inside the child thread. The identity
does not grant cancellation, joining, affinity, or execution authority and remains an ordinary value after the thread terminates.
`ThreadFailure.native_code()` returns a target error code when the operating system supplied one and `none` otherwise. The numeric
code is inspection data and does not change the stable `ThreadError` category.

After waiting, automatic thread-owner finalization treats an unobserved `Completed(T)` by the payload rules below, accepts
`Cancelled`, propagates an unobserved thread panic on ordinary exit, and records it as a suppressed child-run panic when another
panic is already active. During cancellation through the async bridge, that suppressed report is delivered through the host
cleanup-report sink.

The generic bodies infer open independent-run requirements for `State`, the entry callable, and `T`: state and entry dependencies
must survive transfer into the native-thread root, and the completed `T` must survive publication back to the thread owner. These
requirements are exported and instantiated at each concrete use. The entry callable is non-capturing so its callable identity has
no hidden local capture, but its explicit state and any declaration dependency still undergo the same check. Its callable contract
records both execution requirements because the native-thread root establishes both before invoking it. An entry implementation that needs
either condition therefore remains assignable without charging the creating run.

Implicit `Thread<T>` finalization on normal scope exit is valid only when the current context establishes
`blocking_execution()` and an unobserved `Completed(T)` can be resolved synchronously and infallibly. If `T` has asynchronous or
fallible finalization, source must consume the owner with `join()` or `cancel()` and explicitly preserve or handle the completed
payload. During panic, synchronous infallible payload cleanup proceeds normally. Payloads needing asynchronous finalization cannot
be owned by the public thread owner whose possible implicit cleanup path cannot drive them, so public `start` rejects that
instantiation.

Thread cancellation is observed through `std.thread.cancellation_requested()` or `std.run.cancellation_requested()`.
`std.thread.checkpoint()` and `std.run.checkpoint()` terminate the thread run with `RunResult.Cancelled` when a request is pending
and otherwise return normally. Native or noncooperative work can delay cancellation indefinitely. The safe contracts of `Entry`,
`Thread`, and `start` carry every ownership, dependency, affinity, and cross-run visibility rule required by the transferred state.

`run` is the async bridge. It creates an operating-system thread without blocking a cooperative runtime worker and suspends the
current task until the thread completes. Creation failure produces `Result.Error(ThreadError)`. Normal thread completion produces
`Result.Ok(value)`. A child cancellation or panic is forwarded into the current async computation rather than nested inside the
outer `Result`. Starting the returned computation creates the ordinary `Task<Result<T, ThreadError>>` observation boundary. Unlike
the synchronously finalized `Thread<T>` owner, `run` can resolve a `T` whose lifecycle requires an async context.

If cancellation of the current task is observed while `run` is waiting, the bridge requests native-thread cancellation exactly
once, enters shielded cleanup, and waits for the thread to terminate before allowing current-task cancellation to continue. The
current task remains cancelled even if the thread races to normal completion. Any completed `T` is lifecycle-resolved in the
shielded async context. A thread panic encountered during that cancellation is recorded as a suppressed panic, while type-erased
payload-finalization failures become cleanup incidents. A noncooperative thread can therefore delay task cancellation indefinitely.
If no current-task cancellation is active, a thread `Cancelled` outcome cancels the awaiting computation and a thread `Panicked`
outcome panics it.

Synchronous programs can use `start` and `Thread<T>` without selecting the async runtime. `std.thread.Thread<T>` is an ordinary
standard-library owner rather than a compiler-known type, and there is no thread-spawn syntax.

## Child processes

`std.process.Process<T>` is the ordinary standard-library owner for a child process whose selected completion protocol can produce
a value of type `T`. Process binding is explicit and does not depend on compiler reflection, synthesized serialization, or a
compiler-recognized entrypoint shape.

The public `std.process` owner and descriptor types are `Executable`, `Codec<T>`, `TerminationPolicy`, `Program<Input, T>`,
`Process<T>`, and `ProcessFailure`. They are nonforgeable and have no public primary construction. `CodecFailure` is the public
failure value constructed by codec implementations. The public typed-process declarations are:

```bray
struct ProtocolFingerprint
{
    internal bytes: [u8; 32];
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

union CodecFailure
{
    InvalidValue;
    InvalidBytes;
    ResourceUnavailable;
    Application(pos code: u64);
}

impl ProcessFailure
{
    func native_code() -> i64?;
}

impl Process<T>
{
    func id() -> Id;

    mut func take_standard_input() -> ChildInput?;
    mut func take_standard_output() -> ChildOutput?;
    mut func take_standard_error() -> ChildOutput?;

    consume async func join() -> Result<RunResult<T>, ProcessError>;
    consume async func cancel() -> Result<RunResult<T>, ProcessError>;
    async finalize() -> Result<unit, ProcessError>;
}

callable Worker<Input, T> = async func(pos input: Input) -> T;
callable Encoder<T> = func(pos value: &T) -> Result<std.bytes.Buffer, CodecFailure>;
callable Decoder<T> = func(pos bytes: std.bytes.Buffer) -> Result<T, CodecFailure>;

impl Program<Input, T>
{
    mut func argument(pos value: std.path.NativeText) -> Result<unit, ProcessError>;
    mut func environment_policy(policy: EnvironmentPolicy) -> Result<unit, ProcessError>;

    mut func set_environment(
        pos key: std.path.NativeText,
        pos value: std.path.NativeText,
    ) -> Result<unit, ProcessError>;

    mut func remove_environment(pos key: &std.path.NativeText) -> unit;
    mut func working_directory(pos path: std.path.Path) -> unit;
    mut func standard_input(policy: ChildStreamPolicy) -> unit;
    mut func standard_output(policy: ChildStreamPolicy) -> unit;
    mut func standard_error(policy: ChildStreamPolicy) -> unit;
}

func protocol_fingerprint(pos bytes: [u8; 32]) -> ProtocolFingerprint;
func declared_product_dependency(pos name: string) -> Result<ProductDependency, ProcessError>;
func executable_from_dependency(pos dependency: ProductDependency) -> Result<Executable, ProcessError>;

func wait_for_cooperative_exit() -> TerminationPolicy;
func force_after(pos grace: std.time.Duration) -> Result<TerminationPolicy, ProcessError>;

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

`ProtocolFingerprint` is an application-defined 256-bit identifier for the complete wire schema and protocol contract. Constructing
one records bytes and does not verify a protocol.

`ProductDependency` is nonforgeable. `declared_product_dependency` resolves an exact dependency name from the current executable
product's selected dependency graph and rejects missing dependencies and dependencies that do not expose an executable product.
It performs no ambient filesystem, registry, or network search.

`Executable` is an authenticated descriptor obtained from a declared product dependency and uses the exact artifact selected by
the product graph. Typed Bray process execution does not accept an ambient path. Arbitrary path-based external programs use
`ChildCommand` and its explicit exit representation instead. `Codec<T>` owns ordinary encoder and decoder callable witnesses plus a
protocol fingerprint.
`TerminationPolicy` selects cooperative wait and permitted target-specific escalation. `Program<Input, T>` stores those four values
plus explicit arguments, environment, working directory, and standard-stream policy. It validates their target, protocol,
panic-report, and cancellation compatibility.

`program` begins with no child arguments, `EnvironmentPolicy.InheritSnapshot`, the current process's startup working-directory
snapshot, and `ChildStreamPolicy.Inherit` for all three standard streams. Configuration is applied only to the returned program
value and never mutates the current process. Configuration allocation or target validation failure is
`ProcessError.Creation(ProcessFailure)`.

The codec byte owner is `std.bytes.Buffer`, and grace periods use `std.time.Duration`.
`wait_for_cooperative_exit()` waits indefinitely after cancellation. `force_after(grace)` constructs a policy only when the target
supports the required termination and reaping behavior. A codec is explicit. The compiler synthesizes no serialization
implementation and does not inspect `T` to invent one. Checking the generic bodies infers the portable dependency requirements for
encoded input and decoded output.

Program process-context methods have the same snapshot, target-native text, environment-key comparison, and stream-policy semantics
as `ChildCommand`. The typed protocol uses dedicated authenticated transport and never interprets standard input, output, or error
as protocol frames. A piped standard-stream owner can be taken at most once. Untaken pipes are closed when terminal waiting begins.
A taken pipe is independently owned, and failure to drain a taken child output can delay child completion through ordinary
operating-system backpressure.

`CodecFailure.InvalidValue` reports an input value the encoder rejects. `InvalidBytes` reports bytes the decoder rejects.
`ResourceUnavailable` reports failure of a resource required by the codec. `Application(code)` preserves a codec-defined stable
numeric category. `ProcessFailure.native_code()` returns a target error code when the platform supplied one and `none` otherwise.
Target codes are inspection data and do not change the enclosing stable `ProcessError` variant.

A conforming child executable uses an ordinary async entrypoint that directly awaits `std.process.serve(worker, input_codec,
output_codec)`. `serve` authenticates the parent protocol, decodes the input, and directly awaits `worker(input)` in the child
executable root. Normal completion encodes `T`. A worker or child-root panic or cancellation is transmitted as the corresponding
run outcome. No language directive, compiler-known process type, callable reflection, or synthesized child-entry form exists. An
input-decoding or output-encoding error is reported through the protocol as observer-side `ProcessError`, not as a child
`Completed(T)`.

The parent handshake authenticates the executable digest and the input, output, panic-report, cancellation, and protocol
fingerprints before transferring the input. A mismatch is `ProcessError.Protocol`.

`start` performs input encoding, process creation, and protocol negotiation without blocking a cooperative worker. `Result.Error`
represents creation, transport, protocol, or encoding failure in the observing process. `Result.Ok(process)` transfers the sole
source-level child-process ownership obligation to the returned `Process<T>`. If failure or current-run cancellation occurs after
operating-system creation but before ownership is returned, `start` applies the program's termination policy and reaps the child
before returning the error or continuing cancellation.

`join` waits without requesting cancellation. `cancel` requests cooperative process cancellation and then waits. Their outer
`Result` represents observer-side infrastructure failure. Their inner `RunResult<T>` represents the child run:

- `Completed(value)` is a normally decoded child value,
- `Panicked(report)` is a panic report received through a compatible Bray child protocol,
- `Cancelled` is confirmed cooperative child cancellation.

`Process.id()` returns the same observational process identity as the underlying child owner. The identity does not preserve or
transfer any process ownership or control authority.

A consuming `join` or `cancel` returns only after the operating-system child has terminated and been reaped, including on
`Result.Error`. Protocol or decoding failure cannot discard ownership of a still-running process. Received terminal payload bytes
remain raw protocol storage until termination, reaping, framing, authentication, and all other infrastructure checks that can
produce `ProcessError` have completed. Only then may the decoder construct and commit `T` or `PanicReport`. A decoder's partial
values are ordinarily lifecycle-resolved on error, and after a decoded terminal payload is committed there is no later outer
`ProcessError` path.

If the observing run is cancelled while `join` or `cancel` is suspended, the operation requests child cancellation, applies the
stored termination policy, reaps the child, and resolves every terminal payload under shielding before continuing the observing
run's cancellation. A cleanup failure becomes a cleanup incident and does not abandon the child.

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

An arbitrary external executable uses `ChildCommand`, `ChildProcess`, and `ExitStatus` rather than the typed protocol. A signal,
nonzero status, malformed output, Bray panic, and cooperative cancellation are not silently conflated. Only a conforming Bray
protocol can construct `RunResult.Panicked(PanicReport)`.

`Process<T>` has an explicitly fallible async finalizer. If ownership ends unresolved, cleanup requests cancellation, waits or
escalates according to the stored termination policy, reaps the operating-system child, and resolves any terminal payload. Because
that cleanup can return `ProcessError`, implicit unresolved `Process<T>` finalization is rejected on normal scope exit. Source must
consume the process with `join()` or `cancel()` and handle the outer infrastructure result. During panic or cancellation, shielded
cleanup attempts the same policy. Failure becomes an owned cleanup incident under the ordinary abnormal-exit rules.

`Process<T>` cannot detach or transfer its ownership obligation to the operating system. It resolves only through `join`, `cancel`,
or abnormal-exit finalization.

`run_blocking` provides the synchronous composition path. It creates, waits for, observes, and reaps the child as one call and never
publishes a `Process<T>` owner. It is available without selecting an async runtime, but cannot make progress on a cooperative async
lane because its contract requires `blocking_execution()`. It returns `ProcessError.Creation` without spawning when any standard
stream policy is `Piped`, because the operation has no result surface through which to return pipe owners.

Process isolation prevents direct sharing of Bray borrows or memory capabilities. Values cross the boundary only through the
checked protocol. This contract provides no shared-memory or inherited-handle transfer.

## Parallel algorithms and resource budgets

`std.parallel` provides ordinary Bray algorithms over explicit execution resources. Parallelism is not inferred from an ordinary
loop and no algorithm creates an unbounded private worker pool.

`TaskDomain`, `ThreadDomain`, and `ProcessDomain` are public `std.parallel` domain marker types. `Budget<Domain>` and
`Permit<Domain>` are nonforgeable owners with no public primary construction. The public resource-policy declarations are:

```bray
struct TaskDomain {}
struct ThreadDomain {}
struct ProcessDomain {}

union BudgetError
{
    Zero;
    Insufficient(pos requested: usize, pos available: usize);
}

impl Budget<Domain>
{
    func maximum_active() -> usize;
    func try_acquire() -> Permit<Domain>?;
    async func acquire() -> Permit<Domain>;

    func acquire_blocking() -> Permit<Domain>
        requires(blocking_execution());

    mut func split(pos maximum_active: usize) -> Result<Budget<Domain>, BudgetError>;
}

impl Permit<Domain>
{
    consume func release();
}

func task_budget(pos maximum_active: usize) -> Result<Budget<TaskDomain>, BudgetError>;
func thread_budget(pos maximum_active: usize) -> Result<Budget<ThreadDomain>, BudgetError>;
func process_budget(pos maximum_active: usize) -> Result<Budget<ProcessDomain>, BudgetError>;
```

`Budget<Domain>` is an owned synchronized algorithm bound, not a copyable integer hint and not global machine or product authority.
Its domain parameter prevents passing a task budget to a thread or process operation. One live `Permit<Domain>` authorizes one
active child in that domain. The child operation retains the permit until its terminal outcome and payload lifecycle have been
resolved.

Independent budgets can collectively request more work than the machine or product can supply. Runtime task limits bound
simultaneously executing lanes and queue excess ready tasks without changing the infallible source contract of
`Future<T>.start()`. Native-thread and child-process hard limits are enforced through their recoverable creation errors.
`Future<T>.start()` does not consult or charge a `Budget<TaskDomain>`. An operation using a budget acquires a permit before calling
`start()`. Thread and process operations likewise acquire a permit before invoking their fallible creation operations and release
it only after terminal observation.

`try_acquire` returns `none` immediately when no capacity is available. `acquire` admits waiters in registration order and suspends
until a permit is available. `acquire_blocking` uses the same admission order while blocking its operating-system thread.
Cancelling an uncommitted asynchronous acquisition removes that waiter without consuming capacity or reordering the remaining
waiters. Consuming `Permit.release()` and destroying a permit both return its capacity exactly once.

`split` rejects zero. It reserves unacquired, unreserved capacity from the mutable parent and returns a child budget carrying a
dependency on that parent. `BudgetError.Insufficient` reports the requested capacity and the capacity available for reservation at
the operation's atomic commit point. A failed split leaves the parent unchanged. The parent cannot end while the child or any child
permit exists. Destroying the child after all its permits have ended returns its entire reserved capacity to the parent.

A parallel operation receives or owns a domain-specific budget and states its cancellation, failure, ordering, and
deterministic-result policy in its ordinary callable contract or explicit policy value. It uses runtime tasks, native threads, or
child processes according to the budget's explicit domain:

- task parallelism uses `Future<T>`, `Task<T>`, and structured task cleanup,
- thread parallelism uses `std.thread.Thread<T>` or its async bridge,
- process parallelism uses `std.process.Process<T>` and explicit value protocols.

The algorithm's ordinary generic dependency contract proves which values can be borrowed within its scope, transferred to another
thread, or encoded for another process. Scoped task or thread parallelism can borrow caller storage only while the algorithm
statically owns every child and joins it before returning. Process parallelism cannot borrow caller memory. It transfers encoded
values or uses an explicit shared-memory contract.

Cancellation is hierarchical. Cancelling a parallel operation broadcasts cancellation to all owned runs before waiting for any of
them. A policy can collect terminal outcomes, stop after a failure, or reduce successful values, but it cannot silently discard a
panic, cancellation, recoverable error, or payload lifecycle obligation.

A thread-domain algorithm handles `ThreadError` as its recoverable creation layer. Its async bridge can return that error while
forwarding a successfully created child's later panic or cancellation into the algorithm run. Process-domain algorithms preserve
the distinct `ProcessError` and `RunResult<T>` layers.

This chapter standardizes budget and permit ownership rather than a closed collection-algorithm catalog. Parallel collection
algorithms are ordinary standard-library declarations. Every such declaration must identify its domain through its budget type and
must state its input ordering, output ordering, early-stop behavior, recoverable-error behavior, and reduction determinism in its
public contract.

## Navigation

- [Language index](../index.md)
- [Async and concurrency index](../async-and-concurrency.md)
- Previous: [Capability transfer across run boundaries](capability-transfer-across-run-boundaries.md)
- Next: [Low-level runtime](low-level-runtime.md)
