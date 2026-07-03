# Yield expressions

A **yield expression** supplies a value to the nearest enclosing yield-capable region.

```bray
yield value;
```

Yield-capable regions include:

- value-producing block expressions,
- value-producing conditional arm block expressions,
- match arm block expressions,
- general generator expressions,
- array generator expressions.

A single-yield region with result type other than `unit` must receive exactly one yielded value on every normal completion path or
have no normal continuation.

A single-yield region with result type `unit` can complete naturally without `yield`.

A multi-yield region can receive zero or more yielded values according to the region’s contract.

A fixed-size array generator receives exactly the number of yielded values required by the array length.

Nested yield-capable regions capture their own yields.

An inner `yield` supplies the inner region.

The yielded value must be compatible with the target region’s expected result or element type.

`yield;` is shorthand for `yield unit;`.

When `yield` targets a single-yield region, it has type `never` because control exits that region.

When `yield` targets a multi-yield region, it contributes an element and its continuation behavior is defined by that multi-yield
region's contract.

## Navigation

- [Language index](../index.md)
- [Expressions index](../expressions.md)
- Previous: [Match expressions](match-expressions.md)
- Next: [Return expressions](return-expressions.md)
