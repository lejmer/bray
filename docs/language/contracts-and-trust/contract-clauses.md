# Contract clauses

Contract lists use parentheses and commas.

```bray
requires(
    length <= capacity,
    trusted core.memory.owned_allocation(pointer = pointer, bytes = capacity, align = align),
)
```

The same list form applies to `requires(...)`, `ensures(...)`, `with(...)`, and `uses(...)`.

`executes(...)` lists execution-property names. A `when(condition) { ... }` group contains conditional guarantees.
It accepts only `executes(...)`, `ensures(...)`, and
nested `when(...)` groups. See [conditional execution guarantees](execution-guarantees.md) for entry-state semantics,
property verification, and callable compatibility.

```bray
ensures(
    result.length == length,
    result.capacity == capacity,
)

uses(raw_memory, manual_alloc)
```

A `requires(...)` clause lists conditions that must hold before the declaration body executes.

An `ensures(...)` clause lists conditions established after the declaration completes normally.

Inside an `ensures(...)` clause for a declaration that completes with a value, `result` is the compiler-introduced
postcondition binding for that normal completion value.

`result` is available only in postcondition predicate contexts.

`result` is not available in `requires(...)`, `with(...)`, predicate declaration bodies, guard expressions, ordinary
expression contexts, or declarations whose normal completion does not produce a value.

Bray does not support named result bindings.

A `with(...)` clause lists static constraint conditions required by a generic declaration.

Predicate expressions in callable contract clauses are
[declaration-owned expressions](../declarations/declaration-owned-expressions.md). They are checked with the declaration
whose semantic contract they define.

A `uses(...)` clause lists trusted implementation capabilities used by a trusted declaration body.

Contract clauses are part of a declaration's semantic surface when they affect callers, implementers, dynamic dispatch,
separate compilation, or public API compatibility.

## Navigation

- [Language index](../index.md)
- [Contracts and trust index](../contracts-and-trust.md)
- Previous: [Overview](overview.md)
- Next: [Ordinary requirements and postconditions](preconditions-and-postconditions.md)
