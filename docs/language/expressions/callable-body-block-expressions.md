# Callable-body block expressions

A **callable-body block expression** is the block expression used as the body of a callable program element.

Functions have callable-body block expressions.

Other callable forms such as lambdas and async functions also have callable-body block expressions.

```bray
func add(left: i32, right: i32) -> i32
{
    return left + right;
}
```

A callable-body block expression creates a **callable execution scope**.

Callable result and execution-scope rules are defined in
[Return and execution scopes](../callables/return-and-execution-scopes.md).

`return` exits the nearest callable execution scope.

A callable body supplies callable result values through `return`.

```bray
func f() -> i32
{
    return 1;
}
```

A callable body with declared result type `unit` can complete normally.

```bray
func log(pos message: string)
{
    print(message);
}
```

A callable body with declared result type other than `unit` must ensure every reachable normal completion path either
supplies a callable result through `return` or reaches a `never` expression.

A callable with omitted result type has result type `unit`.

```bray
func log(pos message: string)
{
    print(message);
}
```

This has the same callable result contract as:

```bray
func log(pos message: string) -> unit
{
    print(message);
}
```

A `return` expression must supply a value compatible with the callable’s declared result type.

```bray
func add(left: i32, right: i32) -> i32
{
    return left + right;
}
```

A callable returning `unit` can return explicitly with `return unit;`.

```bray
func log(pos message: string) -> unit
{
    print(message);
    return unit;
}
```

`return;` is shorthand for `return unit;`.

Return expression typing is defined in [Return expressions](return-expressions.md).

A `never` expression can satisfy any callable result requirement because it has no normal continuation.

```bray
func fail(pos message: string) -> never
{
    panic(message);
}
```

A callable declared to return `never` has no normal completion path.

A callable-body block expression can contain local binding declarations.

```bray
func outer() -> i32
{
    let inner = lambda () -> i32
    {
        return 1;
    };

    inner();

    return 2;
}
```

The first `return` exits the lambda.

The second `return` exits `outer`.

A callable-body block expression can contain nested yield-capable regions.

```bray
func f() -> i32
{
    let x: i32 =
    {
        yield 1;
    };

    return x;
}
```

`yield` targets the nearest enclosing yield-capable region.

`return` targets the nearest enclosing callable execution scope.

A `yield` inside a nested value-producing block expression supplies that nested block expression, not the callable
result.

Callable result values are supplied through `return`.

A callable-body block expression introduces a scope for local bindings, ownership tracking, borrow tracking,
destruction, finalization tracking, capability checking, effect checking, and condition refinement.

Local bindings introduced inside the callable-body block expression are visible according to ordinary block-expression
scope rules.

Local owned values whose ownership remains in the callable-body block expression are resolved when the callable-body
block expression exits. In async execution, this includes structured task and asynchronous-finalization obligations.

Callable-body exits include:

- normal completion,
- `return`,
- uncaught panic propagation,
- nullable propagation whose target boundary is the callable execution scope,
- result propagation whose target boundary is the callable execution scope,
- run-result propagation whose target boundary is the callable execution scope,
- cancellation of the current async computation,
- any expression path with type `never` that prevents the callable body from continuing.

`yield`, `break`, and `continue` are not callable-body exits merely because they occur inside a callable body.

They leave the callable-body block expression only when the region they target or the enclosing expression path that
contains them also leaves the callable body.

A `return` expression first evaluates its returned expression.

The returned value is transferred to the callable result according to the result type, ownership rules, copy rules,
borrow rules, and lifetime rules.

A local value moved into the callable result is no longer destroyed as a local owned value.

Local owned values that remain in the callable-body block expression after the returned value is formed are destroyed
according to Bray destruction order.

A returned borrow must be valid for the callable result contract.

A callable body cannot return a borrow that outlives the storage it reaches.

A callable body cannot return a value with unresolved finalization obligations unless the callable result type or
surrounding contract transfers those obligations.

A callable-body block expression must leave every reachable exit with compatible type, ownership, initialization,
destruction, finalization, capability, and effect states and compatible available contract guarantees.

For async callables, the callable-body block expression is checked in an active async execution context and is the root
lexical task scope for that invocation. Every nested ordinary block is also a structured task ownership boundary.

Calling an async callable creates an owned inactive `Future<T>`. Suspension points capture live values, borrows,
capabilities, effects, execution requirements, and lifecycle obligations into its dependency contract.

Async scope exit requests cancellation for every owned unresolved task before awaiting any of them, then performs
reverse lifecycle resolution. Ordinary destruction remains synchronous. Asynchronous finalization obligations must be
driven in async cleanup or transferred to another compatible owner.

## Navigation

- [Language index](../index.md)
- [Expressions index](../expressions.md)
- Previous: [Block expressions](block-expressions.md)
- Next: [Local binding declarations inside block expressions](local-binding-declarations-inside-block-expressions.md)
