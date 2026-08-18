# Predicate expressions

A **predicate expression** is a restricted contract-level expression checked in predicate-expression context.

Predicate expressions are used in predicate bodies, `requires(...)` clauses, `ensures(...)` clauses, `with(...)`
clauses, and other contract-level positions.

```bray
predicate fits(length: usize, capacity: usize) =
    length <= capacity;
```

A predicate expression is pure, deterministic, total, terminating, and observational.

Predicate-expression contexts, allowed forms, forbidden forms, callable-call rules, boolean folds, static constraints,
and contract arithmetic are defined in
[Predicates and predicate expressions](../contracts-and-trust/predicates-and-predicate-expressions.md),
[Static constraints](../contracts-and-trust/static-constraints.md),
[Predicate-expression callable calls](../contracts-and-trust/predicate-expression-callable-calls.md), and
[Contract arithmetic](../contracts-and-trust/contract-arithmetic.md).

A predicate expression with type `bool` can serve as an ordinary contract requirement.

A trusted predicate call can appear in a contract clause when prefixed with `trusted`.

```bray
requires(
    length <= capacity,
    trusted core.memory.owned_allocation(pointer = pointer, bytes = capacity, align = align),
)
```

Ordinary predicate expressions and trusted predicate calls are distinct contract requirements.

Predicate expressions can establish value conditions when used in `ensures(...)`.

Predicate expressions can require value conditions when used in `requires(...)`.

Predicate expressions can require static constraint conditions when used in `with(...)`.

Value conditions introduced by predicate expressions are tied to the values, storage identities, lifetimes,
capabilities, and versions mentioned by the expression.

Mutation, movement, consumption, destruction, reinitialization, finalization, or capability loss can invalidate
conditions that depend on the affected state.

Static constraint conditions are compile-time conditions scoped to the constrained declaration and its generic checking
context.

Runtime assertions generated from predicate expressions must preserve predicate-expression semantics.

## Navigation

- [Language index](../index.md)
- [Expressions index](../expressions.md)
- Previous: [Pattern-bearing expressions](pattern-bearing-expressions.md)
- Next: [Guard expressions](guard-expressions.md)
