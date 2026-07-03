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

Contract clauses are parenthesized comma-separated lists.

Predicate expressions, fact contexts, trusted obligations, and contract clauses use the contract and trust rules.

## Navigation

- [Language index](../index.md)
- [Callables index](../callables.md)
- Previous: [Generic functions](generic-functions.md)
- Next: [Trusted functions](trusted-functions.md)
