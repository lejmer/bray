# Summary

This summary is informative. The detailed rules include:

- [expression results](expression-results.md).
- [expression evaluation order](expression-evaluation-order.md).
- [callable-body block expressions](callable-body-block-expressions.md).

Expressions are typed and can produce values, access paths, control-flow outcomes, or compile-time entities. Block
expressions always have a type. Sequenced expressions use semicolons, with an exception for self-delimiting block items.
`return`, `yield`, and `break` supply values to callable execution scopes, yield-capable regions, and break-capable
regions respectively.

Construction uses named fields where field identity matters. Struct construction can omit the type when the expected
type is known. Union construction can use leading-dot or contextually resolved unqualified shorthand when the expected
union type is known. No-payload variants construct without parentheses. `box(...)` constructs owned indirection.

Calls follow [callable contracts](../callables.md). `start..end` constructs a finite ascending half-open `Range<T>` over
one integer scalar type. Assignment produces `unit` on normal completion, and a match produces its selected arm's
result. Patterns have their own grammar category, defined in [Patterns](../patterns.md).

Predicate expressions use a restricted contract-expression context. All expression forms remain subject to the
ownership, borrowing, initialization, lifecycle, effect, capability, and condition-refinement rules that govern their
operations.

## Navigation

- [Language index](../index.md)
- [Expressions index](../expressions.md)
- Previous: [Expression evaluation order](expression-evaluation-order.md)
