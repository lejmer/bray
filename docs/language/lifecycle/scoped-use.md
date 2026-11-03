# Scoped use

Scoped use is lifecycle behavior entered by a `with` expression. Scope enter declarations define how a value enters
that use, and scope exit declarations define how it leaves.

Scope enter bodies have a compiler-introduced `self` binding for the access path used as the `with` initializer.

The enter declaration's receiver modifiers determine the required access. No modifier requires shared access, `mut`
requires mutable access, `consume` consumes the value, and `consume mut` also grants mutable local authority during
entry.

The selected enter declaration must be able to satisfy its declared ownership, borrowing, mutation, capability, effect,
trusted, and lifecycle requirements from that access path.

The successful enter result is the scoped capability matched by the `with` pattern.

The scoped capability can:

- borrow from the value,
- carry access authority for the value,
- carry access authority for represented parts,
- carry an independent resource token.

The scoped capability type determines which of those behaviors applies.

Scope exit bodies operate on the scoped capability produced by the matching enter declaration. Exit has no `self`
receiver, and its capability parameter is its only value input. It can reach the original value only through access
carried by that capability.

An active scoped capability can restrict observation, mutation, borrowing, movement, finalization, destruction,
replacement, active-variant replacement, and partial moves of the value for the lifetime of the `with` body.

Fallible `enter` or `exit` behavior contributes its failure contract to the `with` expression, and asynchronous behavior
contributes its execution contract. The surrounding context must satisfy the resulting type, failure, execution,
effect, capability, task-obligation, and lifecycle contract under the [`with` expression rules](with-expressions.md).

## Navigation

- [Language index](../index.md)
- [Lifecycle index](../lifecycle.md)
- Previous: [Destruction](destruction.md)
- Next: [With expressions](with-expressions.md)
