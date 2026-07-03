# Borrow expressions

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

A shared borrow requires an access path that can be observed.

A mutable borrow requires mutation authority over the reached storage and compatible exclusivity for the duration of the borrow.

A borrow expression can borrow a local binding, field access path, indexed access path, active union payload access path, dereferenced type-form projection, or another expression that produces a compatible access path.

[Borrow type forms](../types/type-forms.md#borrow-type-forms) define borrow ownership behavior, compatibility, copying, movement,
storage, returning, reborrowing, nesting, lifetime validity, and invalidation.

Borrow expressions participate in fact-context checking.

Facts about borrowed storage can remain available through a borrow when observation preserves those facts.

Mutation through a mutable borrow invalidates facts that depend on the changed storage.

Movement, destruction, reinitialization, finalization, or capability loss invalidates facts that depend on the borrowed storage.

## Navigation

- [Language index](../index.md)
- [Expressions index](../expressions.md)
- Previous: [Assignment expressions](assignment-expressions.md)
- Next: [Function call expressions](function-call-expressions.md)
