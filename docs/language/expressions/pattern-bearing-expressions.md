# Pattern-bearing expressions

A **pattern-bearing expression** is an expression form that applies a pattern to a subject value, subject access path, or subject element.

Pattern-bearing expression forms include match expressions, for expressions, array generator expressions, general
generator iteration expressions, and local destructuring constructs.

Patterns are a dedicated grammar category.

A pattern-bearing expression supplies a subject type to the pattern.

A pattern-bearing expression supplies a pattern operation mode.

Pattern identifiers resolve in pattern context before they introduce bindings.

A bare identifier that resolves to a pattern-capable declaration is a named pattern.

A bare identifier that does not resolve to a pattern-capable declaration introduces a binding.

Ambiguous pattern-name resolution is rejected.

The core pattern operation modes are:

```text
observe
shared borrow
mutable borrow
consume
copy
```

The pattern is checked against the subject type.

The pattern operation mode determines how bindings introduced by the pattern are produced.

A pattern binding can receive an owned value, copied value, observed access path, shared borrowed access path, or mutable borrowed access path according to the operation mode.

A pattern-bearing expression defines whether the pattern must be irrefutable or can be refutable.

Local destructuring requires an irrefutable pattern.

Iteration binding requires an irrefutable pattern for the iteration element type.

For iteration binding, the pattern operation mode is determined by the selected element type, element ownership behavior, and element borrowing behavior.

Match expressions accept refutable patterns and perform coverage checking according to the subject type and arm set.

A context that requires irrefutable patterns rejects refutable patterns during checking.

A context that accepts refutable patterns defines what happens when the pattern does not match.

A successful pattern can introduce bindings.

A successful pattern can refine the subject.

A successful pattern can add facts to the fact context.

Facts established by a pattern can include active union variant, literal equality, field availability, initialized payload fields, tuple shape, fixed array shape, nullable present or absent state, and narrowed control-flow state.

Pattern-introduced bindings are scoped to the region defined by the pattern-bearing expression.

Pattern-introduced bindings are initialized when the pattern has successfully matched and the operation mode has produced the corresponding bound values or access paths.

Pattern-introduced bindings participate in ownership, borrowing, mutation authority, initialization, destruction, finalization, capability checking, effect checking, and fact-context refinement.

A pattern-bearing expression can partially move from its subject in consume mode.

Partial movement through a pattern requires ownership of the subject and no conflicting active borrows.

A partially moved subject becomes partially initialized.

A partially moved subject can be reinitialized or consumed by a rule that accounts for its state.

Destruction of a partially moved subject destroys only the still-initialized parts.

A pattern-bearing expression can borrow from its subject in shared borrow or mutable borrow mode.

Borrowing through a pattern must satisfy Bray's borrowing rules.

A mutable borrow pattern operation requires mutation authority and compatible exclusivity for the reached storage.

A pattern-bearing expression can copy from its subject in copy mode when the reached types satisfy the required copy contracts.

Pattern matching is structural, deterministic, and effect-free.

Pattern matching can inspect tags, fields, tuple elements, array elements, nullable state, and type-form structure according to the subject type and operation mode.

Pattern matching can bind names, refine facts, and move, copy, borrow, or observe parts according to ownership rules.

Pattern matching does not execute ordinary user code.

Pattern matching does not call ordinary functions or methods.

Pattern matching does not allocate, perform I/O, run asynchronous work, finalize, destroy, or use `with` expressions.

Additional boolean filtering is handled by guards when the surrounding expression form defines guards.

Match `case` arms are the only currently defined guard-bearing control-flow arms. Other control-flow forms use their condition expression directly rather than a separate `when` guard.

## Navigation

- [Language index](../index.md)
- [Expressions index](../expressions.md)
- Previous: [Conversion expressions](conversion-expressions.md)
- Next: [Predicate expressions](predicate-expressions.md)
