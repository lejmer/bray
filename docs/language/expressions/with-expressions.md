# With expressions

A **with expression** enters scoped lifecycle behavior for a resource and evaluates a body while the scoped capability is active.

With expressions use `with`.

```bray
with file = File.open(path)
{
    file.write(bytes);
}
```

A with expression has the form:

```bray
with pattern = initializer
{
    ...
}
```

The binding side can include a type annotation:

```bray
with file: File = File.open(path)
{
    file.write(bytes);
}
```

The binding side is an irrefutable pattern.

```bray
with (file, metadata) = open_with_metadata(path)
{
    ...
}
```

The initializer expression is evaluated once.

The with body is a block expression.

In value-producing context, the with body is a single-yield region and supplies the with expression result.

```bray
let count: usize = with file = File.open(path)
{
    yield file.count_lines();
};
```

With expression lifecycle selection, scoped capability behavior, exit behavior, failure behavior, async behavior, escape rules, and checking rules are defined in [With expressions](../lifecycle/with-expressions.md).

## Navigation

- [Language index](../index.md)
- [Expressions index](../expressions.md)
- Previous: [Loop expressions](loop-expressions.md)
- Next: [Lambda expressions and anonymous callable expressions](lambda-expressions-and-anonymous-callable-expressions.md)
