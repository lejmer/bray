# Return expressions

Syntax follows the [completion expressions grammar](../syntax-grammar.md#completion-expressions).

A **return expression** exits the nearest callable execution scope.

```bray
return value;
```

The returned value must be compatible with the callable's declared result type.

[Callable execution scopes](../callables/return-and-execution-scopes.md#callable-execution-scopes) define the scopes
created by callable bodies. [Callable body completion](../callables/callable-results-and-bodies.md#function-bodies)
defines when a body can complete normally.

A callable with result type `unit` can return explicitly with `return unit;`.

`return;` is shorthand for `return unit;`.

A `return` expression has type `never` in the current control-flow path because control exits the callable execution
scope.

## Evaluation and ownership

A `return` expression first evaluates its returned expression.

The returned value is transferred to the callable result according to the result type, ownership rules, copy rules,
borrow rules, and lifetime rules.

A local value moved into the callable result is no longer destroyed as a local owned value.

Local owned values that remain in the callable-body block expression after the returned value is formed are destroyed
according to the [destruction rules](../lifecycle/destruction.md).

A returned borrow must be valid for the callable result contract.

A callable body cannot return a borrow that outlives the storage it reaches.

A callable body cannot return a value with unresolved finalization obligations unless the callable result type or
surrounding contract transfers those obligations.

[Scope exits and ownership boundaries](../ownership-and-borrowing/scope-exits-and-ownership-boundaries.md) define
preservation of dependency contracts across a return.

## Navigation

- [Language index](../index.md)
- [Expressions index](../expressions.md)
- Previous: [Yield expressions](yield-expressions.md)
- Next: [Panic expressions](panic-expressions.md)
