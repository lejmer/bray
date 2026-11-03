# Pattern contexts

A **pattern context** is a language construct that applies a pattern to a subject. The construct supplies the subject
type, matching mode, and refutability rules.

Local destructuring and iteration require irrefutable patterns, with iteration checked against its element type. These
contexts reject refutable patterns during checking.

Union variant matching and match expressions accept refutable patterns and check coverage according to the subject
type. A context accepting refutable patterns defines what happens when a pattern does not match.

The syntax grammar names this split with `irrefutable-pattern` for irrefutable-only contexts and `case-pattern` for
match arms, `matches`, and conditional `let`.

`matches` accepts refutable patterns but rejects bindings at every depth. `if let` and `while let` accept refutable
patterns and expose bindings to later `&&` condition operands and their successful body. These contexts do not require exhaustiveness and use the
existing observing match mode.

The syntax root does not prove refutability by itself. Refutability is checked against the subject type.

## Navigation

- [Language index](../index.md)
- [Patterns index](../patterns.md)
- Previous: [Refutability](refutability.md)
- Next: [Pattern operation modes](pattern-operation-modes.md)
