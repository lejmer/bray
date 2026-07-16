# Cancellation

Cancellation is a cooperative abnormal exit from an incomplete async task and from child computations composed into that task.

Driving the computation returned by `Task<T>.cancel()` and structured scope exit request task cancellation. A request sets the
task's cancellation state and wakes the task if it is suspended in a cancellation-aware runtime operation. Repeated requests are
idempotent.

A task observes a pending request:

- before an await suspends,
- after an await resumes,
- at `std.task.checkpoint()`,
- in cancellation-aware standard-library and runtime operations.

After observation, the task stops ordinary body execution, cancels the currently awaited child computation, and resolves initialized
frame state through abnormal-exit lifecycle ordering. Cancellation does not produce the callable's ordinary result type. The task
boundary reaches `RunResult.Cancelled` unless cleanup panics or normal completion won the race before cancellation was committed.

Pure computation that contains no await, checkpoint, or cancellation-aware operation can delay cancellation indefinitely. Bray does
not preempt arbitrary source instructions. Implementations should diagnose async loops or long-running computation paths with no
suspension or checkpoint opportunity.

## Cleanup shielding

Cancellation cleanup executes in a shielded context. The original request remains observable, but automatic delivery of further
cancellation is masked until cleanup completes. Cleanup can await asynchronous finalizers and runtime operations. Repeated requests
do not interrupt a finalizer halfway through.

## Fallible finalization during abnormal exit

Normal exit preserves the ordinary rule that `Result.Error` from a finalizer leaves its obligation unresolved and prevents
destruction.

Cancellation and panic cleanup provide the universal abandonment path:

1. Attempt the ordinary finalizer in shielded cleanup.
2. If it succeeds, continue to ordinary destruction.
3. If it returns `Result.Error`, record the error as a suppressed cleanup incident, abandon graceful finalization, and run the
   synchronous infallible destructor and represented-part destruction anyway.
4. If cleanup panics, the task boundary reports `RunResult.Panicked`; an already active panic is retained as the primary report and
   later cleanup panics are attached as suppressed reports.

A cleanup incident is an owned, type-erased runtime record containing the finalizer error value, its concrete type descriptor, the
finalizer and source location that produced it, and its deterministic encounter ordinal. Cleanup owns incidents in reverse
lifecycle encounter order until the surrounding run boundary is resolved. Creating the incident is the language-defined explicit
abandonment representation for that error payload: its graceful finalization obligation has already been abandoned, so the
descriptor performs only synchronous infallible destruction when incident ownership ends.

If cancellation remains the terminal outcome, `RunResult.Cancelled` intentionally remains payload-free: source callers do not gain
an unbounded union of arbitrary finalizer error types. Instead, observing or automatically resolving that boundary transfers the
ordered incidents and any suppressed child-run panic reports to the product's mandatory host cleanup-report sink. The host and
runtime inspection tooling can report each entry's kind, type, origin, and ordinal; richer error rendering is available only when
the error type's ordinary diagnostic contract provides it. The sink consumes each error value or panic report and runs its
infallible destruction after reporting. It must not silently discard an entry.

If cleanup panics, the terminal outcome is `RunResult.Panicked`. The cleanup panic becomes the primary `PanicReport` unless a panic
was already active; non-panic incidents and later panics are retained as ordered suppressed entries owned by that report. Destroying
a `PanicReport` resolves every attached entry. A non-panic finalizer error alone does not change `Cancelled` into `Panicked`.

Destructors therefore remain synchronous, infallible, and last-resort representational cleanup. No cancellation-specific lifecycle
declaration, parameter, or modifier exists.

A lifecycle body that needs to select graceful behavior can call `std.task.cancellation_requested()`. That function is an ordinary
standard-library declaration, not a keyword or compiler-known source name.

## Navigation

- [Language index](../index.md)
- [Async and concurrency index](../async-and-concurrency.md)
- Previous: [Structured task scope exit](structured-task-scope-exit.md)
- Next: [Execution requirements](execution-requirements.md)
