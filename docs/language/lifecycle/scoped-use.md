# Scoped use

Scoped use is lifecycle behavior entered by a `with` expression.

Scope enter declarations define how a value enters scoped use.

Scope exit declarations define how scoped use is left.

Scope enter bodies have a compiler-introduced `self` binding for the access path used as the `with` initializer.

The selected enter declaration must be able to satisfy its declared ownership, borrowing, mutation, capability, effect, trusted, and lifecycle requirements from that access path.

The successful enter result is the scoped capability matched by the `with` pattern.

The scoped capability can:

- borrow from the value,
- carry access authority for the value,
- carry access authority for represented parts,
- carry an independent resource token.

The scoped capability type determines which of those behaviors applies.

Scope exit bodies receive the scoped capability produced by the matching enter declaration.

Exit operates on the scoped capability.

Exit can reach the original value only through access carried by that scoped capability.

An active scoped capability can restrict observation, mutation, borrowing, movement, finalization, destruction, replacement, active-variant replacement, and partial moves of the value for the lifetime of the `with` body.

Fallible `enter` or `exit` behavior contributes its failure contract to the `with` expression.

Asynchronous `enter` or `exit` behavior contributes its execution contract to the `with` expression.

The surrounding context must be able to satisfy the `with` expression's type, failure, execution, effect, capability, task-obligation, and lifecycle contract.

## Navigation

- [Language index](../index.md)
- [Lifecycle index](../lifecycle.md)
- Previous: [Destruction](destruction.md)
- Next: [With expressions](with-expressions.md)
