# Predicate expressions

A **predicate expression** is a restricted contract-level expression checked in predicate-expression context.

Predicate expressions are used in predicate bodies, `requires(...)` clauses, `ensures(...)` clauses, `with(...)` clauses, and other contract-level positions.

```bray
predicate fits(length: usize, capacity: usize) =
    length <= capacity;
```

A predicate expression is pure, deterministic, total, terminating, and observational.

Predicate expressions form their own checking context.

The contract and trust rules define value predicate context and static constraint context.

The parser can reuse ordinary expression grammar pieces, but binding and checking apply predicate-expression restrictions.

A predicate body is a single predicate expression.

```bray
predicate non_empty(length: usize) =
    length > 0;
```

Block expressions are excluded from predicate expressions.

```bray
predicate valid(length: usize, capacity: usize) =
{
    yield length <= capacity;
};
```

This is rejected.

Predicate expressions can reference predicate parameters.

Predicate expressions can reference `self` where applicable.

Predicate expressions in postcondition context can reference `result`.

`result` is the compiler-introduced binding for the value produced by normal completion of the declaration being checked.

`result` is available only inside `ensures(...)` clauses and other postcondition predicate contexts that describe a normal
completion value.

`result` is not available in `requires(...)`, `with(...)`, predicate declaration bodies, guard expressions, ordinary expression
contexts, or declarations whose normal completion does not produce a value.

Bray does not support named result bindings.

Predicate expressions can reference constants and constant-valued members.

Predicate expressions can use field access through observable access paths.

Predicate expressions can inspect tuples, arrays, nullable values, and union values by observation.

Predicate expressions can use arithmetic under contract arithmetic semantics.

Integer-valued contract arithmetic is exact and does not silently wrap.

Predicate expressions can use boolean operators.

Predicate expressions can use comparisons.

Predicate expressions can call predicates.

Predicate expressions can call functions and methods whose selected callable contract is valid in predicate-expression context.

Predicate-expression callable contract rules belong to the contract and trust rules.

Predicate expressions can use conditional expressions when the condition and every branch are valid predicate expressions.

Predicate expressions can use `all(...)` and `any(...)` boolean fold expressions.

The operand of a predicate-context boolean fold expression must be predicate-valid.

If the operand is a generator expression, its source expression, pattern operation, iteration body, and yielded expressions must be
predicate-valid.

The yielded element type must be `bool`.

The operand must be finite and bounded in the current predicate context.

In static constraint context, the compiler must be able to statically enumerate the operand or reason about its finite bound.

In value predicate context, a runtime contract check may iterate a runtime-sized operand only when its finite bound is available
from observable state in that predicate context.

If finiteness or boundedness cannot be proven in the required predicate context, the predicate expression is rejected.

Predicate expressions exclude local binding declarations.

Predicate expressions exclude assignment.

Predicate expressions exclude mutation.

Predicate expressions exclude movement and consumption.

Predicate expressions exclude destruction.

Predicate expressions exclude finalization.

Predicate expressions exclude allocation.

Predicate expressions exclude I/O.

Predicate expressions exclude function calls whose selected callable contract is not valid in predicate-expression context.

Predicate expressions exclude method calls whose selected callable contract is not valid in predicate-expression context.

Predicate expressions exclude asynchronous execution forms.

Predicate expressions exclude `try` propagation and `catch` expressions.

Predicate expressions exclude `with` expressions and resource-scope behavior.

Predicate expressions exclude runtime loops other than generator iteration expressions used to produce finite boolean operands for
`all(...)` or `any(...)`.

Predicate expressions exclude dynamic dispatch with effects.

Predicate expressions exclude trusted capability use.

Predicate expressions exclude reading mutable global state.

Predicate expressions exclude dependence on time, randomness, scheduler state, address layout, or implementation scheduling.

A predicate expression with type `bool` can serve as an ordinary contract requirement.

A trusted predicate call can appear in a contract clause when prefixed with `trusted`.

```bray
requires(
    length <= capacity,
    trusted core.memory.owned_allocation(pointer = pointer, bytes = capacity, align = align),
)
```

Ordinary predicate expressions and trusted predicate calls are distinct contract requirements.

Predicate expressions can establish value facts when used in `ensures(...)`.

Predicate expressions can require value facts when used in `requires(...)`.

Predicate expressions can require static constraint facts when used in `with(...)`.

Value facts introduced by predicate expressions are tied to the values, storage identities, lifetimes, capabilities, and versions mentioned by the expression.

Mutation, movement, consumption, destruction, reinitialization, finalization, or capability loss can invalidate facts that depend on the affected state.

Static constraint facts are compile-time facts scoped to the constrained declaration and its generic checking context.

Runtime assertions generated from predicate expressions must preserve predicate-expression semantics.

## Navigation

- [Language index](../index.md)
- [Expressions index](../expressions.md)
- Previous: [Pattern-bearing expressions](pattern-bearing-expressions.md)
- Next: [Guard expressions](guard-expressions.md)
