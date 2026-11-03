# Pattern-bearing expressions

A **pattern-bearing expression** is an expression form that applies a pattern to a subject value, subject access path,
or subject element.

The pattern-bearing expression forms are match expressions, `matches` tests, conditional `let`, for expressions,
array generator expressions, general generator iteration expressions, and local destructuring constructs.

[Pattern contexts](../patterns/pattern-contexts.md) defines refutability requirements.
[Pattern operation modes](../patterns/pattern-operation-modes.md) defines observation, borrowing, and consumption.
[Pattern refinement](../patterns/pattern-refinement.md) defines guarantees established by a successful match.

A pattern-bearing expression supplies the subject type and operation mode and determines whether the pattern must be
irrefutable. [Pattern contexts](../patterns/pattern-contexts.md) defines which constructs require irrefutable patterns
and which accept refutable patterns. Each expression's rules specify its coverage checks and failure path.

Bindings are initialized after a successful match produces their values or access paths under the selected operation
mode. Their scope is the region defined by the surrounding expression. The resulting values and paths retain their
ownership, borrowing, mutation-authority, capability, and cleanup requirements under
[pattern operation modes](../patterns/pattern-operation-modes.md). The compiler checks the operation's effects and
[refinements](../patterns/pattern-refinement.md) in that region.

Additional boolean filtering is handled by guards when the surrounding expression form defines guards.

Match `case` arms are the only guard-bearing control-flow arms. Other control-flow forms use their condition expression
directly rather than a separate `when` guard.

## Navigation

- [Language index](../index.md)
- [Expressions index](../expressions.md)
- Previous: [Conversion expressions](conversion-expressions.md)
- Next: [Predicate expressions](predicate-expressions.md)
