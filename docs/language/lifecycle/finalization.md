# Finalization

Finalizers complete required lifecycle obligations before ownership ends.

Finalization is used for fallible or asynchronous cleanup.

Fallible or asynchronous cleanup belongs to finalization, not destruction.

A value with finalization obligations must satisfy those obligations before ownership ends unless the value is transferred to another owner that assumes them or converted into an explicit fallback ownership form.

Finalizer bodies have a compiler-introduced `self` binding for the whole value being finalized.

That binding is an implicit mutable receiver. Finalization does not consume the receiver because a successful or failed finalizer
leaves the value fully initialized and owned until its lifecycle obligation is resolved.

The finalizer has exclusive lifecycle authority over `self` for the duration of the finalizer.

A finalizer can observe and mutate represented parts when its declaration contract permits those operations.

A finalizer cannot let `self`, a represented-part access path, a borrow from `self`, or a capability derived from `self` escape unless the finalizer contract explicitly transfers the corresponding obligation.

A finalizer must return with the value fully initialized.

If a finalizer returns `Result.Error`, the finalization obligation remains unresolved.

A value with an unresolved finalization obligation cannot be destroyed during ordinary execution. Panic and cancellation cleanup
can apply the language-defined abandonment fallback after attempting the finalizer.

A finalizer can return `unit` or `Result<unit, E>`.

A finalizer can be synchronous or asynchronous.

The surrounding context must be able to drive asynchronous finalization before ownership ends, transfer the obligation, or convert it into an explicit fallback ownership form.

During panic or cancellation cleanup, a finalizer returning `Result.Error` records an owned suppressed cleanup incident and permits
the synchronous destructor and represented-part destruction to run as the universal abnormal-exit fallback. This fallback is not
available to ordinary source-level scope exit and cannot be explicitly invoked to ignore a finalization failure.

Cleanup-incident ownership, reporting, ordering, and destruction are defined by
[Cancellation](../async-and-concurrency/cancellation.md#fallible-finalization-during-abnormal-exit).

## Navigation

- [Language index](../index.md)
- [Lifecycle index](../lifecycle.md)
- Previous: [Construction](construction.md)
- Next: [Destruction](destruction.md)
