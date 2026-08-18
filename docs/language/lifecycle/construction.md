# Construction

Constructors create fully initialized values of `Self`.

Constructor bodies have no `self` binding.

A constructor body must produce a fully initialized `Self` value or a `Result.Ok` carrying a fully initialized `Self`
value.

A constructor body can produce that value with:

- a construction expression,
- another constructor call,
- another expression whose result type is `Self`.

Constructor failure through `Result.Error`, panic, cancellation, or another non-success exit does not produce a value.

Values, temporaries, and partially initialized storage created before such an exit are resolved by ordinary ownership,
destruction, and finalization rules.

A successfully constructed value carries:

- lifecycle obligations declared by the type,
- lifecycle obligations of initialized represented parts,
- lifecycle obligations produced by defaults,
- lifecycle obligations produced by constructor body expressions.

Defaults used during construction are evaluated before the value becomes fully initialized.

Initialization performed by construction is initialization, not ordinary mutation.

A value being constructed has no stable observable identity until construction is complete.

Field and payload mutability controls post-initialization mutation.

It does not restrict initialization of represented parts during construction.

Constructors can be trusted when they use trusted implementation capabilities or expose trusted caller obligations.

Trusted constructor behavior follows [Contracts and trust](../contracts-and-trust.md).

## Navigation

- [Language index](../index.md)
- [Lifecycle index](../lifecycle.md)
- Previous: [Lifecycle ordering](lifecycle-ordering.md)
- Next: [Finalization](finalization.md)
