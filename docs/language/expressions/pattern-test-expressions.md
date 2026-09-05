# Pattern-test expressions

`subject matches pattern` evaluates the subject exactly once, observes it, and returns `bool` according to the existing
structural pattern rules. The subject remains available under its existing ownership and capability state.

```bray
result matches Ok(_)
run_result matches Panicked(_) | Cancelled
value matches ?(_, 0)
```

The pattern can be refutable or irrefutable. No coverage or exhaustiveness requirement applies. Literal and constant
patterns, union variants, products, tuples, arrays, nullable patterns, box patterns, grouped patterns, and alternatives
have the same structural meaning as in a match arm.

`matches` introduces no bindings. A binding at any depth, including a field shorthand or a binding in an alternative,
is an error. Use `_` to ignore a matched part, or a conditional `let` when the body needs that part.

The subject type supplies variant resolution. Unqualified variants such as `Ok(_)` are the ordinary spelling.
Qualified variants and leading-dot variants remain valid.

`matches` has comparison-level precedence and is non-associative. Parenthesize a completed pattern test before comparing
its boolean result. `&&`, `||`, and negation combine `matches` results without introducing bindings. Conditional `let`
operands can introduce bindings in an `&&` chain, as described in [conditional expressions](conditional-expressions.md).

True and false branches receive the structural guarantees justified by their outcome. A successful alternative grants
only guarantees common to every possible successful alternative. A failed compound pattern grants only facts justified
regardless of which part failed. Guarantees about a field apply to that field, not its enclosing value or a sibling.
Mutation and ownership changes invalidate dependent guarantees according to the ordinary refinement rules.

`left == right` compares runtime values through `Equatable`. `value matches pattern` tests structure without requiring
or invoking `Equatable`. `if let pattern = value` and `while let pattern = value` expose successful bindings to their
body. `match value { ... }` selects among branching cases.

## Navigation

- [Expressions index](../expressions.md)
- [Patterns](../patterns.md)
- [Conditional expressions](conditional-expressions.md)
- [While expressions](while-expressions.md)
