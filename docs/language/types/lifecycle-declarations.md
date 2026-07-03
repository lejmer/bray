# Lifecycle declarations

Lifecycle declarations attach construction, finalization, destruction, or scoped-use behavior to a type.

Lifecycle declarations can be declared in a type body or in an inherent implementation for that type.

The lifecycle declaration kinds are:

- `construct`,
- `finalize`,
- `destruct`,
- `enter`,
- `exit`.

Constructors create fully initialized values.

Finalizers complete required lifecycle obligations before ownership ends.

Destructors perform synchronous cleanup when ownership ends.

Scope enter and scope exit declarations define scoped capability behavior for `with` expressions.

Lifecycle declarations participate in ownership, borrowing, mutation authority, finalization obligations, effects, trusted capability checking, and contract checking.

## Lifecycle selection

`Self` in a lifecycle declaration means the declaring type.

In an inherent implementation, the implementation subject must be the declaring type.

For a given type, lifecycle kind, and lifecycle path, at most one participating lifecycle declaration can be visible in a coherence domain.

Lifecycle declaration selection uses the type whose value is being constructed, finalized, destroyed, entered, or exited.

## Lifecycle ordering

When all lifecycle kinds apply to the same value, the lifecycle order is:

```text
construct -> ordinary use -> enter -> with body -> exit -> finalize -> destruct -> represented-part destruction
```

Not every value passes through every lifecycle step.

`enter` and `exit` occur only for scoped use through a `with` expression.

`finalize` occurs only for values with finalization obligations.

`destruct` occurs when ownership ends for a fully initialized value.

Represented-part destruction is field destruction for product types and active-payload destruction for union types.

Partial values do not run whole-value finalizers, whole-value destructors, or whole-value scope enter/exit behavior.

Panic propagation and cancellation use the same lifecycle ordering as ordinary scope exit.

## Constructors

A constructor with no name after `construct` is the primary constructor form.

A constructor with a name after `construct` becomes a named constructor under the type.

Constructor bodies have no `self` binding.

A constructor body must produce a fully initialized `Self` value or a `Result.Ok` carrying a fully initialized `Self` value.

A constructor body can produce that value with a construction expression, another constructor call, or another expression whose result type is `Self`.

Constructor failure through `Result.Error`, panic, cancellation, or another non-success exit does not produce a value.

Values, temporaries, and partially initialized storage created before such an exit are resolved by ordinary ownership, destruction, and finalization rules.

A successfully constructed value carries:

- lifecycle obligations declared by the type,
- lifecycle obligations of initialized represented parts,
- lifecycle obligations produced by defaults or constructor body expressions.

Defaults used during construction are evaluated before the value becomes fully initialized.

## Finalizers

Finalizer bodies have a compiler-introduced `self` binding for the whole value being finalized.

The finalizer has exclusive lifecycle authority over `self` for the duration of the finalizer.

A finalizer can observe and mutate represented parts when its declaration contract permits those operations.

A finalizer cannot let `self`, a represented-part access path, a borrow from `self`, or a capability derived from `self` escape unless the finalizer contract explicitly transfers the corresponding obligation.

A finalizer must return with the value fully initialized.

If a finalizer returns `Result.Error`, the finalization obligation remains unresolved.

A value with an unresolved finalization obligation cannot be destroyed.

## Destructors

Destructor bodies have a compiler-introduced `self` binding for the whole value being destroyed.

The destructor has exclusive destruction authority over `self` for the duration of the destructor.

A destructor is synchronous and infallible.

A destructor can observe and mutate represented parts when its declaration contract permits those operations.

A destructor cannot create a finalization obligation that remains unresolved after the destructor returns.

A destructor cannot let `self`, a represented-part access path, a borrow from `self`, or a capability derived from `self` escape.

If a destructor consumes or destroys a represented part, that part becomes uninitialized and is not destroyed again after the destructor returns.

Any initialized represented parts remaining after the destructor returns are destroyed in the type's represented-part destruction order.

## Scope enter and exit

Scope enter bodies have a compiler-introduced `self` binding for the access path used as the `with` initializer.

The selected enter declaration must be able to satisfy its declared ownership, borrowing, mutation, capability, effect, trusted, and lifecycle requirements from that access path.

The successful enter result is the scoped capability matched by the `with` pattern.

The scoped capability can borrow from the value, carry access authority for the value or represented parts, or carry an independent resource token according to the scoped capability type.

Scope exit bodies receive the scoped capability produced by the matching enter declaration.

Exit operates on the scoped capability.

Exit can reach the original value only through access carried by that scoped capability.

An active scoped capability can restrict observation, mutation, borrowing, movement, finalization, destruction, replacement, and partial moves of the value for the lifetime of the `with` body.

## API compatibility

Changing lifecycle declarations can be a public API change when ownership, destruction, finalization, construction, or scoped-use behavior changes.

## Navigation

- [Language index](../index.md)
- [Types index](../types.md)
- Previous: [Copy contracts](copy-contracts.md)
- Next: [Product Types](product-types.md)
