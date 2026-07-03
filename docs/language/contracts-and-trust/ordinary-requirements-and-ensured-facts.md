# Ordinary requirements and ensured facts

An ordinary requirement is a predicate expression in a `requires(...)` clause.

```bray
requires(
    length <= capacity,
)
```

Ordinary requirements are checked in value predicate context.

They must be pure, deterministic, total, terminating, and observational.

Calls inside ordinary requirements must resolve to predicates, compiler-known predicate-valid operations, or const callables valid in predicate-expression context.

An ordinary requirement can be proven statically, established by previous facts, or checked through a runtime assertion mechanism where appropriate.

A failed runtime check of an ordinary requirement panics.

A declaration can establish ordinary facts in `ensures(...)`.

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

Ensured facts become available after successful normal completion when their referenced values, storage identities, lifetimes, capabilities, and versions remain valid.

Facts from `ensures(...)` are not available on panic, cancellation, propagation, or any path that does not complete normally through the declaration result described by the clause.

Assertion expressions can establish ordinary facts after a successful assertion.

Assertion expressions are defined in [Assertion expressions](../expressions/assertion-expressions.md).

## Navigation

- [Language index](../index.md)
- [Contracts and trust index](../contracts-and-trust.md)
- Previous: [Contract clauses](contract-clauses.md)
- Next: [Trusted implementation capabilities](trusted-implementation-capabilities.md)
