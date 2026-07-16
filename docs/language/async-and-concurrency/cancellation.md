# Cancellation

Cancellation is a cooperative abnormal exit from an incomplete run. Every executable root, async task, standard-library native
thread, and conforming child-process root has a logical cancellation state.

A run enters cancellation in either of two ways:

- an owner or host requests cancellation and the run later observes the pending request,
- `try RunResult.Cancelled` forwards an already observed child-run cancellation into the current run.

A request marks the run's cancellation state and wakes it when its execution domain supports a cancellation-aware wait. Repeated
requests are idempotent. Requesting cancellation does not itself prove completion.

Forwarding `RunResult.Cancelled` marks the current run as cancellation-requested and immediately commits the current control path to
cancellation cleanup. It does not wait for another checkpoint. Lifecycle bodies therefore observe cancellation as requested while
resolving that path.

The ordinary standard-library run surface is semantically equivalent to:

```bray
func cancellation_requested() -> bool;
func checkpoint();
```

These declarations belong to `std.run`. `cancellation_requested()` reports the current run's logical request state.
`checkpoint()` enters cancellation when a request is pending and otherwise returns normally. Neither declaration is compiler-known.

Each run domain adds its own observation operations:

- an async task observes before await suspension, after await resumption, at `std.task.checkpoint()`, and in cancellation-aware async
  operations;
- a native thread observes at `std.thread.checkpoint()`, `std.run.checkpoint()`, and cancellation-aware blocking operations;
- an async executable root uses the task observation rules on its distinguished main-thread lane;
- a synchronous executable root observes at `std.run.checkpoint()` and cancellation-aware synchronous standard-library operations;
- a conforming child-process host maps a parent cancellation message into cancellation of its executable root;
- a trusted foreign execution root observes only at points declared by its trusted contract.

After observation or forwarding, the run stops ordinary body execution and resolves initialized state through abnormal-exit
lifecycle ordering. An async task also cancels its currently directly awaited child computation. Cancellation does not produce the
callable's ordinary result type. An uncaught cancellation reaches the current run boundary as `RunResult.Cancelled` unless cleanup
panics or normal completion won a permitted request race before cancellation was committed.

Pure computation containing no checkpoint or cancellation-aware operation can delay a request indefinitely. Bray does not preempt
arbitrary source instructions. Implementations should diagnose evident async loops or long-running computation paths with no
observation opportunity.

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
4. If cleanup panics, the run boundary reports `RunResult.Panicked`; an already active panic is retained as the primary report and
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

A lifecycle body that needs to select graceful behavior can call `std.run.cancellation_requested()`. Task- and thread-specific
helpers delegate to the same logical state. These functions are ordinary standard-library declarations, not keywords or
compiler-known source names.

## Navigation

- [Language index](../index.md)
- [Async and concurrency index](../async-and-concurrency.md)
- Previous: [Structured task scope exit](structured-task-scope-exit.md)
- Next: [Execution requirements](execution-requirements.md)
