# Predicates and predicate expressions

A predicate is a contract-level relation.

It is not an ordinary runtime function.

Predicate declaration syntax is defined in [Predicate declarations](../declarations/predicate-declarations.md).

Predicate expressions are contract-level expressions checked in predicate-expression context.

Predicate expressions are used in:

- predicate bodies,
- `requires(...)` clauses,
- `ensures(...)` clauses,
- `with(...)` clauses,
- trusted obligations,
- static constraint contexts,
- contract-level fact reasoning.

Predicate expressions are not ordinary runtime Bray expressions.

A predicate expression must be pure, deterministic, total, terminating, and observational.

Predicate expressions are checked in one of these contexts:

- value predicate context,
- static constraint context.

Value predicate context is used for value predicate bodies, `requires(...)`, `ensures(...)`, and trusted obligations.

Static constraint context is used for `with(...)` clauses on generic declarations and static predicate bodies.

Both contexts use the same purity and determinism rules.

Each context decides which names, entities, and built-in predicate forms are available.

Allowed in predicate expressions:

- literals,
- references to predicate parameters,
- `self` where applicable,
- `result` in postcondition predicate contexts,
- constants and constant-valued members,
- field access through observable access paths,
- tuple, array, nullable, and union inspection by observation,
- arithmetic using contract arithmetic semantics,
- boolean operators,
- comparisons,
- calls to predicates,
- calls to functions and methods whose selected callable contract is valid in predicate-expression context,
- conditional expressions whose condition and branches are valid predicate expressions,
- `all(...)` and `any(...)` boolean fold expressions whose operands are predicate-valid, finite, bounded, and iterable as `bool`.

Forbidden in predicate expressions:

- block expressions,
- local binding declarations,
- assignment,
- mutation,
- movement or consumption,
- destruction,
- finalization,
- allocation,
- I/O,
- function calls whose selected callable contract is not valid in predicate-expression context,
- method calls whose selected callable contract is not valid in predicate-expression context,
- async, await, spawn, `try`, or `catch`,
- `with` expressions and resource-scope behavior,
- runtime loops other than generator iteration expressions used to produce finite boolean operands for `all(...)` or `any(...)`,
- dynamic dispatch with effects,
- trusted capability use,
- reading mutable global state,
- depending on time, randomness, address layout, scheduler state, or implementation scheduling.

Because ordinary Bray blocks are expressions, block expressions are explicitly excluded from predicate expressions.

```bray
predicate valid(length: usize, capacity: usize) =
{
    yield length <= capacity;
};
```

This is rejected.

```bray
predicate valid(length: usize, capacity: usize) =
    length <= capacity;
```

This is valid.

## Navigation

- [Language index](../index.md)
- [Contracts and trust index](../contracts-and-trust.md)
- Previous: [Trusted caller obligations](trusted-caller-obligations.md)
- Next: [Static constraints](static-constraints.md)
