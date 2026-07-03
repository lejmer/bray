# Tuple and array patterns

## Tuple patterns

A tuple pattern matches a tuple by position.

```bray
(x, y)
```

A one-element tuple pattern uses a trailing comma.

```bray
(x,)
```

Tuple pattern arity must match the subject tuple arity.

Each element pattern is checked against the corresponding tuple element type.

A tuple pattern is irrefutable when all element patterns are irrefutable.

## Fixed-size array patterns

A fixed-size array pattern matches an array by position.

```bray
[first, second, third]
```

The number of listed element patterns must match the array length unless `..` is present.

```bray
[first, ..]
[first, .., last]
```

Each element pattern is checked against the array element type.

A fixed-size array pattern is irrefutable when it accounts for the array shape and every listed element pattern is irrefutable.

`..` accounts for remaining elements and introduces no binding.

Binding remaining elements requires an explicit slice projection outside the pattern.

## Navigation

- [Language index](../index.md)
- [Patterns index](../patterns.md)
- Previous: [Product patterns](product-patterns.md)
- Next: [Nullable patterns](nullable-patterns.md)
