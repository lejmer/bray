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
requires mutation authority and compatible exclusivity for the source access path:

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

An iteration expression calls `iterate` exactly once after evaluating the source expression and creating the selected
iteration subject.

The produced cursor is owned by the iteration expression.

The iteration expression repeatedly advances the cursor with `next`.

Each non-`none` result from `next` produces one iteration element.

The first `none` result from `next` is natural exhaustion.

The selected `iterate` call and each selected `next` call participate in type checking, ownership checking, borrowing
checking, capability checking, effect checking, panic checking, and contract checking like ordinary method calls.

The hidden cursor is destroyed or finalized when the iteration expression exits.

For shared and mutable iteration, the source borrow ends when the cursor is destroyed or finalized.

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
