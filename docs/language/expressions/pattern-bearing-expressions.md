# Pattern-bearing expressions

A **pattern-bearing expression** is an expression form that applies a pattern to a subject value, subject access path, or subject element.

The pattern-bearing expression forms are match expressions, for expressions, array generator expressions, general
generator iteration expressions, and local destructuring constructs.

Pattern syntax, resolution, refutability, operation modes, bindings, partial moves, fact refinement, and structural matching rules
are defined in [Patterns](../patterns.md).

A pattern-bearing expression supplies a subject type to the pattern.

A pattern-bearing expression supplies a pattern operation mode.

A pattern-bearing expression defines whether the pattern must be irrefutable or can be refutable.

Local destructuring requires an irrefutable pattern.

Iteration binding requires an irrefutable pattern for the iteration element type.

For iteration binding, the pattern operation mode is determined by the selected element type, element ownership behavior, and element borrowing behavior.

Match expressions accept refutable patterns and perform coverage checking according to the subject type and arm set.

A context that requires irrefutable patterns rejects refutable patterns during checking.

A context that accepts refutable patterns defines what happens when the pattern does not match.

Pattern-introduced bindings are scoped to the region defined by the pattern-bearing expression.

Pattern-introduced bindings are initialized when the pattern has successfully matched and the operation mode has produced the
corresponding bound values or access paths.

Pattern-introduced bindings participate in ownership, borrowing, mutation authority, initialization, destruction, finalization, capability checking, effect checking, and fact-context refinement.

Additional boolean filtering is handled by guards when the surrounding expression form defines guards.

Match `case` arms are the only guard-bearing control-flow arms. Other control-flow forms use their condition expression directly rather than a separate `when` guard.

## Navigation

- [Language index](../index.md)
- [Expressions index](../expressions.md)
- Previous: [Conversion expressions](conversion-expressions.md)
- Next: [Predicate expressions](predicate-expressions.md)
