# Contract clauses on functions

Functions can have contract clauses.

```bray
func clamp(pos value: i32, min: i32, max: i32) -> i32
    requires(
        min <= max,
    )
    ensures(
        result >= min,
        result <= max,
    )
{
    ...
}
```

`requires(...)` declares preconditions.

`ensures(...)` declares postconditions.

Postconditions can reference the compiler-introduced `result` binding for the declaration's normal completion value.

Bray does not support named result bindings.

Contract clause semantics are defined in [Contract clauses](../contracts-and-trust/contract-clauses.md).

Ordinary requirements and ensured facts are defined in [Ordinary requirements and ensured facts](../contracts-and-trust/ordinary-requirements-and-ensured-facts.md).

Predicate expressions, fact contexts, and trusted obligations are defined in [Contracts and trust](../contracts-and-trust.md).

## Navigation

- [Language index](../index.md)
- [Callables index](../callables.md)
- Previous: [Generic functions](generic-functions.md)
- Next: [Trusted functions](trusted-functions.md)
