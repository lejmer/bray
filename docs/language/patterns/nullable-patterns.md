# Nullable patterns

Nullable patterns match the nullable storage state of a subject with type `T?`.

Absent pattern:

```bray
none
```

Present pattern:

```bray
?inner
```

The `none` pattern is valid only when the subject type is a concrete nullable type `T?`.

It matches the absent state and introduces no binding.

`none` is a built-in nullable pattern and does not introduce a binding named `none`.

The `?inner` pattern is valid only when the subject type is a concrete nullable type `T?`.

It matches the present state and applies `inner` to the contained `T`.

`?inner` is a nullable pattern form, not a general pattern modifier.

Bare binding patterns bind the whole nullable value.

```bray
value   // binds T?
?value  // matches present state and binds contained T
```

Both `none` and `?inner` are refutable.

A nullable pattern set is exhaustive when it covers the absent state and covers the present state for every possible contained `T` value.

```bray
case ?value
{
    ...
}
case none
{
    ...
}
```

This is exhaustive because `value` is irrefutable for the contained `T`.

An alternative pattern can cover both states only when the alternatives bind the same names.

```bray
?_ | none
```

The pattern operation mode determines whether the contained value is observed, borrowed, mutably borrowed, copied, or consumed.

Successful matching of `?inner` refines the subject to present state in the matched region.

Successful matching of `none` refines the subject to absent state in the matched region.

## Navigation

- [Language index](../index.md)
- [Patterns index](../patterns.md)
- Previous: [Tuple and array patterns](tuple-and-array-patterns.md)
- Next: [Box patterns](box-patterns.md)
