# Summary

Expressions are typed.

Expressions can produce values, access paths, control-flow outcomes, or compile-time entities.

Block expressions always have a type.

Sequenced expressions use semicolons.

Callable results are supplied with `return`.

Yield-capable regions receive values through `yield`.

Break-capable regions receive values through `break`.

Construction expressions use named fields where field identity matters.

Struct construction can omit the type when the expected type is known.

Union variant construction can use leading-dot shorthand when the expected union type is known.

No-payload union variants construct without parentheses.

`box(...)` is the owned-indirection construction expression.

Function calls and method calls are governed by callable contracts.

Assignment returns `unit` on normal completion.

Patterns belong to pattern-bearing expressions and remain their own grammar category.

Match expressions are expressions and produce the selected arm result.

Predicate expressions use a restricted contract-expression context.

Expressions participate in ownership, borrowing, initialization, destruction, finalization, effects, capabilities, and fact-context refinement.

## Navigation

- [Language index](../index.md)
- [Expressions index](../expressions.md)
- Previous: [Expression evaluation order](expression-evaluation-order.md)
