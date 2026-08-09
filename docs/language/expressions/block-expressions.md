# Block expressions

A **block expression** is a braced expression region.

```bray
{
    ...
}
```

A brace-enclosed expression whose only top-level child is a general generator iteration expression is a general generator
expression, not a block expression.

Every block expression has a type.

A block expression introduces a scope for:

- local bindings,
- ownership tracking,
- borrow tracking,
- destruction,
- finalization tracking,
- capability checking,
- effect checking,
- condition refinement.

A block expression can produce `unit`, `never`, or another value type.

A block expression in value-producing context receives its value through `yield`.

```bray
let x: i32 =
{
    yield 1;
};
```

A block expression in `unit` context can complete normally.

```bray
{
    log("done");
}
```

A block expression used directly as a block item is self-delimiting and does not require a trailing semicolon. The same rule applies
to conditional, match, while, for, loop, and with expressions. Other expression block items are terminated with semicolons.

A block expression whose control flow has no normal continuation has type `never`.

A block expression's exits must agree on type, ownership, initialization, destruction, finalization, capabilities, effects, and
which contract guarantees remain available.

## Navigation

- [Language index](../index.md)
- [Expressions index](../expressions.md)
- Previous: [Sequenced expressions](sequenced-expressions.md)
- Next: [Callable-body block expressions](callable-body-block-expressions.md)
