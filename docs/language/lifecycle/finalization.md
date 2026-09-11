# Finalization

Finalizers complete required lifecycle obligations before ownership ends.

Finalization is used for fallible or asynchronous cleanup.

Fallible or asynchronous cleanup belongs to finalization, not destruction.

A value with finalization obligations must satisfy those obligations before ownership ends unless the value is
transferred to another owner that assumes them or converted into an explicit fallback ownership form.

Finalizer bodies have a compiler-introduced `self` binding for the whole value being finalized.

That binding is an implicit mutable receiver. Finalization does not consume the receiver because a successful or failed
finalizer leaves the value fully initialized and owned until its lifecycle obligation is resolved.

The finalizer has exclusive lifecycle authority over `self` for the duration of the finalizer.

A finalizer can observe and mutate represented parts when its declaration contract permits those operations.

A finalizer cannot let `self`, a represented-part access path, a borrow from `self`, or a capability derived from `self`
escape unless the finalizer contract explicitly transfers the corresponding obligation.

A finalizer must return with the value fully initialized.

The finalizer's checked postconditions determine completion state after `Result.Error`. The error remains observable
even when those postconditions establish completion. A retryable error retains or returns the unresolved owner.

A value with an unresolved finalization obligation cannot be destroyed during ordinary execution. Panic and cancellation
cleanup can apply the language-defined abandonment fallback after attempting the finalizer.

A finalizer can return `unit` or `Result<unit, E>`.

A finalizer can be synchronous or asynchronous.

The surrounding context must be able to drive asynchronous finalization before ownership ends, transfer the obligation,
or convert it into an explicit fallback ownership form.

## Completion through ordinary operations

Resource-specific operations such as `flush()`, `close()`, `finish()`, or `join()` establish completion through their
checked postconditions. Completion state can be described by an ordinary predicate.

When available conditions prove that the selected whole-value finalizer satisfies `executes(pure, total)` and returns
`unit` or the `Ok(unit)` case of its permitted `Result` type, its graceful step is discharged without invocation.
The compiler applies this rule before optimization, in both debug and optimized builds.

Note: These are the general [conditional execution guarantees](../contracts-and-trust/execution-guarantees.md),
not a second finalizer body.

This proof discharges only that whole-value step. Receiver acquisition, storage-policy projection, represented parts,
result disposition, destructors, release, child runs, and dependent owners retain their own obligations. No arbitrary
result or owner is synthesized from a postcondition. A discharged async finalizer creates no inactive frame, but an
existing `Future<T>` is never driven early or reinterpreted as synchronous execution.

A synchronous scope is legal only when all remaining cleanup is synchronous and otherwise admissible. Execution-lane
requirements remain requirements even when a graceful step has no work. Pending possibly fallible implicit
finalization is rejected on normal ownership end until ordinary operations or ownership transfer resolve it.

Moves transfer responsibility and value-dependent conditions that remain valid at the destination. Mutation invalidates
every affected completion condition. Branch joins preserve only common available conditions. Wrapping an owner in a
union or error transfers its obligation to the resulting owner. Condition validity follows the
[contract and borrow validity rules](../ownership-and-borrowing/contract-and-borrow-validity.md).

Successful finalizer execution can also resolve the current obligation. A later relevant mutation can make the
completion proof unavailable.

Statics, partial values, replacement, arrays, storage-policy values, and active union payloads obey the same recursive
ownership rules. Active, initialized represented parts participate in cleanup.

## Abnormal exit

During panic or cancellation cleanup, a finalizer returning `Result.Error` records an owned suppressed cleanup incident
and permits the synchronous destructor and represented-part destruction to run as the universal abnormal-exit fallback.
This fallback is restricted to abnormal exit.

Cleanup-incident ownership, reporting, ordering, and destruction are defined by
[Cancellation](../async-and-concurrency/cancellation.md#fallible-finalization-during-abnormal-exit).

## Navigation

- [Language index](../index.md)
- [Lifecycle index](../lifecycle.md)
- Previous: [Construction](construction.md)
- Next: [Destruction](destruction.md)
