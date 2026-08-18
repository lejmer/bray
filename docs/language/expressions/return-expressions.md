# Return expressions

A **return expression** exits the nearest callable execution scope.

```bray
return value;
```

The returned value must be compatible with the callable’s declared result type.

Callable result and execution-scope rules are defined in
[Return and execution scopes](../callables/return-and-execution-scopes.md).

A callable with result type `unit` can complete normally.

A callable with result type `unit` can also return explicitly with `return unit;`.

`return;` is shorthand for `return unit;`.

A `return` expression has type `never` in the current control-flow path because control exits the callable execution
scope.

`return` targets the nearest callable execution scope.

A callable expression nested inside another callable body creates a separate callable execution scope.

## Navigation

- [Language index](../index.md)
- [Expressions index](../expressions.md)
- Previous: [Yield expressions](yield-expressions.md)
- Next: [Panic expressions](panic-expressions.md)
