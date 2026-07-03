# Pattern contexts

A pattern context is a language construct that applies a pattern to a subject.

Different pattern contexts use different matching modes and refutability rules.

Local destructuring requires an irrefutable pattern.

Iteration patterns require an irrefutable pattern for the iteration element type.

Union variant matching and match expressions accept refutable patterns and perform coverage checking according to the subject type.

A context that accepts refutable patterns defines what happens when a pattern does not match.

A context that requires irrefutable patterns rejects refutable patterns during checking.

The syntax grammar names this split with `irrefutable-pattern` for irrefutable-only contexts and `case-pattern` for match arms.

The syntax root does not prove refutability by itself. Refutability is checked against the subject type.

## Navigation

- [Language index](../index.md)
- [Patterns index](../patterns.md)
- Previous: [Refutability](refutability.md)
- Next: [Pattern operation modes](pattern-operation-modes.md)
