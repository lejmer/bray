# Boolean fold expressions

A **boolean fold expression** reduces a finite iterable expression with boolean elements to a single `bool`.

Boolean fold expressions use `all(...)` and `any(...)`.

```bray
all(flags)
any(errors)
```

The operand is resolved as a shared iteration source.

The selected element type must be `bool`.

The operand must be finite and bounded.

`all(operand)` evaluates to `true` when every produced element is `true`.

`all(operand)` evaluates to `true` for an empty operand.

`any(operand)` evaluates to `true` when at least one produced element is `true`.

`any(operand)` evaluates to `false` for an empty operand.

Both forms short-circuit.

`all(...)` stops iterating the operand after the first `false` element.

`any(...)` stops iterating the operand after the first `true` element.

The operand expression is evaluated once.

Iteration observes or borrows elements according to the selected `Iterable` and `Iterator` contracts.

Boolean fold expressions do not consume the operand.

A generator expression can be used as the operand.

```bray
let every_valid = all(
    {
        each item in items
        {
            yield item.is_valid();
        }
    }
);
```

Generator expressions are not required.

Any finite bounded iterable expression with `bool` elements can be used.

In ordinary expression context, the operand and any generator body used to produce it obey ordinary expression, ownership,
borrowing, effect, capability, and finalization rules.

In predicate-expression context, the operand and any generator body used to produce it must also obey predicate-expression
restrictions.

## Navigation

- [Language index](../index.md)
- [Expressions index](../expressions.md)
- Previous: [General generator iteration expressions](general-generator-iteration-expressions.md)
- Next: [Struct construction expressions](struct-construction-expressions.md)
