# Product patterns

A product pattern matches a product type such as a `struct`.

Explicit type form:

```bray
Point { x = px, y = py }
```

Expected-type shorthand:

```bray
{ x = px, y = py }
```

Field patterns are matched by name.

Field order does not matter.

Duplicate fields are errors.

Unknown fields are errors.

Missing fields are errors unless `..` is present.

Successful matching of a product pattern makes the selected fields available according to the pattern operation's
ownership and access mode.

## Field shorthand

A field pattern can use shorthand when the binding name is the same as the field name.

```bray
{ x, y }
```

This means:

```bray
{ x = x, y = y }
```

The same rule applies to variant payload fields when the pattern entry is not filling a positional payload pattern slot.

```bray
Circle(center, radius)
```

means:

```bray
Circle(center = center, radius = radius)
```

The shorthand introduces bindings with the same names as the matched fields.

Field shorthand is binding shorthand. The introduced field binding is not resolved as a named constant or variant.

For a payload variant with `pos` payload fields, a pattern entry without `=` fills the next positional payload pattern
slot while one is available.

## Remaining fields

The `..` pattern explicitly accounts for remaining fields or remaining elements.

```bray
{ x, .. }
Circle(radius, ..)
[first, ..]
[first, .., last]
```

For product and variant payload patterns, `..` accounts for unlisted fields.

For fixed-size array patterns, `..` accounts for unlisted elements.

Omitting fields without `..` is an error.

The `..` pattern introduces no bindings.

## Navigation

- [Language index](../index.md)
- [Patterns index](../patterns.md)
- Previous: [Union variant patterns](union-variant-patterns.md)
- Next: [Tuple and array patterns](tuple-and-array-patterns.md)
