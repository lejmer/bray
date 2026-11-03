# Iteration source resolution

Iteration-bearing expressions use the compiler-known `Iterable` and `Iterator` traits defined in
[Iteration traits](../types/implementations.md#iteration-traits).

Iteration-bearing expressions include:

- `for` expressions,
- array generator expressions,
- general generator iteration expressions,
- boolean fold expressions.

An iteration source is resolved in one of three access modes:

- shared iteration,
- mutable iteration,
- consuming iteration.

The unmarked source form selects shared iteration.

Shared iteration evaluates the source expression once, creates a shared borrow for the duration of the iteration, and
requires the selected subject to satisfy `Iterable`:

```bray
&SourceType: Iterable
```

The source form marked with `mut` selects mutable iteration.

Mutable iteration evaluates the source expression once, creates a mutable borrow for the duration of the iteration, and
requires mutation authority and [borrow compatibility](../ownership-and-borrowing/borrow-rules.md) for the source access path:

```bray
&mut SourceType: Iterable
```

The source form marked with `move` selects consuming iteration.

Consuming iteration evaluates the source expression once, moves the resulting value into the iteration cursor, and
requires the source type itself to satisfy `Iterable`:

```bray
SourceType: Iterable
```

For the selected iterable subject `S`, the source element type is:

```bray
S(Iterable).Element
```

The cursor type is:

```bray
S(Iterable).Cursor
```

The selected `Iterable` implementation guarantees that the cursor satisfies `Iterator` and has the same element type.

The selected `Iterable` and `Iterator` contracts define:

- the element type,
- the element access mode,
- the iteration order,
- the cardinality when known,
- whether the iteration is finite,
- ownership and borrowing behavior for each produced element.

For pattern-bearing iteration expressions, the pattern operation mode follows the iteration binding rule in
[Pattern contexts](../patterns/pattern-contexts.md).

After evaluating the source and creating the selected subject, an iteration expression calls `iterate` exactly once
and owns the resulting cursor. It repeatedly calls `next`, producing one element for each non-`none` result. The first
`none` is natural exhaustion.

The selected `iterate` and `next` calls follow ordinary [method-call checking](method-call-expressions.md). Their
contracts constrain the types, ownership transfers, borrows, capabilities, effects, and permitted panic behavior of the
iteration.

On exit, the hidden cursor is destroyed or finalized. For shared and mutable iteration, this also ends the source
borrow.

Iteration order, cardinality conditions, finiteness conditions, element borrowing behavior, and element ownership
behavior come from the selected `Iterable` implementation, the selected `Iterator` implementation, and conditions
established for the source expression.

The compiler-known [`Range<T>`](range-expressions.md) type provides shared and consuming finite iteration with exact
cardinality when its bounds are known.

## Navigation

- [Language index](../index.md)
- [Expressions index](../expressions.md)
- Previous: [Range expressions](range-expressions.md)
- Next: [Array generator expressions](array-generator-expressions.md)
