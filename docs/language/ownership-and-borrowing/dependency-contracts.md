# Dependency contracts

Bray does not have source-level lifetime parameters or source-level lifetime annotations.

Lifetime and capability dependency contracts are semantic facts inferred and checked by the compiler.

A dependency contract records the non-local requirements that must remain true for a value, access path, callable value, trait view, task handle, thread handle, or stored field to remain valid.

A dependency contract can include:

- storage that must remain alive,
- storage that must remain initialized,
- borrow capability that must remain active,
- mutation authority that must remain exclusive,
- scoped capability that must remain live,
- finalization or destruction obligations that must remain attached to the value,
- facts whose validity depends on the same storage, capability, or ownership state.

A dependency contract is not part of surface syntax.

It is part of the compiler-visible semantic contract of the value or declaration that carries it.

For exported declarations, compiled interfaces, documentation, diagnostics, incremental compilation, and separate compilation, the compiler records the inferred dependency contract as interface metadata.

Two compilers must reject and accept the same programs according to these dependency-contract rules, even though the source code does not write those contracts explicitly.

Each expression that produces a value or access path also produces a dependency contract.

Owned values carry the dependency contracts of their initialized subvalues.

Borrow values carry the reached storage, borrow capability, and invalidation requirements of the borrow.

Nullable values carry the dependency contract of their contained value only while present.

Product, union, tuple, array, and box values carry the dependency contracts of the parts they currently own or borrow.

Callable values carry the dependency contract of the callable declaration or lambda expression that produced them.

Lambda expressions do not capture enclosing local state.

A trait view carries the dependency contract of the access or storage form that contains the view, plus the requirements of the implementation witness needed for the selected trait application.

A task handle carries the dependency contract of the captured task state and the task obligation represented by the handle.

A thread handle carries the dependency contract of the captured thread state and the thread obligation represented by the handle.

Moving a value moves its dependency contract with the value.

Destroying, finalizing, cancelling, joining, assigning `none`, or otherwise resolving a value resolves or invalidates the dependency contract carried by that value according to the operation's contract.

## Navigation

- [Language index](../index.md)
- [Ownership and borrowing index](../ownership-and-borrowing.md)
- Previous: [Reborrowing and borrow values](reborrowing-and-borrow-values.md)
- Next: [Partial moves](partial-moves.md)
