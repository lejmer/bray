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

An omitted result type means `unit`.

## Callable result syntax

Callable result types use `->`.

```bray
func add(left: i32, right: i32) -> i32
{
    return left + right;
}
```

The `:` syntax annotates the declared thing before it.

The `->` syntax declares the result type of a callable.

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

Callable-body context gives the block expression a callable execution scope.

The callable execution scope determines the target of `return`.

Callable result values are supplied through explicit callable exits.

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

For a function with a declared result type other than `unit`, every reachable normal completion path supplies the
callable result through `return` or ends in a `never` expression.

## Semicolons

Semicolons are mandatory for sequenced expressions.

```bray
func f()
{
    log("hello");
    log("world");
}
```

A `return` expression is a sequenced expression and is terminated with a semicolon.

```bray
func f() -> i32
{
    return 1;
}
```

## Navigation

- [Language index](../index.md)
- [Callables index](../callables.md)
- Previous: [Const functions](const-functions.md)
- Next: [Parameters](parameters.md)
