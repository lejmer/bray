# Return and execution scopes

## Return

[Return expressions](../expressions/return-expressions.md) define return targets, operand checking, result transfer,
and expression typing. [Callable results and bodies](callable-results-and-bodies.md#function-bodies) define which
callable bodies can complete normally.

## Callable execution scopes

A **callable execution scope** is created by the block expression body of a callable program element. Functions,
lambdas, and async functions create callable execution scopes.

The following example illustrates the [return target rule](../expressions/return-expressions.md).

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

The first `return` exits `inner`. The second exits `outer`.

## Return and `unit`

[Omitted result types](callable-results-and-bodies.md#omitted-result-type) default to `unit`.
[Return expressions](../expressions/return-expressions.md) define explicit unit returns and omitted return operands.

## Return and `never`

[Unit and never in expression context](../expressions/literal-expressions.md#unit-and-never-in-expression-context)
defines how `never` participates in control-flow merges.
[Callable body completion](callable-results-and-bodies.md#function-bodies) defines the requirement for a callable
whose declared result is `never`.

[Panic expressions](../expressions/panic-expressions.md) define failure outside the ordinary callable result contract.
[Catch expressions](../expressions/catch-expressions.md) define expression-level panic-catching boundaries.

## Callable result exits

[Callable body completion](callable-results-and-bodies.md#function-bodies) distinguishes explicit returns from normal
`unit` completion.
The following example contrasts [yield targets](../expressions/yield-expressions.md) with return targets.

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

The `yield` supplies the nested block's value. The `return` supplies the function result.

## Yield and return

[Yield expressions](../expressions/yield-expressions.md) define single-yield and multi-yield regions, nested targets,
and omitted operands. [Return expressions](../expressions/return-expressions.md) define callable exits.

## Block expression results and callable results

[Block expressions](../expressions/block-expressions.md) define value-producing blocks.
[Callable-body block expressions](../expressions/callable-body-block-expressions.md) define callable exits and local
ownership resolution. A function body uses those callable-body rules even though its syntax is a block expression.

## Navigation

- [Language index](../index.md)
- [Callables index](../callables.md)
- Previous: [Function calls](function-calls.md)
- Next: [Callable types and values](callable-types-and-values.md)
