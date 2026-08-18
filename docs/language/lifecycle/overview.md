# Overview

Lifecycle behavior controls how values are constructed, used, finalized, destroyed, and entered for scoped use.

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

Lifecycle declarations participate in ownership, borrowing, mutation authority, finalization obligations, effects,
trusted capability checking, and contract checking.

Lifecycle rules apply to product types, union types, implementation-eligible type forms with lifecycle behavior, and
compiler-known types whose language-defined contract includes lifecycle behavior.

They also apply when a product host or exact native-thread attachment resolves a static owner. Every materialized static
instance has one cleanup owner and one deterministic position in its instantiated lifecycle dependency graph.

## Navigation

- [Language index](../index.md)
- [Lifecycle index](../lifecycle.md)
- Next: [Lifecycle declarations](lifecycle-declarations.md)
