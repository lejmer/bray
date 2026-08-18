# Range expressions

A **range expression** produces a finite ascending half-open integer sequence.

```bray
let indices: Range<i32> = 0..4;
```

The range contains `0`, `1`, `2`, and `3`.

## Form and precedence

A range expression supplies both bounds:

```text
start..end
```

The range operator has lower precedence than logical-or expressions and higher precedence than assignment. A range
expression contains one top-level `..` operator.

The start expression is evaluated first and exactly once. The end expression is then evaluated exactly once.

Inside an indexing operation, a top-level `..` belongs to the slice selector:

```bray
let middle: &[i32] = &values[start..end];
```

A grouped range is an ordinary element selector expression and can participate in a matching custom indexing contract:

```bray
let selected = table[(start..end)];
```

## Type and bounds

Both bounds have the same target-available integer scalar type `T`, which makes `Range<T>` well formed.

An expected `Range<T>` guides the type of both bounds. Otherwise, ordinary numeric literal adaptation and inference
determine `T`. An unconstrained integer-literal range uses `i32`.

```bray
let bytes: Range<u8> = 2..6;
let defaults_to_i32 = 0..4;
```

The result type is the compiler-known protected-representation type `Range<T>`. `Range<T>` has a language-defined copy
contract and has no partial state or lifecycle obligations.

The `..` form is compiler-defined range construction and selects no operator trait.

The expression's effects and capability requirements come from evaluating its bounds.

## Values and iteration

For bounds `start` and `end`, the range produces the ordered values beginning at `start`, increasing by one, and
strictly less than `end`.

When `start >= end`, the range is empty.

The compiler provides exact implementations for:

```text
Range<T>(Iterator)
&Range<T>(Iterable)
Range<T>(Iterable)
```

`Range<T>(Iterator).Element` is `T`. Calling `next` yields the current value and advances the cursor by one while the
current value is less than the end bound. At exhaustion, `next` returns `none` and the cursor remains exhausted.

`&Range<T>(Iterable)` copies the range value into its cursor. `Range<T>(Iterable)` consumes the range value as its
cursor. Both implementations use `Range<T>` as `Cursor`, produce elements in ascending order, and are finite.

These contracts make an unmarked range source use shared iteration and a `move` range source use consuming iteration:

```bray
for index in 0..4
{
    inspect(index);
}

let bounds: Range<i32> = 2..6;

for index in move bounds
{
    consume(index);
}
```

## Cardinality

The range cardinality is the mathematical integer value `max(end - start, 0)`. This contract introduces no arithmetic
overflow in the endpoint type.

When the compiler can prove both bounds, it can prove the exact cardinality. A constant range such as `0..4` therefore
supplies the exact cardinality required by a fixed-array generator.

```bray
let doubled: [i32; 4] =
[
    each value in 0..4
    {
        yield value * 2;
    }
];
```

## Navigation

- [Language index](../index.md)
- [Expressions index](../expressions.md)
- Previous: [Array expressions](array-expressions.md)
- Next: [Iteration source resolution](iteration-source-resolution.md)
