# Borrow expressions

Syntax follows the [expression roots and precedence structure grammar](../syntax-grammar.md#expression-roots-and-precedence-structure).

A **borrow expression** creates a non-owning access path to reached storage.

A shared borrow expression uses `&`.

```bray
&value
```

A mutable borrow expression uses `&mut`.

```bray
&mut value
```

A shared borrow expression produces a value whose type uses the shared-borrow type form.

```bray
&T
```

A mutable borrow expression produces a value whose type uses the mutable-borrow type form.

```bray
&mut T
```

A borrow expression can borrow a local binding, field access path, indexed access path, active union payload access
path, dereferenced type-form projection, or another expression that produces a compatible access path.

[Borrow rules](../ownership-and-borrowing/borrow-rules.md) define borrow compatibility, storage requirements, and
exclusivity.

[Reborrowing and borrow values](../ownership-and-borrowing/reborrowing-and-borrow-values.md) defines copying, movement,
returning, reborrowing, nesting, and lifetime validity for borrow values.

[Contract and borrow validity](../ownership-and-borrowing/contract-and-borrow-validity.md) defines preservation through
observation and invalidation through mutation, movement, destruction, reinitialization, finalization, or capability loss.

## Navigation

- [Language index](../index.md)
- [Expressions index](../expressions.md)
- Previous: [Assignment expressions](assignment-expressions.md)
- Next: [Function call expressions](function-call-expressions.md)
