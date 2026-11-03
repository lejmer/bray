# Callable results and bodies

## Omitted result type

If the result type is omitted, the function returns `unit`.

```bray
func log(pos message: string)
{
    print(message);
}
```

This means:

```bray
func log(pos message: string) -> unit
{
    print(message);
}
```

## Callable result syntax

Callable result types use `->`.

```bray
func add(left: i32, right: i32) -> i32
{
    return left + right;
}
```

The `:` syntax annotates the declared thing before it, while `->` declares the result type of a callable.

```text
:    type annotation for the declared thing before it
->   result type of a callable
```

## Function bodies

A function body is a block expression in callable-body context.

```bray
func f() -> i32
{
    return 1;
}
```

Callable-body context creates a [callable execution scope](return-and-execution-scopes.md#callable-execution-scopes).
Result values are supplied through explicit callable exits.
[Return expressions](../expressions/return-expressions.md) define the target and transfer of a returned value.

```bray
func add(left: i32, right: i32) -> i32
{
    return left + right;
}
```

A function whose declared result type is `unit` can complete normally.

```bray
func log(pos message: string)
{
    print(message);
}
```

A function whose declared result type is not `unit` is rejected if a reachable path falls through the end of its body.
A `return` or an expression with no normal continuation prevents fallthrough on that path. A function declared to return
`never` has no normal completion path.

## Semicolons

[Sequenced expressions](../expressions/sequenced-expressions.md) define required semicolons, including sequenced
`return` expressions, and the exception for self-delimiting block items.

## Reachable fallthrough example

Example: this function is rejected because the false path reaches the end without returning an `i32`:

```bray
func missing(pos condition: bool) -> i32
{
    if condition
    {
        return 1;
    }
}
```

Adding `return 2;` after the conditional makes both paths return and accepts the function. Adding a non-returning
`panic("missing result");` there also accepts it, but that path panics instead of returning a value.
A nested block's `yield` would supply that block, not the callable result.
[Return and execution scopes](return-and-execution-scopes.md#callable-result-exits) defines the different targets.

## Navigation

- [Language index](../index.md)
- [Callables index](../callables.md)
- Previous: [Const functions](const-functions.md)
- Next: [Parameters](parameters.md)
