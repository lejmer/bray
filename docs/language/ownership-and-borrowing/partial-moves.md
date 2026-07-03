# Partial moves

A partial move moves an initialized subpart out of a value while leaving other initialized subparts under the original owner.

Moving a field, payload field, tuple element, array element, or other move-eligible subpart is an ownership operation.

A partial move requires:

- ownership of the containing value,
- an initialized moved subpart,
- no conflicting active borrow of the containing value or moved subpart,
- type-specific permission for moving that subpart.

After a partial move, the containing value is partially initialized.

The moved subpart becomes moved from inside the containing value.

Still-initialized subparts remain governed by their own ownership and destruction rules.

A partially moved value can be reinitialized or consumed by a rule that accounts for its partial state.

A partially moved value can be destroyed as a partial value.

Destruction of a partially moved value destroys only initialized subparts.

A partially moved value can become fully initialized again when all moved-from subparts are reinitialized and the storage and type contract permit reinitialization.

A value with whole-value lifecycle behavior must be fully initialized whenever a whole-value lifecycle declaration can run.

A partial move from such a value is valid only when every reachable path reinitializes the moved subpart before:

- the value is finalized,
- the value is destroyed as a complete value,
- the value is used as a `with` initializer,
- the value is moved, copied, consumed, borrowed, or observed as a complete value,
- ownership of the value can end.

For unions, the moved active payload field must also be reinitialized before the active variant is replaced.

If the compiler cannot prove that the value becomes fully initialized before one of those events, the partial move is rejected.

Partial storage resolution never runs whole-value finalizers, whole-value destructors, or whole-value scope enter or exit behavior.

## Navigation

- [Language index](../index.md)
- [Ownership and borrowing index](../ownership-and-borrowing.md)
- Previous: [Dependency contracts](dependency-contracts.md)
- Next: [Scope exits and ownership boundaries](scope-exits-and-ownership-boundaries.md)
