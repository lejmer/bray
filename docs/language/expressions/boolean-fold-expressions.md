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

In ordinary expression context, the operand and any generator body used to produce it obey ordinary expression,
ownership, borrowing, effect, capability, and finalization rules.

In predicate-expression context, the operand and any generator body used to produce it must also obey
predicate-expression restrictions. The fold owns its hidden cursor. Its implicit selected protocol calls may mutate
that cursor when checked summaries prove confinement to the fold's local state and preserve observational evaluation.
Writes through references stored in the cursor are classified by their reached storage. The protocol calls, delegated
operations, yielded expressions and cursor cleanup must have established admissible behavior. Termination and finite
exhaustion require their own proofs. This protocol admission does not make the selected methods unconditionally pure
or supply a const contract for ordinary direct calls.

Compiled standard-library iterators publish these checked proofs with their dependencies. Consumers reuse the proofs
and apply only their selected input requirements. Standard-library membership does not discharge dependencies on a
user-supplied iterator or callable.

## Navigation

- [Language index](../index.md)
- [Expressions index](../expressions.md)
- Previous: [General generator iteration expressions](general-generator-iteration-expressions.md)
- Next: [Struct construction expressions](struct-construction-expressions.md)
