# Predicate-expression callable calls

A predicate expression can call a function or method when the selected callable contract is valid in
predicate-expression context.

The callable can return any value type that is valid in predicate-expression context.

```bray
const func remaining(pos capacity: usize, length: usize) -> usize
    requires(length <= capacity)
{
    return capacity - length;
}

predicate has_space(length: usize, capacity: usize) =
    remaining(capacity, length = length) > 0;
```

A callable contract is valid in predicate-expression context only when the callable is:

- pure,
- deterministic,
- total,
- terminating,
- observational,
- effect-free,
- allocation-free,
- async-free,
- free of trusted capability use,
- checked only through predicate-valid operations.

Predicate-expression invocation requires the selected callable declaration or lambda expression to expose `const`.
The same rule applies through callable values, aliases, selected implementations, and compiled interfaces.
Body availability and `pure` or `total` guarantees do not supply this contract.

Ordinary runtime callers of const functions do not themselves need a const contract.

The selected callable contract is the full callable contract of the resolved function or method after overload selection
and generic substitution.

The callable body may use ordinary callable-body structure, including `return`, when every reachable result-producing
path uses only predicate-valid expressions and predicate-expression-valid calls.

The callable's parameters and result type must be valid predicate-expression values.

The call arguments must be predicate-valid expressions.

The callable's `requires(...)` obligations must follow from the conditions available at the call.

The callable's `ensures(...)` guarantees are available after the call inside the predicate-expression check.

## Navigation

- [Language index](../index.md)
- [Contracts and trust index](../contracts-and-trust.md)
- Previous: [Static constraints](static-constraints.md)
- Next: [Contract arithmetic](contract-arithmetic.md)
