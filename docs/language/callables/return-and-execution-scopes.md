# Return and execution scopes

## Return

`return` exits the current callable execution scope.

```bray
func add(left: i32, right: i32) -> i32
{
    return left + right;
}
```

The returned expression must be compatible with the callable's declared result type.

The `return` expression itself has type `never` because control exits the callable execution scope.

`return` always targets the nearest enclosing callable execution scope.

## Callable execution scopes

A callable execution scope is created by the block expression body of a callable program element.

Functions create callable execution scopes.

Lambdas and async functions also create callable execution scopes.

`return` exits the nearest callable execution scope.

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

The first `return` exits `inner`.

The second `return` exits `outer`.

## Return and `unit`

A function returning `unit` can complete normally.

```bray
func log(pos message: string)
{
    print(message);
}
```

A function returning `unit` can also return explicitly with `return unit;`.

```bray
func log(pos message: string)
{
    print(message);
    return unit;
}
```

`return;` is shorthand for `return unit;`.

## Return and `never`

A `never` expression satisfies any required result type at a control-flow merge because it has no normal continuation.

At a control-flow merge, `never` contributes no value and does not determine the merged result type.

```bray
func fail(pos message: string) -> never
{
    panic(message);
}
```

A function declared to return `never` has no normal completion path.

Panic is outside the ordinary callable result contract.

If a callable panics, the panic propagates to the nearest panic-catching boundary instead of producing the callable's
declared result.

A `catch` expression creates an expression-level panic-catching boundary.

The never-producing expression forms are defined by the Expressions chapter.

## Callable result exits

Callable result values are supplied explicitly through `return`.

```bray
func add(left: i32, right: i32) -> i32
{
    return left + right;
}
```

This keeps callable exits visible in the source.

## Yield and return

`yield` supplies a value to the nearest enclosing yield-capable region.

`yield;` is shorthand for `yield unit;`.

`return` exits the nearest callable execution scope.

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

The `yield` supplies the value of the value-producing block expression.

The `return` exits the function.

## Block expression results and callable results

Block expression results and callable results are related but distinct semantic layers.

A value-producing block expression receives its value through `yield`.

```bray
let x: i32 =
{
    yield 1;
};
```

A callable receives its result through `return`.

```bray
func f() -> i32
{
    return 1;
}
```

`yield` targets the nearest yield-capable region.

`return` targets the nearest callable execution scope.

A function body is a block expression, and its callable result is governed by callable-body rules.

## Navigation

- [Language index](../index.md)
- [Callables index](../callables.md)
- Previous: [Function calls](function-calls.md)
- Next: [Callable types and values](callable-types-and-values.md)
