# Contract clauses

Contract clauses are parenthesized comma-separated lists.

```bray
requires(
    length <= capacity,
    trusted core.memory.owned_allocation(pointer = pointer, bytes = capacity, align = align),
)
```

The same list form applies to `requires(...)`, `ensures(...)`, `with(...)`, and `uses(...)`.

```bray
ensures(
    result.length == length,
    result.capacity == capacity,
)

uses(raw_memory, manual_alloc)
```

A `requires(...)` clause lists facts that must hold before the declaration body executes.

An `ensures(...)` clause lists facts established after the declaration completes normally.

Inside an `ensures(...)` clause for a declaration that completes with a value, `result` is the compiler-introduced postcondition binding for that normal completion value.

`result` is available only in postcondition predicate contexts.

`result` is not available in `requires(...)`, `with(...)`, predicate declaration bodies, guard expressions, ordinary expression contexts, or declarations whose normal completion does not produce a value.

Bray does not support named result bindings.

A `with(...)` clause lists static constraint facts required by a generic declaration.

Predicate expressions in callable contract clauses are [declaration-owned expressions](../declarations/declaration-owned-expressions.md).
They are checked with the declaration whose semantic contract they define.

A `uses(...)` clause lists trusted implementation capabilities used by a trusted declaration body.

Contract clauses are part of a declaration's semantic surface when they affect callers, implementers, dynamic dispatch, separate compilation, or public API compatibility.

## Navigation

- [Language index](../index.md)
- [Contracts and trust index](../contracts-and-trust.md)
- Previous: [Overview](overview.md)
- Next: [Ordinary requirements and ensured facts](ordinary-requirements-and-ensured-facts.md)
