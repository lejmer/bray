# Async and Concurrency

**Specification:** [Async and concurrency](https://github.com/lejmer/bray/blob/develop/docs/language/async-and-concurrency.md)

## Contents

- [Async computations](#async-computations)
- [Awaiting and starting](#awaiting-and-starting)
- [Task results and obligations](#task-results-and-obligations)
- [Execution requirements and entrypoints](#execution-requirements-and-entrypoints)
- [Cooperative cancellation](#cooperative-cancellation)
- [Cross-run state and synchronization](#cross-run-state-and-synchronization)
- [Atomic storage and ordering](#atomic-storage-and-ordering)
- [Standard-library concurrency](#standard-library-concurrency)
- [Choose the boundary](#choose-the-boundary)

## Async computations

**Core model:** An async call evaluates its receiver and arguments, then returns an inactive owned `Future<T>`. Its owner carries every captured dependency and lifecycle obligation until the computation is driven or resolved.

The examples assume suitable application types, ordinary imports, and the helper functions named in their bodies.

```bray
async func fetch(pos request: Request) -> Response
{
    return await send(request);
}

func deferred_fetch(pos request: Request) -> Future<Response>
{
    return fetch(request);
}

struct Client
{
    endpoint: Endpoint;

    async func request(pos request: Request) -> Response
    {
        return await send_to(&self.endpoint, request);
    }
}

func request_handler() -> async func(pos request: Request) -> Response
{
    return async lambda(pos request: Request) -> Response
    {
        return await fetch(request);
    };
}

struct Session
{
    connection: Connection;

    async finalize() -> Result<unit, CloseError>
    {
        return await close_connection(&self.connection);
    }
}
```

An `async func(...) -> T` and an ordinary `func(...) -> Future<T>` are different callable types. The ordinary function above merely forwards an existing future.

`Future<T>` is move-only protected state. Storing an inactive future transfers its captured values, borrows, capabilities, and lifecycle obligations into the new owner. Resolving that owner without driving the future destroys its captures without executing the async body.

See [async functions and computations](https://github.com/lejmer/bray/blob/develop/docs/language/async-and-concurrency/async-functions-and-computations.md), [async representation and storage](https://github.com/lejmer/bray/blob/develop/docs/language/async-and-concurrency/async-representation-and-storage.md), and [captured state](https://github.com/lejmer/bray/blob/develop/docs/language/async-and-concurrency/captured-state.md).

## Awaiting and starting

`await` consumes a `Future<T>` and composes the computation directly into the current asynchronous run. `.start()` consumes the future, creates an independent structured task, and returns the sole `Task<T>` owner.

```bray
async func direct(pos request: Request) -> Response
{
    let pending: Future<Response> = fetch(request);

    return await pending;
}

async func concurrent(pos first: Request, pos second: Request) -> (Response, Response)
{
    let first_task: Task<Response> = fetch(first).start();
    let second_task: Task<Response> = fetch(second).start();

    let first_result: Response = try await first_task.join();
    let second_result: Response = try await second_task.join();

    return(first_result, second_result);
}
```

Call `.start()` in an active async context to begin a task. The selected execution lane must satisfy the async callable's requirements.

Write recursive async calls directly. The compiler supplies stable frame storage for suspended recursive depth, and `start()` establishes task-owned storage before the first resume.

See [await expressions](https://github.com/lejmer/bray/blob/develop/docs/language/async-and-concurrency/await-expressions.md) and [starting tasks](https://github.com/lejmer/bray/blob/develop/docs/language/async-and-concurrency/starting-tasks.md).

## Task results and obligations

`Task<T>` is a move-only handle and a structured obligation. Both observation methods consume the handle and return an inactive `Future<RunResult<T>>`. Driving a join future observes the run. Driving a cancellation future first requests cooperative cancellation and then observes the run.

```bray
async func inspect(pos task: Task<Response>)
{
    match await task.join()
    {
        case Completed(response) { use_response(response); }
        case Panicked(report) { report_failure(report); }
        case Cancelled { record_cancellation(); }
    }
}

async func stop(pos task: Task<Response>) -> RunResult<Response>
{
    let cancellation: Future<RunResult<Response>> = task.cancel();

    return await cancellation;
}

async func propagate(pos task: Task<Response>) -> Response
{
    return try await task.join();
}

async func recover_panic(pos task: Task<Response>) -> Result<Response, PanicReport>
{
    return catch (try await task.join());
}
```

`try` forwards `Panicked` and `Cancelled` into the current run. `catch` can recover the forwarded panic, but cancellation continues to propagate.

Every lexical scope is a structured task boundary. On scope exit, Bray first broadcasts cancellation to every task still owned by that scope, then resolves those obligations. A task may move to another owner only when that owner can preserve its dependencies, capabilities, affinity, and async finalization requirements.

```bray
async func update(pos counter: &mut Counter)
{
    counter.value += 1;
}

async func scoped_update(pos counter: &mut Counter)
{
    let task: Task<unit> = update(counter).start();

    try await task.join();

    counter.value += 1;
}
```

The second mutation is legal only after joining releases the task's dependency on `counter`.

See [task handles and obligations](https://github.com/lejmer/bray/blob/develop/docs/language/async-and-concurrency/task-handles-and-obligations.md), [task transfers and escapes](https://github.com/lejmer/bray/blob/develop/docs/language/async-and-concurrency/task-transfers-and-escapes.md), and [structured task scope exit](https://github.com/lejmer/bray/blob/develop/docs/language/async-and-concurrency/structured-task-scope-exit.md).

## Execution requirements and entrypoints

Execution requirements are ordinary callable predicates. Synchronous calls must satisfy them immediately. Async calls defer the check until `await`, while `.start()` may route the new task to a compatible execution lane.

```bray
async func read_file(pos path: Path) -> Data
    requires(blocking_execution())
{
    return blocking_read(path);
}

async func analyze(pos data: Data) -> Report
    requires(compute_execution())
{
    return calculate_report(data);
}

func update_window(pos report: &Report)
    requires(main_thread_execution())
{
    render(report);
}

@entrypoint
async func main() -> Result<unit, ApplicationError>
{
    let loading: Task<Data> = read_file(input_path()).start();
    let data: Data = try await loading.join();

    let analysis: Task<Report> = analyze(data).start();
    let report: Report = try await analysis.join();

    update_window(&report);

    return Ok(unit);
}
```

An async entrypoint begins on a main-thread lane. That lane satisfies `main_thread_execution()`, but it does not automatically satisfy blocking or compute execution.

The entrypoint run is a product execution root. Product shutdown requests cancellation for every remaining root, resolves their structured task trees and async cleanup, then destroys represented state before the host returns or exits.

See [execution requirements](https://github.com/lejmer/bray/blob/develop/docs/language/async-and-concurrency/execution-requirements.md), [execution roots and product shutdown](https://github.com/lejmer/bray/blob/develop/docs/language/async-and-concurrency/execution-roots-and-product-shutdown.md), and [entrypoints and runtime](https://github.com/lejmer/bray/blob/develop/docs/language/async-and-concurrency/entrypoints-and-runtime.md).

## Cooperative cancellation

Cancellation is an idempotent request observed at explicit or implicit checkpoints. Cleanup runs under a shield so it can restore invariants without being interrupted by the same request.

```bray
async func process(pos items: &mut Queue<Item>) -> Result<unit, ProcessError>
{
    while !items.empty()
    {
        if std.task.cancellation_requested()
        {
            await std.task.checkpoint();
        }

        handle(items.pop());
        std.run.checkpoint();
        await std.task.yield_now();
    }

    return Ok(unit);
}
```

`std.run.cancellation_requested()` and `std.task.cancellation_requested()` observe the current logical run. `std.run.checkpoint()` is synchronous. The task checkpoint and yield operations are async and therefore must be awaited.

A cancellation request proves neither completion nor memory visibility.

See [cancellation](https://github.com/lejmer/bray/blob/develop/docs/language/async-and-concurrency/cancellation.md) and [cancellation and memory visibility](https://github.com/lejmer/bray/blob/develop/docs/language/async-and-concurrency/cancellation-and-memory-visibility.md).

## Cross-run state and synchronization

A value may cross a run boundary only when its type, ownership, and capabilities make that transfer valid. Copyability alone is not enough. Ordinary shared mutable state requires a synchronization contract, usually exposed through a scoped guard whose enter and exit establish the relevant memory ordering.

```bray
async func record(pos journal: &SynchronizedJournal, pos entry: Entry)
{
    with guard = journal.lock()
    {
        guard.entries.push(entry);
    }
}

async func record_both(pos journal: &SynchronizedJournal, pos first: Entry, pos second: Entry)
{
    let first_task: Task<unit> = record(journal, first).start();
    let second_task: Task<unit> = record(journal, second).start();

    try await first_task.join();
    try await second_task.join();
}
```

The `with` expression guarantees the guard exits on normal return, early return, panic, and cancellation cleanup. The synchronization semantics come from the guard type's contract, not from `with` itself. A blocking guard may cross suspension only when its contract explicitly permits it.

Starting a task synchronizes parent writes before child execution. Successful completion observation synchronizes child writes before the observer continues. Raw pointers, internal visibility acknowledgements, and trusted syntax do not create exceptions to cross-run memory validity.

See [cross-run memory model](https://github.com/lejmer/bray/blob/develop/docs/language/async-and-concurrency/cross-run-memory-model.md), [data-race prevention](https://github.com/lejmer/bray/blob/develop/docs/language/async-and-concurrency/data-race-prevention.md), [shared state and synchronization](https://github.com/lejmer/bray/blob/develop/docs/language/async-and-concurrency/shared-state-and-synchronization.md), and [capability transfer across run boundaries](https://github.com/lejmer/bray/blob/develop/docs/language/async-and-concurrency/capability-transfer-across-run-boundaries.md).

## Atomic storage and ordering

Use `std.atomic.Atomic<T>` for the safe ordinary atomic surface. Operations are available only for target-supported representations and never fall back to locks.

```bray
func new_counter() -> Atomic<u64>
{
    return std.atomic.atomic<u64>(0);
}

func atomic_surface(pos counter: &Atomic<u64>)
{
    let initial: u64 = std.atomic.load<u64>(counter, order = LoadOrder.Acquire);

    std.atomic.store<u64>(counter, initial + 1, order = StoreOrder.Release);

    let replaced: u64 = std.atomic.exchange<u64>(counter, 10, order = ReadModifyWriteOrder.AcquireRelease);

    let (observed, exchanged): (u64, bool) = std.atomic.compare_exchange<u64>(
        counter,
        replaced,
        20,
        order = CompareExchangeOrder.AcquireReleaseAcquire,
    );

    let (weak_observed, weak_exchanged): (u64, bool) = std.atomic.compare_exchange_weak<u64>(
        counter,
        observed,
        30,
        order = CompareExchangeOrder.AcquireReleaseAcquire,
    );

    let before_add: u64 = std.atomic.fetch_add<u64>(counter, 2, order = ReadModifyWriteOrder.AcquireRelease);
    let before_sub: u64 = std.atomic.fetch_sub<u64>(counter, 1, order = ReadModifyWriteOrder.AcquireRelease);
    let before_and: u64 = std.atomic.fetch_and<u64>(counter, 0xff, order = ReadModifyWriteOrder.AcquireRelease);
    let before_or: u64 = std.atomic.fetch_or<u64>(counter, 0x10, order = ReadModifyWriteOrder.AcquireRelease);
    let before_xor: u64 = std.atomic.fetch_xor<u64>(counter, 0x01, order = ReadModifyWriteOrder.AcquireRelease);

    std.atomic.fence(order = FenceOrder.SequentiallyConsistent);
    std.atomic.compiler_fence(order = FenceOrder.AcquireRelease);

    observe_atomic_results(
        exchanged,
        weak_observed,
        weak_exchanged,
        before_add,
        before_sub,
        before_and,
        before_or,
        before_xor,
    );
}

func wait_until_ready(pos ready: &Atomic<bool>)
{
    while !std.atomic.load<bool>(ready, order = LoadOrder.Acquire)
    {
        std.atomic.wait<bool>(ready, false, order = LoadOrder.Acquire);
    }
}

func publish_one(pos ready: &Atomic<bool>)
{
    std.atomic.store<bool>(ready, true, order = StoreOrder.Release);
    std.atomic.notify_one<bool>(ready);
}

func publish_all(pos ready: &Atomic<bool>)
{
    std.atomic.store<bool>(ready, true, order = StoreOrder.Release);
    std.atomic.notify_all<bool>(ready);
}
```

Loads cannot use release ordering. Stores cannot use acquire ordering. Compare-exchange failure ordering may not contain release semantics and may not be stronger than success ordering. Weak compare-exchange may fail spuriously. Atomic wait may also wake spuriously, and notification is advisory rather than a memory-ordering operation.

See [atomic operation contracts](https://github.com/lejmer/bray/blob/develop/docs/language/async-and-concurrency/atomic-operation-contracts.md).

## Standard-library concurrency

The concurrency library exposes ordinary declarations for one-time initialization, bounded channels, task combinators, threads, child processes, and typed parallelism budgets.

```bray
static CONFIGURATION: std.sync.Once<Configuration> = std.sync.Once<Configuration>.empty();

func configuration() -> &Configuration
{
    return CONFIGURATION.get_or_init(load_configuration);
}

async func channel_round_trip(pos first: Message, pos second: Message) -> Result<unit, ChannelError>
{
    let (sender, receiver) = try std.channel.bounded<Message>(capacity = 2);

    let duplicate = sender.duplicate();

    let first_send = sender.send(first).start();
    let second_send = duplicate.try_send(second);
    let received = await receiver.receive();
    let first_result = await first_send.join();

    inspect_channel(first_result, second_send, received);

    duplicate.close();
    sender.close();
    receiver.close();

    return Ok(unit);
}

async func race(pos left: Request, pos right: Request) -> RunResult<Response>
{
    return await std.concurrent.first<Response, 2>([fetch(left), fetch(right)]);
}

async func collect(pos left: Request, pos right: Request) -> [RunResult<Response>; 2]
{
    return await std.concurrent.all<Response, 2>([fetch(left), fetch(right)]);
}

func hash_on_thread(pos data: Data) -> Result<Hash, ThreadError>
    requires(blocking_execution())
{
    let thread = try std.thread.start<Data, Hash>(hash_entry, data);

    return Ok(try thread.join());
}

async func hash_without_blocking(pos data: Data) -> Result<Hash, ThreadError>
{
    return await std.thread.run<Data, Hash>(hash_entry, data);
}

async func run_child(pos program: Program<Request, Response>, pos request: Request) -> Result<Response, ProcessError>
{
    let process = try await std.process.start<Request, Response>(program, request);
    let child_run = try await process.join();

    return Ok(try child_run);
}

async func budgeted(pos budget: &Budget<TaskDomain>) -> RunResult<Report>
{
    let permit: Permit<TaskDomain> = await budget.acquire();
    let task: Task<Report> = create_report().start();
    let result: RunResult<Report> = await task.join();

    permit.release();

    return result;
}
```

`Once<T>` publishes one initialized value and retries after initializer failure. Bounded channels make backpressure explicit. `first` cancels and resolves losers before returning, while `all` returns every run result in input order. Threads and processes remain structured owners. Child processes communicate through codecs and expose both transport failure and child `RunResult`. Budgets provide domain-specific admission rather than global execution authority, and they do not automatically charge `.start()`.

See [standard-library concurrency](https://github.com/lejmer/bray/blob/develop/docs/language/async-and-concurrency/standard-library-concurrency.md) and [low-level runtime](https://github.com/lejmer/bray/blob/develop/docs/language/async-and-concurrency/low-level-runtime.md).

## Choose the boundary

| Need                                                   | Use                                            | Result boundary                                              |
|--------------------------------------------------------|------------------------------------------------|--------------------------------------------------------------|
| Continue one async computation directly                | `await future`                                 | `T`                                                          |
| Run async work independently within a structured owner | `future.start()`                               | `Task<T>`                                                    |
| Observe or cancel a task                               | `task.join()` or `task.cancel()`               | `Future<RunResult<T>>`                                       |
| Coordinate several futures                             | `std.concurrent.first` or `std.concurrent.all` | `Future<RunResult<T>>` or ordered run results                |
| Run blocking or thread-affine native work              | `std.thread`                                   | `Result<T, ThreadError>` or `Future<Result<T, ThreadError>>` |
| Isolate work in another process                        | `std.process`                                  | `Result<RunResult<T>, ProcessError>`                         |
| Share mutable state across runs                        | Synchronization owner or atomics               | Guarded access or atomic operation result                    |
| Bound resource admission                               | `std.parallel.Budget<D>`                       | `Permit<D>`                                                  |

**Remember:** Async calls create inactive owned futures. Await composes one future into the current run. Start creates a structured task whose owner must eventually join, cancel, transfer, or resolve it at scope exit. Cross-run state must remain valid for the whole run, and visibility comes from task boundaries or explicit synchronization contracts rather than scheduling accidents.
