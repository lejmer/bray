# Preconditions and postconditions

An ordinary requirement is a predicate expression in a `requires(...)` clause.

```bray
requires(
    length <= capacity,
)
```

Ordinary requirements are checked in value predicate context.

They must be pure, deterministic, total, terminating, and observational.

Calls inside ordinary requirements must resolve to predicates, compiler-known predicate-valid operations, or const
callables valid in predicate-expression context.

An ordinary requirement can be established by the language-defined contract reasoning available at the call site or
checked at runtime where the declaration permits a runtime check.

A failed runtime check of an ordinary requirement panics.

A declaration states postconditions in `ensures(...)`.

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

[Contract reasoning](contract-reasoning.md) defines condition availability after normal completion.
[Contract and borrow validity](../ownership-and-borrowing/contract-and-borrow-validity.md) defines when those conditions
remain usable.

Postconditions are not available on panic, cancellation, propagation, or any path that does not complete normally
through the declaration result described by the clause.

After a successful assertion, its asserted condition is available for subsequent contract reasoning.

Assertion expressions are defined in [Assertion expressions](../expressions/assertion-expressions.md).

## Ordinary requirement example

Example: this ordinary function has a checkable requirement:

```bray
func positive(pos value: i32) -> i32
    requires(value > 0)
{
    return value;
}
```

In a runtime body whose `input: i32` parameter has no established sign, this call is accepted with a runtime requirement
check:

```bray
let checked = positive(input);
```

A positive input returns its value. A nonpositive input fails the requirement check and panics.

Adding a trust boundary rejects the fragment because `positive` has no trusted caller obligation to acknowledge:

```bray
let checked = trusted positive(input);
```

[Trust boundaries](trust-boundaries.md) acknowledge trusted caller obligations and reject a redundant boundary.
They do not replace ordinary requirement checks.

## Navigation

- [Language index](../index.md)
- [Contracts and trust index](../contracts-and-trust.md)
- Previous: [Contract clauses](contract-clauses.md)
- Next: [Trusted implementation capabilities](trusted-implementation-capabilities.md)
