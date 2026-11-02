# Pattern-bearing expressions

A **pattern-bearing expression** is an expression form that applies a pattern to a subject value, subject access path,
or subject element.

The pattern-bearing expression forms are match expressions, `matches` tests, conditional `let`, for expressions,
array generator expressions, general generator iteration expressions, and local destructuring constructs.

[Pattern contexts](../patterns/pattern-contexts.md) defines refutability requirements.
[Pattern operation modes](../patterns/pattern-operation-modes.md) defines observation, borrowing, and consumption.
[Pattern refinement](../patterns/pattern-refinement.md) defines guarantees established by a successful match.

A pattern-bearing expression supplies a subject type to the pattern.

A pattern-bearing expression supplies a pattern operation mode.

A pattern-bearing expression defines whether the pattern must be irrefutable or can be refutable.

[Pattern contexts](../patterns/pattern-contexts.md) defines which constructs require irrefutable patterns and which
accept refutable patterns. Each expression's rules define its subject, operation mode, coverage checks, and failure path.

Pattern-introduced bindings are scoped to the region defined by the pattern-bearing expression.

Pattern-introduced bindings are initialized when the pattern has successfully matched and the operation mode has
produced the corresponding bound values or access paths.

Pattern-introduced bindings participate in ownership, borrowing, mutation authority, initialization, destruction,
finalization, capability checking, effect checking, and condition refinement.

Additional boolean filtering is handled by guards when the surrounding expression form defines guards.

Match `case` arms are the only guard-bearing control-flow arms. Other control-flow forms use their condition expression
directly rather than a separate `when` guard.

## Navigation

- [Language index](../index.md)
- [Expressions index](../expressions.md)
- Previous: [Conversion expressions](conversion-expressions.md)
- Next: [Predicate expressions](predicate-expressions.md)
