# Sequenced expressions

A **sequenced expression** is an expression evaluated as part of an ordered sequence inside a block expression.

Sequenced expressions are terminated with semicolons.

```bray
{
    log("hello");
    log("world");
}
```

Semicolons are mandatory for sequenced expressions.

Self-delimiting block-shaped expressions can instead appear directly as block items without becoming sequenced expressions. These
are block, conditional, match, while, for, loop, and with expressions. A trailing semicolon remains valid and makes such an
expression a sequenced expression.

A sequenced expression can be used for its value, effects, lifecycle behavior, control-flow outcome, flow-sensitive contract changes, or `unit` completion according to the surrounding context.

A `return` expression used in a sequence is terminated with a semicolon.

```bray
func f() -> i32
{
    return 1;
}
```

A `yield` expression used in a sequence is terminated with a semicolon.

```bray
let x: i32 =
{
    yield 1;
};
```

## Navigation

- [Language index](../index.md)
- [Expressions index](../expressions.md)
- Previous: [Expression context](expression-context.md)
- Next: [Block expressions](block-expressions.md)
